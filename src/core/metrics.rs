use crate::core::profile::AdaptiveProfile;
use crate::core::simulation::SimulatedAction;
use crate::core::speed::SpeedMode;
use crate::neuro::rsnn::{NeuroAction, RsnnDecision};
use std::fmt::Write;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ResponseMetric {
    pub path: String,
    pub status: u16,
    pub latency_ms: u64,
    pub body_size: usize,
    pub body_words: usize,
    pub body_fingerprint: u64,
    pub transport_error: bool,
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub total_requests: u64,
    pub discovered_jobs: u64,
    pub success_2xx: u64,
    pub redirect_3xx: u64,
    pub client_4xx: u64,
    pub server_5xx: u64,
    pub not_found_404: u64,
    pub blocked_responses: u64,
    pub transport_errors: u64,
    pub latency_sum_ms: u128,
    pub latency_samples: u64,
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub total_body_bytes: u128,
    pub filtered_word_counts: Vec<usize>,
    pub hits_200: Vec<String>,
    pub final_delay_ms: u64,
    pub client_profile: String,
    pub speed_mode: String,
    pub simulation_mode: bool,
    pub neuro_decisions: u64,
    pub throttle_increases: u64,
    pub throttle_decreases: u64,
    pub cautious_entries: u64,
    pub defensive_entries: u64,
    pub baseline_recoveries: u64,
    pub latency_burst_events: u64,
    pub block_burst_events: u64,
    pub not_found_burst_events: u64,
    pub logged_findings: u64,
    pub suppressed_soft_404: u64,
    pub findings_file: String,
    pub soft_404_filter_active: bool,
    pub client_rebuilds: u64,
    pub simulated_action_events: u64,
    pub simulated_rotate_advisories: u64,
    pub simulated_circuit_advisories: u64,
    pub simulation_notes: Vec<String>,
    pub final_profile: AdaptiveProfile,
    pub recursion_depth: u8,
}

impl Default for RunSummary {
    fn default() -> Self {
        Self {
            total_requests: 0,
            discovered_jobs: 0,
            success_2xx: 0,
            redirect_3xx: 0,
            client_4xx: 0,
            server_5xx: 0,
            not_found_404: 0,
            blocked_responses: 0,
            transport_errors: 0,
            latency_sum_ms: 0,
            latency_samples: 0,
            min_latency_ms: u64::MAX,
            max_latency_ms: 0,
            total_body_bytes: 0,
            filtered_word_counts: Vec::new(),
            hits_200: Vec::new(),
            final_delay_ms: 0,
            client_profile: "research-default".to_string(),
            speed_mode: SpeedMode::Balanced.as_str().to_string(),
            simulation_mode: false,
            neuro_decisions: 0,
            throttle_increases: 0,
            throttle_decreases: 0,
            cautious_entries: 0,
            defensive_entries: 0,
            baseline_recoveries: 0,
            latency_burst_events: 0,
            block_burst_events: 0,
            not_found_burst_events: 0,
            logged_findings: 0,
            suppressed_soft_404: 0,
            findings_file: "findings.txt".to_string(),
            soft_404_filter_active: false,
            client_rebuilds: 0,
            simulated_action_events: 0,
            simulated_rotate_advisories: 0,
            simulated_circuit_advisories: 0,
            simulation_notes: Vec::new(),
            final_profile: AdaptiveProfile::Baseline,
            recursion_depth: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NeuroTelemetry {
    pub total_decisions: u64,
    pub increase_actions: u64,
    pub decrease_actions: u64,
    pub hold_actions: u64,
    pub cautious_entries: u64,
    pub defensive_entries: u64,
    pub baseline_recoveries: u64,
    pub latency_burst_events: u64,
    pub block_burst_events: u64,
    pub not_found_burst_events: u64,
    pub logged_findings: u64,
    pub suppressed_soft_404: u64,
    pub final_delay_ms: u64,
    pub final_profile: AdaptiveProfile,
    pub client_rebuilds: u64,
    pub simulated_action_events: u64,
    pub simulated_rotate_advisories: u64,
    pub simulated_circuit_advisories: u64,
    pub simulation_notes: Vec<String>,
    last_profile: AdaptiveProfile,
}

impl Default for NeuroTelemetry {
    fn default() -> Self {
        Self {
            total_decisions: 0,
            increase_actions: 0,
            decrease_actions: 0,
            hold_actions: 0,
            cautious_entries: 0,
            defensive_entries: 0,
            baseline_recoveries: 0,
            latency_burst_events: 0,
            block_burst_events: 0,
            not_found_burst_events: 0,
            logged_findings: 0,
            suppressed_soft_404: 0,
            final_delay_ms: 0,
            final_profile: AdaptiveProfile::Baseline,
            client_rebuilds: 0,
            simulated_action_events: 0,
            simulated_rotate_advisories: 0,
            simulated_circuit_advisories: 0,
            simulation_notes: Vec::new(),
            last_profile: AdaptiveProfile::Baseline,
        }
    }
}

impl NeuroTelemetry {
    pub fn record_decision(&mut self, decision: &RsnnDecision) {
        self.total_decisions += 1;

        match decision.action {
            NeuroAction::IncreaseThrottle => self.increase_actions += 1,
            NeuroAction::DecreaseThrottle => self.decrease_actions += 1,
            NeuroAction::Hold => self.hold_actions += 1,
        }

        if decision.patterns.latency_burst {
            self.latency_burst_events += 1;
        }
        if decision.patterns.block_burst {
            self.block_burst_events += 1;
        }
        if decision.patterns.not_found_burst {
            self.not_found_burst_events += 1;
        }

        if decision.profile != self.last_profile {
            match decision.profile {
                AdaptiveProfile::Baseline => self.baseline_recoveries += 1,
                AdaptiveProfile::Cautious => self.cautious_entries += 1,
                AdaptiveProfile::Defensive => self.defensive_entries += 1,
            }
        }

        self.last_profile = decision.profile;
        self.final_profile = decision.profile;
        self.final_delay_ms = decision.next_delay_ms;
    }

    pub fn record_simulated_action(&mut self, action: SimulatedAction) {
        self.simulated_action_events += 1;
        match action {
            SimulatedAction::RotateUserAgent => self.simulated_rotate_advisories += 1,
            SimulatedAction::RebuildCircuit => self.simulated_circuit_advisories += 1,
        }
    }

    pub fn record_client_rebuild(&mut self) {
        self.client_rebuilds += 1;
    }

    pub fn push_simulation_note(&mut self, note: String) {
        self.simulation_notes.push(note);
        if self.simulation_notes.len() > 8 {
            self.simulation_notes.remove(0);
        }
    }

    pub fn record_finding(&mut self) {
        self.logged_findings += 1;
    }

    pub fn record_soft_404_suppression(&mut self) {
        self.suppressed_soft_404 += 1;
    }
}

impl RunSummary {
    pub fn record(&mut self, metric: &ResponseMetric) {
        self.total_requests += 1;
        self.total_body_bytes += metric.body_size as u128;

        self.latency_sum_ms += metric.latency_ms as u128;
        self.latency_samples += 1;
        self.min_latency_ms = self.min_latency_ms.min(metric.latency_ms);
        self.max_latency_ms = self.max_latency_ms.max(metric.latency_ms);

        if metric.transport_error {
            self.transport_errors += 1;
            return;
        }

        match metric.status {
            200..=299 => {
                self.success_2xx += 1;
                if metric.status == 200 {
                    self.hits_200.push(metric.path.clone());
                }
            }
            300..=399 => self.redirect_3xx += 1,
            400..=499 => {
                self.client_4xx += 1;
                if metric.status == 404 {
                    self.not_found_404 += 1;
                }
                if matches!(metric.status, 403 | 429) {
                    self.blocked_responses += 1;
                }
            }
            500..=599 => self.server_5xx += 1,
            _ => {}
        }

        if metric.status == 503 {
            self.blocked_responses += 1;
        }
    }

    pub fn merge(&mut self, other: RunSummary) {
        self.total_requests += other.total_requests;
        self.discovered_jobs += other.discovered_jobs;
        self.success_2xx += other.success_2xx;
        self.redirect_3xx += other.redirect_3xx;
        self.client_4xx += other.client_4xx;
        self.server_5xx += other.server_5xx;
        self.not_found_404 += other.not_found_404;
        self.blocked_responses += other.blocked_responses;
        self.transport_errors += other.transport_errors;

        self.latency_sum_ms += other.latency_sum_ms;
        self.latency_samples += other.latency_samples;
        self.min_latency_ms = self.min_latency_ms.min(other.min_latency_ms);
        self.max_latency_ms = self.max_latency_ms.max(other.max_latency_ms);
        self.total_body_bytes += other.total_body_bytes;
        self.filtered_word_counts.extend(other.filtered_word_counts);
        self.hits_200.extend(other.hits_200);
        self.simulation_notes.extend(other.simulation_notes);
        self.logged_findings += other.logged_findings;
        self.suppressed_soft_404 += other.suppressed_soft_404;
        self.client_rebuilds += other.client_rebuilds;
        self.simulated_action_events += other.simulated_action_events;

        if other.final_delay_ms > 0 {
            self.final_delay_ms = other.final_delay_ms;
        }
    }

    pub fn apply_neuro(&mut self, neuro: NeuroTelemetry) {
        self.neuro_decisions += neuro.total_decisions;
        self.throttle_increases += neuro.increase_actions;
        self.throttle_decreases += neuro.decrease_actions;
        self.cautious_entries += neuro.cautious_entries;
        self.defensive_entries += neuro.defensive_entries;
        self.baseline_recoveries += neuro.baseline_recoveries;
        self.latency_burst_events += neuro.latency_burst_events;
        self.block_burst_events += neuro.block_burst_events;
        self.not_found_burst_events += neuro.not_found_burst_events;
        self.logged_findings += neuro.logged_findings;
        self.suppressed_soft_404 += neuro.suppressed_soft_404;
        self.client_rebuilds += neuro.client_rebuilds;
        self.simulated_action_events += neuro.simulated_action_events;
        self.simulated_rotate_advisories += neuro.simulated_rotate_advisories;
        self.simulated_circuit_advisories += neuro.simulated_circuit_advisories;
        self.simulation_notes.extend(neuro.simulation_notes);
        self.final_profile = neuro.final_profile;
        if neuro.final_delay_ms > 0 {
            self.final_delay_ms = neuro.final_delay_ms;
        }
    }

    fn average_latency_ms(&self) -> f64 {
        if self.latency_samples == 0 {
            return 0.0;
        }

        self.latency_sum_ms as f64 / self.latency_samples as f64
    }

    pub fn render(&self, elapsed: Duration) -> String {
        let mut out = String::new();

        let min_latency = if self.min_latency_ms == u64::MAX {
            0
        } else {
            self.min_latency_ms
        };

        let _ = writeln!(&mut out, "\n===== Sigma Morpho Summary =====");
        let _ = writeln!(&mut out, "Elapsed: {:.2?}", elapsed);
        let _ = writeln!(&mut out, "Client profile: {}", self.client_profile);
        let _ = writeln!(&mut out, "Speed mode: {}", self.speed_mode);
        let _ = writeln!(&mut out, "Simulation mode: {}", self.simulation_mode);
        let _ = writeln!(&mut out, "Findings file: {}", self.findings_file);
        let _ = writeln!(&mut out, "Recursion depth: {}", self.recursion_depth);
        let _ = writeln!(
            &mut out,
            "Soft-404 filter active: {}",
            self.soft_404_filter_active
        );
        let _ = writeln!(&mut out, "Total requests: {}", self.total_requests);
        let _ = writeln!(&mut out, "Discovered jobs: {}", self.discovered_jobs);
        let _ = writeln!(&mut out, "2xx: {}", self.success_2xx);
        let _ = writeln!(&mut out, "3xx: {}", self.redirect_3xx);
        let _ = writeln!(&mut out, "4xx: {}", self.client_4xx);
        let _ = writeln!(&mut out, "404s: {}", self.not_found_404);
        let _ = writeln!(
            &mut out,
            "Protective block signals (403/429/503): {}",
            self.blocked_responses
        );
        let _ = writeln!(&mut out, "5xx: {}", self.server_5xx);
        let _ = writeln!(&mut out, "Transport errors: {}", self.transport_errors);
        let _ = writeln!(&mut out, "Avg latency: {:.2} ms", self.average_latency_ms());
        let _ = writeln!(&mut out, "Min latency: {} ms", min_latency);
        let _ = writeln!(&mut out, "Max latency: {} ms", self.max_latency_ms);
        let _ = writeln!(&mut out, "Total body bytes: {}", self.total_body_bytes);
        if !self.filtered_word_counts.is_empty() {
            let _ = writeln!(
                &mut out,
                "Filtered word counts: {}",
                self.filtered_word_counts
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        let _ = writeln!(&mut out, "Final adaptive delay: {} ms", self.final_delay_ms);
        let _ = writeln!(
            &mut out,
            "Final adaptive profile: {}",
            self.final_profile.as_str()
        );
        let _ = writeln!(&mut out, "Neuro decisions: {}", self.neuro_decisions);
        let _ = writeln!(&mut out, "Throttle increases: {}", self.throttle_increases);
        let _ = writeln!(&mut out, "Throttle decreases: {}", self.throttle_decreases);
        let _ = writeln!(&mut out, "Cautious entries: {}", self.cautious_entries);
        let _ = writeln!(&mut out, "Defensive entries: {}", self.defensive_entries);
        let _ = writeln!(
            &mut out,
            "Baseline recoveries: {}",
            self.baseline_recoveries
        );
        let _ = writeln!(
            &mut out,
            "Latency burst events: {}",
            self.latency_burst_events
        );
        let _ = writeln!(&mut out, "Block burst events: {}", self.block_burst_events);
        let _ = writeln!(
            &mut out,
            "404 burst events: {}",
            self.not_found_burst_events
        );
        let _ = writeln!(&mut out, "Logged findings: {}", self.logged_findings);
        let _ = writeln!(
            &mut out,
            "Suppressed soft-404 matches: {}",
            self.suppressed_soft_404
        );
        let _ = writeln!(&mut out, "Client rebuilds: {}", self.client_rebuilds);
        let _ = writeln!(
            &mut out,
            "Simulated action events: {}",
            self.simulated_action_events
        );
        let _ = writeln!(
            &mut out,
            "Simulated rotate advisories: {}",
            self.simulated_rotate_advisories
        );
        let _ = writeln!(
            &mut out,
            "Simulated circuit advisories: {}",
            self.simulated_circuit_advisories
        );

        if !self.hits_200.is_empty() {
            let _ = writeln!(&mut out, "200 hits:");
            for path in &self.hits_200 {
                let _ = writeln!(&mut out, "  - {}", path);
            }
        }

        if !self.simulation_notes.is_empty() {
            let _ = writeln!(&mut out, "Simulation notes:");
            for note in &self.simulation_notes {
                let _ = writeln!(&mut out, "  - {}", note);
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::{NeuroTelemetry, ResponseMetric, RunSummary};
    use crate::core::profile::AdaptiveProfile;
    use crate::neuro::encoder::PatternSummary;
    use crate::neuro::rsnn::{NeuroAction, RsnnDecision};
    use std::time::Duration;

    #[test]
    fn summary_counts_not_found_and_blocks() {
        let mut summary = RunSummary::default();
        summary.record(&ResponseMetric {
            path: "/missing".to_string(),
            status: 404,
            latency_ms: 20,
            body_size: 0,
            body_words: 0,
            body_fingerprint: 0,
            transport_error: false,
        });
        summary.record(&ResponseMetric {
            path: "/rate".to_string(),
            status: 429,
            latency_ms: 30,
            body_size: 0,
            body_words: 0,
            body_fingerprint: 0,
            transport_error: false,
        });

        assert_eq!(summary.not_found_404, 1);
        assert_eq!(summary.blocked_responses, 1);
    }

    #[test]
    fn neuro_telemetry_tracks_profile_transitions() {
        let mut telemetry = NeuroTelemetry::default();
        telemetry.record_decision(&RsnnDecision {
            next_delay_ms: 100,
            action: NeuroAction::IncreaseThrottle,
            profile: AdaptiveProfile::Cautious,
            patterns: PatternSummary {
                latency_burst: true,
                block_burst: false,
                not_found_burst: false,
                stable_success: false,
            },
        });
        telemetry.record_decision(&RsnnDecision {
            next_delay_ms: 220,
            action: NeuroAction::IncreaseThrottle,
            profile: AdaptiveProfile::Defensive,
            patterns: PatternSummary {
                latency_burst: true,
                block_burst: true,
                not_found_burst: false,
                stable_success: false,
            },
        });

        assert_eq!(telemetry.cautious_entries, 1);
        assert_eq!(telemetry.defensive_entries, 1);
        assert_eq!(telemetry.block_burst_events, 1);
    }

    #[test]
    fn summary_render_includes_recursion_fields() {
        let mut summary = RunSummary::default();
        summary.recursion_depth = 2;
        summary.discovered_jobs = 42;
        summary.client_rebuilds = 1;
        summary.simulated_action_events = 3;

        let rendered = summary.render(Duration::from_millis(100));
        assert!(rendered.contains("Recursion depth: 2"));
        assert!(rendered.contains("Discovered jobs: 42"));
        assert!(rendered.contains("Client rebuilds: 1"));
        assert!(rendered.contains("Simulated action events: 3"));
    }
}
