use std::path::Path;
use sam_core::{finder, output};

pub fn run(repo: &Path, max_depth: usize) -> anyhow::Result<()> {
    eprintln!("{}", output::header(&format!("Setting up Finder integration (depth: {})...", max_depth)));

    let mut spinner = output::Spinner::new("Creating skeleton directories...");
    let (total, created) = finder::setup_skeleton_dirs(repo, max_depth)?;
    spinner.stop_with_message(&format!(
        "{} {} directories — {} created, {} already on disk",
        output::success("✓"),
        total,
        created,
        total - created
    ));

    eprintln!();
    eprintln!(
        "{}",
        output::info("Open the repo in Finder — dimmed folders are not yet hydrated.")
    );
    eprintln!(
        "{}",
        output::hint("Double-click any dimmed folder in Finder to hydrate it with dependencies.")
    );
    eprintln!(
        "{}",
        output::hint("To dehydrate: sam dehydrate <domain>")
    );
    eprintln!(
        "{}",
        output::hint(&format!("To change depth: sam setup --depth N (current: {})", max_depth))
    );

    Ok(())
}
