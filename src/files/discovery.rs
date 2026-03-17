use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::AppResult;
use crate::files::FileSelector;

pub struct FileDiscovery {
    selector: FileSelector,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFiles {
    pub files: Vec<PathBuf>,
    pub dirs: HashSet<PathBuf>,
}

impl FileDiscovery {
    pub fn new(selector: FileSelector) -> Self {
        Self { selector }
    }

    pub fn discover(
        &self,
        root: &Path,
        filter: &[String],
        include_protected: bool,
    ) -> AppResult<DiscoveredFiles> {
        let mut files = Vec::new();
        let mut dirs = HashSet::new();

        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.is_dir() {
                continue;
            }

            let relative = path.strip_prefix(root).unwrap_or(path);

            if !self.selector.should_promote(relative, include_protected) {
                continue;
            }

            if !FileSelector::matches_filter(relative, filter) {
                continue;
            }

            if let Some(parent) = relative.parent() {
                if parent != Path::new("") {
                    dirs.insert(parent.to_path_buf());
                }
            }

            files.push(relative.to_path_buf());
        }

        Ok(DiscoveredFiles { files, dirs })
    }

    pub fn get_subdirs(&self, root: &Path) -> HashSet<PathBuf> {
        let mut subdirs = HashSet::new();

        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() && path != root {
                let relative = path.strip_prefix(root).unwrap_or(path);
                if !self.selector.is_protected(relative) {
                    subdirs.insert(relative.to_path_buf());
                }
            }
        }

        subdirs
    }
}

pub fn get_subdirs_recursive(root: &Path, include_root: bool) -> HashSet<PathBuf> {
    let mut subdirs = HashSet::new();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() && (include_root || path != root) {
            let relative = path.strip_prefix(root).unwrap_or(path);
            subdirs.insert(relative.to_path_buf());
        }
    }

    subdirs
}
