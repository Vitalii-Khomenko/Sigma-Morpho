# Sigma Morpho Full Complex Audit

Date: 2026-05-05

## Scope

This audit covered:

- repository documentation and architectural claims
- CLI safety controls and target authorization checks
- request execution, findings filtering, adaptive pacing, and recursion logic
- client rebuild, proxy, and Tor control paths
- local mechanical validation with Rust build and test commands
- controlled localhost runtime validation against `test_server.py`

## Executive Summary

The repository is mechanically healthy but behaviorally inconsistent with part of its own stealth-oriented documentation.

Verified positives:

- `cargo test` passes cleanly.
- `cargo check --lib` passes.
- `cargo build --release` passes.
- The remote-target safety gate is implemented and covered by tests.
- Findings filtering, recursion queue behavior, and profile-comparison paths have direct unit-test coverage.

Primary concerns:

1. Advisory-driven user-agent rotation and Tor circuit rebuilds are not produced by the live RSNN path. They are only emitted through `SafeActionSimulator`, which is disabled unless `--simulation-mode` is set.
2. `vhost` mode only rewrites the HTTP `Host` header. For HTTPS targets, the destination host and TLS SNI still remain tied to `base_url`, which can cause virtual-host discovery to miss valid hosts.
3. Setting `--rate` replaces adaptive sleep-based pacing with a semaphore rate limiter, so RSNN delay changes no longer control request spacing.
4. User-agent rotation can randomly choose the same profile, producing a nominal rotation event with no actual identity change.
5. `cargo fmt --check` currently fails, indicating style drift in a small set of files.

## Overall Assessment

Sigma Morpho is in good shape as a Rust codebase: the build passes, the tests pass, and the local validation workflow works. The main audit issues are product-correctness and operator-expectation gaps rather than outright breakage.

The strongest mismatch is between the public documentation and the actual runtime behavior of the evasion controls. The code can adapt delay in live runs, but the documented claims about live advisory-driven identity rotation and Tor `NEWNYM` are not true unless simulation is explicitly enabled.

## Contents

- `FINDINGS.md`: detailed findings, impact, and recommendations
- `VALIDATION.md`: commands run and observed results