use std::path::Path;
use std::time::Duration;

use sam_core::monograph::{self, Client, ResolveRequest, ResolveResponse};
use sam_core::output;
use sam_core::profile::{self, Domains};

pub fn run(repo: &Path, profile_name: &str, dry_run: bool) -> anyhow::Result<()> {
    let profiles_config = profile::load_profiles(repo)?;
    let repo_config = profile::load_repo_config(repo)?;
    let resolved_name = profile::resolve_profile_name(&profiles_config, profile_name)?;
    let prof = profile::get_profile(&profiles_config, profile_name)?;

    eprintln!(
        "{}",
        output::header(&format!("Activating profile: {}", output::bold(resolved_name)))
    );

    // Handle wildcard ("*") domains
    if matches!(prof.domains, Domains::All) {
        if dry_run {
            eprintln!(
                "{}",
                output::info("Profile uses domains: \"*\" — would disable sparse checkout (full repo)")
            );
            return Ok(());
        }
        let mut spinner = output::Spinner::new("Disabling sparse checkout (full repo)...");
        sam_core::git::disable_sparse(repo)?;
        spinner.stop_with_message(&output::success("Sparse checkout disabled — full repo materialized"));

        let mut state = sam_core::workspace::load(repo)?;
        state.set_profile(resolved_name);
        sam_core::workspace::save(repo, &state)?;

        eprintln!(
            "{}",
            output::success(&format!("Profile '{}' activated (full monolith)", resolved_name))
        );
        return Ok(());
    }

    // Get domain list from profile
    let domain_list = match &prof.domains {
        Domains::List(list) => list.clone(),
        Domains::All => unreachable!(),
    };

    // Resolve dependencies
    let mut spinner = output::Spinner::new("Resolving dependencies...");
    let (resolved, used_fallback) = resolve_domains(
        &repo_config,
        &domain_list,
        &prof.auto_include,
        prof.ai_infer,
    );
    if used_fallback {
        spinner.stop_with_message(&output::warn(
            "MonoGraph unavailable — using static profile resolution (no AI deps)",
        ));
    } else {
        spinner.stop_with_message(&output::success("Dependencies resolved via MonoGraph"));
    }

    // Load workspace to check hydration status
    let state = sam_core::workspace::load(repo)?;

    // Build display table
    let mut table = output::Table::new(vec!["Domain", "Source", "Files", "Status"]);
    for rd in &resolved.domains {
        let status = if state.has_domain(&rd.path) {
            "hydrated".to_string()
        } else {
            "\x1b[33mnew\x1b[0m".to_string()
        };
        let files = rd
            .file_count
            .map(|c| output::format_number(c))
            .unwrap_or_else(|| "\u{2014}".to_string());
        let source = format_source(&rd.reason, rd.score);
        table.add_row(vec![
            output::domain(&rd.path),
            source,
            files,
            status,
        ]);
    }

    eprintln!("{}", table.render());

    if dry_run {
        eprintln!(
            "{}",
            output::info(&format!(
                "Dry run: would hydrate {} domains",
                resolved.domains.len()
            ))
        );
        eprintln!(
            "{}",
            output::hint("Run without --dry-run to apply")
        );
        return Ok(());
    }

    // Hydrate domains
    let all_paths: Vec<String> = resolved.domains.iter().map(|d| d.path.clone()).collect();
    let mut spinner = output::Spinner::new("Hydrating domains...");
    sam_core::git::set_sparse(repo, &all_paths)?;
    spinner.stop_with_message(&output::success(&format!(
        "Hydrated {} domains",
        all_paths.len()
    )));

    // Update workspace state
    let mut state = sam_core::workspace::load(repo)?;
    state.set_profile(resolved_name);
    state.hydrated_domains.clear();
    for d in &all_paths {
        state.add_domain(d);
    }
    sam_core::workspace::save(repo, &state)?;

    eprintln!(
        "{}",
        output::success(&format!(
            "Profile '{}' activated ({} domains)",
            resolved_name,
            all_paths.len()
        ))
    );

    Ok(())
}

fn resolve_domains(
    repo_config: &profile::RepoConfig,
    domains: &[String],
    auto_include: &[String],
    ai_infer: bool,
) -> (ResolveResponse, bool) {
    let client = Client::new(
        &repo_config.monograph.address,
        Duration::from_secs(2),
    );

    let req = ResolveRequest {
        domains: domains.to_vec(),
        auto_include: auto_include.to_vec(),
        ai_infer,
        cochange_commits: Some(repo_config.monograph.cochange_commits),
        cochange_min_score: Some(repo_config.monograph.cochange_min_score),
    };

    match client.resolve(&req) {
        Ok(resp) => (resp, false),
        Err(_) => (monograph::fallback_resolve(domains, auto_include), true),
    }
}

fn format_source(reason: &str, score: Option<f64>) -> String {
    match reason {
        "profile" => reason.to_string(),
        "auto_include" => "auto_include".to_string(),
        "ai_inferred" => "\x1b[35mai_inferred\x1b[0m".to_string(),
        "co_change" => {
            if let Some(s) = score {
                format!("\x1b[33mco_change ({:.2})\x1b[0m", s)
            } else {
                "\x1b[33mco_change\x1b[0m".to_string()
            }
        }
        other => other.to_string(),
    }
}
