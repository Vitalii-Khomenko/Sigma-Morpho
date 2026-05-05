# Validation Evidence

## Mechanical Validation

### `cargo test`

Result: pass

- `32` unit tests passed in `src/lib.rs`
- `1` integration test passed in `tests/public_api.rs`
- no failing doctests

### `cargo check --lib`

Result: pass

The library target compiled successfully.

### `cargo build --release`

Result: pass

The optimized release binary built successfully.

### `cargo fmt --check`

Result: fail

Formatting drift was reported in:

- `src/network/runtime.rs`
- `src/network/tor.rs`
- `src/neuro/mod.rs`

## Controlled Runtime Validation

### Local server

Command used:

```text
/home/warmond/AI-Research/Sigma-Morpho/.venv/bin/python test_server.py
```

Observed startup:

```text
Starting WAF Test Server on http://127.0.0.1:8080
```

The server behavior is deterministic:

- paths containing `api` or `public` return `200`
- paths containing `admin` or `hidden` return `403`
- request bursts above 5 requests/second return `429`
- everything else returns `404`

### Run A: advisory rebuild flag without simulation

Command used:

```text
cargo run --release -- --base-url http://127.0.0.1:8080 --wordlist dict/local-validation-control.txt --workers 10 --rebuild-client-on-advisory --findings-file /tmp/sigma-audit-nosim-findings.txt
```

Observed result summary:

- `Simulation mode: false`
- `Protective block signals (403/429/503): 4`
- `Final adaptive profile: defensive`
- `Client rebuilds: 0`
- `Simulated action events: 0`

Interpretation:

The tool adapted delay and profile, but no advisory-driven rebuild path activated.

### Run B: advisory rebuild flag with simulation enabled

Command used:

```text
cargo run --release -- --base-url http://127.0.0.1:8080 --wordlist dict/local-validation-control.txt --workers 10 --rebuild-client-on-advisory --simulation-mode --findings-file /tmp/sigma-audit-sim-findings.txt
```

Observed result summary:

- `Simulation mode: true`
- `Protective block signals (403/429/503): 6`
- `Final adaptive profile: defensive`
- `Client rebuilds: 2`
- `Simulated rotate advisories: 1`
- `Simulated circuit advisories: 1`

Observed runtime log excerpts:

```text
[SIMULATION] action=rotate-user-agent-advisory profile=defensive status=429 latency=0ms path=/public
[SIMULATION] action=rebuild-circuit-advisory profile=defensive status=429 latency=0ms path=/public
[ROTATOR] Rotated User-Agent to profile: mobile-safari
[CLIENT] rebuilt fixed-profile transport reason=simulated-rotate-advisory
[CLIENT] rebuilt fixed-profile transport reason=simulated-circuit-advisory
```

Interpretation:

The advisory rebuild path is reachable, but only when simulation is enabled.

## Files Reviewed Directly

- `README.md`
- `docs/TECHNICAL_SPEC.md`
- `src/cli.rs`
- `src/core/engine.rs`
- `src/core/findings.rs`
- `src/core/metrics.rs`
- `src/core/research.rs`
- `src/core/simulation.rs`
- `src/network/client.rs`
- `src/network/runtime.rs`
- `src/network/tor.rs`
- `src/network/profile.rs`
- `test_server.py`

## Audit Limits

This audit did not include:

- internet-facing target validation
- fuzzing or load testing beyond the provided local control server
- third-party dependency vulnerability scanning
- external proxy or Tor integration tests against a live Tor daemon