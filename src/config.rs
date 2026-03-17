//! Configuration handling for promrail.
//!
//! Supports two config versions:
//! - v1: Single repo with multiple environments
//! - v2: Multiple standalone repos for cross-repo promotion

use std::collections::HashMap;
use std::path::PathBuf;

use config_doc::ConfigDoc;
use serde::Deserialize;

/// Main configuration structure.
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
#[config_doc(header = "Promrail Configuration")]
pub struct Config {
    /// Config schema version (currently 1).
    #[config_doc(default = "1", required)]
    pub version: u32,

    /// Repository definitions. Each repo has a path and optional environments.
    #[config_doc(example = "gitops: { path: ~/gitops }")]
    #[serde(default)]
    pub repos: HashMap<String, RepoConfig>,

    /// Default repository name. Required when multiple repos are defined.
    #[config_doc(env = "PROMRAIL_REPO")]
    #[serde(default)]
    pub default_repo: String,

    /// Directories that are never modified during promotion.
    /// Useful for environment-specific customizations like secret values.
    #[config_doc(example = "[custom, env, local]")]
    #[serde(default)]
    pub protected_dirs: Vec<String>,

    /// Glob patterns for files that can be promoted.
    /// Files must match at least one pattern to be promoted.
    #[config_doc(example = "[\"platform/**/*.yaml\"]")]
    #[serde(default)]
    pub allowlist: Vec<String>,

    /// Glob patterns for files excluded from promotion.
    /// Takes precedence over allowlist.
    #[config_doc(example = "[\"**/*secret*\"]")]
    #[serde(default)]
    pub denylist: Vec<String>,

    /// Delete behavior configuration.
    #[serde(default)]
    pub delete: DeleteConfig,

    /// Git integration settings.
    #[serde(default)]
    pub git: GitConfig,

    /// Audit logging settings.
    #[serde(default)]
    pub audit: AuditConfig,
}

/// Repository configuration.
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
pub struct RepoConfig {
    /// Local path to repository. Supports ~ expansion for home directory.
    #[config_doc(example = "~/gitops")]
    pub path: String,

    /// Environment definitions. Map of environment name to relative path.
    #[config_doc(example = "staging: { path: clusters/staging }")]
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,
}

impl RepoConfig {
    /// Resolve path with ~ expansion.
    pub fn resolved_path(&self) -> PathBuf {
        shellexpand::full(&self.path)
            .map(|p| PathBuf::from(p.as_ref()))
            .unwrap_or_else(|_| PathBuf::from(&self.path))
    }

    /// Check if this repo has environments defined (v1 style).
    pub fn has_environments(&self) -> bool {
        !self.environments.is_empty()
    }
}

/// Environment configuration (staging, production, etc.).
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
pub struct EnvironmentConfig {
    /// Relative path from repo root.
    #[config_doc(example = "clusters/staging")]
    pub path: String,
}

/// Delete behavior configuration.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct DeleteConfig {
    /// Enable deletion of files in destination that don't exist in source.
    /// Note: The promote command deletes by default; use --no-delete to disable.
    #[config_doc(default = "false")]
    #[serde(default)]
    pub enabled: bool,

    /// Only delete files in directories that exist in source.
    /// Useful for partial environments.
    #[serde(default)]
    pub dest_based: bool,
}

/// Git integration configuration.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct GitConfig {
    /// Require clean git working tree before operations.
    #[config_doc(default = "true")]
    #[serde(default = "default_true")]
    pub require_clean_tree: bool,
}

fn default_true() -> bool {
    true
}

/// Audit logging configuration.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct AuditConfig {
    /// Enable promotion logging to file.
    #[config_doc(default = "true")]
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Path to audit log file.
    #[config_doc(default = ".promotion-log.yaml")]
    #[serde(default = "default_log_file")]
    pub log_file: String,
}

fn default_log_file() -> String {
    ".promotion-log.yaml".to_string()
}

impl Config {
    /// Load configuration from a YAML file.
    pub fn load(path: &std::path::Path) -> crate::error::AppResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|_| crate::error::PromrailError::ConfigNotFound(path.display().to_string()))?;
        let config: Config = serde_yaml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration consistency.
    pub fn validate(&self) -> crate::error::AppResult<()> {
        if self.repos.is_empty() {
            return Err(crate::error::PromrailError::ConfigInvalid(
                "no repos defined".to_string(),
            ));
        }

        if !self.default_repo.is_empty() && !self.repos.contains_key(&self.default_repo) {
            return Err(crate::error::PromrailError::ConfigInvalid(format!(
                "default_repo '{}' not found in repos",
                self.default_repo
            )));
        }

        Ok(())
    }

    /// Get a repository by name, or the default if none specified.
    pub fn get_repo(&self, name: Option<&str>) -> crate::error::AppResult<(&String, &RepoConfig)> {
        let repo_name = name.unwrap_or(&self.default_repo);
        self.repos
            .get_key_value(repo_name)
            .ok_or_else(|| crate::error::PromrailError::RepoNotFound(repo_name.to_string()))
    }

    /// Get first repo name (for single-repo configs).
    pub fn first_repo(&self) -> Option<(&String, &RepoConfig)> {
        self.repos.iter().next()
    }

    /// Check if using v2 style (standalone repos without environments).
    pub fn is_v2_style(&self) -> bool {
        self.repos.values().all(|r| r.environments.is_empty())
    }

    /// Generate full configuration documentation including environment variables.
    pub fn generate_full_docs() -> String {
        let mut output = Self::generate_docs();

        output.push_str("\n\x1b[1mEnvironment Variables\x1b[0m\n");
        output.push_str("─────────────────────\n\n");
        output.push_str("\x1b[36mPROMRAIL_CONFIG\x1b[0m\n");
        output.push_str("  Path to configuration file\n\n");
        output.push_str("\x1b[36mPROMRAIL_REPO\x1b[0m\n");
        output.push_str("  Default repository name\n\n");

        output
    }

    /// Generate a complete example configuration with comments.
    pub fn generate_full_example() -> String {
        r#"# Promrail Configuration
# See: promrail config show

# Config schema version (currently 1)
version: 1

# Repository definitions
repos:
  # Repository name (you can define multiple)
  gitops:
    # Local path to repository (~ expansion supported)
    path: ~/gitops

    # Environment definitions
    environments:
      staging: { path: clusters/staging }
      production: { path: clusters/production }

# Default repository name (required if multiple repos defined)
default_repo: gitops

# Directories that are never modified during promotion
protected_dirs:
  - custom
  - env
  - local

# Glob patterns for files that can be promoted
allowlist:
  - "platform/**/*.yaml"
  - "system/**/*.yaml"

# Glob patterns for files excluded from promotion
# Takes precedence over allowlist
denylist:
  - "**/*secret*"

# Delete behavior configuration
delete:
  # Enable deletion of files in destination that don't exist in source
  enabled: false
  # Only delete files in directories that exist in source
  dest_based: false

# Git integration settings
git:
  # Require clean git working tree before operations
  require_clean_tree: true

# Audit logging settings
audit:
  # Enable promotion logging to file
  enabled: true
  # Path to audit log file
  log_file: .promotion-log.yaml
"#
        .to_string()
    }
}

mod shellexpand {
    pub fn full(s: &str) -> Result<std::borrow::Cow<'_, str>, ()> {
        if s.starts_with("~")
            && let Some(home) = dirs::home_dir()
        {
            return Ok(std::borrow::Cow::Owned(s.replacen(
                "~",
                &home.display().to_string(),
                1,
            )));
        }
        Ok(std::borrow::Cow::Borrowed(s))
    }
}
