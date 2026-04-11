use crate::core::metrics::ResponseMetric;
use crate::network::profile::ClientProfile;
use anyhow::{Context, Result};
use reqwest::Client;
use std::time::{Duration, Instant};
use url::Url;

#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    base_url: Url,
}

impl NetworkClient {
    pub fn new(base_url: Url, timeout_ms: u64, profile: ClientProfile) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .user_agent(profile.user_agent())
            .default_headers(profile.default_headers())
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { client, base_url })
    }

    pub async fn execute_path(&self, path: &str) -> ResponseMetric {
        let normalized_path = path.trim_start_matches('/');
        let path_for_metric = if normalized_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", normalized_path)
        };

        let url = match self.base_url.join(normalized_path) {
            Ok(url) => url,
            Err(_) => {
                return ResponseMetric {
                    path: path_for_metric,
                    status: 0,
                    latency_ms: 0,
                    body_size: 0,
                    transport_error: true,
                };
            }
        };

        let start = Instant::now();
        let response = self.client.get(url).send().await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_size = match resp.bytes().await {
                    Ok(bytes) => bytes.len(),
                    Err(_) => 0,
                };

                ResponseMetric {
                    path: path_for_metric,
                    status,
                    latency_ms,
                    body_size,
                    transport_error: false,
                }
            }
            Err(_) => ResponseMetric {
                path: path_for_metric,
                status: 0,
                latency_ms,
                body_size: 0,
                transport_error: true,
            },
        }
    }
}
