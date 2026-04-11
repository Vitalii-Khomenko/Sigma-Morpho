# Core Architecture

## Data flow

1. CLI validates the target, reads proxies (if any), and loads the path corpus.
2. The engine (`FuzzOrchestrator`) fans out jobs to async `tokio` workers.
3. Each worker takes a URL, proxies through Tor/Rotators, and executes an HTTP request.
4. Each worker emits a `ResponseMetric` into an `mpsc` telemetry channel.
5. A separated CPU-bound **Neuromorphic Actor** consumes metrics, calculates stress (LIF + STDP), and computes the next shared delay.
6. A decoupled **Evasion Controller** watches neuro outputs. If the network flags a `SimulatedRotateAdvisory` or `SimulatedCircuitAdvisory`, the Engine takes action without suspending workers:
   - For UA Rotation: Hot-swaps the underlying `reqwest::Client`.
   - For Circuit: Signals `NEWNYM` to the Tor Control port.

## Extensibility

The tool uses native `Rustls` and HTTP/2 configs to simulate TLS Fingerprinting and bypass standard CDN checks. Distributed Nodes parsing (`--proxies-file`) allows feeding 100+ proxy servers and cycling them automatically under fire. SNN's `RsnnState` is fully serializable into `json`, enabling persistent threat intelligence retention between assessments.
