use std::path::Path;
use sam_core::{finder, output};

pub fn run(repo: &Path) -> anyhow::Result<()> {
    eprintln!("{}", output::header("Setting up Finder integration..."));

    let mut spinner = output::Spinner::new("Creating skeleton directories for all domains...");
    let (total, created) = finder::setup_skeleton_dirs(repo)?;
    spinner.stop_with_message(&format!(
        "{} {} total domains — {} skeleton dirs created, {} already on disk",
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

    Ok(())
}
