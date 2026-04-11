# Core Architecture

## Data flow

1. CLI validates the target, reads proxies, and loads the path corpus.
2. The engine (`FuzzOrchestrator`) initializes a **High-Performance Lazy Queue**.
3. **Lazy Recursion**: Instead of immediate fan-out of sub-directories, the queue yields search paths on-the-fly (`O(1)`), preventing memory bloat.
4. Each worker takes a URL, and a **Multiplexed Proxy Pool** selects an optimal `reqwest::Client` (Tor circuit or proxy node).
5. Each worker emits a `ResponseMetric` into an `mpsc` telemetry channel.
6. A separated CPU-bound **Neuromorphic Actor** (in `spawn_blocking`) consumes metrics, calculates stress (LIF + STDP), and updates shared strategy.
7. If a threshold is met, the **Evasion Controller** hot-swaps clients or signals Tor to rebuild circuits without stopping the worker pool.
8. Recursive findings are tracked via a **Concurrent Lock-Free DashSet** to ensure scalability across millions of nodes.

## Extensibility

The tool uses native `Rustls` and HTTP/2 configs to simulate TLS Fingerprinting and bypass standard CDN checks. Distributed Nodes parsing (`--proxies-file`) allows feeding 100+ proxy servers and cycling them automatically under fire. SNN's `RsnnState` is fully serializable into `json`, enabling persistent threat intelligence retention between assessments.
