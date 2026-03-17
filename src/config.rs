//! Configuration handling for promrail.
//!
//! Supports two config versions:
//! - v1: Single repo with multiple environments
//! - v2: Multiple standalone repos for cross-repo promotion

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Main configuration structure.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub repos: HashMap<String, RepoConfig>,
    #[serde(default)]
    pub default_repo: String,
    #[serde(default)]
    pub protected_dirs: Vec<String>,
    #[serde(default)]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub denylist: Vec<String>,
    #[serde(default)]
    pub delete: DeleteConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

/// Repository configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct RepoConfig {
    pub path: String,
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
#[derive(Debug, Deserialize, Clone)]
pub struct EnvironmentConfig {
    pub path: String,
}

/// Delete behavior configuration.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DeleteConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dest_based: bool,
}

/// Git integration configuration.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub require_clean_tree: bool,
}

fn default_true() -> bool {
    true
}

/// Audit logging configuration.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
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
