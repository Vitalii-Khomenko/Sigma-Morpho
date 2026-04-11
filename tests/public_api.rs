use sigma_morpho::core::metrics::{NeuroTelemetry, ResponseMetric, RunSummary};
use sigma_morpho::core::profile::AdaptiveProfile;
use sigma_morpho::neuro::encoder::PatternSummary;
use sigma_morpho::neuro::rsnn::{NeuroAction, RsnnDecision};

#[test]
fn summary_render_includes_neuro_profile() {
    let mut summary = RunSummary::default();
    summary.record(&ResponseMetric {
        path: "/health".to_string(),
        status: 200,
        latency_ms: 42,
        body_size: 128,
        transport_error: false,
    });

    let mut telemetry = NeuroTelemetry::default();
    telemetry.record_decision(&RsnnDecision {
        next_delay_ms: 90,
        action: NeuroAction::DecreaseThrottle,
        profile: AdaptiveProfile::Baseline,
        patterns: PatternSummary {
            latency_burst: false,
            block_burst: false,
            not_found_burst: false,
            stable_success: true,
        },
    });
    summary.apply_neuro(telemetry);

    let rendered = summary.render(std::time::Duration::from_millis(100));
    assert!(rendered.contains("Final adaptive profile: baseline"));
    assert!(rendered.contains("Throttle decreases: 1"));
}
