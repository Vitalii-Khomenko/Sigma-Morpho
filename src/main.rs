use anyhow::{bail, Result};
use sigma_morpho::network::profile::ClientProfile;
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

        if config.compare_profiles_live {
            println!("[*] Sigma Morpho live profile comparison started");
            println!("[*] Target: {}", config.base_url);
            println!("[*] Scan mode: {}", config.scan_mode.as_str());
            if let Some(template) = &config.vhost_template {
                println!("[*] VHost template: {}", template);
            }
            if let Some(rate) = config.rate_per_second {
                println!("[*] Rate limit: {} req/s", rate);
            }
            println!(
                "[*] Workload: {} seed paths x {} rounds, workers: {}, recursion depth: {}",
                paths.len(),
                config.rounds,
                config.workers,
                config.recursion_depth
            );

            let mut outcomes = Vec::new();
            for profile in ClientProfile::ALL {
                let profile_config = config.for_profile(profile);
                let profile_started = Instant::now();
                println!("[*] Running profile: {}", profile.as_str());
                let summary = core::engine::run(profile_config, paths.clone()).await?;
                println!("{}", summary.render(profile_started.elapsed()));
                outcomes.push(core::research::ProfileOutcome { profile, summary });
            }

            let report = core::research::LiveProfileReport {
                target: config.base_url.to_string(),
                outcomes,
            };
            println!("{}", report.render());
        } else {
            println!("[*] Sigma Morpho started");
            println!("[*] Target: {}", config.base_url);
            println!("[*] Client profile: {}", config.client_profile.as_str());
            println!("[*] Scan mode: {}", config.scan_mode.as_str());
            if let Some(template) = &config.vhost_template {
                println!("[*] VHost template: {}", template);
            }
            if let Some(rate) = config.rate_per_second {
                println!("[*] Rate limit: {} req/s", rate);
            }
            println!(
                "[*] Workload: {} seed paths x {} rounds, workers: {}, recursion depth: {}",
                paths.len(),
                config.rounds,
                config.workers,
                config.recursion_depth
            );

            let summary = core::engine::run(config, paths).await?;
            let elapsed = started.elapsed();
            println!("{}", summary.render(elapsed));
        }
    }
    Ok(())
}
