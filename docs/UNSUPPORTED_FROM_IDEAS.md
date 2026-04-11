# Unsupported Items From `Ideas.md`

This file records the parts of [`Ideas.md`](../Ideas.md) that are intentionally not implemented in Sigma Morpho.

## Not implemented

- real `User-Agent rotation`
- real `RotateUserAgent` behavior that rebuilds the active client during a run
- real `Tor NEWNYM`
- real `RebuildTorCircuit`
- Tor control port access
- proxy or IP rotation
- live identity-changing rotator modules
- client rebuilding intended to evade attribution or bypass defensive controls
- “evasive maneuvers” that materially increase stealth against third-party protections

## What is implemented instead

- simulation-only advisory events for `RotateUserAgent` and `RebuildCircuit`
- fixed, non-rotating client profiles for comparative research
- local replay scenarios for studying rate limits, tarpits, and mixed protection signals
- adaptive pacing, telemetry, and reporting for authorized defensive research

## Reason

These omitted capabilities would move the project from defensive research toward practical stealth or bypass assistance. Sigma Morpho therefore keeps the analytical and educational parts of the ideas while excluding the real execution of identity or route rotation.
