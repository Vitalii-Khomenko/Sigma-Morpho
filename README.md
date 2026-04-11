# Sigma Morpho (RED Teaming & CTF Edition)

Sigma Morpho is an advanced, defensive, adaptive HTTP workload tester and directory buster written in Rust. Following the architectures outlined in `Ideas.md`, it leverages a lightweight **Recurrent Spiking Neural Network (RSNN)** to analyze response timings and HTTP status-code sequences. By processing these sequences natively, it dynamically adjusts its request pacing, seamlessly evading modern Web Application Firewalls (WAFs), Intrusion Prevention Systems (IPS), Tarpits, and tight Rate Limits.

This version is optimized for **RED Teaming** operations and advanced **CTFs** (like HackTheBox). It introduces stealth mechanisms including **real-time User-Agent Rotation** and **Tor Circuit Rebuilding (NEWNYM)** driven by the SNN's advisory signals, drastically reducing digital footprints and avoiding fingerprinting.

## 🚀 Key Implemented Features

- **Neuromorphic Engine (SNN)**:
  - Sequences of 429s, 403s, and 503s translate into "stress spikes" in the Spiking Neural Network.
  - LIF (Leaky Integrate-and-Fire) neurons simulate biological adaptation, meaning delays rise exponentially under stress and fall gradually when conditions improve (mimicking human surfing behavior).
- **Asynchronous Evasion Controller**:
  - Implements an **Actors architecture** separating heavy CPU-bound SNN calculations (`tokio::spawn_blocking`) from the thousands of async HTTP workers (`Reqwest + Tokio`).
  - Hot-swaps the HTTP connection pool (`reqwest::Client`) on the fly when User-Agents are rotated, effectively purging old tracking mechanisms without blocking worker threads.
- **Dynamic Identity Rotation**:
  - Automatically shifts User-Agent and Headers based on SNN evasion requirements (triggered by sustained 403 patterns).
- **Tor Circuit Integration**:
  - Direct integration over raw TCP with the **Tor Control Port**.
  - Sends the `SIGNAL NEWNYM` command to change the egress IP dynamically upon critical blockage detection, bypassing IP-based filtering.
- **Deep Recursion & Filtering**:
  - Automatically recurses into discovered directories with depth control (`--recursion-depth`).
  - Built-in soft 404 detection using randomized baseline probes.

## ⚙️ How to use for Testing (CTF & RED Teaming)

### Prerequisites
To unleash the complete evasion sequence, ensure **Tor** is running with the Control Port exposed. Set this in your `torrc`:
```text
ControlPort 9051
HashedControlPassword 16:YOUR_GENERATED_HASH # Use `tor --hash-password your_password` to get this
```

### 1. Simple Adaptive Scan (No Tor)
Great for standard endpoints with light Rate Limiting. The SNN will automatically back off to prevent blocking.
```bash
cargo run --release -- \
  --base-url http://target.lab \
  --wordlist wordlist.txt \
  --workers 30 \
  --authorized-target
```

### 2. Full Stealth Scan (Tor Proxy + Rotation + Circuit Rebuild)
Designed for WAF-protected endpoints.
```bash
cargo run --release -- \
  --base-url http://target.lab \
  --wordlist wordlist.txt \
  --workers 15 \
  --authorized-target \
  --rebuild-client-on-advisory \
  --tor-proxy "socks5h://127.0.0.1:9050" \
  --tor-control "127.0.0.1:9051" \
  --tor-password "your_password"
```
**What happens under the hood:**
1. If latency spikes or 429s appear, pacing increases smoothly.
2. If 403 blocks become consistent, the Rotator issues a new `reqwest::Client` with a vastly different User-Agent and connection headers.
3. If severe blocking persists continuously, the Tor rotater talks to port 9051, sends `NEWNYM`, waits 5 seconds for proxy propagation, and restarts request flow on the new IP address.

## 🛠️ What Can Still Be Improved
There are always routes for further stealth:
1. **Adding Real Device Fingerprinting**: Currently, User-Agent strings rotate between static client profiles. It could be expanded with Chromium's JA3 / TLS Fingerprinting (e.g., using `reqwest-impersonate`) to trick advanced CDNs (Cloudflare, Akamai).
2. **Distributed Nodes**: Controlling multiple Tor instances (or bot proxies) simultaneously instead of just rebuilding one Circuit.
3. **STDP Memory Persistence**: Currently, synaptic weights are localized to the program timeline. Saving the Synaptic state to physical storage for repeated runs on similar CDNs.

## Interactive Run Wrapper

If you want every run to archive itself automatically under `runs/`, use the interactive shell wrapper:

```bash
chmod +x scripts/run_sigma_session.sh
./scripts/run_sigma_session.sh
```

What the wrapper does automatically:
- Prompts for the most common launch parameters with short inline explanations.
- Creates a run directory in `runs/` using the current date, time, run label, and target host or IP.
- Saves `command.txt`, `rerun.sh`, `metadata.txt`, build logs, runtime stdout/stderr, timing, and `findings.txt`.
- Adds `--authorized-target` only after explicit confirmation for non-local hosts.

Typical local smoke-test flow:

```bash
/usr/bin/python test_server.py
./scripts/run_sigma_session.sh
```

The exact resolved Sigma Morpho command for each run is written to the generated `command.txt`, and a ready-to-run copy is saved as `rerun.sh` inside the same run directory.

## 📝 Troubleshooting & Compilation Notes
When building via `cargo build --release` on Windows:
- A C++ build toolset is required for `ring` (the cryptography backend inside `reqwest` + `rustls`).
- The necessary GCC can be easily installed via MSYS2 / WinLibs.
- For MSVC environments, you can ensure Visual Studio Build Tools are installed, or install WinLibs GCC with Winget using `winget install --id "BrechtSanders.WinLibs.POSIX.MSVCRT" --silent --accept-package-agreements --accept-source-agreements`
- Run `cargo check` post-install to ensure the environment is valid.
