# Architecture

## Data flow

1. CLI validates the target and loads the path corpus.
2. The engine fans out jobs to async workers.
3. When recursion is enabled, directory-like hits can enqueue child jobs into the shared queue until the configured depth limit is reached.
4. Each worker executes a request and emits a `ResponseMetric`.
5. The neuro actor consumes metrics, updates the RSNN, and computes the next shared delay.
6. Workers observe delay changes through a watch channel before subsequent requests.
7. Workers also read the active HTTP client through a watch channel, which keeps the transport layer decoupled from worker lifetime.
8. Final summaries merge HTTP-level and neuro-level telemetry.

## Why actor isolation matters

The RSNN is stateful and mutation-heavy. Putting it behind a shared lock would create contention across workers and would mix CPU-bound work with async I/O. The actor model solves both:

- one owner of mutable neuro state
- clean telemetry pipeline
- stable worker behavior under load
- easier auditability for lab review

## Sequence analysis

Recent response windows are used to distinguish isolated errors from meaningful patterns:

- repeated `429` or `403` values suggest active protection
- sustained latency inflation suggests tarpit-like slowdown or server stress
- dense `404` streaks can indicate low-value path exploration and justify pacing down

This sequence layer gives the RSNN context without turning the tool into a bypass framework.

## Safety boundary

The architecture deliberately stops at adaptive throttling. Any feature that would change identity, route, or attribution is excluded.
