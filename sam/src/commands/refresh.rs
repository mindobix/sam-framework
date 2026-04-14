use std::path::Path;
use sam_core::{finder, output};

pub fn run(repo: &Path) -> anyhow::Result<()> {
    let mut spinner = output::Spinner::new("Refreshing Finder tags...");
    let updated = finder::refresh_tags(repo)?;
    spinner.stop_with_message(&format!(
        "{} Updated tags on {} domains",
        output::success("✓"),
        updated
    ));

    Ok(())
}
