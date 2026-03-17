use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};

use crate::config::Config;
use crate::error::AppResult;

pub struct FileSelector {
    allowlist: globset::GlobSet,
    denylist: globset::GlobSet,
    protected_dirs: Vec<PathBuf>,
}

impl FileSelector {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let allowlist = Self::build_globset(&config.allowlist)?;
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

    pub fn matches_allowlist(&self, path: &Path) -> bool {
        if self.allowlist.is_empty() {
            return true;
        }
        let path_str = path.to_string_lossy();
        self.allowlist.is_match(path_str.as_ref())
    }

    pub fn matches_denylist(&self, path: &Path) -> bool {
        if self.denylist.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.denylist.is_match(path_str.as_ref())
    }

    pub fn is_protected(&self, path: &Path) -> bool {
        for protected in &self.protected_dirs {
            if path.starts_with(protected) || path.components().any(|c| c.as_os_str() == protected)
            {
                return true;
            }
        }
        false
    }

    pub fn should_promote(&self, path: &Path, include_protected: bool) -> bool {
        let protected_ok = include_protected || !self.is_protected(path);
        protected_ok && !self.matches_denylist(path) && self.matches_allowlist(path)
    }

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
