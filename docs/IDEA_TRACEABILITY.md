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
- **User-Agent Rotation**: Native hot-swapping of clients with randomized profiles.
- **Tor integration**: `SIGNAL NEWNYM` support via Control Port.
- **Lazy Recursion**: High-performance `O(1)` memory path generation.
- **Concurrent State**: Lock-free visited set via `dashmap`.

## Adaptations & Safety

- “WAF/IPS analysis”
  Implemented as protective-pattern detection and pacing control.
- “Stealth mode”
  Realized through adaptive delay profiles and identity rotation signals.
- “Sequence anomaly detection”
  Implemented through recent-window burst analysis for latency, blocks, and 404 density.

## Project Context
Originally designed for HTB Academy review with a defensive focus, the project has transitioned into a **RED Teaming & CTF Edition**. This version includes previously restricted features (IP/UA rotation) to provide a complete research platform for studying evasion in high-security environments.
