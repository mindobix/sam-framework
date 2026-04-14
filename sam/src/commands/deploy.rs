use std::path::Path;
use std::process::Command;

use sam_core::output;
use sam_core::profile::{self, Domains};

pub fn run(repo: &Path, profile_name: &str) -> anyhow::Result<()> {
    let profiles_config = profile::load_profiles(repo)?;
    let resolved_name = profile::resolve_profile_name(&profiles_config, profile_name)?;
    let prof = profile::get_profile(&profiles_config, profile_name)?;

    let deploy_config = prof.deploy.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "profile '{}' has no deploy configuration. Add a `deploy:` section to .sam/profiles.yaml.",
            resolved_name
        )
    })?;

    eprintln!(
        "{}",
        output::header(&format!("Deploying profile: {}", output::bold(resolved_name)))
    );

    // Get domain list
    let domain_list: Vec<String> = match &prof.domains {
        Domains::All => {
            eprintln!(
                "{}",
                output::info("Profile uses domains: \"*\" — deploying all")
            );
            vec!["*".to_string()]
        }
        Domains::List(list) => list.clone(),
    };

    // Warn about non-hydrated domains
    let state = sam_core::workspace::load(repo)?;
    for d in &domain_list {
        if d != "*" && !state.has_domain(d) {
            eprintln!(
                "{}",
                output::warn(&format!(
                    "{} is not hydrated — deploying from non-local state",
                    output::domain(d)
                ))
            );
        }
    }

    // Pre-deploy impact check
    if deploy_config.pre_deploy_impact {
        eprintln!("{}", output::info("Running pre-deploy impact check..."));
        let changed = sam_core::git::changed_files(repo)?;
        if !changed.is_empty() {
            let repo_config = profile::load_repo_config(repo)?;
            let client = sam_core::monograph::Client::new(
                &repo_config.monograph.address,
                std::time::Duration::from_secs(5),
            );
            match client.impact(&changed) {
                Ok(impact) => {
                    let critical = impact
                        .entries
                        .iter()
                        .any(|e| e.risk.to_uppercase() == "CRITICAL");
                    if critical {
                        eprintln!(
                            "{}",
                            output::error_msg("Critical-risk impacts detected. Aborting deploy.")
                        );
                        eprintln!(
                            "{}",
                            output::hint("Run `sam impact` to see details. Use deploy config `pre_deploy_impact: false` to skip.")
                        );
                        anyhow::bail!("deploy aborted due to critical impact");
                    }
                    eprintln!("{}", output::success("Impact check passed"));
                }
                Err(_) => {
                    eprintln!(
                        "{}",
                        output::warn("MonoGraph unavailable — skipping impact check")
                    );
                }
            }
        } else {
            eprintln!("{}", output::info("No changed files — skipping impact check"));
        }
    }

    // Run deploy command(s)
    if deploy_config.per_domain {
        // Run once per domain, substituting {domain}
        let mut failures = Vec::new();
        for d in &domain_list {
            let cmd_str = deploy_config.command.replace("{domain}", d);
            eprintln!(
                "{}",
                output::info(&format!("Deploying {}...", output::domain(d)))
            );
            let success = run_deploy_command(&cmd_str, d, resolved_name, repo)?;
            if success {
                eprintln!(
                    "{}",
                    output::success(&format!("Deployed {}", output::domain(d)))
                );
            } else {
                eprintln!(
                    "{}",
                    output::error_msg(&format!("Deploy failed for {}", output::domain(d)))
                );
                failures.push(d.clone());
            }
        }

        if failures.is_empty() {
            eprintln!(
                "{}",
                output::success(&format!(
                    "All {} domains deployed successfully",
                    domain_list.len()
                ))
            );
        } else {
            anyhow::bail!(
                "deploy failed for {} domain(s): {}",
                failures.len(),
                failures.join(", ")
            );
        }
    } else {
        // Run command once
        let domain_str = domain_list.join(",");
        eprintln!("{}", output::info("Running deploy command..."));
        let success = run_deploy_command(
            &deploy_config.command,
            &domain_str,
            resolved_name,
            repo,
        )?;
        if success {
            eprintln!("{}", output::success("Deploy completed successfully"));
        } else {
            anyhow::bail!("deploy command failed");
        }
    }

    Ok(())
}

fn run_deploy_command(
    cmd: &str,
    domain: &str,
    profile: &str,
    repo: &Path,
) -> anyhow::Result<bool> {
    let status = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(repo)
        .env("SAM_DOMAIN", domain)
        .env("SAM_PROFILE", profile)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;

    Ok(status.success())
}
