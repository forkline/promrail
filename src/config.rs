//! Configuration handling for promrail.
//!
//! Supports two config styles:
//! - Single-repo: Top-level `environments` with optional `default_source`/`default_dest`
//! - Multi-repo: `repos` section with per-repo environments (for cross-repo promotion)
//!
//! Multi-source promotion rules for complex workflows.

use std::collections::HashMap;
use std::path::PathBuf;

use config_doc::ConfigDoc;
use serde::Deserialize;

/// Main configuration structure.
#[derive(Debug, Default, Deserialize, Clone, ConfigDoc)]
#[config_doc(header = "Promrail Configuration")]
pub struct Config {
    /// Config schema version (currently 1).
    #[config_doc(default = "1", required)]
    pub version: u32,

    /// Top-level environments for single-repo mode.
    /// Use this when promrail.yaml is in the repo root.
    /// Mutually exclusive with defining environments under repos.
    #[config_doc(example = "staging: { path: clusters/staging }")]
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,

    /// Default source environment(s) for promote/diff commands.
    /// Enables running `prl promote` without --source flag.
    /// For multi-source promotion, list multiple sources.
    #[config_doc(example = "[staging]", example = "[staging, homelab]")]
    #[serde(default)]
    pub default_sources: Vec<String>,

    /// Default destination environment for promote/diff commands.
    /// Enables running `prl promote` without --dest flag.
    #[serde(default)]
    pub default_dest: Option<String>,

    /// Repository definitions. Each repo has a path and optional environments.
    /// Required for multi-repo setups. Optional for single-repo (use top-level environments).
    #[config_doc(example = "gitops: { path: ~/gitops }")]
    #[serde(default)]
    pub repos: HashMap<String, RepoConfig>,

    /// Default repository name. Required when multiple repos are defined.
    #[config_doc(env = "PROMRAIL_REPO")]
    #[serde(default)]
    pub default_repo: String,

    /// Directories that are never modified during promotion.
    /// Useful for environment-specific customizations.
    /// Recommended: custom, env (for env-specific patches and config)
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

    /// Multi-source promotion rules for complex workflows.
    #[serde(default)]
    pub rules: PromotionRules,

    /// Output configuration for promotion feedback.
    #[serde(default)]
    pub output: OutputConfig,
}

/// Promotion rules for multi-source workflows.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct PromotionRules {
    /// Source definitions with priorities and filters.
    #[serde(default)]
    pub sources: HashMap<String, SourceRule>,

    /// Conflict resolution strategies.
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,

    /// Component-level rules.
    #[serde(default)]
    pub components: HashMap<String, ComponentRule>,

    /// Global rules applied to all promotions.
    #[serde(default)]
    pub global: GlobalRules,
}

/// Source rule for multi-source promotion.
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
pub struct SourceRule {
    /// Priority for conflict resolution (higher = higher priority).
    #[config_doc(example = "1")]
    #[serde(default)]
    pub priority: u32,

    /// Description of this source.
    #[serde(default)]
    pub description: String,

    /// Path override for this source (if different from repo path).
    #[serde(default)]
    pub path: Option<String>,

    /// Components to include from this source (glob patterns).
    #[serde(default)]
    pub include: Vec<String>,

    /// Components to exclude from this source (glob patterns).
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Conflict resolution strategies.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct ConflictResolution {
    /// Strategy for version conflicts: highest, source_priority.
    #[config_doc(default = "highest", example = "highest")]
    #[serde(default)]
    pub version_strategy: VersionStrategy,

    /// Strategy for config conflicts: source_priority, merge, fail.
    #[config_doc(default = "source_priority", example = "source_priority")]
    #[serde(default)]
    pub config_strategy: ConfigStrategy,

    /// Source priority order (for source_priority strategy).
    #[serde(default)]
    pub source_order: Vec<String>,
}

/// Version conflict resolution strategy.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionStrategy {
    /// Use highest version number.
    #[default]
    Highest,
    /// Use version from highest priority source.
    SourcePriority,
}

impl std::fmt::Display for VersionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionStrategy::Highest => write!(f, "highest"),
            VersionStrategy::SourcePriority => write!(f, "source_priority"),
        }
    }
}

/// Config conflict resolution strategy.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigStrategy {
    /// Use config from highest priority source.
    #[default]
    SourcePriority,
    /// Attempt to merge configs.
    Merge,
    /// Fail on conflict.
    Fail,
}

impl std::fmt::Display for ConfigStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigStrategy::SourcePriority => write!(f, "source_priority"),
            ConfigStrategy::Merge => write!(f, "merge"),
            ConfigStrategy::Fail => write!(f, "fail"),
        }
    }
}

/// Promotion action for a component.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionAction {
    /// Always promote without question.
    #[default]
    Always,
    /// Flag for human/opencode review.
    Review,
    /// Never promote this component.
    Never,
}

impl std::fmt::Display for PromotionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromotionAction::Always => write!(f, "always"),
            PromotionAction::Review => write!(f, "review"),
            PromotionAction::Never => write!(f, "never"),
        }
    }
}

/// Version handling strategy for version-managed files.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VersionHandling {
    /// Use structured version updates, preserve destination content.
    #[default]
    Structured,
    /// Copy entire file from source (override for special cases).
    WholeFile,
}

impl std::fmt::Display for VersionHandling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionHandling::Structured => write!(f, "structured"),
            VersionHandling::WholeFile => write!(f, "whole_file"),
        }
    }
}

/// Component-level promotion rule.
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
pub struct ComponentRule {
    /// Action to take: always, review, never.
    #[config_doc(example = "always")]
    #[serde(default)]
    pub action: PromotionAction,

    /// Human-readable notes for reviewers.
    #[serde(default)]
    pub notes: String,

    /// Preserve destination-specific configuration paths for matching files.
    #[serde(default)]
    pub preserve: Vec<PreserveRule>,

    /// Override version-managed file handling strategy.
    #[serde(default)]
    pub version_handling: VersionHandling,
}

/// File-specific preserve rule for YAML/JSON promotion.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct PreserveRule {
    /// File path or glob relative to the component directory.
    #[serde(default)]
    pub file: String,

    /// Dot-separated YAML/JSON paths to preserve from destination.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Global rules applied to all promotions.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct GlobalRules {
    /// Patterns to always exclude.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Patterns that require review.
    #[serde(default)]
    pub review_required: Vec<String>,

    /// Multi-source promotion options.
    #[serde(default)]
    pub promote_options: PromoteOptions,
}

/// Options for multi-source promotion.
#[derive(Debug, Deserialize, Clone, Default, ConfigDoc)]
pub struct PromoteOptions {
    /// Allow duplicate files across sources.
    /// When false (default), promotion fails if the same file exists in multiple sources.
    #[config_doc(default = "false")]
    #[serde(default)]
    pub allow_duplicates: bool,

    /// Only promote components that already exist in destination.
    /// When true, new components from sources are skipped.
    #[config_doc(default = "false")]
    #[serde(default)]
    pub only_existing: bool,

    /// Do not delete extra files in destination.
    /// When true, files in destination that don't exist in source are kept.
    #[config_doc(default = "false")]
    #[serde(default)]
    pub no_delete: bool,

    /// Ignore files matching .gitignore patterns.
    /// When true (default), files matching .gitignore are excluded from promotion.
    #[config_doc(default = "true")]
    #[serde(default = "default_ignore_gitignore")]
    pub ignore_gitignore: bool,
}

fn default_ignore_gitignore() -> bool {
    true
}

/// Output configuration for promotion feedback.
#[derive(Debug, Deserialize, Clone, ConfigDoc)]
pub struct OutputConfig {
    /// Output verbosity level for promotion results.
    #[config_doc(default = "normal", example = "verbose")]
    #[serde(default)]
    pub level: OutputLevel,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            level: OutputLevel::Normal,
        }
    }
}

/// Output verbosity level.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputLevel {
    /// Show only summary counts.
    Minimal,
    /// Show summary and version changes list.
    #[default]
    Normal,
    /// Show detailed output with component grouping.
    Verbose,
}

impl std::fmt::Display for OutputLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputLevel::Minimal => write!(f, "minimal"),
            OutputLevel::Normal => write!(f, "normal"),
            OutputLevel::Verbose => write!(f, "verbose"),
        }
    }
}

impl PromotionRules {
    /// Check if rules are defined.
    pub fn has_rules(&self) -> bool {
        !self.sources.is_empty() || !self.components.is_empty()
    }

    /// Get action for a component path.
    pub fn get_action(&self, component: &str) -> PromotionAction {
        // Check exact match first
        if let Some(rule) = self.components.get(component) {
            return rule.action.clone();
        }

        // Check global exclusions
        for pattern in &self.global.exclude {
            if glob_match::glob_match(pattern, component) {
                return PromotionAction::Never;
            }
        }

        // Check global review required
        for pattern in &self.global.review_required {
            if glob_match::glob_match(pattern, component) {
                return PromotionAction::Review;
            }
        }

        // Default: always promote
        PromotionAction::Always
    }

    /// Get explicit rule for a component path.
    pub fn get_component_rule(&self, component: &str) -> Option<&ComponentRule> {
        self.components.get(component)
    }

    /// Check if a source should include a component.
    pub fn source_includes(&self, source: &str, component: &str) -> bool {
        if let Some(rule) = self.sources.get(source) {
            // Check exclusions first
            for pattern in &rule.exclude {
                if glob_match::glob_match(pattern, component) {
                    return false;
                }
            }

            // If no includes defined, include all
            if rule.include.is_empty() {
                return true;
            }

            // Check inclusions
            for pattern in &rule.include {
                if glob_match::glob_match(pattern, component) {
                    return true;
                }
            }

            return false;
        }

        // Source not defined, include by default
        true
    }

    /// Get source priority (higher = higher priority).
    pub fn get_source_priority(&self, source: &str) -> u32 {
        self.sources.get(source).map(|r| r.priority).unwrap_or(0)
    }

    /// Resolve config source by source priority.
    pub fn resolve_config_source(&self, sources: &[String]) -> Option<String> {
        if sources.is_empty() {
            return None;
        }

        if sources.len() == 1 {
            return Some(sources[0].clone());
        }

        match self.conflict_resolution.config_strategy {
            ConfigStrategy::SourcePriority => sources
                .iter()
                .max_by_key(|source| self.get_source_priority(source))
                .cloned(),
            ConfigStrategy::Merge | ConfigStrategy::Fail => None,
        }
    }

    /// Resolve version conflict between sources.
    pub fn resolve_version_conflict(
        &self,
        versions: &[(String, String)], // (source, version)
    ) -> Option<(String, String)> {
        if versions.is_empty() {
            return None;
        }

        if versions.len() == 1 {
            return Some(versions[0].clone());
        }

        match &self.conflict_resolution.version_strategy {
            VersionStrategy::Highest => {
                // Find highest version
                versions
                    .iter()
                    .max_by(|a, b| compare_versions(&a.1, &b.1))
                    .cloned()
            }
            VersionStrategy::SourcePriority => {
                // Find highest priority source
                versions
                    .iter()
                    .max_by_key(|(source, _)| self.get_source_priority(source))
                    .cloned()
            }
        }
    }
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse_parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .trim_start_matches('V')
            .split('.')
            .filter_map(|p| p.split('-').next()?.parse().ok())
            .collect()
    };

    let a_parts = parse_parts(a);
    let b_parts = parse_parts(b);

    for i in 0..std::cmp::max(a_parts.len(), b_parts.len()) {
        let a_val = a_parts.get(i).unwrap_or(&0);
        let b_val = b_parts.get(i).unwrap_or(&0);
        match a_val.cmp(b_val) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    std::cmp::Ordering::Equal
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
        // Single-repo mode: top-level environments defined
        if !self.environments.is_empty() {
            // Validate default_sources/default_dest if set
            for source in &self.default_sources {
                if !self.environments.contains_key(source) && !self.repos.contains_key(source) {
                    return Err(crate::error::PromrailError::ConfigInvalid(format!(
                        "default_sources '{}' not found in environments or repos",
                        source
                    )));
                }
            }
            if let Some(ref dest) = self.default_dest
                && !self.environments.contains_key(dest)
            {
                return Err(crate::error::PromrailError::ConfigInvalid(format!(
                    "default_dest '{}' not found in environments",
                    dest
                )));
            }
            return Ok(());
        }

        // Multi-repo mode: repos defined
        if self.repos.is_empty() {
            return Err(crate::error::PromrailError::ConfigInvalid(
                "no environments or repos defined".to_string(),
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

    /// Get environments (either top-level or from default repo).
    pub fn get_environments(&self) -> &HashMap<String, EnvironmentConfig> {
        if !self.environments.is_empty() {
            return &self.environments;
        }

        // Get from default repo
        if let Ok((_, repo)) = self.get_repo(None) {
            return &repo.environments;
        }

        static EMPTY: std::sync::OnceLock<HashMap<String, EnvironmentConfig>> =
            std::sync::OnceLock::new();
        EMPTY.get_or_init(HashMap::new)
    }

    /// Check if using single-repo mode (top-level environments).
    pub fn is_single_repo(&self) -> bool {
        !self.environments.is_empty()
    }

    /// Get a repository by name, or the default if none specified.
    /// Returns an implicit repo for single-repo mode.
    pub fn get_repo(&self, name: Option<&str>) -> crate::error::AppResult<(&String, &RepoConfig)> {
        // Single-repo mode: return implicit repo
        if self.is_single_repo() {
            static IMPLICIT_REPO: std::sync::OnceLock<RepoConfig> = std::sync::OnceLock::new();
            static IMPLICIT_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
            let repo = IMPLICIT_REPO.get_or_init(|| RepoConfig {
                path: ".".to_string(),
                environments: HashMap::new(), // Not used in single-repo mode
            });
            let name = IMPLICIT_NAME.get_or_init(|| "default".to_string());
            return Ok((name, repo));
        }

        let repo_name = name.unwrap_or(&self.default_repo);
        self.repos
            .get_key_value(repo_name)
            .ok_or_else(|| crate::error::PromrailError::RepoNotFound(repo_name.to_string()))
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

# SIMPLIFIED SINGLE-REPO MODE
# Use this when promrail.yaml is in the repo root.
# No need for repos/default_repo - just define environments directly.

# Environment definitions (required)
environments:
  staging: { path: clusters/staging }
  production: { path: clusters/production }

# Default source/dest for promote/diff (optional)
# Enables running `prl promote` without --source/--dest
# For multi-source, list multiple sources
default_sources:
  - staging
default_dest: production

# Directories that are never modified during promotion
# Recommended: custom (env-specific patches), env (env-specific config)
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
  - "**/charts/**"

# Git integration settings
git:
  require_clean_tree: true

# ─────────────────────────────────────────────────────────────────────────────
# MULTI-REPO MODE (optional, for cross-repo promotion)
# Uncomment if you need to promote across multiple repositories
# ─────────────────────────────────────────────────────────────────────────────
#
# repos:
#   gitops:
#     path: ~/gitops
#     environments:
#       staging: { path: clusters/staging }
#       production: { path: clusters/production }
#
#   homelab:
#     path: ~/homelab
#
# default_repo: gitops
#
# # Multi-source promotion rules (for complex workflows)
# rules:
#   sources:
#     staging-homelab:
#       priority: 1
#       include: [platform/*, system/monitoring/*]
#
#     staging-work:
#       priority: 2
#       include: [apps/*, system/auth/*]
#
#   conflict_resolution:
#     version_strategy: highest
#     source_order:
#       - staging-work
#       - staging-homelab
#
#   components:
#     platform/homeassistant:
#       action: never
#       notes: "Home-specific, not for work production"
#
#   global:
#     exclude:
#       - "*/custom/*"
#       - "*/env/*"
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
