use std::path::Path;
use sam_core::{finder, output};

pub fn run(repo: &Path, domain: Option<&str>, all: bool) -> anyhow::Result<()> {
    if all {
        let mut spinner = output::Spinner::new("Dehydrating all domains...");
        let count = finder::dehydrate_all(repo)?;
        spinner.stop_with_message(&format!(
            "{} Dehydrated {} domains — all folders reset to ghost state",
            output::success("\u{2713}"),
            count
        ));
        return Ok(());
    }

    let domain = domain.ok_or_else(|| anyhow::anyhow!("specify a domain or use --all"))?;

    let ws = sam_core::workspace::load(repo)?;
    if !ws.has_domain(domain) {
        eprintln!("{}", output::info(&format!("{} is not hydrated.", domain)));
        return Ok(());
    }

    let mut spinner = output::Spinner::new(&format!("Dehydrating {}...", domain));
    finder::dehydrate_domain(repo, domain)?;
    spinner.stop_with_message(&format!(
        "{} {} dehydrated",
        output::success("\u{2713}"),
        output::domain(domain)
    ));

    Ok(())
}
