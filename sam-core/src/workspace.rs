use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SamError};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub hydrated_domains: Vec<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
}

impl State {
    /// Check whether a domain is in the hydrated set.
    pub fn has_domain(&self, domain: &str) -> bool {
        self.hydrated_domains.iter().any(|d| d == domain)
    }

    /// Add a domain to the hydrated set (idempotent).
    pub fn add_domain(&mut self, domain: &str) {
        if !self.has_domain(domain) {
            self.hydrated_domains.push(domain.to_string());
        }
    }

    /// Set the active profile name.
    pub fn set_profile(&mut self, name: &str) {
        self.active_profile = Some(name.to_string());
    }
}

/// Load workspace state from `.sam/workspace.yaml`.
/// Returns a default (empty) state if the file does not exist.
pub fn load(repo: &Path) -> Result<State> {
    let path = repo.join(".sam").join("workspace.yaml");
    if !path.exists() {
        return Ok(State::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| {
        SamError::WorkspaceError(format!(
            "cannot read {}: {e}",
            path.display()
        ))
    })?;
    if content.trim().is_empty() {
        return Ok(State::default());
    }
    let state: State = serde_yaml::from_str(&content)?;
    Ok(state)
}

/// Save workspace state to `.sam/workspace.yaml`.
/// Uses atomic write (write to `.tmp` then rename).
/// Sorts and deduplicates `hydrated_domains` before saving.
pub fn save(repo: &Path, state: &State) -> Result<()> {
    let sam_dir = repo.join(".sam");
    if !sam_dir.exists() {
        std::fs::create_dir_all(&sam_dir)?;
    }

    let mut state = state.clone();

    // Sort and deduplicate
    state.hydrated_domains.sort();
    state.hydrated_domains.dedup();

    // Update timestamp
    state.last_updated = Some(Utc::now().to_rfc3339());

    let yaml = serde_yaml::to_string(&state)?;

    let path = sam_dir.join("workspace.yaml");
    let tmp_path = sam_dir.join("workspace.yaml.tmp");

    std::fs::write(&tmp_path, yaml.as_bytes())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| {
        SamError::WorkspaceError(format!(
            "atomic rename failed: {e}"
        ))
    })?;

    Ok(())
}
