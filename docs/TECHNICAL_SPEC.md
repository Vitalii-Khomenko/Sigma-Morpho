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

## Concurrency & Performance
- **Workers**: High-concurrency I/O workers (thousands) using `reqwest` and `tokio`.
- **Lazy Recursion**: Implements a high-efficiency job queue where new paths are generated on-the-fly (`O(1)`), avoiding `Mutex` lock contention and memory surges during massive directory expansion.
- **State Tracking**: Uses `dashmap` (a concurrent, non-locking hash map) for the `visited` set to ensure zero-latency lookups for discovered paths.
- **Brain**: Isolated `tokio::spawn_blocking` actor task for SNN floating-point operations, preventing "the world will stop" pauses.

## Advanced Evasion
1. **Proxy Multiplexing**: Automatically balances traffic across multiple proxies or Tor circuits simultaneously using a pool of internal HTTP clients.
2. **Tor Control Integration**: Deep integration with `SIGNAL NEWNYM` to rebuild exit circuits gracefully when detection limits are reached.
3. **Identity Rotation**: Native hot-swapping of `reqwest::Client` triggers a fresh TCP/TLS handshake with a new User-Agent and connection fingerprint.
4. **Soft-404 Suppression**: Employs baseline probing and body-fingerprint analysis to filter out synthetic success responses from adaptive WAFs.
