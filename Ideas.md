# Sigma Morpho: SNN-Driven Adaptive Evasion Roadmap

## 1. Project Vision & Mission
**Sigma Morpho** (formerly NeuroBuster) is an advanced, defensive, and adaptive web fuzzer designed for high-security environments. Unlike traditional tools that focus purely on speed, Sigma Morpho treats the target WAF/IPS as a black-box environment to be navigated using **Neuromorphic Computing**.

### The Core Objective
To leverage **Recurrent Spiking Neural Networks (RSNN)** to analyze temporal server response patterns and autonomously adapt request cadence, identity, and network routing to maintain "Digital Weightlessness" (Anti-Gravity) during Red Teaming operations.

---

## 2. Neuromorphic Evasion Core: The "Brain"

### A. LIF Neuron Model (Leaky Integrate-and-Fire)
Instead of a simple integer delay, every request status and latency value is converted into an electrical current injected into a virtual neuron.

```rust
// Core LIF (Leaky Integrate-and-Fire) Neuron Parameters
const THRESHOLD: f32 = 1.0;
const LEAK_RATE: f32 = 0.9;
const REFRACTORY_PERIOD: u8 = 2;

struct LifNeuron {
    potential: f32,
    refractory_timer: u8,
}

impl LifNeuron {
    fn new() -> Self {
        Self { potential: 0.0, refractory_timer: 0 }
    }

    fn step(&mut self, input_current: f32) -> bool {
        if self.refractory_timer > 0 {
            self.refractory_timer -= 1;
            return false;
        }
        self.potential = (self.potential * LEAK_RATE) + input_current;
        if self.potential >= THRESHOLD {
            self.potential = 0.0; // Reset after spike
            self.refractory_timer = REFRACTORY_PERIOD;
            return true;
        }
        false
    }
}
```

### B. STDP (Spike-Timing-Dependent Plasticity)
A self-learning mechanism where synaptic weights adjust dynamically based on the timing of spikes.

```rust
const LEARNING_RATE: f32 = 0.1;
const MAX_WEIGHT: f32 = 2.0;

pub struct WafEvasionNetwork {
    input_neurons: Vec<LifNeuron>,  // 0: Latency, 1: 429/403, 2: 404 Rate
    output_neuron: LifNeuron,       // Action Trigger
    synapses: Vec<Synapse>,
}

impl WafEvasionNetwork {
    fn apply_stdp(&mut self) {
        let post_spikes = &self.output_neuron.spike_history;
        if post_spikes.is_empty() { return; }
        let last_post = *post_spikes.back().unwrap();

        for (i, syn) in self.synapses.iter_mut().enumerate() {
            let pre_spikes = &self.input_neurons[i].spike_history;
            if let Some(&last_pre) = pre_spikes.back() {
                let delta_t = last_post as i64 - last_pre as i64;
                if delta_t > 0 && delta_t < 50 {
                    syn.weight = (syn.weight + LEARNING_RATE).min(MAX_WEIGHT);
                } else {
                    syn.weight = (syn.weight - (LEARNING_RATE / 2.0)).max(0.1);
                }
            }
        }
    }
}
```

### C. Recurrent SNN (RSNN)
Architecture allowing contextual memory of past requests.

```rust
pub struct RSNN {
    hidden_neurons: Vec<Neuron>,
    input_weights: Vec<Vec<f32>>,      
    recurrent_weights: Vec<Vec<f32>>,  
    last_spikes: Vec<bool>,            
    output_neuron: Neuron,
}

impl RSNN {
    pub fn process_step(&mut self, inputs: &[f32]) -> Vec<EvasionAction> {
        let hidden_size = self.hidden_neurons.len();
        let mut current_spikes = vec![false; hidden_size];

        for i in 0..hidden_size {
            let mut current = 0.0;
            for j in 0..inputs.len() { current += inputs[j] * self.input_weights[j][i]; }
            for k in 0..hidden_size {
                if self.last_spikes[k] { current += self.recurrent_weights[k][i]; }
            }
            let n = &mut self.hidden_neurons[i];
            n.potential = (n.potential * LEAK_RATE) + current;
            if n.potential >= THRESHOLD {
                n.potential = 0.0;
                current_spikes[i] = true;
            }
        }
        self.last_spikes = current_spikes;
        // Logic for output actions...
    }
}
```

---

## 3. The "Body": Actor-Based Concurrency

### A. Neuro-Actor Engine
Separating CPU-bound math from I/O workers using channels.

```rust
pub fn spawn_neuro_actor(
    mut metric_rx: mpsc::Receiver<ResponseMetric>,
    delay_tx: watch::Sender<u64>,
    cmd_tx: mpsc::Sender<EvasionAction>,
) {
    task::spawn_blocking(move || {
        let mut rsnn = RSNN::new(3, 20);
        let mut current_delay = 50;

        while let Some(metric) = metric_rx.blocking_recv() {
            let inputs = encode_metrics(&metric);
            let actions = rsnn.process_step(&inputs);
            
            for action in actions {
                let _ = cmd_tx.blocking_send(action);
            }
            // Dynamic delay cooling...
            let _ = delay_tx.send(current_delay);
        }
    });
}
```

---

## 4. Stealth & Identity Management

### A. Tor Control & Circuit Rebuilding
Native integration with the Tor Control Port via TCP.

```rust
pub struct TorController {
    control_address: String,
    password: Option<String>,
}

impl TorController {
    pub async fn request_new_ip(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = TcpStream::connect(&self.control_address).await?;
        let auth_cmd = format!("AUTHENTICATE \"{}\"\r\n", self.password.as_deref().unwrap_or(""));
        stream.write_all(auth_cmd.as_bytes()).await?;
        stream.write_all(b"SIGNAL NEWNYM\r\n").await?;
        Ok(())
    }
}
```

### B. Client Hot-Swapping (Rotator)
Updating the global `reqwest::Client` on-the-fly without stopping workers.

```rust
pub struct ClientFactory {
    proxy_url: Option<String>,
}

impl ClientFactory {
    pub fn build(&self) -> Client {
        let ua = SELECT_USER_AGENT();
        Client::builder()
            .user_agent(ua)
            .proxy(Proxy::all(&self.proxy_url).unwrap())
            .pool_max_idle_per_host(0) // Purge TCP pool on rotation
            .build().unwrap()
    }
}
```

---

## 5. Final Assembly: The Orchestrator
Connecting all actors into a unified high-performance stealth fuzzer.

```rust
pub struct FuzzOrchestrator {
    factory: ClientFactory,
    tor: TorController,
    wordlist: Vec<String>,
}

impl FuzzOrchestrator {
    pub async fn run(self) {
        let (metric_tx, metric_rx) = mpsc::channel(1000);
        let (delay_tx, delay_rx) = watch::channel(100u64);
        let (client_tx, client_rx) = watch::channel(self.factory.build());
        let (cmd_tx, mut cmd_rx) = mpsc::channel(100);

        spawn_neuro_actor(metric_rx, delay_tx, cmd_tx);
        
        // Evasion Controller Task
        tokio::spawn(async move {
            while let Some(action) = cmd_rx.recv().await {
                match action {
                    EvasionAction::RotateUserAgent => { client_tx.send(factory.build()); },
                    EvasionAction::RebuildTorCircuit => { 
                        tor.request_new_ip().await;
                        client_tx.send(factory.build());
                    }
                }
            }
        });

        // Worker tasks consuming client_rx and delay_rx via clones...
    }
}
```

---

## 6. Current Implementation Status

✅ **Neuromorphic Core**: LIF Neurons, STDP Weights, and RSNN Topology implemented.
✅ **High-Concurrency Engine**: Actor-based model with non-blocking I/O separation.
✅ **Identity Management**: Hot-swappable UA/Proxy clients and Tor Control integration.
✅ **Optimization Layer**: Lazy Recursion and `DashMap` for high-volume fuzzing.
✅ **Interactive CLI**: Comprehensive session wrapper with automated run archiving.

---

## 7. Future Roadmap

1. **Fingerprint Impersonation**: Integration of `reqwest-impersonate` (JA3/H2 fingerprints).
2. **Neural Web UI**: Real-time tactical dashboard for neuron activity.
3. **Swarm Fuzzing**: Sharding synaptic intelligence across multiple project instances.
4. **Behavioral Wordlists**: SNN-triggered dictionary mutations (e.g., fuzzing for `.git` or `.env` when anomaly spikes).
