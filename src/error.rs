//! Error types for promrail.

#[derive(Debug, thiserror::Error)]
pub enum PromrailError {
    #[error("Git repository not found at '{0}'")]
    GitNotFound(String),

    #[error("Git tree is not clean. Commit or stash changes first.")]
    DirtyTree,

    #[error("Environment '{env}' not found in repo '{repo}'")]
    EnvironmentNotFound { repo: String, env: String },

    #[error("Repository '{0}' not found in config")]
    RepoNotFound(String),

    #[error("Protected path cannot be modified: '{0}'")]
    ProtectedPath(String),

    #[error("No files matched allowlist patterns")]
    NoFilesMatched,

    #[error("Config file not found: '{0}'")]
    ConfigNotFound(String),

    #[error("Config validation failed: {0}")]
    ConfigInvalid(String),

    #[error("Source and destination are the same: '{0}'")]
    SameEnvironment(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Glob pattern error: {0}")]
    Glob(#[from] globset::Error),
}

pub type AppResult<T> = std::result::Result<T, PromrailError>;
