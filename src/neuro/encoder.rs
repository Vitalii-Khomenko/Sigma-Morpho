use crate::core::metrics::ResponseMetric;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct EncoderConfig {
    pub latency_threshold_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatternSummary {
    pub latency_burst: bool,
    pub block_burst: bool,
    pub not_found_burst: bool,
    pub stable_success: bool,
}

pub fn analyze_patterns(
    recent_statuses: &VecDeque<u16>,
    recent_latencies: &VecDeque<u64>,
    recent_errors: &VecDeque<bool>,
    cfg: &EncoderConfig,
) -> PatternSummary {
    let block_count = recent_statuses
        .iter()
        .rev()
        .take(6)
        .filter(|status| matches!(**status, 403 | 429 | 503))
        .count()
        + recent_errors
            .iter()
            .rev()
            .take(6)
            .filter(|err| **err)
            .count();

    let not_found_count = recent_statuses
        .iter()
        .rev()
        .take(8)
        .filter(|status| **status == 404)
        .count();

    let latency_window: Vec<u64> = recent_latencies.iter().rev().take(5).copied().collect();
    let latency_average = if latency_window.is_empty() {
        0.0
    } else {
        latency_window.iter().sum::<u64>() as f64 / latency_window.len() as f64
    };

    let stable_success = recent_statuses.len() >= 4
        && recent_statuses
            .iter()
            .rev()
            .take(4)
            .all(|status| (200..=299).contains(status))
        && recent_errors.iter().rev().take(4).all(|err| !*err);

    PatternSummary {
        latency_burst: latency_average > cfg.latency_threshold_ms as f64
            || recent_latencies
                .iter()
                .rev()
                .take(3)
                .any(|latency| *latency > cfg.latency_threshold_ms.saturating_mul(2)),
        block_burst: block_count >= 3,
        not_found_burst: not_found_count >= 4,
        stable_success,
    }
}

pub fn encode_metric(
    metric: &ResponseMetric,
    cfg: &EncoderConfig,
    patterns: PatternSummary,
) -> [f32; 3] {
    let latency_signal = if metric.latency_ms > cfg.latency_threshold_ms {
        let over = metric.latency_ms - cfg.latency_threshold_ms;
        let normalized = (over as f32 / cfg.latency_threshold_ms.max(1) as f32).min(2.0);
        (0.2 + normalized).min(1.4)
    } else {
        0.0
    } + if patterns.latency_burst { 0.35 } else { 0.0 };

    let block_signal: f32 = if matches!(metric.status, 403 | 429 | 503) {
        1.2
    } else if metric.transport_error {
        0.8
    } else {
        0.0
    } + if patterns.block_burst { 0.45 } else { 0.0 };

    let not_found_signal: f32 = if metric.status == 404 { 0.35 } else { 0.0 }
        + if patterns.not_found_burst && !patterns.stable_success {
            0.25
        } else {
            0.0
        };

    [
        latency_signal.min(1.7),
        block_signal.min(1.8),
        not_found_signal.min(0.9),
    ]
}

#[cfg(test)]
mod tests {
    use super::{analyze_patterns, encode_metric, EncoderConfig, PatternSummary};
    use crate::core::metrics::ResponseMetric;
    use std::collections::VecDeque;

    #[test]
    fn detects_bursts_from_recent_windows() {
        let statuses = VecDeque::from(vec![404, 404, 404, 404, 429, 429, 429]);
        let latencies = VecDeque::from(vec![900, 950, 1100, 1200, 1250]);
        let errors = VecDeque::from(vec![false, false, false, false, false, false, false]);
        let cfg = EncoderConfig {
            latency_threshold_ms: 800,
        };

        let patterns = analyze_patterns(&statuses, &latencies, &errors, &cfg);
        assert!(patterns.latency_burst);
        assert!(patterns.block_burst);
        assert!(patterns.not_found_burst);
    }

    #[test]
    fn burst_patterns_raise_encoded_signal_strength() {
        let metric = ResponseMetric {
            path: "/admin".to_string(),
            status: 404,
            latency_ms: 1000,
            body_size: 10,
            body_fingerprint: 0,
            transport_error: false,
        };
        let cfg = EncoderConfig {
            latency_threshold_ms: 800,
        };

        let baseline = encode_metric(&metric, &cfg, PatternSummary::default());
        let amplified = encode_metric(
            &metric,
            &cfg,
            PatternSummary {
                latency_burst: true,
                block_burst: true,
                not_found_burst: true,
                stable_success: false,
            },
        );

        assert!(amplified[0] > baseline[0]);
        assert!(amplified[1] > baseline[1]);
        assert!(amplified[2] > baseline[2]);
    }
}
