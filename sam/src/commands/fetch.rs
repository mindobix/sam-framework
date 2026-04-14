use std::path::Path;
use std::time::Duration;

use sam_core::monograph::{self, Client, ResolveRequest};
use sam_core::output;
use sam_core::profile;

pub fn run(repo: &Path, domain: &str, with_deps: bool, force: bool) -> anyhow::Result<()> {
    // Validate domain exists in the git tree
    if !sam_core::git::domain_exists(repo, domain)? {
        anyhow::bail!(
            "domain '{}' does not exist in this repository. Run `git ls-tree --name-only -d HEAD` to see available paths.",
            domain
        );
    }

    let mut state = sam_core::workspace::load(repo)?;

    // Check if already hydrated
    if state.has_domain(domain) && !force {
        let count = sam_core::git::count_files(repo, domain)?;
        eprintln!(
            "{}",
            output::info(&format!(
                "{} is already hydrated ({} files). Use --force to re-fetch.",
                output::domain(domain),
                output::format_number(count)
            ))
        );
        return Ok(());
    }

    if with_deps {
        // Resolve dependencies then hydrate all
        let repo_config = profile::load_repo_config(repo)?;
        let mut spinner = output::Spinner::new(&format!("Resolving dependencies for {}...", domain));

        let client = Client::new(&repo_config.monograph.address, Duration::from_secs(2));
        let req = ResolveRequest {
            domains: vec![domain.to_string()],
            auto_include: Vec::new(),
            ai_infer: true,
            cochange_commits: Some(repo_config.monograph.cochange_commits),
            cochange_min_score: Some(repo_config.monograph.cochange_min_score),
        };

        let (resolved, used_fallback) = match client.resolve(&req) {
            Ok(resp) => (resp, false),
            Err(_) => (
                monograph::fallback_resolve(&[domain.to_string()], &[]),
                true,
            ),
        };

        if used_fallback {
            spinner.stop_with_message(&output::warn("MonoGraph unavailable — fetching domain only"));
        } else {
            spinner.stop_with_message(&output::success(&format!(
                "Resolved {} domains",
                resolved.domains.len()
            )));
        }

        let all_paths: Vec<String> = resolved.domains.iter().map(|d| d.path.clone()).collect();
        let mut spinner = output::Spinner::new("Hydrating...");
        sam_core::git::add_sparse(repo, &all_paths)?;
        for d in &all_paths {
            let _ = sam_core::git::checkout_domain(repo, d);
        }
        spinner.stop_with_message(&output::success("Domains hydrated"));

        for d in &all_paths {
            state.add_domain(d);
        }
    } else {
        // Single domain fetch
        let mut spinner = output::Spinner::new(&format!("Fetching {}...", domain));
        sam_core::git::add_sparse(repo, &[domain.to_string()])?;
        let _ = sam_core::git::checkout_domain(repo, domain);
        spinner.stop_with_message(&output::success(&format!("Fetched {}", output::domain(domain))));
        state.add_domain(domain);
    }

    sam_core::workspace::save(repo, &state)?;

    // Clear Finder gray tag now that domain is hydrated
    let _ = sam_core::finder::mark_hydrated(repo, domain);

    let count = sam_core::git::count_files(repo, domain)?;
    eprintln!(
        "{}",
        output::success(&format!(
            "{} ready ({} files)",
            output::domain(domain),
            output::format_number(count)
        ))
    );

    Ok(())
}
