use crate::core::speed::SpeedMode;
use crate::network::client::NetworkClient;
use crate::network::profile::ClientProfile;
use anyhow::Result;
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
}

#[derive(Clone, Debug)]
pub struct SafeClientFactory {
    config: ClientRuntimeConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientControlCommand {
    RebuildConnections,
}

impl SafeClientFactory {
    pub fn new(config: ClientRuntimeConfig) -> Self {
        Self { config }
    }

    pub fn build_shared(&self) -> Result<SharedNetworkClient> {
        Ok(Arc::new(NetworkClient::new(
            self.config.base_url.clone(),
            self.config.timeout_ms,
            self.config.profile,
            self.config.speed_mode,
            self.config.workers,
        )?))
    }

    pub fn channel(
        &self,
    ) -> Result<(
        watch::Sender<SharedNetworkClient>,
        watch::Receiver<SharedNetworkClient>,
    )> {
        let initial_client = self.build_shared()?;
        Ok(watch::channel(initial_client))
    }
}

pub fn spawn_safe_client_controller(
    mut command_rx: mpsc::Receiver<ClientControlCommand>,
    client_tx: watch::Sender<SharedNetworkClient>,
    factory: SafeClientFactory,
) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            match command {
                ClientControlCommand::RebuildConnections => {
                    let rebuilt = factory.build_shared()?;
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
        spawn_safe_client_controller, ClientControlCommand, ClientRuntimeConfig, SafeClientFactory,
    };
    use crate::core::speed::SpeedMode;
    use crate::network::profile::ClientProfile;
    use anyhow::Result;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use url::Url;

    #[tokio::test]
    async fn safe_client_controller_rebuilds_shared_client() -> Result<()> {
        let factory = SafeClientFactory::new(ClientRuntimeConfig {
            base_url: Url::parse("http://127.0.0.1:8000")?,
            timeout_ms: 1000,
            profile: ClientProfile::ResearchDefault,
            speed_mode: SpeedMode::Balanced,
            workers: 4,
        });
        let (client_tx, mut client_rx) = factory.channel()?;
        let initial = client_rx.borrow().clone();

        let (command_tx, command_rx) = mpsc::channel(1);
        let controller = spawn_safe_client_controller(command_rx, client_tx, factory);

        command_tx
            .send(ClientControlCommand::RebuildConnections)
            .await
            .expect("command send");
        client_rx.changed().await.expect("client update");
        let rebuilt = client_rx.borrow().clone();

        assert!(!Arc::ptr_eq(&initial, &rebuilt));

        drop(command_tx);
        controller.await??;
        Ok(())
    }
}
