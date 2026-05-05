# Detailed Findings

## Methodology

This audit combined:

- static review of the CLI, engine, findings, runtime, and network layers
- build and test validation with Cargo
- controlled localhost runs against `test_server.py`
- comparison of implemented behavior against `README.md` and `docs/TECHNICAL_SPEC.md`

## Finding 1: Live advisory rebuilds are simulation-gated

Severity: High

### What was verified

The documentation claims live SNN-driven user-agent rotation and Tor circuit rebuilds:

- `README.md` states that the tool introduces real-time user-agent rotation and Tor `NEWNYM` rebuilding driven by SNN advisory signals.
- `docs/TECHNICAL_SPEC.md` states that the RSNN sends Tor control signals upon extreme stress and that identity rotation is part of the advanced evasion path.

The implementation path does not match those claims.

- In `src/core/engine.rs`, client rebuild commands are emitted only inside the loop over `simulator.observe(...)`.
- The simulator is constructed as `SafeActionSimulator::new(simulation_mode)`.
- When `simulation_mode` is `false`, the simulator emits no actions, so `--rebuild-client-on-advisory` has no live advisory source.

### Runtime evidence

Controlled localhost runs used the same blocked workload and only changed `--simulation-mode`.

Without simulation:

- `Client rebuilds: 0`
- `Simulated action events: 0`
- `Simulated rotate advisories: 0`
- `Simulated circuit advisories: 0`

With simulation enabled:

- `Client rebuilds: 2`
- `Simulated action events: 2`
- one rotate advisory and one circuit advisory were logged

This confirms that advisory-triggered rebuild behavior currently depends on simulation, not on the live RSNN alone.

### Impact

- Operators can believe that live stealth reactions are active when they are not.
- Tor control integration can appear available in live mode while remaining dormant.
- Results and OPSEC expectations differ from the public documentation.

### Recommendation

- Either move advisory generation into the live RSNN decision path or explicitly document that rebuild advisories are currently simulation-backed.
- If the current behavior is intentional, rename the CLI flag or add a hard warning when `--rebuild-client-on-advisory` is used without `--simulation-mode`.

## Finding 2: HTTPS vhost mode does not change SNI or destination host

Severity: Medium

### What was verified

In `src/network/client.rs`, `execute_vhost` sends a request to `self.base_url.clone()` and only overrides the HTTP `Host` header.

That means:

- DNS resolution still targets the `base_url` host
- for HTTPS, TLS SNI also remains bound to the `base_url` host

This is sufficient for some plain HTTP virtual-host checks, but it is not enough for many HTTPS virtual-host discovery cases.

### Impact

- HTTPS subdomain or vhost enumeration can miss valid virtual hosts that depend on correct SNI.
- Scan results can under-report reachable hosts while appearing to support a `vhost` mode.

### Recommendation

- Document `vhost` mode as HTTP-header-only today.
- If HTTPS support is intended, add a transport path that can vary destination host resolution and TLS SNI safely.

## Finding 3: `--rate` bypasses adaptive delay control

Severity: Medium

### What was verified

In `src/core/engine.rs`, `ScanPlan::wait_turn` behaves in two mutually exclusive modes:

- with no configured rate, it sleeps for the adaptive `delay_ms`
- with `rate_per_second` configured, it acquires a semaphore permit instead and does not sleep on the adaptive delay

The neuro actor still computes updated delays, but those delay changes no longer directly control request spacing when `--rate` is active.

### Impact

- Users can enable `--rate` and still assume adaptive backoff is governing live pacing.
- The tool's key adaptive behavior becomes partially bypassed by a CLI combination that is easy to reach.

### Recommendation

- Define and document precedence explicitly.
- Prefer combining the rate ceiling with adaptive delay rather than replacing adaptive delay completely.

## Finding 4: User-agent rotation can be a no-op

Severity: Low

### What was verified

In `src/network/runtime.rs`, `SafeClientFactory::build_shared(true)` chooses the next profile from `ClientProfile::ALL` without excluding the current profile.

As a result, a rotate event can randomly pick the same profile and produce no material identity change.

### Impact

- A logged rotation event does not guarantee that the user agent actually changed.
- The probability of a no-op is non-trivial because there are only four profiles.

### Recommendation

- Choose from the set of profiles excluding the current one.
- Log both old and new profiles for auditability.

## Finding 5: Formatting drift is present

Severity: Low

### What was verified

`cargo fmt --check` fails and reports formatting diffs in:

- `src/network/runtime.rs`
- `src/network/tor.rs`
- `src/neuro/mod.rs`

### Impact

- Low direct runtime risk.
- Increases noise in future reviews and makes style compliance less predictable.

### Recommendation

- Run `cargo fmt` and keep it in CI if the project wants a stable formatting baseline.

## Positive Controls Verified

These areas were checked and did not produce audit findings:

- `src/cli.rs` correctly refuses non-local targets unless `--authorized-target` is supplied.
- Wordlist loading removes blank lines, comments, and duplicate normalized paths.
- Findings filtering supports status, body-size, word-count, and soft-404 suppression controls.
- The recursion queue enforces depth limits and prevents duplicate directory expansion.