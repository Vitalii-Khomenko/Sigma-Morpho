use crate::core::speed::SpeedMode;
use crate::network::client::NetworkClient;
use crate::network::profile::ClientProfile;
use crate::network::tor::TorController;
use anyhow::Result;
use rand::seq::SliceRandom;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use url::Url;

pub type SharedNetworkClient = Arc<NetworkClient>;

#[derive(Clone, Debug)]
pub struct ClientRuntimeConfig {
    pub base_url: Url,
    pub timeout_ms: u64,
    pub profile: ClientProfile,
    pub speed_mode: SpeedMode,
    pub workers: usize,
    pub tor_proxy: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SafeClientFactory {
    config: ClientRuntimeConfig,
    current_profile: ClientProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientRebuildReason {
    ManualInterval,
    SimulatedRotateAdvisory,
    SimulatedCircuitAdvisory,
}

impl ClientRebuildReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManualInterval => "manual-interval",
            Self::SimulatedRotateAdvisory => "simulated-rotate-advisory",
            Self::SimulatedCircuitAdvisory => "simulated-circuit-advisory",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientControlCommand {
    RebuildConnections { reason: ClientRebuildReason },
}

impl SafeClientFactory {
    pub fn new(config: ClientRuntimeConfig) -> Self {
        let current_profile = config.profile;
        Self { config, current_profile }
    }

    pub fn build_shared(&mut self, rotate: bool) -> Result<SharedNetworkClient> {
        if rotate {
            let mut rng = rand::thread_rng();
            self.current_profile = *ClientProfile::ALL.choose(&mut rng).unwrap();
            println!("[ROTATOR] Rotated User-Agent to profile: {}", self.current_profile.as_str());
        }

        Ok(Arc::new(NetworkClient::new(
            self.config.base_url.clone(),
            self.config.timeout_ms,
            self.current_profile,
            self.config.speed_mode,
            self.config.workers,
            self.config.tor_proxy.clone(),
        )?))
    }

    pub fn channel(
        &mut self,
    ) -> Result<(
        watch::Sender<SharedNetworkClient>,
        watch::Receiver<SharedNetworkClient>,
    )> {
        let initial_client = self.build_shared(false)?;
        Ok(watch::channel(initial_client))
    }
}

pub fn spawn_safe_client_controller(
    mut command_rx: mpsc::Receiver<ClientControlCommand>,
    client_tx: watch::Sender<SharedNetworkClient>,
    mut factory: SafeClientFactory,
    rebuild_count: Arc<AtomicUsize>,
    tor: Option<TorController>,
) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            match command {
                ClientControlCommand::RebuildConnections { reason } => {
                    if reason == ClientRebuildReason::SimulatedCircuitAdvisory {
                        if let Some(ref t) = tor {
                            println!("[TOR REBUILD] Sending NEWNYM signal to Tor Control Port...");
                            if let Err(e) = t.request_new_ip().await {
                                eprintln!("[TOR ERROR] Failed to request new IP: {}", e);
                            } else {
                                println!("[TOR REBUILD] Circuit rebuild acknowledged.");
                                // Let Tor rebuild circuit
                                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            }
                        }
                    }

                    let rotate_ua = reason == ClientRebuildReason::SimulatedRotateAdvisory;
                    let rebuilt = factory.build_shared(rotate_ua)?;
                    rebuild_count.fetch_add(1, Ordering::Relaxed);
                    println!(
                        "[CLIENT] rebuilt fixed-profile transport reason={}",
                        reason.as_str()
                    );
                    let _ = client_tx.send(rebuilt);
                }
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        spawn_safe_client_controller, ClientControlCommand, ClientRebuildReason,
        ClientRuntimeConfig, SafeClientFactory,
    };
    use crate::core::speed::SpeedMode;
    use crate::network::profile::ClientProfile;
    use anyhow::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use url::Url;

    #[tokio::test]
    async fn safe_client_controller_rebuilds_shared_client() -> Result<()> {
        let mut factory = SafeClientFactory::new(ClientRuntimeConfig {
            base_url: Url::parse("http://127.0.0.1:8000")?,
            timeout_ms: 1000,
            profile: ClientProfile::ResearchDefault,
            speed_mode: SpeedMode::Balanced,
            workers: 4,
            tor_proxy: None,
        });
        let (client_tx, mut client_rx) = factory.channel()?;
        let initial = client_rx.borrow().clone();

        let (command_tx, command_rx) = mpsc::channel(1);
        let rebuild_count = Arc::new(AtomicUsize::new(0));
        let controller = spawn_safe_client_controller(
            command_rx,
            client_tx,
            factory,
            Arc::clone(&rebuild_count),
            None,
        );

        command_tx
            .send(ClientControlCommand::RebuildConnections {
                reason: ClientRebuildReason::ManualInterval,
            })
            .await
            .expect("command send");
        client_rx.changed().await.expect("client update");
        let rebuilt = client_rx.borrow().clone();

        assert!(!Arc::ptr_eq(&initial, &rebuilt));
        assert_eq!(rebuild_count.load(Ordering::Relaxed), 1);

        drop(command_tx);
        controller.await??;
        Ok(())
    }
}
