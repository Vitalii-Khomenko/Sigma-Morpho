use crate::cli::AppConfig;
use crate::core::metrics::{NeuroTelemetry, ResponseMetric, RunSummary};
use crate::core::simulation::SafeActionSimulator;
use crate::network::profile::ClientProfile;
use crate::neuro::rsnn::{Rsnn, RsnnConfig};
use clap::ValueEnum;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ScenarioKind {
    Healthy,
    RateLimit,
    Tarpit,
    MixedDefense,
}

impl ScenarioKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::RateLimit => "rate-limit",
            Self::Tarpit => "tarpit",
            Self::MixedDefense => "mixed-defense",
        }
    }
}

pub struct ResearchReport {
    pub scenario: ScenarioKind,
    pub outcomes: Vec<ScenarioOutcome>,
}

pub struct LiveProfileReport {
    pub target: String,
    pub outcomes: Vec<ProfileOutcome>,
}

pub struct ProfileOutcome {
    pub profile: ClientProfile,
    pub summary: RunSummary,
}

pub struct ScenarioOutcome {
    pub profile: ClientProfile,
    pub summary: RunSummary,
}

impl ResearchReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            &mut out,
            "\n===== Sigma Morpho Scenario Report ({}) =====",
            self.scenario.as_str()
        );

        for outcome in &self.outcomes {
            let _ = writeln!(&mut out, "\nProfile: {}", outcome.profile.as_str());
            let _ = writeln!(
                &mut out,
                "Requests: {}, Final delay: {} ms, Final profile: {}",
                outcome.summary.total_requests,
                outcome.summary.final_delay_ms,
                outcome.summary.final_profile.as_str()
            );
            let _ = writeln!(
                &mut out,
                "Blocks: {}, 404s: {}, Increases: {}, Decreases: {}",
                outcome.summary.blocked_responses,
                outcome.summary.not_found_404,
                outcome.summary.throttle_increases,
                outcome.summary.throttle_decreases
            );
            let _ = writeln!(
                &mut out,
                "Simulated rotate advisories: {}, simulated circuit advisories: {}",
                outcome.summary.simulated_rotate_advisories,
                outcome.summary.simulated_circuit_advisories
            );
        }

        out
    }
}

impl LiveProfileReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            &mut out,
            "\n===== Sigma Morpho Live Profile Comparison ====="
        );
        let _ = writeln!(&mut out, "Target: {}", self.target);

        for outcome in &self.outcomes {
            let _ = writeln!(&mut out, "\nProfile: {}", outcome.profile.as_str());
            let _ = writeln!(
                &mut out,
                "Requests: {}, Avg latency: {:.2} ms, Final delay: {} ms",
                outcome.summary.total_requests,
                if outcome.summary.latency_samples == 0 {
                    0.0
                } else {
                    outcome.summary.latency_sum_ms as f64 / outcome.summary.latency_samples as f64
                },
                outcome.summary.final_delay_ms
            );
            let _ = writeln!(
                &mut out,
                "Findings: {}, Blocks: {}, Transport errors: {}, Client rebuilds: {}",
                outcome.summary.logged_findings,
                outcome.summary.blocked_responses,
                outcome.summary.transport_errors,
                outcome.summary.client_rebuilds
            );
            let _ = writeln!(
                &mut out,
                "Simulated actions: {}, Final profile: {}",
                outcome.summary.simulated_action_events,
                outcome.summary.final_profile.as_str()
            );
        }

        out
    }
}

pub fn run_scenario(config: &AppConfig) -> ResearchReport {
    let scenario = config
        .scenario
        .expect("run_scenario must be called with scenario mode enabled");
    let profiles: Vec<ClientProfile> = if config.compare_profiles {
        ClientProfile::ALL.to_vec()
    } else {
        vec![config.client_profile]
    };

    let outcomes = profiles
        .into_iter()
        .map(|profile| ScenarioOutcome {
            profile,
            summary: execute_scenario(config, scenario, profile),
        })
        .collect();

    ResearchReport { scenario, outcomes }
}

fn execute_scenario(
    config: &AppConfig,
    scenario: ScenarioKind,
    profile: ClientProfile,
) -> RunSummary {
    let metrics = build_scenario_metrics(scenario);
    let mut rsnn = Rsnn::new(RsnnConfig {
        hidden_size: 20,
        latency_threshold_ms: config.latency_threshold_ms,
        min_delay_ms: config.min_delay_ms,
        max_delay_ms: config.max_delay_ms,
        learning_rate: 0.08,
        history_window: 12,
    });
    let mut delay = profile.adjusted_initial_delay(
        config.initial_delay_ms,
        config.min_delay_ms,
        config.max_delay_ms,
    );
    let mut summary = RunSummary::default();
    summary.client_profile = profile.as_str().to_string();
    summary.simulation_mode = config.simulation_mode;

    let mut telemetry = NeuroTelemetry::default();
    let mut simulator = SafeActionSimulator::new(config.simulation_mode);

    for (tick, metric) in metrics.into_iter().enumerate() {
        summary.record(&metric);
        let decision = rsnn.process_metric(&metric, delay);
        telemetry.record_decision(&decision);

        for action in simulator.observe((tick + 1) as u64, &metric, &decision) {
            telemetry.record_simulated_action(action);
        }

        delay = decision.next_delay_ms;
    }

    for line in simulator.advisory_log() {
        telemetry.push_simulation_note(line.clone());
    }

    telemetry.final_delay_ms = delay;
    summary.apply_neuro(telemetry);
    summary.final_delay_ms = delay;
    summary
}

fn build_scenario_metrics(scenario: ScenarioKind) -> Vec<ResponseMetric> {
    match scenario {
        ScenarioKind::Healthy => (0..12)
            .map(|idx| ResponseMetric {
                path: format!("/ok-{idx}"),
                status: 200,
                latency_ms: 60 + (idx % 3) as u64 * 10,
                body_size: 128,
                body_fingerprint: 0,
                transport_error: false,
            })
            .collect(),
        ScenarioKind::RateLimit => vec![
            metric("/seed", 200, 70),
            metric("/seed-2", 200, 75),
            metric("/gate-1", 429, 420),
            metric("/gate-2", 429, 650),
            metric("/gate-3", 403, 710),
            metric("/gate-4", 429, 780),
            metric("/recover-1", 200, 140),
            metric("/recover-2", 200, 110),
        ],
        ScenarioKind::Tarpit => vec![
            metric("/warmup-1", 200, 90),
            metric("/warmup-2", 200, 110),
            metric("/slow-1", 200, 1200),
            metric("/slow-2", 404, 1500),
            metric("/slow-3", 404, 1800),
            metric("/slow-4", 404, 2100),
            metric("/slow-5", 404, 2400),
            metric("/recover", 200, 120),
        ],
        ScenarioKind::MixedDefense => vec![
            metric("/prelude", 200, 80),
            metric("/nf-1", 404, 140),
            metric("/nf-2", 404, 160),
            metric("/nf-3", 404, 210),
            metric("/nf-4", 404, 260),
            metric("/block-1", 403, 650),
            metric("/block-2", 429, 850),
            metric("/block-3", 503, 920),
            metric("/error", 0, 1000).with_transport_error(),
            metric("/recover-1", 200, 130),
            metric("/recover-2", 200, 95),
        ],
    }
}

fn metric(path: &str, status: u16, latency_ms: u64) -> ResponseMetric {
    ResponseMetric {
        path: path.to_string(),
        status,
        latency_ms,
        body_size: if status == 200 { 128 } else { 0 },
        body_fingerprint: 0,
        transport_error: false,
    }
}

trait MetricExt {
    fn with_transport_error(self) -> Self;
}

impl MetricExt for ResponseMetric {
    fn with_transport_error(mut self) -> Self {
        self.transport_error = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{run_scenario, ScenarioKind};
    use crate::cli::AppConfig;
    use crate::core::speed::SpeedMode;
    use crate::network::profile::ClientProfile;
    use url::Url;

    #[test]
    fn compare_profiles_generates_multiple_outcomes() {
        let config = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000").unwrap(),
            wordlist: "wordlist.txt".into(),
            workers: 4,
            rounds: 1,
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 50,
            initial_delay_ms: 50,
            rebuild_client_every: None,
            rebuild_client_on_advisory: false,
            min_delay_ms: 25,
            max_delay_ms: 500,
            latency_threshold_ms: 300,
            findings_file: "findings.txt".into(),
            interesting_statuses: vec![200, 403],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: true,
            scenario: Some(ScenarioKind::MixedDefense),
            compare_profiles: true,
            compare_profiles_live: false,
            tor_proxy: None,
            tor_control: None,
            tor_password: None,
            snn_state_file: None,
            proxies_file: None,
            proxies: vec![],
        };

        let report = run_scenario(&config);
        assert_eq!(report.outcomes.len(), 4);
        assert!(report
            .outcomes
            .iter()
            .any(|outcome| outcome.summary.simulated_rotate_advisories > 0));
    }
}
