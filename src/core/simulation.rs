use std::collections::VecDeque;

use crate::core::metrics::ResponseMetric;
use crate::neuro::rsnn::RsnnDecision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulatedAction {
    RotateUserAgent,
    RebuildCircuit,
}

impl SimulatedAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RotateUserAgent => "rotate-user-agent-advisory",
            Self::RebuildCircuit => "rebuild-circuit-advisory",
        }
    }
}

#[derive(Debug, Default)]
pub struct SafeActionSimulator {
    enabled: bool,
    advisory_log: VecDeque<String>,
    last_rotate_tick: u64,
    last_rebuild_tick: u64,
}

impl SafeActionSimulator {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            advisory_log: VecDeque::with_capacity(8),
            last_rotate_tick: 0,
            last_rebuild_tick: 0,
        }
    }

    pub fn observe(
        &mut self,
        tick: u64,
        metric: &ResponseMetric,
        decision: &RsnnDecision,
    ) -> Vec<SimulatedAction> {
        if !self.enabled {
            return Vec::new();
        }

        let mut actions = Vec::new();

        if decision.patterns.block_burst
            && matches!(metric.status, 403 | 429 | 503)
            && tick.saturating_sub(self.last_rotate_tick) >= 4
        {
            self.last_rotate_tick = tick;
            self.push_log(format!(
                "[SIMULATION] {} triggered by repeated protective blocks at {}",
                SimulatedAction::RotateUserAgent.as_str(),
                metric.path
            ));
            actions.push(SimulatedAction::RotateUserAgent);
        }

        if decision.patterns.block_burst
            && matches!(metric.status, 429 | 503)
            && decision.profile.as_str() == "defensive"
            && tick.saturating_sub(self.last_rebuild_tick) >= 6
        {
            self.last_rebuild_tick = tick;
            self.push_log(format!(
                "[SIMULATION] {} triggered by defensive escalation at {}",
                SimulatedAction::RebuildCircuit.as_str(),
                metric.path
            ));
            actions.push(SimulatedAction::RebuildCircuit);
        }

        actions
    }

    pub fn advisory_log(&self) -> impl Iterator<Item = &String> {
        self.advisory_log.iter()
    }

    fn push_log(&mut self, message: String) {
        self.advisory_log.push_back(message);
        while self.advisory_log.len() > 8 {
            self.advisory_log.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SafeActionSimulator, SimulatedAction};
    use crate::core::metrics::ResponseMetric;
    use crate::core::profile::AdaptiveProfile;
    use crate::neuro::encoder::PatternSummary;
    use crate::neuro::rsnn::{NeuroAction, RsnnDecision};

    #[test]
    fn simulator_emits_advisories_without_real_actions() {
        let mut simulator = SafeActionSimulator::new(true);
        let metric = ResponseMetric {
            path: "/blocked".to_string(),
            status: 429,
            latency_ms: 900,
            body_size: 0,
            transport_error: false,
        };
        let decision = RsnnDecision {
            next_delay_ms: 300,
            action: NeuroAction::IncreaseThrottle,
            profile: AdaptiveProfile::Defensive,
            patterns: PatternSummary {
                latency_burst: true,
                block_burst: true,
                not_found_burst: false,
                stable_success: false,
            },
        };

        let actions = simulator.observe(10, &metric, &decision);
        assert!(actions.contains(&SimulatedAction::RotateUserAgent));
        assert!(actions.contains(&SimulatedAction::RebuildCircuit));
    }
}
