# Technical Specification

## Core Technology
- Language: Rust
- Frameworks: `tokio` (Async runtime), `reqwest` (HTTP processing), `serde` (Serialization)
- Cryptography: `rustls` (Native TLS to bypass OS-dependent checks)

## Recurrent Spiking Neural Network (RSNN)
An actor-based neural layer monitors HTTP signals.
- **Input Encoding Layer**: `Latency`, `Transport Error`, `HTTP Status (200, 403, 429)`.
- **Hidden Layer**: Default `20` Leaky Integrate-and-Fire (LIF) neurons.
- **Synaptic Weights**: Mutated through STDP (Spike-Time Dependent Plasticity) and saved iteratively via `--snn-state-file snn.json`.
- **Output Decisions**: Issues Action triggers like `Hold`, `IncreaseThrottle`, or sends Tor control signals upon extreme stress.

## Concurrency
To ensure minimal collision:
- **Workers**: Pool sizes up to thousands (I/O Bound). Uses `watch` to receive throttle intervals.
- **Brain**: Single lock-free thread executing floating-point Math.

## Advanced Evasion
1. **Tor Circuit Control**: Socket `127.0.0.1:9051` integration via `SIGNAL NEWNYM`.
2. **TLS Obfuscation**: Forces Chromium-like features by restricting default ALPN strictly to `h2` and minimizing standard TLS negotiations.
3. **Distributed Endpoints**: Loads external `--proxies-file` endpoints to bounce HTTP requests securely across the internet.
