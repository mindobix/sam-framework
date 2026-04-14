use thiserror::Error;

#[derive(Debug, Error)]
pub enum SamError {
    #[error("git error: {0}")]
    GitError(String),

    #[error("profile '{name}' not found. Available: {}", available.join(", "))]
    ProfileNotFound {
        name: String,
        available: Vec<String>,
    },

    #[error("no SAM repository found (looked for .sam/ directory walking up from cwd)")]
    NoRepoFound,

    #[error("workspace error: {0}")]
    WorkspaceError(String),

    #[error("monograph error: {0}")]
    MonoGraphError(String),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, SamError>;
