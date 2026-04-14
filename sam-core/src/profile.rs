use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Result, SamError};

// ---------------------------------------------------------------------------
// Domain type (custom deserialization: "*" -> All, list -> List)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub enum Domains {
    All,
    List(Vec<String>),
}

impl<'de> Deserialize<'de> for Domains {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Star(String),
            List(Vec<String>),
        }

        match Raw::deserialize(deserializer)? {
            Raw::Star(s) if s == "*" => Ok(Domains::All),
            Raw::Star(s) => {
                // Single non-"*" string: treat as one-element list
                Ok(Domains::List(vec![s]))
            }
            Raw::List(v) => Ok(Domains::List(v)),
        }
    }
}

// ---------------------------------------------------------------------------
// Profile types
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub command: String,
    #[serde(default)]
    pub per_domain: bool,
    #[serde(default)]
    pub pre_deploy_impact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub domains: Domains,
    #[serde(default)]
    pub auto_include: Vec<String>,
    #[serde(default = "default_true")]
    pub ai_infer: bool,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default)]
    pub owners: Vec<String>,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    pub profiles: HashMap<String, Profile>,
}

// ---------------------------------------------------------------------------
// Repo config (.sam/config.yaml)
// ---------------------------------------------------------------------------

fn default_monograph_addr() -> String {
    "127.0.0.1:7474".to_string()
}
fn default_commits() -> u32 {
    500
}
fn default_score() -> f64 {
    0.3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonoGraphConfig {
    #[serde(default = "default_monograph_addr")]
    pub address: String,
    #[serde(default = "default_commits")]
    pub cochange_commits: u32,
    #[serde(default = "default_score")]
    pub cochange_min_score: f64,
}

impl Default for MonoGraphConfig {
    fn default() -> Self {
        Self {
            address: default_monograph_addr(),
            cochange_commits: default_commits(),
            cochange_min_score: default_score(),
        }
    }
}

fn default_monograph() -> MonoGraphConfig {
    MonoGraphConfig::default()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonoWatchConfig {
    #[serde(default)]
    pub block_on_critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployDefaults {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub per_domain: bool,
    #[serde(default)]
    pub pre_deploy_impact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(default = "default_monograph")]
    pub monograph: MonoGraphConfig,
    #[serde(default)]
    pub monowatch: MonoWatchConfig,
    #[serde(default)]
    pub deploy: Option<DeployDefaults>,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            monograph: MonoGraphConfig::default(),
            monowatch: MonoWatchConfig::default(),
            deploy: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Walk up from `from` looking for a `.sam/` directory.
pub fn find_repo_root(from: &Path) -> Result<PathBuf> {
    let mut current = from.to_path_buf();
    loop {
        if current.join(".sam").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            return Err(SamError::NoRepoFound);
        }
    }
}

/// Load `.sam/profiles.yaml`.
pub fn load_profiles(repo: &Path) -> Result<ProfilesConfig> {
    let path = repo.join(".sam").join("profiles.yaml");
    let content = std::fs::read_to_string(&path).map_err(|e| {
        SamError::WorkspaceError(format!(
            "cannot read {}: {e}",
            path.display()
        ))
    })?;
    let config: ProfilesConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load `.sam/config.yaml` (returns defaults if the file is missing).
pub fn load_repo_config(repo: &Path) -> Result<RepoConfig> {
    let path = repo.join(".sam").join("config.yaml");
    if !path.exists() {
        return Ok(RepoConfig::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| {
        SamError::WorkspaceError(format!(
            "cannot read {}: {e}",
            path.display()
        ))
    })?;
    let config: RepoConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Get a profile by name. Tries exact match first, then prefix match.
/// If prefix matches multiple profiles, returns `ProfileNotFound` with all matches.
pub fn get_profile<'a>(config: &'a ProfilesConfig, name: &str) -> Result<&'a Profile> {
    // Exact match
    if let Some(profile) = config.profiles.get(name) {
        return Ok(profile);
    }

    // Prefix match
    let matches: Vec<&String> = config
        .profiles
        .keys()
        .filter(|k| k.starts_with(name))
        .collect();

    match matches.len() {
        0 => Err(SamError::ProfileNotFound {
            name: name.to_string(),
            available: sorted_keys(&config.profiles),
        }),
        1 => Ok(&config.profiles[matches[0]]),
        _ => {
            // Multiple prefix matches — list them as available
            let mut matching: Vec<String> = matches.into_iter().cloned().collect();
            matching.sort();
            Err(SamError::ProfileNotFound {
                name: name.to_string(),
                available: matching,
            })
        }
    }
}

/// Get the resolved (full) profile name. Same matching logic as `get_profile`.
pub fn resolve_profile_name<'a>(config: &'a ProfilesConfig, name: &str) -> Result<&'a str> {
    // Exact match
    if config.profiles.contains_key(name) {
        return Ok(config.profiles.get_key_value(name).unwrap().0.as_str());
    }

    // Prefix match
    let matches: Vec<&String> = config
        .profiles
        .keys()
        .filter(|k| k.starts_with(name))
        .collect();

    match matches.len() {
        0 => Err(SamError::ProfileNotFound {
            name: name.to_string(),
            available: sorted_keys(&config.profiles),
        }),
        1 => Ok(matches[0].as_str()),
        _ => {
            let mut matching: Vec<String> = matches.into_iter().cloned().collect();
            matching.sort();
            Err(SamError::ProfileNotFound {
                name: name.to_string(),
                available: matching,
            })
        }
    }
}

fn sorted_keys(map: &HashMap<String, Profile>) -> Vec<String> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys
}
