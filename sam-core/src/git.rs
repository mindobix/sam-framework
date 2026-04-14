use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{Result, SamError};

/// A single entry from `git ls-tree`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub mode: String,
    pub kind: String, // "blob" or "tree"
    pub hash: String,
    pub path: String,
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

fn run_git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| SamError::GitError(format!("failed to run git: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SamError::GitError(stderr.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Clone with `--filter=blob:none --sparse`.
pub fn clone(url: &str, dest: &Path) -> Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| SamError::GitError("destination has no parent directory".into()))?;

    let dest_name = dest
        .file_name()
        .ok_or_else(|| SamError::GitError("destination has no file name".into()))?;

    let output = Command::new("git")
        .current_dir(parent)
        .args([
            "clone",
            "--filter=blob:none",
            "--sparse",
            url,
            &dest_name.to_string_lossy(),
        ])
        .output()
        .map_err(|e| SamError::GitError(format!("failed to run git clone: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SamError::GitError(stderr.trim().to_string()));
    }
    Ok(())
}

/// Extract directory name from a git URL.
///
/// Examples:
/// - `git@github.com:org/repo.git` -> `repo`
/// - `https://github.com/org/repo.git` -> `repo`
/// - `https://github.com/org/repo` -> `repo`
pub fn dir_from_url(url: &str) -> String {
    let s = url.trim_end_matches('/');
    let name = s.rsplit('/').next().unwrap_or(s);
    // Also handle git@host:org/repo.git (colon separator)
    let name = name.rsplit(':').next().unwrap_or(name);
    // In case the colon-split still has org/repo, take the last segment
    let name = name.rsplit('/').next().unwrap_or(name);
    name.trim_end_matches(".git").to_string()
}

/// Resolve to absolute path.
pub fn abs_dir(dir: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(dir).map_err(|e| {
        SamError::GitError(format!(
            "cannot resolve path '{}': {e}",
            dir.display()
        ))
    })
}

/// Initialize sparse-checkout in no-cone mode.
pub fn init_sparse(repo: &Path) -> Result<()> {
    run_git(repo, &["sparse-checkout", "init", "--no-cone"])?;
    Ok(())
}

/// Set sparse-checkout patterns (replaces all existing patterns).
pub fn set_sparse(repo: &Path, patterns: &[String]) -> Result<()> {
    let mut args: Vec<&str> = vec!["sparse-checkout", "set"];
    let pattern_refs: Vec<&str> = patterns.iter().map(|s| s.as_str()).collect();
    args.extend(pattern_refs);
    run_git(repo, &args)?;
    Ok(())
}

/// Add patterns to sparse-checkout (cumulative, never removes).
pub fn add_sparse(repo: &Path, patterns: &[String]) -> Result<()> {
    // Always include .sam so profiles.yaml and config stay accessible
    let mut all_patterns = patterns.to_vec();
    if !all_patterns.iter().any(|p| p == ".sam") {
        all_patterns.push(".sam".to_string());
    }
    let mut args: Vec<&str> = vec!["sparse-checkout", "add"];
    let pattern_refs: Vec<&str> = all_patterns.iter().map(|s| s.as_str()).collect();
    args.extend(pattern_refs);
    run_git(repo, &args)?;
    Ok(())
}

/// List current sparse-checkout patterns.
pub fn list_sparse(repo: &Path) -> Result<Vec<String>> {
    let output = run_git(repo, &["sparse-checkout", "list"])?;
    Ok(output
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Remove a domain from sparse-checkout (dehydrate from disk).
/// Rebuilds the sparse-checkout from workspace state minus the target domain.
pub fn remove_sparse(repo: &Path, pattern: &str) -> Result<()> {
    // Read workspace to get the definitive list of hydrated domains
    let ws = crate::workspace::load(repo).unwrap_or_default();

    // Build clean pattern list: .sam + all hydrated domains EXCEPT the target
    let mut patterns: Vec<String> = vec![".sam".to_string()];
    for domain in &ws.hydrated_domains {
        if domain != pattern {
            patterns.push(domain.clone());
        }
    }

    // Use set (not add) to completely replace the sparse-checkout
    set_sparse(repo, &patterns)?;
    Ok(())
}

/// Force git to re-materialize files for a domain.
/// Used after add_sparse to ensure files actually appear on disk.
pub fn checkout_domain(repo: &Path, domain: &str) -> Result<()> {
    run_git(repo, &["checkout", "HEAD", "--", domain])?;
    Ok(())
}

/// Disable sparse-checkout (materialize everything).
pub fn disable_sparse(repo: &Path) -> Result<()> {
    run_git(repo, &["sparse-checkout", "disable"])?;
    Ok(())
}

/// Check if directory is inside a git worktree.
pub fn is_worktree(dir: &Path) -> bool {
    Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the repository top-level directory.
pub fn top_level(dir: &Path) -> Result<PathBuf> {
    let out = run_git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out))
}

/// Check if a domain path exists in the git tree (works on blobless clones).
pub fn domain_exists(repo: &Path, domain: &str) -> Result<bool> {
    let result = run_git(repo, &["ls-tree", "--name-only", "HEAD", domain]);
    match result {
        Ok(output) => Ok(!output.is_empty()),
        Err(SamError::GitError(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Count files in a domain from `git ls-tree` (without materializing).
pub fn count_tree_files(repo: &Path, domain: &str) -> Result<usize> {
    let output = run_git(repo, &["ls-tree", "-r", "--name-only", "HEAD", domain])?;
    if output.is_empty() {
        return Ok(0);
    }
    Ok(output.lines().count())
}

/// Count materialized files on disk.
pub fn count_files(repo: &Path, domain: &str) -> Result<usize> {
    let domain_path = repo.join(domain);
    if !domain_path.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    count_files_recursive(&domain_path, &mut count)?;
    Ok(count)
}

fn count_files_recursive(dir: &Path, count: &mut usize) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_file() {
            *count += 1;
        } else if ft.is_dir() {
            // Skip .git directories
            if entry.file_name() != ".git" {
                count_files_recursive(&entry.path(), count)?;
            }
        }
    }
    Ok(())
}

/// Default max depth for directory listing.
pub const DEFAULT_MAX_DEPTH: usize = 4;

/// List all directories in the git tree up to `max_depth` levels deep.
/// Returns sorted list of paths like `["api", "api/v1", "api/v1/users", "shared/auth"]`.
/// Skips dotfiles/dotdirs at any level.
pub fn list_all_dirs(repo: &Path, max_depth: usize) -> Result<Vec<String>> {
    // Use git ls-tree -r -d to get ALL directories recursively
    let output = run_git(repo, &["ls-tree", "-r", "-d", "--name-only", "HEAD"])?;

    let mut dirs: Vec<String> = output
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('.') && !l.contains("/."))
        .filter(|l| l.split('/').count() <= max_depth)
        .map(|l| l.to_string())
        .collect();

    dirs.sort();
    Ok(dirs)
}

/// Backward-compatible wrapper. Returns dirs at default depth.
pub fn list_top_level_dirs(repo: &Path) -> Result<Vec<String>> {
    list_all_dirs(repo, DEFAULT_MAX_DEPTH)
}

/// Check if a directory in the git tree contains blobs (files) directly,
/// not just subdirectories. Used to determine if a dir is a "leaf project"
/// (should be dimmed) vs a "container" (should stay visible for navigation).
pub fn dir_has_files(repo: &Path, dir_path: &str) -> Result<bool> {
    let tree_spec = format!("HEAD:{}", dir_path);
    let output = run_git(repo, &["ls-tree", &tree_spec])?;
    // Check if any entry is a blob (file), not a tree (directory)
    Ok(output.lines().any(|line| {
        let parts: Vec<&str> = line.splitn(4, |c| c == ' ' || c == '\t').collect();
        parts.len() >= 3 && parts[1] == "blob"
    }))
}

/// List all tree entries (files and dirs) under a path from `git ls-tree`.
pub fn list_tree_entries(repo: &Path, path: &str) -> Result<Vec<TreeEntry>> {
    let tree_spec = if path.is_empty() {
        "HEAD".to_string()
    } else {
        format!("HEAD:{path}")
    };

    let output = run_git(repo, &["ls-tree", &tree_spec])?;
    let mut entries = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        // Format: <mode> <type> <hash>\t<name>
        let (meta, name) = match line.split_once('\t') {
            Some(pair) => pair,
            None => continue,
        };
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let full_path = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        };

        entries.push(TreeEntry {
            mode: parts[0].to_string(),
            kind: parts[1].to_string(),
            hash: parts[2].to_string(),
            path: full_path,
        });
    }

    Ok(entries)
}

/// Get changed files (unstaged + staged + unpushed commits). Deduplicated.
pub fn changed_files(repo: &Path) -> Result<Vec<String>> {
    let mut files = HashSet::new();

    // Unstaged changes
    if let Ok(output) = run_git(repo, &["diff", "--name-only"]) {
        for line in output.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Staged changes
    if let Ok(output) = run_git(repo, &["diff", "--cached", "--name-only"]) {
        for line in output.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Unpushed commits (ignore errors if no upstream is configured)
    if let Ok(output) = run_git(
        repo,
        &["log", "@{upstream}..HEAD", "--name-only", "--format="],
    ) {
        for line in output.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    let mut result: Vec<String> = files.into_iter().collect();
    result.sort();
    Ok(result)
}
