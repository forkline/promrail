//! Git repository operations using git2.

use std::path::{Path, PathBuf};

use git2::{Repository, StatusOptions};

use crate::error::{AppResult, PromrailError};

/// Git repository wrapper providing common operations.
pub struct GitRepo {
    inner: Repository,
    /// Path to the working directory.
    pub path: PathBuf,
}

impl GitRepo {
    /// Discover and open a git repository at or above the given path.
    pub fn discover(path: &Path) -> AppResult<Self> {
        let repo = Repository::discover(path)
            .map_err(|_| PromrailError::GitNotFound(path.display().to_string()))?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| PromrailError::GitNotFound(path.display().to_string()))?
            .to_path_buf();

        Ok(Self {
            inner: repo,
            path: workdir,
        })
    }

    /// Check if the working tree has no uncommitted changes.
    pub fn is_clean(&self) -> AppResult<bool> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true);

        let statuses = self.inner.statuses(Some(&mut opts))?;

        let has_changes = statuses.iter().any(|entry| {
            let status = entry.status();
            status.contains(git2::Status::INDEX_NEW)
                || status.contains(git2::Status::INDEX_MODIFIED)
                || status.contains(git2::Status::INDEX_DELETED)
                || status.contains(git2::Status::INDEX_RENAMED)
                || status.contains(git2::Status::WT_NEW)
                || status.contains(git2::Status::WT_MODIFIED)
                || status.contains(git2::Status::WT_DELETED)
                || status.contains(git2::Status::WT_RENAMED)
                || status.contains(git2::Status::WT_TYPECHANGE)
        });

        Ok(!has_changes)
    }

    /// Get the current HEAD branch name.
    pub fn current_head(&self) -> AppResult<String> {
        let head = self.inner.head()?;
        let shorthand = head.shorthand().unwrap_or("HEAD");
        Ok(shorthand.to_string())
    }

    /// Get the current commit SHA (short form).
    pub fn current_commit(&self) -> AppResult<String> {
        let head = self.inner.head()?;
        let commit = head.peel_to_commit()?;
        let short_id = commit.id().to_string();
        Ok(short_id.chars().take(7).collect())
    }

    /// Read file contents from the working directory.
    pub fn read_file(&self, path: &Path) -> AppResult<Option<String>> {
        let full_path = self.path.join(path);
        if full_path.exists() {
            let content = std::fs::read_to_string(&full_path)?;
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    /// Copy a file within the repository.
    pub fn copy_file(&self, source: &Path, dest: &Path) -> AppResult<()> {
        let source_full = self.path.join(source);
        let dest_full = self.path.join(dest);

        if let Some(parent) = dest_full.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(&source_full, &dest_full)?;
        Ok(())
    }

    /// Delete a file from the working directory.
    pub fn delete_file(&self, path: &Path) -> AppResult<()> {
        let full_path = self.path.join(path);
        if full_path.exists() {
            std::fs::remove_file(&full_path)?;
        }
        Ok(())
    }
}
