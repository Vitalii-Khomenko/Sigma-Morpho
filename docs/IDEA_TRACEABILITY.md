# Idea Traceability

This file maps `Ideas.md` to the implemented project state.

## Implemented or translated directly

- Async Rust/Tokio + Reqwest request engine
- Separate encoder, neuron, synapse, and RSNN modules
- LIF neuron model
- Recurrent hidden state for temporal context
- STDP-inspired online weight updates
- Actor-based neuro pipeline using channels
- Dynamic throttling from observed response patterns
- Documentation describing the full research context
- Automated tests

## Implemented as a safe adaptation

- “WAF/IPS analysis”
  Implemented as protective-pattern detection and pacing control, not bypass logic.
- “Stealth mode”
  Reframed as safe `cautious` and `defensive` profiles that only increase delay.
- “Sequence anomaly detection”
  Implemented through recent-window burst analysis for latency, blocks, and 404 density.
- “RotateUserAgent” and “RebuildTorCircuit”
  Reframed as simulation-only advisory events used for telemetry and offline analysis.
- “hot-swap active client via watch channels”
  Implemented as safe transport-control infrastructure for rebuilding the same fixed client profile and connection pool, not as identity rotation.

## Intentionally not implemented

- User-Agent rotation
- Tor `NEWNYM`
- IP/proxy churn
- identity-changing rotator modules
- behavior intended to defeat attribution or defensive controls

## Reason

Those features would shift the project from authorized reliability/security research into bypass assistance. For HTB Academy review, the safer and more defensible implementation is adaptive pacing plus transparent telemetry.
