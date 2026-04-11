use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use url::Url;

use crate::core::research::ScenarioKind;
use crate::core::speed::SpeedMode;
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

    #[arg(long, default_value_t = 0)]
    recursion_depth: u8,

    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,

    #[arg(long, default_value_t = 50)]
    initial_delay_ms: u64,

    #[arg(long, value_name = "REQUESTS")]
    rebuild_client_every: Option<usize>,

    #[arg(long)]
    rebuild_client_on_advisory: bool,

    #[arg(long, default_value_t = 25)]
    min_delay_ms: u64,

    #[arg(long, default_value_t = 5000)]
    max_delay_ms: u64,

    #[arg(long, default_value_t = 800)]
    latency_threshold_ms: u64,

    #[arg(long, value_name = "FINDINGS_FILE", default_value = "findings.txt")]
    findings_file: PathBuf,

    #[arg(
        long,
        value_name = "STATUS_LIST",
        default_value = "200,204,301,302,307,308,401,403,405,500"
    )]
    interesting_statuses: String,

    #[arg(long, default_value_t = 0)]
    min_body_bytes: usize,

    #[arg(long)]
    max_body_bytes: Option<usize>,

    #[arg(long)]
    disable_soft_404_filter: bool,

    #[arg(long, value_enum, default_value_t = SpeedMode::Balanced)]
    speed_mode: SpeedMode,

    #[arg(long, value_enum, default_value_t = ClientProfile::ResearchDefault)]
    client_profile: ClientProfile,

    #[arg(long)]
    simulation_mode: bool,

    #[arg(long, value_enum)]
    scenario: Option<ScenarioKind>,

    #[arg(long)]
    compare_profiles: bool,

    #[arg(long)]
    compare_profiles_live: bool,

    #[arg(long)]
    authorized_target: bool,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub base_url: Url,
    pub wordlist: PathBuf,
    pub workers: usize,
    pub rounds: usize,
    pub recursion_depth: u8,
    pub requested_timeout_ms: u64,
    pub timeout_ms: u64,
    pub requested_initial_delay_ms: u64,
    pub initial_delay_ms: u64,
    pub rebuild_client_every: Option<usize>,
    pub rebuild_client_on_advisory: bool,
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
    pub latency_threshold_ms: u64,
    pub findings_file: PathBuf,
    pub interesting_statuses: Vec<u16>,
    pub min_body_bytes: usize,
    pub max_body_bytes: Option<usize>,
    pub disable_soft_404_filter: bool,
    pub speed_mode: SpeedMode,
    pub client_profile: ClientProfile,
    pub simulation_mode: bool,
    pub scenario: Option<ScenarioKind>,
    pub compare_profiles: bool,
    pub compare_profiles_live: bool,
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
        if args
            .max_body_bytes
            .map(|max| max < args.min_body_bytes)
            .unwrap_or(false)
        {
            bail!("--max-body-bytes cannot be smaller than --min-body-bytes");
        }
        if args.compare_profiles && args.scenario.is_none() {
            bail!("--compare-profiles can only be used together with --scenario");
        }
        if args.compare_profiles_live && args.scenario.is_some() {
            bail!("--compare-profiles-live cannot be used together with --scenario");
        }
        if args.compare_profiles && args.compare_profiles_live {
            bail!("Use either --compare-profiles or --compare-profiles-live, not both");
        }
        if args
            .rebuild_client_every
            .map(|interval| interval == 0)
            .unwrap_or(false)
        {
            bail!("--rebuild-client-every must be at least 1 when provided");
        }

        let raw_base_url = match (&args.base_url, args.scenario) {
            (Some(base_url), _) => base_url.clone(),
            (None, Some(_)) => "http://127.0.0.1:8000".to_string(),
            (None, None) => bail!("--base-url is required unless --scenario is used"),
        };

        let base_url = Url::parse(&raw_base_url)
            .with_context(|| format!("Invalid --base-url: {}", raw_base_url))?;

        validate_target(&base_url, args.authorized_target)?;
        let interesting_statuses = parse_status_list(&args.interesting_statuses)?;
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
            recursion_depth: args.recursion_depth,
            requested_timeout_ms: args.timeout_ms,
            timeout_ms: args.client_profile.adjusted_timeout(args.timeout_ms),
            requested_initial_delay_ms: args.initial_delay_ms,
            initial_delay_ms,
            rebuild_client_every: args.rebuild_client_every,
            rebuild_client_on_advisory: args.rebuild_client_on_advisory,
            min_delay_ms: args.min_delay_ms,
            max_delay_ms: args.max_delay_ms,
            latency_threshold_ms: args.latency_threshold_ms,
            findings_file: args.findings_file,
            interesting_statuses,
            min_body_bytes: args.min_body_bytes,
            max_body_bytes: args.max_body_bytes,
            disable_soft_404_filter: args.disable_soft_404_filter,
            speed_mode: args.speed_mode,
            client_profile: args.client_profile,
            simulation_mode: args.simulation_mode,
            scenario: args.scenario,
            compare_profiles: args.compare_profiles,
            compare_profiles_live: args.compare_profiles_live,
        })
    }

    pub fn for_profile(&self, profile: ClientProfile) -> Self {
        let mut next = self.clone();
        next.client_profile = profile;
        next.timeout_ms = profile.adjusted_timeout(self.requested_timeout_ms);
        next.initial_delay_ms = profile.adjusted_initial_delay(
            self.requested_initial_delay_ms,
            self.min_delay_ms,
            self.max_delay_ms,
        );
        next.findings_file = self.findings_file_for_profile(profile);
        next
    }

    pub fn findings_file_for_profile(&self, profile: ClientProfile) -> PathBuf {
        let suffix = profile.as_str();
        let parent = self
            .findings_file
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_default();
        let stem = self
            .findings_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("findings");
        let ext = self
            .findings_file
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let file_name = if ext.is_empty() {
            format!("{stem}-{suffix}")
        } else {
            format!("{stem}-{suffix}.{ext}")
        };

        parent.join(file_name)
    }

    pub fn load_wordlist(&self) -> Result<Vec<String>> {
        if self.wordlist.is_dir() {
            return self.load_wordlist_dir();
        }

        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        self.load_wordlist_file(&self.wordlist, &mut seen, &mut paths)?;
        Ok(paths)
    }

    fn load_wordlist_dir(&self) -> Result<Vec<String>> {
        let mut entries = std::fs::read_dir(&self.wordlist)
            .with_context(|| format!("Failed to read wordlist dir: {}", self.wordlist.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("Failed to enumerate dir: {}", self.wordlist.display()))?;

        entries.sort_by_key(|entry| entry.path());

        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        let mut loaded_files = 0_u64;

        for entry in entries {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let is_text_like = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext, "txt" | "lst" | "list"))
                .unwrap_or(false);

            if !is_text_like {
                continue;
            }

            self.load_wordlist_file(&path, &mut seen, &mut paths)?;
            loaded_files += 1;
        }

        if loaded_files == 0 {
            bail!(
                "No supported wordlist files found in directory: {}",
                self.wordlist.display()
            );
        }

        Ok(paths)
    }

    fn load_wordlist_file(
        &self,
        path: &PathBuf,
        seen: &mut HashSet<String>,
        output: &mut Vec<String>,
    ) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open wordlist: {}", path.display()))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let raw = line
                .with_context(|| format!("Failed reading line in wordlist: {}", path.display()))?;

            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let normalized = trimmed.trim_start_matches('/').to_string();
            if normalized.is_empty() || !seen.insert(normalized.clone()) {
                continue;
            }

            output.push(normalized);
        }

        Ok(())
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

fn parse_status_list(raw: &str) -> Result<Vec<u16>> {
    let mut parsed = Vec::new();

    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let status = trimmed
            .parse::<u16>()
            .with_context(|| format!("Invalid status code in --interesting-statuses: {trimmed}"))?;

        if !(100..=599).contains(&status) {
            bail!("Status code out of range in --interesting-statuses: {status}");
        }

        if !parsed.contains(&status) {
            parsed.push(status);
        }
    }

    if parsed.is_empty() {
        bail!("--interesting-statuses cannot be empty");
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_status_list, validate_target, AppConfig};
    use crate::core::research::ScenarioKind;
    use crate::core::speed::SpeedMode;
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
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 25,
            initial_delay_ms: 25,
            rebuild_client_every: None,
            rebuild_client_on_advisory: false,
            min_delay_ms: 25,
            max_delay_ms: 100,
            latency_threshold_ms: 500,
            findings_file: "findings.txt".into(),
            interesting_statuses: vec![200, 403],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: false,
            scenario: None,
            compare_profiles: false,
            compare_profiles_live: false,
        };

        let paths = cfg.load_wordlist()?;
        assert_eq!(paths, vec!["admin".to_string(), "api".to_string()]);

        let _ = fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn wordlist_directory_loader_merges_and_deduplicates() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "sigma-morpho-dict-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("a.txt"), "admin\napi\n# x\n")?;
        fs::write(dir.join("b.txt"), "/api\nhealth\n")?;
        fs::write(dir.join("notes.md"), "ignored\n")?;

        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000")?,
            wordlist: dir.clone(),
            workers: 1,
            rounds: 1,
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 25,
            initial_delay_ms: 25,
            rebuild_client_every: None,
            rebuild_client_on_advisory: false,
            min_delay_ms: 25,
            max_delay_ms: 100,
            latency_threshold_ms: 500,
            findings_file: "findings.txt".into(),
            interesting_statuses: vec![200, 403],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: false,
            scenario: None,
            compare_profiles: false,
            compare_profiles_live: false,
        };

        let paths = cfg.load_wordlist()?;
        assert_eq!(
            paths,
            vec!["admin".to_string(), "api".to_string(), "health".to_string()]
        );

        let _ = fs::remove_dir_all(dir);
        Ok(())
    }

    #[test]
    fn scenario_config_supports_profile_comparison() {
        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000").unwrap(),
            wordlist: "wordlist.txt".into(),
            workers: 1,
            rounds: 1,
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 50,
            initial_delay_ms: 50,
            rebuild_client_every: None,
            rebuild_client_on_advisory: false,
            min_delay_ms: 25,
            max_delay_ms: 250,
            latency_threshold_ms: 500,
            findings_file: "findings.txt".into(),
            interesting_statuses: vec![200, 403],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ApiDiagnostic,
            simulation_mode: true,
            scenario: Some(ScenarioKind::RateLimit),
            compare_profiles: true,
            compare_profiles_live: false,
        };

        assert_eq!(cfg.scenario, Some(ScenarioKind::RateLimit));
        assert!(cfg.compare_profiles);
        assert!(cfg.simulation_mode);
    }

    #[test]
    fn findings_file_suffixes_by_profile() {
        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000").unwrap(),
            wordlist: "wordlist.txt".into(),
            workers: 1,
            rounds: 1,
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 50,
            initial_delay_ms: 50,
            rebuild_client_every: Some(100),
            rebuild_client_on_advisory: true,
            min_delay_ms: 25,
            max_delay_ms: 250,
            latency_threshold_ms: 500,
            findings_file: "reports/findings.txt".into(),
            interesting_statuses: vec![200, 403],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: true,
            scenario: None,
            compare_profiles: false,
            compare_profiles_live: false,
        };

        assert_eq!(
            cfg.findings_file_for_profile(ClientProfile::BrowserDesktop),
            PathBuf::from("reports/findings-browser-desktop.txt")
        );
    }

    #[test]
    fn for_profile_recalculates_adjusted_timing() {
        let cfg = AppConfig {
            base_url: Url::parse("http://127.0.0.1:8000").unwrap(),
            wordlist: "wordlist.txt".into(),
            workers: 1,
            rounds: 1,
            recursion_depth: 0,
            requested_timeout_ms: 1000,
            timeout_ms: 1000,
            requested_initial_delay_ms: 50,
            initial_delay_ms: 50,
            rebuild_client_every: None,
            rebuild_client_on_advisory: false,
            min_delay_ms: 25,
            max_delay_ms: 250,
            latency_threshold_ms: 500,
            findings_file: "findings.txt".into(),
            interesting_statuses: vec![200],
            min_body_bytes: 0,
            max_body_bytes: None,
            disable_soft_404_filter: false,
            speed_mode: SpeedMode::Balanced,
            client_profile: ClientProfile::ResearchDefault,
            simulation_mode: false,
            scenario: None,
            compare_profiles: false,
            compare_profiles_live: true,
        };

        let profiled = cfg.for_profile(ClientProfile::MobileSafari);
        assert_eq!(profiled.timeout_ms, 1750);
        assert_eq!(profiled.initial_delay_ms, 90);
        assert_eq!(profiled.client_profile, ClientProfile::MobileSafari);
    }

    #[test]
    fn status_list_parser_deduplicates_and_validates() -> Result<()> {
        let statuses = parse_status_list("200, 403, 200, 500")?;
        assert_eq!(statuses, vec![200, 403, 500]);
        assert!(parse_status_list("700").is_err());
        Ok(())
    }
}
