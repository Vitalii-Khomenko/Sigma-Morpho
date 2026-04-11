# Technical Specification

## Overview

Sigma Morpho is a pure Rust, asynchronous HTTP workload tester for authorized cyber range and lab environments. Its purpose is to study how a recurrent spiking neural network can adapt pacing from temporal response patterns while remaining within a defensive, non-bypass-oriented operating model.

## Objectives

- Evaluate whether RSNN-style temporal memory can detect protective response patterns sooner than fixed throttling rules.
- Keep workload generation observable and bounded.
- Produce lab-friendly evidence for HTB Academy style assessment and reporting.
- Avoid offensive identity-rotation or bypass features.

## System modules

- `src/cli.rs`
  Parses arguments, validates target safety, loads wordlists, and selects live or scenario mode.
- `src/network/client.rs`
  Performs async HTTP requests and normalizes response metrics.
- `src/network/profile.rs`
  Defines fixed client profiles and their static header/timeout presets.
- `src/core/engine.rs`
  Coordinates workers and the isolated neuro actor.
- `src/core/metrics.rs`
  Aggregates HTTP and neuro telemetry into a reportable summary.
- `src/core/profile.rs`
  Defines safe adaptive operating profiles.
- `src/core/simulation.rs`
  Produces simulation-only advisory events for research traces.
- `src/core/research.rs`
  Runs local replay scenarios and comparative profile reports.
- `src/neuro/neuron.rs`
  LIF neuron implementation with refractory state and spike history.
- `src/neuro/synapse.rs`
  Bounded synapse model with STDP-style update support.
- `src/neuro/encoder.rs`
  Converts response metrics plus recent context windows into spike currents.
- `src/neuro/rsnn.rs`
  Runs the recurrent hidden reservoir, output neurons, and online adaptation logic.

## Research model

### Input features

The current encoder keeps the original three-input research shape from the idea file:

1. Latency anomaly
2. Protective block indicator (`403`, `429`, `503`, transport failure)
3. Dense `404` activity

These are modulated by recent-window pattern analysis:

- latency burst
- block burst
- dense 404 burst
- stable success recovery window

### Neuron model

Each neuron uses a Leaky Integrate-and-Fire model:

- membrane leak
- spike threshold
- refractory period
- bounded spike history for temporal learning

### RSNN topology

- Input layer: encoded currents from response telemetry
- Hidden layer: recurrent reservoir
- Output layer: throttle and recovery neurons

### Online adaptation

The network updates bounded synaptic weights through STDP-inspired reinforcement and decay. The result is not meant to be biologically exact; it is a practical temporal-learning approximation for controlled experiments.

## Safe operating profiles

- `baseline`
  Normal lab workload, minimal delay
- `cautious`
  Activated when latency or 404 sequence anomalies accumulate
- `defensive`
  Activated when repeated protective blocks or strong stress signals appear

These profiles only affect pacing. They do not rotate identity, rebuild network routes, or alter attribution.

## Research extensions

Sigma Morpho now supports four safe research extensions derived from the ideas file:

1. simulation-only advisory actions
2. fixed per-run client profiles
3. local replay scenarios for defensive behavior
4. comparative scenario reports across profiles

## Concurrency model

- Workers generate HTTP requests concurrently.
- Workers send `ResponseMetric` events over `tokio::sync::mpsc`.
- A dedicated neuro actor runs in `spawn_blocking` so CPU-bound inference does not stall async I/O.
- The actor broadcasts global delay over `tokio::sync::watch`.

This preserves throughput while avoiding lock contention on shared RSNN state.

## Output summary

The final report includes:

- request totals and HTTP class counts
- `404` volume
- protective block signals
- latency distribution
- discovered `200` hits
- final adaptive delay
- final adaptive profile
- neuro decision and burst counters

## Non-goals

The following are intentionally out of scope:

- User-Agent rotation
- IP/proxy rotation
- Tor control port integration
- bypass-focused path mutation
- stealth automation
