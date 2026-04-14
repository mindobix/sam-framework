use std::path::Path;
use std::time::Duration;

use sam_core::monograph::{self, Client, ResolveRequest};
use sam_core::output;
use sam_core::profile;

pub fn run(repo: &Path, domain: &str) -> anyhow::Result<()> {
    // Validate domain exists
    if !sam_core::git::domain_exists(repo, domain)? {
        anyhow::bail!(
            "domain '{}' does not exist in this repository.",
            domain
        );
    }

    let repo_config = profile::load_repo_config(repo)?;

    // Try to load profile auto_include if there's an active profile
    let state = sam_core::workspace::load(repo)?;
    let auto_include = if let Some(ref profile_name) = state.active_profile {
        let profiles = profile::load_profiles(repo)?;
        if let Ok(prof) = profile::get_profile(&profiles, profile_name) {
            prof.auto_include.clone()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    eprintln!(
        "{}",
        output::header(&format!("Plan: {}", output::domain(domain)))
    );

    // Resolve
    let mut spinner = output::Spinner::new("Resolving dependencies...");
    let client = Client::new(&repo_config.monograph.address, Duration::from_secs(2));
    let req = ResolveRequest {
        domains: vec![domain.to_string()],
        auto_include: auto_include.clone(),
        ai_infer: true,
        cochange_commits: Some(repo_config.monograph.cochange_commits),
        cochange_min_score: Some(repo_config.monograph.cochange_min_score),
    };

    let (resolved, used_fallback) = match client.resolve(&req) {
        Ok(resp) => (resp, false),
        Err(_) => (
            monograph::fallback_resolve(&[domain.to_string()], &auto_include),
            true,
        ),
    };

    if used_fallback {
        spinner.stop_with_message(&output::warn("MonoGraph unavailable — showing static resolution only"));
    } else {
        spinner.stop_with_message(&output::success("Dependencies resolved"));
    }

    // Build table
    let mut table = output::Table::new(vec!["Domain", "Source", "Files", "Status"]);
    let mut total_files: usize = 0;

    for rd in &resolved.domains {
        let status = if state.has_domain(&rd.path) {
            "hydrated".to_string()
        } else {
            "\x1b[33mnot hydrated\x1b[0m".to_string()
        };

        let files = rd.file_count.unwrap_or_else(|| {
            sam_core::git::count_tree_files(repo, &rd.path).unwrap_or(0)
        });
        total_files += files;

        let source = match rd.reason.as_str() {
            "co_change" => {
                if let Some(s) = rd.score {
                    format!("\x1b[33mco_change ({:.2})\x1b[0m", s)
                } else {
                    "\x1b[33mco_change\x1b[0m".to_string()
                }
            }
            "ai_inferred" => "\x1b[35mai_inferred\x1b[0m".to_string(),
            other => other.to_string(),
        };

        table.add_row(vec![
            output::domain(&rd.path),
            source,
            output::format_number(files),
            status,
        ]);
    }

    eprintln!("{}", table.render());

    eprintln!(
        "{}",
        output::info(&format!(
            "`sam fetch --with-deps {}` would hydrate {} domains ({} files)",
            domain,
            resolved.domains.len(),
            output::format_number(total_files)
        ))
    );

    if used_fallback {
        eprintln!(
            "{}",
            output::hint("Start MonoGraph for AI-resolved dependencies: monograph serve --port 7474")
        );
    }

    Ok(())
}
