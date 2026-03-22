//! File selection logic using allowlist/denylist patterns.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};

use crate::config::Config;
use crate::error::AppResult;

/// File selector using allowlist, denylist, and protected directory patterns.
pub struct FileSelector {
    allowlist: globset::GlobSet,
    denylist: globset::GlobSet,
    protected_dirs: Vec<PathBuf>,
}

impl FileSelector {
    /// Create a new file selector from configuration.
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let mut allowlist_patterns = config.allowlist.clone();
        if allowlist_patterns.is_empty() {
            allowlist_patterns = vec![
                "**/*.yaml".to_string(),
                "**/*.yml".to_string(),
                "**/README.md".to_string(),
                "**/README*".to_string(),
            ];
        } else {
            allowlist_patterns.push("**/README.md".to_string());
            allowlist_patterns.push("**/README*".to_string());
        }
        let allowlist = Self::build_globset(&allowlist_patterns)?;
        let denylist = Self::build_globset(&config.denylist)?;
        let protected_dirs = config.protected_dirs.iter().map(PathBuf::from).collect();

        Ok(Self {
            allowlist,
            denylist,
            protected_dirs,
        })
    }

    fn build_globset(patterns: &[String]) -> AppResult<globset::GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern)?;
            builder.add(glob);
        }
        Ok(builder.build()?)
    }

    /// Check if a path matches the allowlist patterns.
    pub fn matches_allowlist(&self, path: &Path) -> bool {
        if self.allowlist.is_empty() {
            return true;
        }
        let path_str = path.to_string_lossy();
        self.allowlist.is_match(path_str.as_ref())
    }

    /// Check if a path matches the denylist patterns.
    pub fn matches_denylist(&self, path: &Path) -> bool {
        if self.denylist.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.denylist.is_match(path_str.as_ref())
    }

    /// Check if a path is in a protected directory.
    pub fn is_protected(&self, path: &Path) -> bool {
        for protected in &self.protected_dirs {
            if path.starts_with(protected) || path.components().any(|c| c.as_os_str() == protected)
            {
                return true;
            }
        }
        false
    }

    /// Check if a path should be promoted (allowlist && !denylist && !protected).
    pub fn should_promote(&self, path: &Path, include_protected: bool) -> bool {
        if Self::is_internal_metadata(path) {
            return false;
        }

        let protected_ok = include_protected || !self.is_protected(path);
        protected_ok && !self.matches_denylist(path) && self.matches_allowlist(path)
    }

    fn is_internal_metadata(path: &Path) -> bool {
        if path.starts_with(".promrail") {
            return true;
        }

        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(".promotion-snapshots.yaml")
        )
    }

    /// Check if a path matches any of the filter patterns (regex or substring).
    pub fn matches_filter(path: &Path, filter: &[String]) -> bool {
        if filter.is_empty() || (filter.len() == 1 && filter[0] == ".*") {
            return true;
        }

        let path_str = path.to_string_lossy();

        filter.iter().any(|f| {
            if f == ".*" {
                return true;
            }

            let trimmed = f.trim_start_matches("./");

            if let Ok(re) = regex::Regex::new(trimmed) {
                re.is_match(&path_str)
            } else {
                path_str.contains(trimmed)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FileSelector;
    use crate::config::Config;
    use std::path::Path;

    #[test]
    fn internal_metadata_is_never_promoted() {
        let selector = FileSelector {
            allowlist: globset::GlobSetBuilder::new().build().expect("globset"),
            denylist: globset::GlobSetBuilder::new().build().expect("globset"),
            protected_dirs: Vec::new(),
        };

        assert!(!selector.should_promote(Path::new(".promotion-snapshots.yaml"), false));
        assert!(!selector.should_promote(Path::new(".promrail/review/test.yaml"), false));
        assert!(selector.should_promote(Path::new("platform/app/config.yaml"), false));
    }

    #[test]
    fn readme_files_are_always_allowed() {
        let config = Config {
            allowlist: vec!["platform/**/*.yaml".to_string()],
            denylist: vec![],
            protected_dirs: vec![],
            ..Default::default()
        };
        let selector = FileSelector::from_config(&config).expect("selector");

        assert!(selector.should_promote(Path::new("platform/app/config.yaml"), false));
        assert!(selector.should_promote(Path::new("README.md"), false));
        assert!(selector.should_promote(Path::new("platform/README.md"), false));
        assert!(selector.should_promote(Path::new("apps/README.org"), false));
        assert!(!selector.should_promote(Path::new("apps/other.md"), false));
    }

    #[test]
    fn empty_allowlist_includes_readme_by_default() {
        let config = Config {
            allowlist: vec![],
            denylist: vec![],
            protected_dirs: vec![],
            ..Default::default()
        };
        let selector = FileSelector::from_config(&config).expect("selector");

        assert!(selector.should_promote(Path::new("README.md"), false));
        assert!(selector.should_promote(Path::new("platform/config.yaml"), false));
    }
}
