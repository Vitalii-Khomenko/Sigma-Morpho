use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use url::Url;

use crate::core::research::ScenarioKind;
use crate::network::profile::ClientProfile;

#[derive(Debug, Parser)]
#[command(
    name = "sigma_morpho",
    version,
    about = "Adaptive HTTP workload tester powered by a lightweight RSNN core"
)]
struct CliArgs {
    #[arg(long, value_name = "BASE_URL")]
    base_url: Option<String>,

    #[arg(long, value_name = "WORDLIST", default_value = "wordlist.txt")]
    wordlist: PathBuf,

    #[arg(long, default_value_t = 16)]
    workers: usize,

    #[arg(long, default_value_t = 1)]
    rounds: usize,

    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,

    #[arg(long, default_value_t = 50)]
    initial_delay_ms: u64,

    #[arg(long, default_value_t = 25)]
    min_delay_ms: u64,

    #[arg(long, default_value_t = 5000)]
    max_delay_ms: u64,

    #[arg(long, default_value_t = 800)]
    latency_threshold_ms: u64,

    #[arg(long, value_enum, default_value_t = ClientProfile::ResearchDefault)]
    client_profile: ClientProfile,

    #[arg(long)]
    simulation_mode: bool,

    #[arg(long, value_enum)]
    scenario: Option<ScenarioKind>,

    #[arg(long)]
    compare_profiles: bool,

    #[arg(long)]
    authorized_target: bool,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub base_url: Url,
    pub wordlist: PathBuf,
    pub workers: usize,
    pub rounds: usize,
    pub timeout_ms: u64,
    pub initial_delay_ms: u64,
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
    pub latency_threshold_ms: u64,
    pub client_profile: ClientProfile,
    pub simulation_mode: bool,
    pub scenario: Option<ScenarioKind>,
    pub compare_profiles: bool,
}

impl AppConfig {
    pub fn from_args() -> Result<Self> {
        let args = CliArgs::parse();

        if args.workers == 0 {
            bail!("--workers must be at least 1");
        }
        if args.rounds == 0 {
            bail!("--rounds must be at least 1");
        }
        if args.timeout_ms == 0 {
            bail!("--timeout-ms must be greater than 0");
        }
        if args.min_delay_ms > args.max_delay_ms {
            bail!("--min-delay-ms cannot be greater than --max-delay-ms");
        }
        if args.compare_profiles && args.scenario.is_none() {
            bail!("--compare-profiles can only be used together with --scenario");
        }

        let raw_base_url = match (&args.base_url, args.scenario) {
            (Some(base_url), _) => base_url.clone(),
            (None, Some(_)) => "http://127.0.0.1:8000".to_string(),
            (None, None) => bail!("--base-url is required unless --scenario is used"),
        };

        let base_url = Url::parse(&raw_base_url)
            .with_context(|| format!("Invalid --base-url: {}", raw_base_url))?;

        validate_target(&base_url, args.authorized_target)?;
        let initial_delay_ms = args.client_profile.adjusted_initial_delay(
            args.initial_delay_ms,
            args.min_delay_ms,
            args.max_delay_ms,
        );

        Ok(Self {
            base_url,
            wordlist: args.wordlist,
            workers: args.workers,
            rounds: args.rounds,
            timeout_ms: args.client_profile.adjusted_timeout(args.timeout_ms),
            initial_delay_ms,
            min_delay_ms: args.min_delay_ms,
            max_delay_ms: args.max_delay_ms,
            latency_threshold_ms: args.latency_threshold_ms,
            client_profile: args.client_profile,
            simulation_mode: args.simulation_mode,
            scenario: args.scenario,
            compare_profiles: args.compare_profiles,
        })
    }

    pub fn load_wordlist(&self) -> Result<Vec<String>> {
        let file = File::open(&self.wordlist)
            .with_context(|| format!("Failed to open wordlist: {}", self.wordlist.display()))?;
        let reader = BufReader::new(file);

        let mut paths = Vec::new();
        for line in reader.lines() {
            let raw = line.with_context(|| {
                format!(
                    "Failed reading line in wordlist: {}",
                    self.wordlist.display()
                )
            })?;

            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            paths.push(trimmed.trim_start_matches('/').to_string());
        }

        Ok(paths)
    }
}

fn validate_target(url: &Url, authorized_target: bool) -> Result<()> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("--base-url must contain a host"))?;

    if is_local_host(host) || authorized_target {
        return Ok(());
    }

    bail!(
        "Refusing non-local target without --authorized-target. Use this tool only on systems you own or are explicitly authorized to test."
    )
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1") || host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use super::{validate_target, AppConfig};
    use crate::core::research::ScenarioKind;
    use crate::network::profile::ClientProfile;
    use anyhow::Result;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use url::Url;

    fn unique_temp_file(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sigma-morpho-{name}-{stamp}.txt"))
    }

    #[test]
    fn local_targets_are_allowed_without_override() -> Result<()> {
        let url = Url::parse("http://127.0.0.1:8080")?;
        validate_target(&url, false)?;
        Ok(())
    }

    #[test]
    fn remote_targets_require_explicit_authorization() -> Result<()> {
        let url = Url::parse("https://academy.htb")?;
        assert!(validate_target(&url, false).is_err());
        assert!(validate_target(&url, true).is_ok());
        Ok(())
    }

    #[test]
    fn wordlist_loader_filters_comments_and_slashes() -> Result<()> {
        let path = unique_temp_file("wordlist");
        fs::write(&path, "# comment\n/admin\n\napi\n")?;

        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000")?,
            wordlist: path.clone(),
            workers: 1,
            rounds: 1,
            timeout_ms: 1000,
            initial_delay_ms: 25,
            min_delay_ms: 25,
            max_delay_ms: 100,
            latency_threshold_ms: 500,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: false,
            scenario: None,
            compare_profiles: false,
        };

        let paths = cfg.load_wordlist()?;
        assert_eq!(paths, vec!["admin".to_string(), "api".to_string()]);

        let _ = fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn scenario_config_supports_profile_comparison() {
        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000").unwrap(),
            wordlist: "wordlist.txt".into(),
            workers: 1,
            rounds: 1,
            timeout_ms: 1000,
            initial_delay_ms: 50,
            min_delay_ms: 25,
            max_delay_ms: 250,
            latency_threshold_ms: 500,
            client_profile: ClientProfile::ApiDiagnostic,
            simulation_mode: true,
            scenario: Some(ScenarioKind::RateLimit),
            compare_profiles: true,
        };

        assert_eq!(cfg.scenario, Some(ScenarioKind::RateLimit));
        assert!(cfg.compare_profiles);
        assert!(cfg.simulation_mode);
    }
}
