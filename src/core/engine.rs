use crate::cli::AppConfig;
use crate::core::findings::{FindingDecision, FindingWriter, FindingsConfig, Soft404Fingerprint};
use crate::core::metrics::{NeuroTelemetry, ResponseMetric, RunSummary};
use crate::core::simulation::SafeActionSimulator;
use crate::network::client::NetworkClient;
use crate::neuro::rsnn::{NeuroAction, Rsnn, RsnnConfig};
use anyhow::{Context, Result};
use rand::random;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub async fn run(config: AppConfig, paths: Vec<String>) -> Result<RunSummary> {
    let jobs = Arc::new(build_jobs(&paths, config.rounds));
    let total_jobs = jobs.len();

    let client = Arc::new(NetworkClient::new(
        config.base_url.clone(),
        config.timeout_ms,
        config.client_profile,
        config.speed_mode,
        config.workers,
    )?);
    let soft_404_fingerprint = if config.disable_soft_404_filter {
        None
    } else {
        probe_soft_404(&client).await
    };
    let findings_config = FindingsConfig::new(
        config.findings_file.clone(),
        config.interesting_statuses.clone(),
        config.min_body_bytes,
        config.max_body_bytes,
        soft_404_fingerprint.clone(),
    );
    let index = Arc::new(AtomicUsize::new(0));

    let metric_buffer = (config.workers.saturating_mul(8)).max(64);
    let (metric_tx, metric_rx) = mpsc::channel::<ResponseMetric>(metric_buffer);
    let (delay_tx, delay_rx) = watch::channel(config.initial_delay_ms);

    let actor_handle = spawn_neuro_actor(
        metric_rx,
        delay_tx,
        &config,
        total_jobs as u64,
        findings_config,
    );

    let mut worker_handles = Vec::with_capacity(config.workers);
    for worker_id in 0..config.workers {
        let worker_client = Arc::clone(&client);
        let worker_jobs = Arc::clone(&jobs);
        let worker_index = Arc::clone(&index);
        let worker_metric_tx = metric_tx.clone();
        let worker_delay_rx = delay_rx.clone();

        let handle = tokio::spawn(async move {
            worker_loop(
                worker_id,
                worker_client,
                worker_jobs,
                worker_index,
                worker_metric_tx,
                worker_delay_rx,
            )
            .await
        });

        worker_handles.push(handle);
    }

    drop(metric_tx);

    let mut summary = RunSummary::default();
    summary.client_profile = config.client_profile.as_str().to_string();
    summary.speed_mode = config.speed_mode.as_str().to_string();
    summary.simulation_mode = config.simulation_mode;
    summary.findings_file = config.findings_file.display().to_string();
    summary.soft_404_filter_active = soft_404_fingerprint.is_some();
    for handle in worker_handles {
        let worker_summary = handle.await.context("Worker join failed")??;
        summary.merge(worker_summary);
    }

    let neuro = actor_handle.await.context("Neuro actor join failed")??;
    summary.apply_neuro(neuro);
    summary.final_delay_ms = *delay_rx.borrow();

    if summary.total_requests != total_jobs as u64 {
        println!(
            "[*] Note: processed {} of {} jobs",
            summary.total_requests, total_jobs
        );
    }

    Ok(summary)
}

fn build_jobs(paths: &[String], rounds: usize) -> Vec<String> {
    let mut jobs = Vec::with_capacity(paths.len().saturating_mul(rounds));
    for _ in 0..rounds {
        jobs.extend(paths.iter().cloned());
    }
    jobs
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    _worker_id: usize,
    client: Arc<NetworkClient>,
    jobs: Arc<Vec<String>>,
    index: Arc<AtomicUsize>,
    metric_tx: mpsc::Sender<ResponseMetric>,
    delay_rx: watch::Receiver<u64>,
) -> Result<RunSummary> {
    let mut local = RunSummary::default();

    loop {
        let idx = index.fetch_add(1, Ordering::Relaxed);
        if idx >= jobs.len() {
            break;
        }

        let path = jobs[idx].clone();
        let delay_ms = *delay_rx.borrow();
        sleep(Duration::from_millis(delay_ms)).await;

        let metric = client.execute_path(&path).await;

        local.record(&metric);

        if metric_tx.send(metric).await.is_err() {
            break;
        }
    }

    Ok(local)
}

fn spawn_neuro_actor(
    mut metric_rx: mpsc::Receiver<ResponseMetric>,
    delay_tx: watch::Sender<u64>,
    config: &AppConfig,
    total_jobs: u64,
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

            if tick % 1000 == 0 || tick == total_jobs {
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
