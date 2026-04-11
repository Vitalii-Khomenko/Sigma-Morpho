use super::synapse::Synapse;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct RsnnState {
    pub input_weights: Vec<Vec<Synapse>>,
    pub recurrent_weights: Vec<Vec<f32>>,
    pub output_throttle_weights: Vec<Synapse>,
    pub output_recover_weights: Vec<Synapse>,
}
