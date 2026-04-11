use anyhow::{bail, Result};
use sigma_morpho::{cli, core};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let config = cli::AppConfig::from_args()?;
    let started = Instant::now();
    if let Some(scenario) = config.scenario {
        println!("[*] Sigma Morpho scenario mode started");
        println!("[*] Scenario: {}", scenario.as_str());
        if config.compare_profiles {
            println!("[*] Comparing all fixed client profiles");
        } else {
            println!("[*] Client profile: {}", config.client_profile.as_str());
        }

        let report = core::research::run_scenario(&config);
        println!("{}", report.render());
    } else {
        let paths = config.load_wordlist()?;

        if paths.is_empty() {
            bail!("Wordlist is empty after filtering blank/comment lines.");
        }

        println!("[*] Sigma Morpho started");
        println!("[*] Target: {}", config.base_url);
        println!("[*] Client profile: {}", config.client_profile.as_str());
        println!(
            "[*] Workload: {} paths x {} rounds, workers: {}",
            paths.len(),
            config.rounds,
            config.workers
        );

        let summary = core::engine::run(config, paths).await?;
        let elapsed = started.elapsed();
        println!("{}", summary.render(elapsed));
    }
    Ok(())
}
