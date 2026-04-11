use std::collections::VecDeque;

use crate::core::metrics::ResponseMetric;
use crate::core::profile::AdaptiveProfile;
use rand::Rng;
#[cfg(test)]
use rand::SeedableRng;

use super::encoder::{analyze_patterns, encode_metric, EncoderConfig, PatternSummary};
use super::neuron::LifNeuron;
use super::synapse::Synapse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuroAction {
    IncreaseThrottle,
    DecreaseThrottle,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsnnDecision {
    pub next_delay_ms: u64,
    pub action: NeuroAction,
    pub profile: AdaptiveProfile,
    pub patterns: PatternSummary,
}

#[derive(Clone, Debug)]
pub struct RsnnConfig {
    pub hidden_size: usize,
    pub latency_threshold_ms: u64,
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
    pub learning_rate: f32,
    pub history_window: usize,
}

impl Default for RsnnConfig {
    fn default() -> Self {
        Self {
            hidden_size: 20,
            latency_threshold_ms: 800,
            min_delay_ms: 25,
            max_delay_ms: 5000,
            learning_rate: 0.08,
            history_window: 12,
        }
    }
}

pub struct Rsnn {
    encoder_cfg: EncoderConfig,
    hidden_neurons: Vec<LifNeuron>,
    input_weights: Vec<Vec<Synapse>>,      // [input_idx][hidden_idx]
    recurrent_weights: Vec<Vec<f32>>,      // [hidden_idx_from][hidden_idx_to]
    output_throttle_weights: Vec<Synapse>, // [hidden_idx]
    output_recover_weights: Vec<Synapse>,  // [hidden_idx]
    last_spikes: Vec<bool>,
    throttle_neuron: LifNeuron,
    recover_neuron: LifNeuron,
    learning_rate: f32,
    min_delay_ms: u64,
    max_delay_ms: u64,
    current_profile: AdaptiveProfile,
    global_tick: u64,
    history_window: usize,
    recent_statuses: VecDeque<u16>,
    recent_latencies: VecDeque<u64>,
    recent_errors: VecDeque<bool>,
    input_trace_ticks: [Option<u64>; 3],
}

impl Rsnn {
    pub fn new(cfg: RsnnConfig) -> Self {
        Self::new_with_rng(cfg, rand::thread_rng())
    }

    fn new_with_rng<R: Rng>(cfg: RsnnConfig, mut rng: R) -> Self {
        let hidden_size = cfg.hidden_size.max(1);

        let hidden_neurons = (0..hidden_size).map(|_| LifNeuron::default()).collect();

        let input_weights = (0..3)
            .map(|_| {
                (0..hidden_size)
                    .map(|_| Synapse::new(rng.gen_range(0.1..0.8), 0.05, 2.0))
                    .collect()
            })
            .collect();

        let recurrent_weights = (0..hidden_size)
            .map(|from| {
                (0..hidden_size)
                    .map(|to| {
                        if from == to {
                            0.0
                        } else {
                            rng.gen_range(-0.35..0.35)
                        }
                    })
                    .collect()
            })
            .collect();

        let output_throttle_weights = (0..hidden_size)
            .map(|_| Synapse::new(rng.gen_range(0.2..0.9), 0.05, 2.5))
            .collect();
        let output_recover_weights = (0..hidden_size)
            .map(|_| Synapse::new(rng.gen_range(0.1..0.7), 0.05, 2.5))
            .collect();

        Self {
            encoder_cfg: EncoderConfig {
                latency_threshold_ms: cfg.latency_threshold_ms,
            },
            hidden_neurons,
            input_weights,
            recurrent_weights,
            output_throttle_weights,
            output_recover_weights,
            last_spikes: vec![false; hidden_size],
            throttle_neuron: LifNeuron::new(1.2, 0.9, 1),
            recover_neuron: LifNeuron::new(1.0, 0.92, 1),
            learning_rate: cfg.learning_rate,
            min_delay_ms: cfg.min_delay_ms,
            max_delay_ms: cfg.max_delay_ms,
            current_profile: AdaptiveProfile::Baseline,
            global_tick: 0,
            history_window: cfg.history_window.max(4),
            recent_statuses: VecDeque::with_capacity(cfg.history_window.max(4)),
            recent_latencies: VecDeque::with_capacity(cfg.history_window.max(4)),
            recent_errors: VecDeque::with_capacity(cfg.history_window.max(4)),
            input_trace_ticks: [None, None, None],
        }
    }

    #[cfg(test)]
    fn new_seeded(cfg: RsnnConfig, seed: u64) -> Self {
        Self::new_with_rng(cfg, rand::rngs::StdRng::seed_from_u64(seed))
    }

    pub fn process_metric(
        &mut self,
        metric: &ResponseMetric,
        current_delay_ms: u64,
    ) -> RsnnDecision {
        self.global_tick += 1;
        self.push_history(metric);

        let patterns = analyze_patterns(
            &self.recent_statuses,
            &self.recent_latencies,
            &self.recent_errors,
            &self.encoder_cfg,
        );
        let inputs = encode_metric(metric, &self.encoder_cfg, patterns);
        for (idx, input) in inputs.iter().enumerate() {
            if *input > 0.0 {
                self.input_trace_ticks[idx] = Some(self.global_tick);
            }
        }

        let hidden_size = self.hidden_neurons.len();
        let mut current_spikes = vec![false; hidden_size];

        for i in 0..hidden_size {
            let mut current = 0.0;

            for (input_idx, input) in inputs.iter().enumerate() {
                current += *input * self.input_weights[input_idx][i].weight;
            }

            for (src_idx, was_spiking) in self.last_spikes.iter().enumerate() {
                if *was_spiking {
                    current += self.recurrent_weights[src_idx][i];
                }
            }

            current_spikes[i] = self.hidden_neurons[i].step(current, self.global_tick);
        }

        self.last_spikes.clone_from(&current_spikes);

        let mut throttle_current = 0.0;
        let mut recover_current = 0.0;

        for (idx, spiked) in current_spikes.iter().enumerate() {
            if *spiked {
                throttle_current += self.output_throttle_weights[idx].weight;
                recover_current += self.output_recover_weights[idx].weight;
            }
        }

        match self.current_profile {
            AdaptiveProfile::Baseline => {}
            AdaptiveProfile::Cautious => {
                throttle_current += 0.15;
                recover_current *= 0.9;
            }
            AdaptiveProfile::Defensive => {
                throttle_current += 0.3;
                recover_current *= 0.6;
            }
        }

        if metric.transport_error || matches!(metric.status, 403 | 429 | 503) {
            throttle_current += 0.85;
            recover_current *= 0.4;
        }

        if (200..=299).contains(&metric.status) {
            recover_current += if patterns.stable_success { 0.45 } else { 0.2 };
        }

        let throttle_spike = self
            .throttle_neuron
            .step(throttle_current, self.global_tick);
        let recover_spike = self.recover_neuron.step(recover_current, self.global_tick);

        self.apply_stdp(
            metric,
            &inputs,
            &current_spikes,
            throttle_spike,
            recover_spike,
        );

        let next_profile = self.select_profile(patterns, throttle_spike, recover_spike);

        let mut next_delay = current_delay_ms;
        let action = if next_profile == AdaptiveProfile::Defensive
            || (throttle_spike && patterns.block_burst)
        {
            next_delay = (current_delay_ms + 320).min(self.max_delay_ms);
            classify_delay_action(current_delay_ms, next_delay)
        } else if next_profile == AdaptiveProfile::Cautious
            || throttle_spike
            || patterns.latency_burst
            || patterns.not_found_burst
        {
            next_delay = (current_delay_ms + 140).min(self.max_delay_ms);
            classify_delay_action(current_delay_ms, next_delay)
        } else if recover_spike || patterns.stable_success {
            let step = if self.current_profile == AdaptiveProfile::Defensive {
                70
            } else {
                110
            };
            next_delay = current_delay_ms.saturating_sub(step).max(self.min_delay_ms);
            classify_delay_action(current_delay_ms, next_delay)
        } else if current_delay_ms > self.min_delay_ms {
            next_delay = current_delay_ms.saturating_sub(5).max(self.min_delay_ms);
            classify_delay_action(current_delay_ms, next_delay)
        } else {
            NeuroAction::Hold
        };

        self.current_profile = next_profile;

        RsnnDecision {
            next_delay_ms: next_delay,
            action,
            profile: next_profile,
            patterns,
        }
    }

    fn push_history(&mut self, metric: &ResponseMetric) {
        self.recent_statuses.push_back(metric.status);
        self.recent_latencies.push_back(metric.latency_ms);
        self.recent_errors.push_back(metric.transport_error);

        while self.recent_statuses.len() > self.history_window {
            self.recent_statuses.pop_front();
        }
        while self.recent_latencies.len() > self.history_window {
            self.recent_latencies.pop_front();
        }
        while self.recent_errors.len() > self.history_window {
            self.recent_errors.pop_front();
        }
    }

    fn select_profile(
        &self,
        patterns: PatternSummary,
        throttle_spike: bool,
        recover_spike: bool,
    ) -> AdaptiveProfile {
        if patterns.block_burst
            || (throttle_spike && self.current_profile == AdaptiveProfile::Cautious)
        {
            AdaptiveProfile::Defensive
        } else if patterns.latency_burst || patterns.not_found_burst || throttle_spike {
            AdaptiveProfile::Cautious
        } else if patterns.stable_success && recover_spike {
            match self.current_profile {
                AdaptiveProfile::Defensive => AdaptiveProfile::Cautious,
                AdaptiveProfile::Cautious => AdaptiveProfile::Baseline,
                AdaptiveProfile::Baseline => AdaptiveProfile::Baseline,
            }
        } else {
            self.current_profile
        }
    }

    fn apply_stdp(
        &mut self,
        metric: &ResponseMetric,
        inputs: &[f32; 3],
        hidden_spikes: &[bool],
        throttle_spike: bool,
        recover_spike: bool,
    ) {
        let punitive = metric.transport_error || matches!(metric.status, 403 | 429 | 503);
        let rewarding = (200..=299).contains(&metric.status);

        if !punitive && !rewarding {
            return;
        }

        for (idx, spiked) in hidden_spikes.iter().enumerate() {
            if !spiked {
                continue;
            }

            for (input_idx, trace_tick) in self.input_trace_ticks.iter().enumerate() {
                if let Some(pre_tick) = trace_tick {
                    let delta_t = self.global_tick as i64 - *pre_tick as i64;
                    let rate = self.learning_rate * inputs[input_idx].max(0.25);
                    self.input_weights[input_idx][idx].stdp_update(delta_t, rate, 6.0, 8.0);
                }
            }

            if punitive {
                for (input_idx, signal) in inputs.iter().enumerate() {
                    if *signal > 0.0 {
                        self.input_weights[input_idx][idx]
                            .reinforce(self.learning_rate * *signal * 0.25);
                    } else {
                        self.input_weights[input_idx][idx].decay(self.learning_rate * 0.05);
                    }
                }

                self.output_throttle_weights[idx].reinforce(self.learning_rate);
                self.output_recover_weights[idx].decay(self.learning_rate * 0.5);
            } else if rewarding {
                self.output_recover_weights[idx].reinforce(self.learning_rate * 0.6);
                self.output_throttle_weights[idx].decay(self.learning_rate * 0.3);
            }

            if throttle_spike {
                let pre_tick = self.hidden_neurons[idx]
                    .last_spike_tick()
                    .unwrap_or(self.global_tick);
                let post_tick = self
                    .throttle_neuron
                    .last_spike_tick()
                    .unwrap_or(self.global_tick);
                self.output_throttle_weights[idx].stdp_update(
                    post_tick as i64 - pre_tick as i64,
                    self.learning_rate,
                    4.0,
                    6.0,
                );
            }

            if recover_spike {
                let pre_tick = self.hidden_neurons[idx]
                    .last_spike_tick()
                    .unwrap_or(self.global_tick);
                let post_tick = self
                    .recover_neuron
                    .last_spike_tick()
                    .unwrap_or(self.global_tick);
                self.output_recover_weights[idx].stdp_update(
                    post_tick as i64 - pre_tick as i64,
                    self.learning_rate * 0.8,
                    4.0,
                    6.0,
                );
            }
        }
    }
}

fn classify_delay_action(current_delay_ms: u64, next_delay_ms: u64) -> NeuroAction {
    if next_delay_ms > current_delay_ms {
        NeuroAction::IncreaseThrottle
    } else if next_delay_ms < current_delay_ms {
        NeuroAction::DecreaseThrottle
    } else {
        NeuroAction::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::{NeuroAction, Rsnn, RsnnConfig};
    use crate::core::metrics::ResponseMetric;
    use crate::core::profile::AdaptiveProfile;

    #[test]
    fn adaptive_delay_stays_in_bounds() {
        let cfg = RsnnConfig {
            hidden_size: 8,
            latency_threshold_ms: 200,
            min_delay_ms: 20,
            max_delay_ms: 300,
            learning_rate: 0.1,
            history_window: 8,
        };

        let mut rsnn = Rsnn::new_seeded(cfg.clone(), 7);
        let mut delay = 50;

        for step in 0..200 {
            let metric = if step % 7 == 0 {
                ResponseMetric {
                    path: "/rate-limited".to_string(),
                    status: 429,
                    latency_ms: 350,
                    body_size: 0,
                    transport_error: false,
                }
            } else {
                ResponseMetric {
                    path: "/ok".to_string(),
                    status: 200,
                    latency_ms: 40,
                    body_size: 64,
                    transport_error: false,
                }
            };

            let decision = rsnn.process_metric(&metric, delay);
            let next_delay = decision.next_delay_ms;
            assert!(next_delay >= cfg.min_delay_ms);
            assert!(next_delay <= cfg.max_delay_ms);
            delay = next_delay;
        }
    }

    #[test]
    fn repeated_blocking_escalates_into_defensive_profile() {
        let mut rsnn = Rsnn::new_seeded(
            RsnnConfig {
                hidden_size: 12,
                latency_threshold_ms: 250,
                min_delay_ms: 20,
                max_delay_ms: 800,
                learning_rate: 0.1,
                history_window: 10,
            },
            11,
        );

        let mut delay = 40;
        let mut entered_defensive = false;

        for _ in 0..8 {
            let decision = rsnn.process_metric(
                &ResponseMetric {
                    path: "/blocked".to_string(),
                    status: 429,
                    latency_ms: 700,
                    body_size: 0,
                    transport_error: false,
                },
                delay,
            );
            delay = decision.next_delay_ms;
            if decision.profile == AdaptiveProfile::Defensive {
                entered_defensive = true;
            }
        }

        assert!(entered_defensive);
        assert!(delay >= 40);
    }

    #[test]
    fn stable_success_recovers_toward_baseline() {
        let mut rsnn = Rsnn::new_seeded(
            RsnnConfig {
                hidden_size: 10,
                latency_threshold_ms: 300,
                min_delay_ms: 25,
                max_delay_ms: 1000,
                learning_rate: 0.08,
                history_window: 10,
            },
            42,
        );

        let mut delay = 300;
        for _ in 0..6 {
            let decision = rsnn.process_metric(
                &ResponseMetric {
                    path: "/rl".to_string(),
                    status: 429,
                    latency_ms: 900,
                    body_size: 0,
                    transport_error: false,
                },
                delay,
            );
            delay = decision.next_delay_ms;
        }

        let mut saw_decrease = false;
        let mut final_profile = AdaptiveProfile::Defensive;
        for _ in 0..10 {
            let decision = rsnn.process_metric(
                &ResponseMetric {
                    path: "/ok".to_string(),
                    status: 200,
                    latency_ms: 80,
                    body_size: 64,
                    transport_error: false,
                },
                delay,
            );
            if decision.action == NeuroAction::DecreaseThrottle {
                saw_decrease = true;
            }
            delay = decision.next_delay_ms;
            final_profile = decision.profile;
        }

        assert!(saw_decrease);
        assert_ne!(final_profile, AdaptiveProfile::Defensive);
    }
}
