# Sigma Morpho vs Gobuster

## Scope

This document compares Sigma Morpho with Gobuster for HTTP path enumeration and directory discovery.

It focuses on:

- scanning model
- request scheduling
- speed characteristics
- data-flow behavior
- impact on the target and the network stream

The comparison is intentionally limited to authorized, lab-style use.

## Version context

- Sigma Morpho: local development version in this repository as of April 11, 2026
- Gobuster: local binary version `3.6`
- Gobuster upstream reference: the official repository listed `v3.8.2` as the latest release on September 4, 2025

Because the local Gobuster binary is older than the latest upstream release, this document distinguishes between:

- measured local behavior
- architecture and feature behavior described by the official project

## Executive summary

Gobuster is a fast, straightforward brute-force enumerator with a mostly static execution model: you choose the mode, thread count, delay, filters, and wordlist, then it runs that plan.

Sigma Morpho is a feedback-driven HTTP workload engine. It still starts from a wordlist, but it can adapt pacing from live response patterns, maintain richer response telemetry, and dynamically expand the queue when directory-like responses justify recursion.

In short:

- Gobuster is simpler and more predictable for flat enumeration.
- Sigma Morpho is more stateful and observable for controlled experiments.
- Gobuster usually has less control-plane overhead.
- Sigma Morpho can make better long-run decisions when the target starts to signal strain, blocking, or tarpitting.

## Core operating model

### Gobuster

Gobuster uses a mode-based, concurrent worker model. In `dir` mode, it sends requests derived from the provided wordlist and reports responses that pass the operator's filters. Concurrency is primarily controlled by thread count and optional per-thread delay.

Operationally, this means:

- the request set is mostly known in advance
- pacing is operator-defined, not feedback-driven
- output is centered on hits, status codes, and lengths
- control flow is simple and low-overhead

### Sigma Morpho

Sigma Morpho uses an async worker pool plus a separate neuro actor. Workers emit normalized `ResponseMetric` objects into a telemetry stream, and the neuro actor can adjust a shared delay based on latency bursts, protective blocks, and dense failure patterns.

Operationally, this means:

- the request stream can change during the run
- pacing can be adapted from observed server behavior
- output includes HTTP results, timing telemetry, soft-404 handling, and adaptive-profile transitions
- recursion can enlarge the work queue when directory-like hits are discovered

## Pipeline and data flow

### Gobuster data flow

For a basic directory scan, Gobuster is close to:

`wordlist -> worker threads -> HTTP requests -> response filters -> terminal/output file`

Properties:

- very direct pipeline
- little state shared across requests
- minimal feedback loop beyond retries and fixed delay
- stable and easy-to-predict throughput profile

### Sigma Morpho data flow

Sigma Morpho is closer to:

`wordlist -> async job queue -> workers -> ResponseMetric stream -> neuro actor -> shared adaptive delay -> workers`

When recursion is enabled, there is an extra loop:

`directory-like hit -> child path generation -> queue expansion`

Properties:

- richer control plane
- more shared state than Gobuster
- adaptive pacing under stress signals
- dynamic growth in total work volume

## Request cardinality and queue growth

This is one of the biggest practical differences.

### Gobuster

For a simple `dir` run, total request count is generally close to:

`wordlist entries x extensions/pattern expansions`

The operator usually knows the upper bound before the scan starts.

### Sigma Morpho

With recursion disabled, Sigma Morpho behaves similarly to a flat enumerator.

With recursion enabled, total request count becomes:

`seed paths -> discovered directory-like hits -> child path expansion -> optional deeper levels`

This makes Sigma Morpho more exploratory, but also less predictable in total request volume. In other words, Sigma Morpho can discover more structure per run, but it can also increase bandwidth, queue depth, and response-processing load if recursion is used aggressively.

## Speed comparison

### Measured local benchmark on April 11, 2026

Target: `http://10.129.70.246`

Wordlist: 6 entries

- `images`
- `fonts`
- `js`
- `themes`
- `cdn-cgi`
- `server-status`

Concurrency:

- Sigma Morpho: `--workers 12`
- Gobuster: `-t 12`

#### Flat scan, same 6-request workload

| Tool | Mode | Requests | Wall-clock time | Effective throughput |
| --- | --- | ---: | ---: | ---: |
| Sigma Morpho | flat (`--recursion-depth 0`) | 6 | 0.605849 s | 9.90 req/s |
| Gobuster | `dir` | 6 | 0.673254 s | 8.91 req/s |

Interpretation:

- In this very small local benchmark, Sigma Morpho was about 11% faster in wall-clock throughput.
- The difference is small enough that it should not be treated as a universal claim.
- On such a tiny sample, startup effects, scheduler timing, socket reuse, and host variability matter a lot.

#### Sigma Morpho recursive smoke run

This is not a like-for-like Gobuster comparison, but it shows what Sigma Morpho's dynamic queue changes in practice.

| Tool | Mode | Requests | Wall-clock time | Effective throughput |
| --- | --- | ---: | ---: | ---: |
| Sigma Morpho | recursive (`--recursion-depth 1`) | 42 | 3.13 s | 13.42 req/s |

What changed:

- 6 seed paths became 42 scheduled jobs
- directory-like hits automatically generated child paths
- the queue stayed warm instead of ending after the initial 6 requests

This is the main reason Sigma Morpho and Gobuster are not directly interchangeable: Sigma Morpho can transform a flat workload into a graph-like exploration process.

## Impact on the data stream

### Request stream shape

Gobuster tends to generate a flatter and more uniform request stream. If the operator chooses 12 threads and no delay, the stream stays close to that pattern until the job completes.

Sigma Morpho can produce three different request-stream shapes:

- flat and fast when the target looks healthy
- intentionally slower when latency or protective behavior rises
- broader in total volume when recursion discovers child paths

### Backpressure and adaptive control

Gobuster's backpressure is mostly manual:

- lower thread count
- add `--delay`
- increase timeout
- enable retries

Sigma Morpho adds a feedback loop:

- workers emit response metrics
- the neuro actor observes timing and block patterns
- a shared delay is pushed back to workers

This adds overhead, but it also means Sigma Morpho can react to stress signals without requiring an operator to stop and retune the run immediately.

### Telemetry density

Gobuster mainly reports actionable hits and can include status and length information. This keeps output simple.

Sigma Morpho tracks more runtime state:

- per-response latency
- body length and body fingerprint
- soft-404 baseline comparison
- adaptive profile transitions
- simulated advisory events in safe research mode
- discovered job count during recursion

The result is heavier internal processing, but much better observability for analysis and reporting.

## Impact on target-side load

### Gobuster

Pros:

- predictable load envelope
- easy to reason about total request count
- low coordination overhead

Trade-off:

- if the target starts rate-limiting, tarpitting, or returning noisy failures, the tool does not natively infer a new strategy from the response sequence

### Sigma Morpho

Pros:

- can detect stress or protection patterns from sequences
- can slow down instead of keeping a fixed pressure profile
- can continue exploration below discovered directories

Trade-offs:

- more logic per response
- less predictable total request count when recursion is on
- queue growth can increase downstream bandwidth and body-processing volume

## Practical interpretation

If the goal is:

- fast, simple, operator-controlled flat enumeration: Gobuster is often the cleaner choice
- adaptive, instrumented, lab-style analysis with controlled feedback: Sigma Morpho is the stronger research platform
- recursive exploration with runtime observability: Sigma Morpho provides capabilities that a plain flat Gobuster run does not provide by default

## What the benchmark here does and does not prove

This repository now has one useful local comparison point:

- Sigma Morpho flat: slightly faster than the local Gobuster 3.6 binary on a 6-request micro-benchmark

This does **not** prove that Sigma Morpho is always faster than Gobuster. It only shows that, on this host, target, and tiny input, Sigma Morpho's current implementation is competitive even with its richer telemetry path.

The more meaningful distinction is not micro-benchmark speed. It is control philosophy:

- Gobuster optimizes for simple concurrent execution
- Sigma Morpho optimizes for adaptive execution plus analysis

## Local artifacts used for this document

- Sigma Morpho flat benchmark:
  `runs/2026-04-11_12-30-19_sigma-vs-gobuster_10.129.70.246/sigma_flat.log`
- Sigma Morpho flat timing:
  `runs/2026-04-11_12-30-19_sigma-vs-gobuster_10.129.70.246/sigma_flat_time.txt`
- Gobuster output:
  `runs/2026-04-11_12-30-19_sigma-vs-gobuster_10.129.70.246/gobuster_output.txt`
- Gobuster timing:
  `runs/2026-04-11_12-30-19_sigma-vs-gobuster_10.129.70.246/gobuster_time.txt`
- Sigma Morpho recursive smoke run:
  `runs/2026-04-11_12-26-24_quick-recursive_10.129.70.246/console.log`

## External source

- Gobuster official repository and README:
  https://github.com/OJ/gobuster
