use crate::cli::{AppConfig, ScanMode};
use crate::core::findings::{FindingDecision, FindingWriter, FindingsConfig, Soft404Fingerprint};
use crate::core::metrics::{NeuroTelemetry, ResponseMetric, RunSummary};
use crate::core::simulation::SafeActionSimulator;
use crate::network::client::NetworkClient;
use crate::network::runtime::{
    spawn_safe_client_controller, ClientControlCommand, ClientRebuildReason, ClientRuntimeConfig,
    SafeClientFactory, SharedNetworkClient,
};
use crate::network::tor::TorController;
use crate::neuro::rsnn::{NeuroAction, Rsnn, RsnnConfig};
use anyhow::{Context, Result};
use dashmap::DashSet;
use rand::random;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex, Notify, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{sleep, MissedTickBehavior};

pub async fn run(config: AppConfig, paths: Vec<String>) -> Result<RunSummary> {
    let initial_jobs = build_jobs(&paths, config.rounds);
    let initial_total_jobs = initial_jobs.len();
    let scan_plan = ScanPlan::from_config(&config);

    let mut client_factory = SafeClientFactory::new(ClientRuntimeConfig {
        base_url: config.base_url.clone(),
        timeout_ms: config.timeout_ms,
        profile: config.client_profile,
        speed_mode: config.speed_mode,
        workers: config.workers,
        tor_proxy: config.tor_proxy.clone(),
        proxies: config.proxies.clone(),
    });
    let (client_tx, client_rx) = client_factory.channel()?;
    let (client_command_tx, client_command_rx) = mpsc::channel(4);
    let client_rebuilds = Arc::new(AtomicUsize::new(0));

    let tor_controller = config
        .tor_control
        .clone()
        .map(|control_address| TorController::new(control_address, config.tor_password.clone()));

    let client_controller = spawn_safe_client_controller(
        client_command_rx,
        client_tx,
        client_factory,
        Arc::clone(&client_rebuilds),
        tor_controller,
    );

    let soft_404_fingerprint = if config.disable_soft_404_filter {
        None
    } else {
        let probe_client = client_rx.borrow().clone();
        probe_soft_404(probe_client.as_ref(), &scan_plan).await
    };
    let findings_config = FindingsConfig::new(
        config.findings_file.clone(),
        config.interesting_statuses.clone(),
        config.min_body_bytes,
        config.max_body_bytes,
        config.filter_words.clone(),
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
        client_command_tx.clone(),
        &config,
        Arc::clone(&queue.scheduled_total),
        findings_config,
    );
    let processed_total = Arc::new(AtomicUsize::new(0));
    let rebuild_client_every = config.rebuild_client_every;

    let mut worker_handles = Vec::with_capacity(config.workers);
    for worker_id in 0..config.workers {
        let worker_client_rx = client_rx.clone();
        let worker_queue = Arc::clone(&queue);
        let worker_metric_tx = metric_tx.clone();
        let worker_delay_rx = delay_rx.clone();
        let worker_command_tx = client_command_tx.clone();
        let worker_processed_total = Arc::clone(&processed_total);
        let worker_scan_plan = scan_plan.clone();

        let handle = tokio::spawn(async move {
            worker_loop(
                worker_id,
                worker_client_rx,
                worker_queue,
                worker_metric_tx,
                worker_delay_rx,
                worker_command_tx,
                worker_processed_total,
                rebuild_client_every,
                worker_scan_plan,
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
    summary.filtered_word_counts = config.filter_words.clone();
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
    summary.client_rebuilds = client_rebuilds.load(Ordering::Relaxed) as u64;

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

#[derive(Clone)]
struct ScanPlan {
    mode: ScanMode,
    vhost_template: Option<String>,
    rate_limiter: Option<Arc<Semaphore>>,
}

impl ScanPlan {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            mode: config.scan_mode,
            vhost_template: config.vhost_template.clone(),
            rate_limiter: config
                .rate_per_second
                .map(|rate| spawn_rate_limiter(rate, config.workers)),
        }
    }

    async fn wait_turn(&self, delay_ms: u64) -> Result<()> {
        if let Some(limiter) = &self.rate_limiter {
            let permit = limiter.acquire().await?;
            permit.forget();
        } else {
            sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }

    async fn execute(&self, client: &NetworkClient, token: &str) -> ResponseMetric {
        match self.mode {
            ScanMode::Path => client.execute_path(token).await,
            ScanMode::Vhost => {
                let host = self.render_vhost(token);
                client.execute_vhost(&host).await
            }
        }
    }

    fn render_vhost(&self, token: &str) -> String {
        let candidate = token.trim().trim_matches('.');
        self.vhost_template
            .as_deref()
            .unwrap_or("FUZZ")
            .replace("FUZZ", candidate)
    }
}

fn spawn_rate_limiter(rate_per_second: u64, workers: usize) -> Arc<Semaphore> {
    let limiter = Arc::new(Semaphore::new(0));
    let refill_limiter = Arc::clone(&limiter);
    let burst_capacity = (rate_per_second as usize).max(workers).max(1);

    tokio::spawn(async move {
        let tick_ms = 10_u64;
        let mut carry = 0_u64;
        let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            carry = carry.saturating_add(rate_per_second.saturating_mul(tick_ms));
            let permits = (carry / 1000) as usize;
            carry %= 1000;

            if permits == 0 {
                continue;
            }

            let available = refill_limiter.available_permits();
            if available < burst_capacity {
                refill_limiter.add_permits(permits.min(burst_capacity - available));
            }
        }
    });

    limiter
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

struct DirExpansion {
    base_path: String,
    depth: u8,
    seed_index: usize,
}

struct JobQueueState {
    jobs: VecDeque<WorkItem>,
    active_expansions: VecDeque<DirExpansion>,
}

struct JobQueue {
    queue: Mutex<JobQueueState>,
    visited_dirs: DashSet<String>,
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
        Arc::new(Self {
            queue: Mutex::new(JobQueueState {
                jobs: initial_jobs.into_iter().collect(),
                active_expansions: VecDeque::new(),
            }),
            visited_dirs: DashSet::new(),
            pending: AtomicUsize::new(initial_jobs_len),
            scheduled_total: Arc::new(AtomicUsize::new(initial_jobs_len)),
            notify: Notify::new(),
            seed_paths,
            recursion_depth,
        })
    }

    async fn pop(&self) -> Option<WorkItem> {
        loop {
            let mut state = self.queue.lock().await;

            // Try explicit jobs first
            if let Some(job) = state.jobs.pop_front() {
                return Some(job);
            }

            // Try expansions
            if let Some(mut exp) = state.active_expansions.pop_front() {
                if exp.seed_index < self.seed_paths.len() {
                    let seed = &self.seed_paths[exp.seed_index];
                    let child_path = join_paths(&exp.base_path, seed);
                    exp.seed_index += 1;

                    let depth = exp.depth;
                    if exp.seed_index < self.seed_paths.len() {
                        state.active_expansions.push_back(exp);
                    }
                    return Some(WorkItem {
                        path: child_path,
                        depth,
                    });
                }
            }

            drop(state);

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

        if !self.visited_dirs.insert(job.path.clone()) {
            return 0;
        }

        let seed_count = self.seed_paths.len();
        if seed_count == 0 {
            return 0;
        }

        let mut queue = self.queue.lock().await;
        queue.active_expansions.push_back(DirExpansion {
            base_path: job.path.clone(),
            depth: job.depth + 1,
            seed_index: 0,
        });
        drop(queue);

        self.pending.fetch_add(seed_count, Ordering::Relaxed);
        self.scheduled_total
            .fetch_add(seed_count, Ordering::Relaxed);
        self.notify.notify_waiters();

        seed_count
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    _worker_id: usize,
    client_rx: watch::Receiver<SharedNetworkClient>,
    queue: Arc<JobQueue>,
    metric_tx: mpsc::Sender<ResponseMetric>,
    delay_rx: watch::Receiver<u64>,
    client_command_tx: mpsc::Sender<ClientControlCommand>,
    processed_total: Arc<AtomicUsize>,
    rebuild_client_every: Option<usize>,
    scan_plan: ScanPlan,
) -> Result<RunSummary> {
    let mut local = RunSummary::default();

    loop {
        let Some(job) = queue.pop().await else {
            break;
        };

        let delay_ms = *delay_rx.borrow();
        scan_plan.wait_turn(delay_ms).await?;

        let client = client_rx.borrow().clone();
        let metric = scan_plan.execute(client.as_ref(), &job.path).await;
        let discovered = queue.maybe_enqueue_children(&job, &metric).await;
        if discovered > 0 {
            println!(
                "[RECURSE] base={} depth={} added_children={}",
                metric.path, job.depth, discovered
            );
            local.discovered_jobs += discovered as u64;
        }

        local.record(&metric);

        if let Some(interval) = rebuild_client_every {
            let processed = processed_total.fetch_add(1, Ordering::Relaxed) + 1;
            if processed % interval == 0 {
                let _ = client_command_tx
                    .send(ClientControlCommand::RebuildConnections {
                        reason: ClientRebuildReason::ManualInterval,
                    })
                    .await;
            }
        } else {
            processed_total.fetch_add(1, Ordering::Relaxed);
        }

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
    client_command_tx: mpsc::Sender<ClientControlCommand>,
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
    let rebuild_client_on_advisory = config.rebuild_client_on_advisory;
    let speed_mode = config.speed_mode;
    let min_delay_ms = config.min_delay_ms;
    let max_delay_ms = config.max_delay_ms;
    let state_file = config.snn_state_file.clone();

    tokio::task::spawn_blocking(move || -> Result<NeuroTelemetry> {
        let mut rsnn = Rsnn::new(rsnn_config);

        if let Some(ref path) = state_file {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(state) = serde_json::from_str(&content) {
                        rsnn.load_state(state);
                        println!("[SNN] Loaded memory state from {}", path.display());
                    }
                }
            }
        }

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
                println!(
                    "[SIMULATION] action={} profile={} status={} latency={}ms path={}",
                    action.as_str(),
                    decision.profile.as_str(),
                    metric.status,
                    metric.latency_ms,
                    metric.path
                );
                telemetry.record_simulated_action(action);
                if rebuild_client_on_advisory {
                    let reason = match action {
                        crate::core::simulation::SimulatedAction::RotateUserAgent => {
                            ClientRebuildReason::SimulatedRotateAdvisory
                        }
                        crate::core::simulation::SimulatedAction::RebuildCircuit => {
                            ClientRebuildReason::SimulatedCircuitAdvisory
                        }
                    };
                    let _ = client_command_tx
                        .blocking_send(ClientControlCommand::RebuildConnections { reason });
                    telemetry.record_client_rebuild();
                }
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
        if let Some(ref path) = state_file {
            let state = rsnn.extract_state();
            if let Ok(content) = serde_json::to_string(&state) {
                if let Err(e) = std::fs::write(path, content) {
                    eprintln!(
                        "[SNN WARNING] Failed to save state to {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    println!("[SNN] Saved memory state to {}", path.display());
                }
            }
        }

        Ok(telemetry)
    })
}

async fn probe_soft_404(
    client: &NetworkClient,
    scan_plan: &ScanPlan,
) -> Option<Soft404Fingerprint> {
    let probe_path = format!("sigma-morpho-soft404-probe-{:016x}", random::<u64>());
    let metric = scan_plan.execute(client, &probe_path).await;
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
            body_words: 0,
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
            body_words: 0,
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
            body_words: 0,
            body_fingerprint: 1,
            transport_error: false,
        };

        let job = queue.pop().await.expect("depth-limited job");
        let added = queue.maybe_enqueue_children(&job, &metric).await;

        assert_eq!(added, 0);
        assert_eq!(queue.scheduled_total.load(Ordering::Relaxed), 1);
    }
}
