use crate::core::metrics::ResponseMetric;
use crate::core::speed::SpeedMode;
use crate::network::profile::ClientProfile;
use anyhow::{Context, Result};
use reqwest::{Client, Proxy};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};
use url::Url;

#[derive(Clone)]
pub struct NetworkClient {
    clients: Vec<Client>,
    base_url: Url,
}

impl NetworkClient {
    pub fn new(
        base_url: Url,
        timeout_ms: u64,
        profile: ClientProfile,
        speed_mode: SpeedMode,
        workers: usize,
        tor_proxy: Option<String>,
        proxies: &[String],
    ) -> Result<Self> {
        let mut proxy_list = proxies.to_vec();
        if proxy_list.is_empty() {
            if let Some(tp) = tor_proxy {
                proxy_list.push(tp);
            }
        }

        let mut clients = Vec::new();
        if proxy_list.is_empty() {
            clients.push(Self::build_client(timeout_ms, profile, speed_mode, workers, None)?);
        } else {
            let per_proxy_workers = (workers / proxy_list.len()).max(1);
            for p in &proxy_list {
                clients.push(Self::build_client(timeout_ms, profile, speed_mode, per_proxy_workers, Some(p.clone()))?);
            }
        }

        Ok(Self { clients, base_url })
    }

    fn build_client(
        timeout_ms: u64,
        profile: ClientProfile,
        _speed_mode: SpeedMode,
        workers: usize,
        proxy_url: Option<String>,
    ) -> Result<Client> {
        let mut builder = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .user_agent(profile.user_agent())
            .default_headers(profile.default_headers())
            .pool_max_idle_per_host(workers.max(1))
            .pool_idle_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2) 
            .brotli(true)
            .deflate(true)
            .gzip(true);

        if let Some(url) = proxy_url {
            let proxy = Proxy::all(&url)
                .with_context(|| format!("Invalid proxy URL: {}", url))?;
            builder = builder.proxy(proxy);
        }

        builder.build().context("Failed to build HTTP client")
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
                    body_fingerprint: 0,
                    transport_error: true,
                };
            }
        };

        let start = Instant::now();
        
        let client = {
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            self.clients.choose(&mut rng).unwrap_or(&self.clients[0]).clone()
        };
        
        let response = client.get(url).send().await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let (body_size, body_fingerprint) = match resp.bytes().await {
                    Ok(bytes) => (bytes.len(), fingerprint_bytes(bytes.as_ref())),
                    Err(_) => (0, 0),
                };

                ResponseMetric {
                    path: path_for_metric,
                    status,
                    latency_ms,
                    body_size,
                    body_fingerprint,
                    transport_error: false,
                }
            }
            Err(_) => ResponseMetric {
                path: path_for_metric,
                status: 0,
                latency_ms,
                body_size: 0,
                body_fingerprint: 0,
                transport_error: true,
            },
        }
    }
}

fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
