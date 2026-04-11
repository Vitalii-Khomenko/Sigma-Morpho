use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Synapse {
    pub weight: f32,
    min_weight: f32,
    max_weight: f32,
}

impl Synapse {
    pub fn new(weight: f32, min_weight: f32, max_weight: f32) -> Self {
        Self {
            weight: weight.clamp(min_weight, max_weight),
            min_weight,
            max_weight,
        }
    }

    pub fn reinforce(&mut self, delta: f32) {
        self.weight = (self.weight + delta).clamp(self.min_weight, self.max_weight);
    }

    pub fn decay(&mut self, delta: f32) {
        self.weight = (self.weight - delta).clamp(self.min_weight, self.max_weight);
    }

    pub fn stdp_update(&mut self, delta_t: i64, learning_rate: f32, tau_plus: f32, tau_minus: f32) {
        if delta_t > 0 {
            let ltp = learning_rate * (-(delta_t as f32) / tau_plus).exp();
            self.reinforce(ltp);
        } else if delta_t < 0 {
            let ltd = learning_rate * ((delta_t as f32) / tau_minus).exp();
            self.decay(ltd.abs());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Synapse;

    #[test]
    fn stdp_strengthens_when_precedes_post() {
        let mut syn = Synapse::new(0.5, 0.1, 2.0);
        syn.stdp_update(2, 0.2, 5.0, 5.0);
        assert!(syn.weight > 0.5);
    }

    #[test]
    fn stdp_weakens_when_post_precedes_pre() {
        let mut syn = Synapse::new(0.5, 0.1, 2.0);
        syn.stdp_update(-2, 0.2, 5.0, 5.0);
        assert!(syn.weight < 0.5);
    }
}
