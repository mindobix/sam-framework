use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use sam_core::output;

pub fn run(url: &str) -> anyhow::Result<()> {
    let cwd = env::current_dir()?;
    let dir_name = sam_core::git::dir_from_url(url);
    let dest = cwd.join(&dir_name);

    eprintln!("{}", output::header("Initializing SAM workspace"));

    if dest.exists() {
        anyhow::bail!("directory '{}' already exists", dest.display());
    }

    // Clone
    let mut spinner = output::Spinner::new("Cloning repository (blobless)...");
    sam_core::git::clone(url, &dest)?;
    spinner.stop_with_message(&output::success("Repository cloned"));

    // Sparse checkout
    let mut spinner = output::Spinner::new("Configuring sparse checkout...");
    sam_core::git::init_sparse(&dest)?;
    // Always include .sam/ and root files in sparse checkout
    sam_core::git::add_sparse(&dest, &[".sam".to_string()])?;
    spinner.stop_with_message(&output::success("Sparse checkout configured"));

    // Check if the repo already has a committed .sam/profiles.yaml
    let sam_dir = dest.join(".sam");
    let profiles_path = sam_dir.join("profiles.yaml");
    if profiles_path.exists() {
        eprintln!("{}", output::info("Using existing .sam/profiles.yaml from repo"));
    } else {
        // No profiles in the repo — create .sam/ and an example
        fs::create_dir_all(&sam_dir)?;
        fs::write(sam_dir.join(".gitignore"), "graph.json\nworkspace.yaml\n")?;
        fs::write(
            &profiles_path,
            r#"# SAM profiles — defines which domains each team works on.
# See: https://github.com/sam-framework/sam/docs/profiles.md
profiles:
  example:
    domains:
      - apis/example
    auto_include:
      - shared/types
    ai_infer: true
"#,
        )?;
        eprintln!("{}", output::info("Created example .sam/profiles.yaml"));
    }

    // Initialize empty workspace state
    let state = sam_core::workspace::State::default();
    sam_core::workspace::save(&dest, &state)?;
    eprintln!("{}", output::info("Initialized workspace state"));

    // Try to install monowatch pre-push hook
    install_monowatch_hook(&dest);

    // Try to start MonoGraph
    try_start_monograph(&dest);

    // Success
    eprintln!("{}", output::success(&format!("SAM workspace ready at {}", dest.display())));
    eprintln!("{}", output::hint("Next: cd into the repo and run `sam use --profile <name>`"));
    eprintln!(
        "{}",
        output::hint(&format!(
            "Available profiles: edit .sam/profiles.yaml to configure your team's domains"
        ))
    );

    Ok(())
}

fn install_monowatch_hook(repo: &Path) {
    // Check if monowatch is in PATH
    let has_monowatch = Command::new("which")
        .arg("monowatch")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_monowatch {
        eprintln!(
            "{}",
            output::hint("Install monowatch for pre-push impact analysis: brew install sam-framework/tap/monowatch")
        );
        return;
    }

    let hooks_dir = repo.join(".git").join("hooks");
    if !hooks_dir.exists() {
        let _ = fs::create_dir_all(&hooks_dir);
    }

    let hook_path = hooks_dir.join("pre-push");
    let hook_content = r#"#!/bin/sh
# SAM MonoWatch pre-push hook — runs impact analysis before push.
# Installed by `sam init`. Remove this file to disable.
if command -v monowatch >/dev/null 2>&1; then
    monowatch check
fi
"#;

    match fs::write(&hook_path, hook_content) {
        Ok(()) => {
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755));
            }
            eprintln!("{}", output::success("Installed MonoWatch pre-push hook"));
        }
        Err(_) => {
            eprintln!(
                "{}",
                output::warn("Could not install pre-push hook — you can add it manually")
            );
        }
    }
}

fn try_start_monograph(repo: &Path) {
    // Check if MonoGraph is already running
    let client = sam_core::monograph::Client::new(
        "127.0.0.1:7474",
        std::time::Duration::from_secs(1),
    );
    if client.health() {
        eprintln!("{}", output::info("MonoGraph daemon is already running"));
        return;
    }

    // Try to start it in background
    let has_monograph = Command::new("which")
        .arg("monograph")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_monograph {
        eprintln!(
            "{}",
            output::hint("Install monograph for AI dependency resolution: brew install sam-framework/tap/monograph")
        );
        return;
    }

    match Command::new("monograph")
        .args(["serve", "--port", "7474"])
        .current_dir(repo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            eprintln!("{}", output::info("Started MonoGraph daemon on port 7474"));
        }
        Err(_) => {
            eprintln!(
                "{}",
                output::hint("Could not start MonoGraph — run `monograph serve` manually for AI features")
            );
        }
    }
}
