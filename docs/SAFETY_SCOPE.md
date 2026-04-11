# Safety Scope

Sigma Morpho is limited to authorized, attributable, defensive security research.

## Allowed purpose

- education
- cyber range exercises
- internal reliability testing
- controlled security validation with explicit authorization

## Prohibited purpose

- unauthorized access
- bypassing protective controls
- anonymity or attribution evasion
- route, identity, or user-agent churn to avoid detection
- use against third-party systems without permission

## Design decisions that enforce scope

- non-local targets require `--authorized-target`
- no rotator module
- no Tor control support
- no proxy/IP switching
- no stealth identity mutation features

## Operator responsibility

The operator remains responsible for authorization, rate selection, legal compliance, and evidence retention.
