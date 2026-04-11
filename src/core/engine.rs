use crate::cli::AppConfig;
use crate::core::findings::{FindingDecision, FindingWriter, FindingsConfig, Soft404Fingerprint};
use crate::core::metrics::{NeuroTelemetry, ResponseMetric, RunSummary};
use crate::core::simulation::SafeActionSimulator;
use crate::network::client::NetworkClient;
use crate::network::runtime::{
    spawn_safe_client_controller, ClientRuntimeConfig, SafeClientFactory, SharedNetworkClient,
};
use crate::neuro::rsnn::{NeuroAction, Rsnn, RsnnConfig};
use anyhow::{Context, Result};
use rand::random;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub async fn run(config: AppConfig, paths: Vec<String>) -> Result<RunSummary> {
    let initial_jobs = build_jobs(&paths, config.rounds);
    let initial_total_jobs = initial_jobs.len();

    let client_factory = SafeClientFactory::new(ClientRuntimeConfig {
        base_url: config.base_url.clone(),
        timeout_ms: config.timeout_ms,
        profile: config.client_profile,
        speed_mode: config.speed_mode,
        workers: config.workers,
    });
    let (client_tx, client_rx) = client_factory.channel()?;
    let (client_command_tx, client_command_rx) = mpsc::channel(4);
    let client_controller =
        spawn_safe_client_controller(client_command_rx, client_tx, client_factory);

    let soft_404_fingerprint = if config.disable_soft_404_filter {
        None
    } else {
        let probe_client = client_rx.borrow().clone();
        probe_soft_404(probe_client.as_ref()).await
    };
    let findings_config = FindingsConfig::new(
        config.findings_file.clone(),
        config.interesting_statuses.clone(),
        config.min_body_bytes,
        config.max_body_bytes,
        soft_404_fingerprint.clone(),
    );
    let seed_paths = Arc::new(paths);
    let queue = JobQueue::new(
        initial_jobs,
        Arc::clone(&seed_paths),
        config.recursion_depth,
    );

    let metric_buffer = (config.workers.saturating_mul(8)).max(64);
    let (metric_tx, metric_rx) = mpsc::channel::<ResponseMetric>(metric_buffer);
    let (delay_tx, delay_rx) = watch::channel(config.initial_delay_ms);

    let actor_handle = spawn_neuro_actor(
        metric_rx,
        delay_tx,
        &config,
        Arc::clone(&queue.scheduled_total),
        findings_config,
    );

    let mut worker_handles = Vec::with_capacity(config.workers);
    for worker_id in 0..config.workers {
        let worker_client_rx = client_rx.clone();
        let worker_queue = Arc::clone(&queue);
        let worker_metric_tx = metric_tx.clone();
        let worker_delay_rx = delay_rx.clone();

        let handle = tokio::spawn(async move {
            worker_loop(
                worker_id,
                worker_client_rx,
                worker_queue,
                worker_metric_tx,
                worker_delay_rx,
            )
            .await
        });

        worker_handles.push(handle);
    }

    drop(metric_tx);
    drop(client_command_tx);

    let mut summary = RunSummary::default();
    summary.client_profile = config.client_profile.as_str().to_string();
    summary.speed_mode = config.speed_mode.as_str().to_string();
    summary.simulation_mode = config.simulation_mode;
    summary.findings_file = config.findings_file.display().to_string();
    summary.soft_404_filter_active = soft_404_fingerprint.is_some();
    summary.recursion_depth = config.recursion_depth;
    for handle in worker_handles {
        let worker_summary = handle.await.context("Worker join failed")??;
        summary.merge(worker_summary);
    }

    let neuro = actor_handle.await.context("Neuro actor join failed")??;
    client_controller
        .await
        .context("Client controller join failed")??;
    summary.apply_neuro(neuro);
    summary.final_delay_ms = *delay_rx.borrow();
    summary.discovered_jobs = queue.scheduled_total.load(Ordering::Relaxed) as u64;

    if summary.total_requests != summary.discovered_jobs {
        println!(
            "[*] Note: processed {} of {} scheduled jobs",
            summary.total_requests, summary.discovered_jobs
        );
        println!(
            "[*] Initial static jobs before recursion: {}",
            initial_total_jobs
        );
    }

    Ok(summary)
}

fn build_jobs(paths: &[String], rounds: usize) -> Vec<WorkItem> {
    let mut jobs = Vec::with_capacity(paths.len().saturating_mul(rounds));
    for _ in 0..rounds {
        jobs.extend(
            paths
                .iter()
                .cloned()
                .map(|path| WorkItem { path, depth: 0 }),
        );
    }
    jobs
}

#[derive(Clone, Debug)]
struct WorkItem {
    path: String,
    depth: u8,
}

struct JobQueue {
    queue: Mutex<VecDeque<WorkItem>>,
    visited: Mutex<HashSet<String>>,
    pending: AtomicUsize,
    scheduled_total: Arc<AtomicUsize>,
    notify: Notify,
    seed_paths: Arc<Vec<String>>,
    recursion_depth: u8,
}

impl JobQueue {
    fn new(
        initial_jobs: Vec<WorkItem>,
        seed_paths: Arc<Vec<String>>,
        recursion_depth: u8,
    ) -> Arc<Self> {
        let initial_jobs_len = initial_jobs.len();
        let mut visited = HashSet::with_capacity(initial_jobs.len());
        for job in &initial_jobs {
            visited.insert(job.path.clone());
        }

        Arc::new(Self {
            queue: Mutex::new(initial_jobs.into_iter().collect()),
            visited: Mutex::new(visited),
            pending: AtomicUsize::new(initial_jobs_len),
            scheduled_total: Arc::new(AtomicUsize::new(initial_jobs_len)),
            notify: Notify::new(),
            seed_paths,
            recursion_depth,
        })
    }

    async fn pop(&self) -> Option<WorkItem> {
        loop {
            if let Some(job) = self.queue.lock().await.pop_front() {
                return Some(job);
            }

            if self.pending.load(Ordering::Relaxed) == 0 {
                return None;
            }

            self.notify.notified().await;
        }
    }

    async fn complete(&self) {
        let previous = self.pending.fetch_sub(1, Ordering::Relaxed);
        if previous <= 1 {
            self.notify.notify_waiters();
        }
    }

    async fn maybe_enqueue_children(&self, job: &WorkItem, metric: &ResponseMetric) -> usize {
        if self.recursion_depth == 0 || job.depth >= self.recursion_depth {
            return 0;
        }

        if !should_recurse(job, metric) {
            return 0;
        }

        let mut discovered = Vec::new();
        let mut visited = self.visited.lock().await;

        for seed in self.seed_paths.iter() {
            let child_path = join_paths(&job.path, seed);
            if visited.insert(child_path.clone()) {
                discovered.push(WorkItem {
                    path: child_path,
                    depth: job.depth + 1,
                });
            }
        }
        drop(visited);

        let discovered_len = discovered.len();
        if discovered_len == 0 {
            return 0;
        }

        {
            let mut queue = self.queue.lock().await;
            for child in discovered {
                queue.push_back(child);
            }
        }

        self.pending.fetch_add(discovered_len, Ordering::Relaxed);
        self.scheduled_total
            .fetch_add(discovered_len, Ordering::Relaxed);
        self.notify.notify_waiters();
        discovered_len
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    _worker_id: usize,
    client_rx: watch::Receiver<SharedNetworkClient>,
    queue: Arc<JobQueue>,
    metric_tx: mpsc::Sender<ResponseMetric>,
    delay_rx: watch::Receiver<u64>,
) -> Result<RunSummary> {
    let mut local = RunSummary::default();

    loop {
        let Some(job) = queue.pop().await else {
            break;
        };

        let delay_ms = *delay_rx.borrow();
        sleep(Duration::from_millis(delay_ms)).await;

        let client = client_rx.borrow().clone();
        let metric = client.execute_path(&job.path).await;
        let discovered = queue.maybe_enqueue_children(&job, &metric).await;
        if discovered > 0 {
            println!(
                "[RECURSE] base={} depth={} added_children={}",
                metric.path, job.depth, discovered
            );
            local.discovered_jobs += discovered as u64;
        }

        local.record(&metric);

        if metric_tx.send(metric).await.is_err() {
            queue.complete().await;
            break;
        }

        queue.complete().await;
    }

    Ok(local)
}

fn spawn_neuro_actor(
    mut metric_rx: mpsc::Receiver<ResponseMetric>,
    delay_tx: watch::Sender<u64>,
    config: &AppConfig,
    scheduled_total: Arc<AtomicUsize>,
    findings_config: FindingsConfig,
) -> JoinHandle<Result<NeuroTelemetry>> {
    let rsnn_config = RsnnConfig {
        hidden_size: 20,
        latency_threshold_ms: config.latency_threshold_ms,
        min_delay_ms: config.min_delay_ms,
        max_delay_ms: config.max_delay_ms,
        learning_rate: 0.08,
        history_window: 12,
    };
    let mut current_delay = config.initial_delay_ms;
    let simulation_mode = config.simulation_mode;
    let speed_mode = config.speed_mode;
    let min_delay_ms = config.min_delay_ms;
    let max_delay_ms = config.max_delay_ms;

    tokio::task::spawn_blocking(move || -> Result<NeuroTelemetry> {
        let mut rsnn = Rsnn::new(rsnn_config);
        let mut telemetry = NeuroTelemetry::default();
        let mut simulator = SafeActionSimulator::new(simulation_mode);
        let mut finding_writer = FindingWriter::new(&findings_config)?;
        let mut tick = 0_u64;

        while let Some(metric) = metric_rx.blocking_recv() {
            tick += 1;
            let soft_404_match = findings_config.matches_soft_404(&metric);
            let neuro_metric = if soft_404_match && speed_mode.ignore_soft_404_for_neuro() {
                let mut sanitized = metric.clone();
                sanitized.status = 0;
                sanitized
            } else {
                metric.clone()
            };

            let decision = rsnn.process_metric(&neuro_metric, current_delay);
            telemetry.record_decision(&decision);
            for action in simulator.observe(tick, &metric, &decision) {
                telemetry.record_simulated_action(action);
            }
            let mut next_delay = decision.next_delay_ms;
            let action = decision.action;

            if matches!(action, NeuroAction::DecreaseThrottle) {
                next_delay = next_delay
                    .saturating_sub(speed_mode.recovery_bonus_ms())
                    .max(min_delay_ms);
            }

            next_delay = next_delay.min(speed_mode.delay_cap(max_delay_ms));

            if next_delay != current_delay {
                current_delay = next_delay;
                let _ = delay_tx.send(current_delay);
            }

            match findings_config.classify(&metric) {
                FindingDecision::Record => {
                    println!(
                        "[FINDING] status={} bytes={} latency={}ms path={}",
                        metric.status, metric.body_size, metric.latency_ms, metric.path
                    );
                    finding_writer.record(&metric)?;
                    telemetry.record_finding();
                }
                FindingDecision::SuppressedSoft404 => {
                    telemetry.record_soft_404_suppression();
                }
                FindingDecision::FilteredByStatus | FindingDecision::FilteredByBodyLength => {}
            }

            let total_jobs = scheduled_total.load(Ordering::Relaxed) as u64;
            if tick % 1000 == 0 || (total_jobs > 0 && tick == total_jobs) {
                let progress = if total_jobs == 0 {
                    0.0
                } else {
                    (tick as f64 / total_jobs as f64) * 100.0
                };
                println!(
                    "[PROGRESS] processed={}/{} ({:.2}%) delay={}ms profile={}",
                    tick,
                    total_jobs,
                    progress,
                    current_delay,
                    decision.profile.as_str()
                );
            }

            if matches!(action, NeuroAction::IncreaseThrottle)
                && (metric.transport_error || matches!(metric.status, 403 | 429 | 503))
            {
                println!(
                    "[NEURO] profile={} status={} latency={}ms -> delay={}ms",
                    decision.profile.as_str(),
                    metric.status,
                    metric.latency_ms,
                    current_delay
                );
            }
        }

        for line in simulator.advisory_log() {
            telemetry.push_simulation_note(line.clone());
        }
        telemetry.final_delay_ms = current_delay;
        Ok(telemetry)
    })
}

async fn probe_soft_404(client: &NetworkClient) -> Option<Soft404Fingerprint> {
    let probe_path = format!("sigma-morpho-soft404-probe-{:016x}", random::<u64>());
    let metric = client.execute_path(&probe_path).await;
    let fingerprint = Soft404Fingerprint::from_metric(&metric);

    if let Some(fingerprint) = &fingerprint {
        println!(
            "[*] Soft-404 baseline: status={} bytes={} fingerprint={}",
            fingerprint.status, fingerprint.body_size, fingerprint.body_fingerprint
        );
    }

    fingerprint
}

fn should_recurse(job: &WorkItem, metric: &ResponseMetric) -> bool {
    if metric.transport_error {
        return false;
    }

    let looks_like_directory = !job.path.rsplit('/').next().unwrap_or("").contains('.');

    looks_like_directory && matches!(metric.status, 200 | 204 | 301 | 302 | 307 | 308 | 401 | 403)
}

fn join_paths(base: &str, child: &str) -> String {
    let base = base.trim_matches('/');
    let child = child.trim_matches('/');

    if base.is_empty() {
        child.to_string()
    } else if child.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{child}")
    }
}

#[cfg(test)]
mod tests {
    use super::{join_paths, should_recurse, JobQueue, WorkItem};
    use crate::core::metrics::ResponseMetric;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[test]
    fn recurse_only_for_directory_like_interesting_hits() {
        let job = WorkItem {
            path: "admin".to_string(),
            depth: 0,
        };
        let metric = ResponseMetric {
            path: "/admin".to_string(),
            status: 301,
            latency_ms: 10,
            body_size: 20,
            body_fingerprint: 1,
            transport_error: false,
        };
        assert!(should_recurse(&job, &metric));

        let file_job = WorkItem {
            path: "robots.txt".to_string(),
            depth: 0,
        };
        assert!(!should_recurse(&file_job, &metric));
    }

    #[test]
    fn join_paths_normalizes_slashes() {
        assert_eq!(join_paths("admin", "api"), "admin/api");
        assert_eq!(join_paths("/admin/", "/api/"), "admin/api");
        assert_eq!(join_paths("", "api"), "api");
    }

    #[tokio::test]
    async fn queue_adds_children_once_within_depth_limit() {
        let queue = JobQueue::new(
            vec![WorkItem {
                path: "admin".to_string(),
                depth: 0,
            }],
            Arc::new(vec!["api".to_string(), "login".to_string()]),
            1,
        );
        let metric = ResponseMetric {
            path: "/admin".to_string(),
            status: 301,
            latency_ms: 10,
            body_size: 20,
            body_fingerprint: 1,
            transport_error: false,
        };

        let root = queue.pop().await.expect("root job");
        let added = queue.maybe_enqueue_children(&root, &metric).await;

        assert_eq!(added, 2);
        assert_eq!(queue.scheduled_total.load(Ordering::Relaxed), 3);

        let child_a = queue.pop().await.expect("first child");
        let child_b = queue.pop().await.expect("second child");
        assert_eq!(child_a.depth, 1);
        assert_eq!(child_b.depth, 1);
        assert!(matches!(child_a.path.as_str(), "admin/api" | "admin/login"));
        assert!(matches!(child_b.path.as_str(), "admin/api" | "admin/login"));
        assert_ne!(child_a.path, child_b.path);
    }

    #[tokio::test]
    async fn queue_stops_recursing_at_configured_depth() {
        let queue = JobQueue::new(
            vec![WorkItem {
                path: "admin/api".to_string(),
                depth: 1,
            }],
            Arc::new(vec!["health".to_string()]),
            1,
        );
        let metric = ResponseMetric {
            path: "/admin/api".to_string(),
            status: 301,
            latency_ms: 10,
            body_size: 20,
            body_fingerprint: 1,
            transport_error: false,
        };

        let job = queue.pop().await.expect("depth-limited job");
        let added = queue.maybe_enqueue_children(&job, &metric).await;

        assert_eq!(added, 0);
        assert_eq!(queue.scheduled_total.load(Ordering::Relaxed), 1);
    }
}
