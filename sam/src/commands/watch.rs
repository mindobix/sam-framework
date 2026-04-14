use std::path::Path;
use sam_core::{finder, output};

pub fn run(repo: &Path) -> anyhow::Result<()> {
    eprintln!("{}", output::header("SAM Watch — auto-hydrate on Finder navigation"));
    eprintln!();
    finder::watch_and_hydrate(repo)?;
    Ok(())
}
