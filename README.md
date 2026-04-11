# Sigma Morpho

Sigma Morpho is a defensive, adaptive HTTP workload tester written in Rust. It uses a lightweight recurrent spiking neural network (RSNN) to analyze response timing and status-code sequences, then adjusts request pacing to keep experiments controlled, observable, and suitable for authorized lab environments such as HTB Academy.

The project intentionally stays on the safe side of security research. It does not implement real identity rotation, proxy churn, Tor control, or bypass-oriented evasion. Instead, it now includes safe research substitutes for those ideas:

- simulation-only advisory mode for rotation/circuit actions
- fixed per-run client profiles
- local replay scenarios for protection testing
- comparative reports across profiles in scenario mode

## What is implemented

- Async HTTP workload engine with Tokio + Reqwest
- Worker pool with actor-style neuro core (`mpsc` in, `watch` out)
- LIF neurons with spike history
- RSNN hidden reservoir with recurrent state
- STDP-inspired synaptic updates for online adaptation
- Sequence-aware encoding for latency bursts, protective block bursts, and dense 404 streaks
- Safe adaptive profiles: `baseline`, `cautious`, `defensive`
- Simulation-only advisory actions for `RotateUserAgent` and `RebuildCircuit`
- Fixed client profiles:
  `research-default`, `browser-desktop`, `mobile-safari`, `api-diagnostic`
- Local scenario runner:
  `healthy`, `rate-limit`, `tarpit`, `mixed-defense`
- Unit and integration tests
- Separate technical, safety, and legal documentation under [`docs/`](./docs/)

## Safety scope

Use this tool only against systems you own or are explicitly authorized to test. For non-local targets, Sigma Morpho requires `--authorized-target`.

Excluded by design:

- real User-Agent rotation during execution
- proxy/IP rotation
- Tor control port integration and `NEWNYM`
- authentication bypass logic
- path mutation intended to defeat defensive controls

## Quick start

Live mode:

```bash
cargo run -- \
  --base-url http://127.0.0.1:8000 \
  --wordlist wordlist.txt \
  --workers 16 \
  --rounds 2 \
  --client-profile browser-desktop
```

Authorized remote lab:

```bash
cargo run -- \
  --base-url https://target.lab \
  --wordlist wordlist.txt \
  --workers 8 \
  --rounds 1 \
  --client-profile research-default \
  --simulation-mode \
  --authorized-target
```

Scenario mode with advisory simulation:

```bash
cargo run -- \
  --scenario mixed-defense \
  --simulation-mode \
  --compare-profiles
```

## CLI highlights

- `--client-profile <profile>`
  Chooses a fixed header and timeout profile for the whole run.
- `--simulation-mode`
  Enables simulation-only advisory events for actions inspired by the research ideas file.
- `--scenario <scenario>`
  Runs a local replay harness without live network requests.
- `--compare-profiles`
  In scenario mode, executes the same replay across all fixed client profiles.

## Documentation

- [`docs/TECHNICAL_SPEC.md`](./docs/TECHNICAL_SPEC.md)
- [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)
- [`docs/IDEA_TRACEABILITY.md`](./docs/IDEA_TRACEABILITY.md)
- [`docs/SAFE_RESEARCH_MODES.md`](./docs/SAFE_RESEARCH_MODES.md)
- [`docs/HTB_ACADEMY_GUIDE.md`](./docs/HTB_ACADEMY_GUIDE.md)
- [`docs/SAFETY_SCOPE.md`](./docs/SAFETY_SCOPE.md)
- [`docs/AUTHOR_PROTECTION.md`](./docs/AUTHOR_PROTECTION.md)
- [`docs/AUTHORIZATION_TEMPLATE.md`](./docs/AUTHORIZATION_TEMPLATE.md)
- [`docs/LEGAL_NOTICE.md`](./docs/LEGAL_NOTICE.md)
- [`docs/UNSUPPORTED_FROM_IDEAS.md`](./docs/UNSUPPORTED_FROM_IDEAS.md)

## Verification

```bash
cargo fmt
cargo test
```
