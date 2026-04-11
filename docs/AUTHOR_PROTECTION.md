# Author Protection

This document is intended to help the project author defend the educational and research nature of the repository.

## Positioning

State clearly that the project is:

- a defensive research tool
- intended for authorized use only
- designed to slow down under protective signals
- intentionally missing identity-evasion capabilities

## Recommended repository protections

- Keep the license, legal notice, and safety scope documents in the repository root.
- Preserve a visible disclaimer in `README.md`.
- Require contributors to keep the defensive scope intact.
- Reject feature requests for proxy rotation, Tor control, or identity obfuscation.

## Recommended publication language

Use language such as:

“Sigma Morpho is an adaptive HTTP workload tester for authorized training and research environments. It is not a stealth, anonymity, or bypass framework.”

## Evidence of good-faith design

- authorization gate for non-local targets
- telemetry-focused architecture
- actor isolation for observability and correctness
- explicit documentation of excluded features

## Contributor policy

Suggested rule:

“Contributions that materially increase bypass, stealth, anonymity, or unauthorized-use potential will not be accepted.”

## Record-keeping

For academic or HTB review, retain:

- course or lab authorization context
- run parameters
- generated summaries
- any changes made to thresholds or worker counts
