//! macOS Finder integration — pure Rust, no Swift, no Xcode.
//!
//! Ghost folders: `chflags hidden` (dimmed text in Finder).
//! `sam watch`: polls Finder's front window path, hydrates on navigation.
//! `sam dehydrate`: terminal only.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::{Result, SamError};
use crate::git;
use crate::workspace;

fn set_hidden(path: &Path, hidden: bool) {
    let flag = if hidden { "hidden" } else { "nohidden" };
    let _ = Command::new("chflags").arg(flag).arg(path).output();
}

fn find_binary(name: &str) -> String {
    for dir in &["/usr/local/bin", "/opt/homebrew/bin"] {
        let p = format!("{}/{}", dir, name);
        if Path::new(&p).exists() { return p; }
    }
    if let Ok(output) = Command::new("which").arg(name).output() {
        if output.status.success() {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !p.is_empty() { return p; }
        }
    }
    name.to_string()
}

fn get_finder_current_path() -> Option<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg("try\ntell application \"Finder\" to return POSIX path of (target of front window as alias)\non error\nreturn \"\"\nend try")
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() { return Some(path); }
    }
    None
}

fn resolve_with_deps(repo: &Path, domain: &str) -> Vec<String> {
    use crate::monograph;
    use crate::profile;

    let address = profile::load_repo_config(repo)
        .map(|c| c.monograph.address.clone())
        .unwrap_or_else(|_| "127.0.0.1:7474".to_string());

    let client = monograph::Client::new(&address, std::time::Duration::from_secs(5));
    if !client.health() {
        return vec![domain.to_string()];
    }

    let auto_include = profile::load_profiles(repo)
        .ok()
        .and_then(|profiles| {
            let ws = workspace::load(repo).ok()?;
            let name = ws.active_profile.as_ref()?;
            let prof = profile::get_profile(&profiles, name).ok()?;
            Some(prof.auto_include.clone())
        })
        .unwrap_or_default();

    let req = monograph::ResolveRequest {
        domains: vec![domain.to_string()],
        auto_include,
        ai_infer: true,
        cochange_commits: None,
        cochange_min_score: None,
    };

    match client.resolve(&req) {
        Ok(resp) => {
            let mut paths: Vec<String> = resp.domains.iter().map(|d| d.path.clone()).collect();
            if paths.is_empty() { paths.push(domain.to_string()); }
            paths
        }
        Err(_) => vec![domain.to_string()],
    }
}

fn ensure_monograph_running(repo: &Path) {
    let health = Command::new("curl")
        .args(["-s", "--max-time", "1", "http://127.0.0.1:7474/health"])
        .output();
    if health.map(|o| o.status.success()).unwrap_or(false) { return; }

    let uv_bin = find_binary("uv");
    let sam = find_binary("sam");
    let sam_path = Path::new(&sam);
    let monograph_dir = if let Some(parent) = sam_path.parent() {
        let candidate = parent.join("../../monograph");
        if candidate.join("pyproject.toml").exists() {
            candidate.to_string_lossy().to_string()
        } else { String::new() }
    } else { String::new() };

    if monograph_dir.is_empty() { return; }

    let repo_str = repo.to_string_lossy().to_string();
    let _ = Command::new(&uv_bin)
        .args(["run", "monograph", "serve", "--port", "7474"])
        .current_dir(&monograph_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    std::thread::sleep(std::time::Duration::from_secs(3));

    let _ = Command::new("curl")
        .args(["-s", "-X", "POST", "http://127.0.0.1:7474/analyze",
               "-H", "Content-Type: application/json",
               "-d", &format!("{{\"repo_path\": \"{}\"}}", repo_str)])
        .output();
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create ghost folders for all directories up to `max_depth` levels.
/// Container dirs (only subdirs, no files) stay visible for navigation.
/// Leaf project dirs (contain files) get dimmed via hidden flag.
pub fn setup_skeleton_dirs(repo: &Path, max_depth: usize) -> Result<(usize, usize)> {
    let all_dirs = git::list_all_dirs(repo, max_depth)?;
    let ws = workspace::load(repo).unwrap_or_default();
    let mut created = 0usize;

    for dir in &all_dirs {
        let dp = repo.join(dir);

        if ws.has_domain(dir) {
            // Hydrated — ensure visible along with parents
            unhide_with_parents(repo, dir);
        } else {
            // Not hydrated — create skeleton
            if !dp.exists() {
                fs::create_dir_all(&dp).map_err(SamError::IoError)?;
                created += 1;
            }

            // Only hide leaf dirs (contain files). Container dirs stay visible.
            let is_leaf = git::dir_has_files(repo, dir).unwrap_or(false);
            if is_leaf {
                set_hidden(&dp, true);
            }
            // Pure container dirs (only subdirs) stay visible for navigation
        }
    }

    let _ = Command::new("defaults")
        .args(["write", "com.apple.finder", "AppleShowAllFiles", "-bool", "true"])
        .output();

    ensure_monograph_running(repo);

    Ok((all_dirs.len(), created))
}

/// Unhide a directory AND all its parent directories.
fn unhide_with_parents(repo: &Path, dir: &str) {
    // Unhide the target
    set_hidden(&repo.join(dir), false);

    // Unhide all parent dirs
    let parts: Vec<&str> = dir.split('/').collect();
    for i in 1..parts.len() {
        let parent = parts[..i].join("/");
        set_hidden(&repo.join(&parent), false);
    }
}

/// Clear hidden flag after hydration — unhides target + parents.
pub fn mark_hydrated(repo: &Path, domain: &str) -> Result<()> {
    unhide_with_parents(repo, domain);
    Ok(())
}

/// Refresh hidden flags based on workspace state.
pub fn refresh_tags(repo: &Path) -> Result<usize> {
    let all_dirs = git::list_all_dirs(repo, git::DEFAULT_MAX_DEPTH)?;
    let ws = workspace::load(repo).unwrap_or_default();
    let mut updated = 0;
    for dir in &all_dirs {
        let dp = repo.join(dir);
        if !dp.exists() { continue; }
        if ws.has_domain(dir) {
            unhide_with_parents(repo, dir);
        } else {
            let is_leaf = git::dir_has_files(repo, dir).unwrap_or(false);
            if is_leaf {
                set_hidden(&dp, true);
            }
        }
        updated += 1;
    }
    Ok(updated)
}

/// Dehydrate a single domain (terminal only).
pub fn dehydrate_domain(repo: &Path, domain: &str) -> Result<()> {
    // Update workspace first
    if let Ok(mut ws) = workspace::load(repo) {
        ws.hydrated_domains.retain(|d| d != domain);
        let _ = workspace::save(repo, &ws);
    }

    // Remove from sparse checkout (git removes the files)
    git::remove_sparse(repo, domain)?;

    // Ensure skeleton dir exists and is hidden
    let dp = repo.join(domain);
    if !dp.exists() {
        fs::create_dir_all(&dp).map_err(SamError::IoError)?;
    }
    set_hidden(&dp, true);

    Ok(())
}

/// Dehydrate all hydrated domains.
pub fn dehydrate_all(repo: &Path) -> Result<usize> {
    let ws = workspace::load(repo).unwrap_or_default();
    let count = ws.hydrated_domains.len();
    for domain in &ws.hydrated_domains.clone() {
        let _ = dehydrate_domain(repo, domain);
    }
    Ok(count)
}

/// Watch Finder's current path. Hydrate ghost folders on navigation.
/// Works at any directory depth.
pub fn watch_and_hydrate(repo: &Path) -> Result<()> {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    let all_dirs = git::list_all_dirs(repo, git::DEFAULT_MAX_DEPTH)?;
    let all_set: HashSet<String> = all_dirs.into_iter().collect();

    let repo_str = repo.to_string_lossy().to_string();
    let repo_prefix = format!("{}/", repo_str);

    let poll = Duration::from_millis(500);
    let refresh_interval = Duration::from_secs(2);
    let mut last_path = String::new();
    let mut last_refresh = Instant::now();
    let mut ghosts = rebuild_ghosts(repo, &all_set);

    eprintln!(
        "Watching {} ghost folders (up to {} levels deep). Navigate into one in Finder to hydrate it.",
        ghosts.len(), git::DEFAULT_MAX_DEPTH
    );
    eprintln!("Press Ctrl+C to stop.\n");

    loop {
        if last_refresh.elapsed() > refresh_interval {
            ghosts = rebuild_ghosts(repo, &all_set);
            last_refresh = Instant::now();
        }

        if let Some(finder_path) = get_finder_current_path() {
            if finder_path != last_path && finder_path.starts_with(&repo_prefix) {
                let dir = finder_path
                    .strip_prefix(&repo_prefix)
                    .unwrap_or("")
                    .trim_end_matches('/');

                if !dir.is_empty() && ghosts.contains(dir) {
                    eprintln!("  Detected: {} — resolving deps & hydrating...", dir);

                    // Resolve dependencies for this dir
                    let mut domains_to_hydrate = resolve_with_deps(repo, dir);

                    // Also include all sub-directories of each domain
                    let mut with_children: Vec<String> = Vec::new();
                    for d in &domains_to_hydrate {
                        with_children.push(d.clone());
                        // Find all ghost subdirs under this domain
                        let prefix = format!("{}/", d);
                        for ghost in ghosts.iter() {
                            if ghost.starts_with(&prefix) {
                                with_children.push(ghost.clone());
                            }
                        }
                    }
                    with_children.sort();
                    with_children.dedup();
                    domains_to_hydrate = with_children;

                    if let Err(e) = git::add_sparse(repo, &domains_to_hydrate) {
                        eprintln!("  \u{2717} sparse-checkout failed: {}", e);
                    } else {
                        for d in &domains_to_hydrate {
                            let _ = git::checkout_domain(repo, d);
                        }

                        std::thread::sleep(Duration::from_millis(300));

                        if let Ok(mut w) = workspace::load(repo) {
                            for d in &domains_to_hydrate {
                                w.add_domain(d);
                            }
                            let _ = workspace::save(repo, &w);
                        }

                        // Unhide hydrated dirs + their parents + all children
                        for d in &domains_to_hydrate {
                            unhide_with_parents(repo, d);
                            ghosts.remove(d.as_str());
                        }

                        // Refresh Finder in place so subfolders redraw
                        let _ = Command::new("osascript")
                            .arg("-e")
                            .arg(format!(
                                "tell application \"Finder\" to set target of front window to (POSIX file \"{}\" as alias)",
                                finder_path.trim_end_matches('/')
                            ))
                            .output();

                        eprintln!("  \u{2713} {} hydrated ({} dirs + deps)",
                            dir, domains_to_hydrate.len());
                    }
                }

                last_path = finder_path;
            }
        }

        std::thread::sleep(poll);
    }
}

fn rebuild_ghosts(repo: &Path, all_dirs: &std::collections::HashSet<String>) -> std::collections::HashSet<String> {
    let ws = workspace::load(repo).unwrap_or_default();
    all_dirs.iter().filter(|d| !ws.has_domain(d)).cloned().collect()
}
