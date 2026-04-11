use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct LifNeuron {
    potential: f32,
    threshold: f32,
    leak: f32,
    refractory_period: u8,
    refractory_left: u8,
    spike_history: VecDeque<u64>,
}

impl LifNeuron {
    pub fn new(threshold: f32, leak: f32, refractory_period: u8) -> Self {
        Self {
            potential: 0.0,
            threshold,
            leak,
            refractory_period,
            refractory_left: 0,
            spike_history: VecDeque::with_capacity(16),
        }
    }

    pub fn step(&mut self, input_current: f32, tick: u64) -> bool {
        if self.refractory_left > 0 {
            self.refractory_left -= 1;
            return false;
        }

        self.potential = (self.potential * self.leak) + input_current;
        if self.potential >= self.threshold {
            self.potential = 0.0;
            self.refractory_left = self.refractory_period;
            self.spike_history.push_back(tick);
            if self.spike_history.len() > 16 {
                self.spike_history.pop_front();
            }
            return true;
        }

        false
    }

    pub fn last_spike_tick(&self) -> Option<u64> {
        self.spike_history.back().copied()
    }
}

impl Default for LifNeuron {
    fn default() -> Self {
        Self::new(1.0, 0.85, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::LifNeuron;

    #[test]
    fn neuron_spikes_when_threshold_reached() {
        let mut neuron = LifNeuron::new(1.0, 0.9, 0);

        assert!(!neuron.step(0.4, 1));
        assert!(!neuron.step(0.4, 2));
        assert!(neuron.step(0.4, 3));
        assert_eq!(neuron.last_spike_tick(), Some(3));
    }

    #[test]
    fn refractory_period_suppresses_immediate_respike() {
        let mut neuron = LifNeuron::new(0.5, 1.0, 2);

        assert!(neuron.step(0.6, 1));
        assert!(!neuron.step(0.6, 2));
        assert!(!neuron.step(0.6, 3));
        assert!(neuron.step(0.6, 4));
    }
}
