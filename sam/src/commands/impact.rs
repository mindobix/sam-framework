use std::path::Path;
use std::time::Duration;

use sam_core::monograph::Client;
use sam_core::output;
use sam_core::profile;

pub fn run(repo: &Path, format: &str) -> anyhow::Result<()> {
    let changed = sam_core::git::changed_files(repo)?;

    if changed.is_empty() {
        eprintln!("{}", output::info("No changed files detected."));
        return Ok(());
    }

    eprintln!(
        "{}",
        output::header(&format!("Impact analysis ({} changed files)", changed.len()))
    );

    // List changed files
    for f in &changed {
        eprintln!("  {}", output::domain(f));
    }
    eprintln!();

    let repo_config = profile::load_repo_config(repo)?;
    let client = Client::new(&repo_config.monograph.address, Duration::from_secs(5));

    let mut spinner = output::Spinner::new("Analyzing impact...");

    let impact = match client.impact(&changed) {
        Ok(resp) => {
            spinner.stop_with_message(&output::success("Impact analysis complete"));
            resp
        }
        Err(e) => {
            spinner.stop_with_message(&output::error_msg(&format!(
                "MonoGraph unavailable: {e}"
            )));
            eprintln!(
                "{}",
                output::hint("Start MonoGraph for impact analysis: monograph serve --port 7474")
            );
            return Ok(());
        }
    };

    if impact.entries.is_empty() {
        eprintln!("{}", output::info("No affected domains detected."));
        return Ok(());
    }

    // JSON output goes to stdout
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&impact)?);
        return Ok(());
    }

    // Table output goes to stderr
    let mut table = output::Table::new(vec!["Domain", "Risk", "Calls/Day", "Teams"]);
    for entry in &impact.entries {
        let risk = output::colored_risk(&entry.risk);
        let calls = entry
            .calls_per_day
            .map(|c| output::format_number(c as usize))
            .unwrap_or_else(|| "\u{2014}".to_string());
        let teams = if entry.teams.is_empty() {
            "\u{2014}".to_string()
        } else {
            entry.teams.join(", ")
        };
        table.add_row(vec![output::domain(&entry.domain), risk, calls, teams]);
    }

    eprintln!("{}", table.render());

    // Summary
    let critical_count = impact
        .entries
        .iter()
        .filter(|e| e.risk.to_uppercase() == "CRITICAL")
        .count();
    let high_count = impact
        .entries
        .iter()
        .filter(|e| e.risk.to_uppercase() == "HIGH")
        .count();

    if critical_count > 0 {
        eprintln!(
            "{}",
            output::warn(&format!(
                "{} critical-risk domain(s) affected",
                critical_count
            ))
        );
    }
    if high_count > 0 {
        eprintln!(
            "{}",
            output::warn(&format!("{} high-risk domain(s) affected", high_count))
        );
    }

    // Always exit 0 — MonoWatch handles blocking
    Ok(())
}
