use std::path::{Path, PathBuf};

use crate::error::{AppResult, PromrailError};
use crate::review::models::ReviewArtifact;

const REVIEW_DIR: &str = ".promrail/review";

pub fn artifact_path(repo_root: &Path, route_key: &str) -> PathBuf {
    repo_root
        .join(REVIEW_DIR)
        .join(format!("{}.yaml", route_key))
}

pub fn save_artifact(repo_root: &Path, artifact: &ReviewArtifact) -> AppResult<PathBuf> {
    let path = artifact_path(repo_root, &artifact.route_key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_yaml::to_string(artifact)?)?;
    Ok(path)
}

pub fn load_artifact(repo_root: &Path, route_key: &str) -> AppResult<Option<ReviewArtifact>> {
    let path = artifact_path(repo_root, route_key);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let artifact: ReviewArtifact = serde_yaml::from_str(&content).map_err(|err| {
        PromrailError::ReviewArtifactInvalid(format!("{}: {}", path.display(), err))
    })?;
    Ok(Some(artifact))
}
