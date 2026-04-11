# Safe Research Modes

This document describes the safe research extensions added to Sigma Morpho instead of real stealth or bypass features.

## 1. Simulation-only advisory mode

When `--simulation-mode` is enabled, the neuro pipeline may emit advisory events inspired by the research ideas:

- `RotateUserAgent`
- `RebuildCircuit`

These are simulated only. They are logged, counted, and included in summaries, but they do not modify the active HTTP client, route, identity, or attribution.

## 2. Fixed client profiles

Sigma Morpho supports fixed per-run client profiles:

- `research-default`
- `browser-desktop`
- `mobile-safari`
- `api-diagnostic`

Each profile changes only static request metadata and conservative timing defaults for the whole run. No mid-run rotation occurs.

## 3. Local replay scenarios

Scenario mode makes it possible to study protective behavior without touching a live target:

- `healthy`
- `rate-limit`
- `tarpit`
- `mixed-defense`

These scenarios feed synthetic `ResponseMetric` sequences into the RSNN and produce comparable summaries.

## 4. Comparative reporting

With `--compare-profiles`, the same scenario is replayed across all fixed profiles. This supports lab research such as:

- how quickly each profile reaches `cautious` or `defensive`
- how much adaptive delay accumulates
- whether simulated advisory events appear

## Why this exists

The goal is to preserve research value from the ideas file while keeping the implementation aligned with authorized, attributable, and defensive experimentation.
