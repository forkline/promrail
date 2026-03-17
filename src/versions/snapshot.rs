//! Snapshot management for promotion tracking.

use std::path::Path;

use tracing::{info, warn};

use crate::error::AppResult;
use crate::versions::models::{
    FileVersionChange, Snapshot, SnapshotFile, SnapshotStatus, VersionChangeKind,
};

const SNAPSHOT_FILE: &str = ".promotion-snapshots.yaml";

/// Load snapshot file for a destination repository.
pub fn load(dest: &Path) -> AppResult<SnapshotFile> {
    let snapshot_path = dest.join(SNAPSHOT_FILE);

    if snapshot_path.exists() {
        let content = std::fs::read_to_string(&snapshot_path)?;
        let file: SnapshotFile =
            serde_yaml::from_str(&content).unwrap_or_else(|_| SnapshotFile::default());
        Ok(file)
    } else {
        Ok(SnapshotFile::default())
    }
}

/// Save snapshot file to a destination repository.
pub fn save(dest: &Path, file: &SnapshotFile) -> AppResult<()> {
    let snapshot_path = dest.join(SNAPSHOT_FILE);
    let content = serde_yaml::to_string(file)?;
    std::fs::write(&snapshot_path, content)?;
    Ok(())
}

/// Create a new snapshot.
pub fn create(
    _dest: &Path,
    source_path: String,
    dest_path: String,
    filters: Vec<String>,
) -> AppResult<Snapshot> {
    let snapshot = Snapshot::new(source_path, dest_path, filters);
    info!("Created snapshot: {}", snapshot.id);
    Ok(snapshot)
}

/// Add a snapshot to the snapshot file.
pub fn add(dest: &Path, snapshot: Snapshot) -> AppResult<()> {
    let mut file = load(dest)?;
    file.snapshots.push(snapshot);
    save(dest, &file)?;
    Ok(())
}

/// List all snapshots for a destination.
pub fn list(dest: &Path) -> AppResult<Vec<Snapshot>> {
    let file = load(dest)?;
    Ok(file.snapshots)
}

/// Get a specific snapshot by ID.
pub fn get(dest: &Path, id: &str) -> AppResult<Option<Snapshot>> {
    let file = load(dest)?;
    Ok(file.snapshots.iter().find(|s| s.id == id).cloned())
}

/// Rollback to a specific snapshot.
pub fn rollback(dest: &Path, id: &str) -> AppResult<bool> {
    let mut file = load(dest)?;

    let snapshot = file.snapshots.iter_mut().find(|s| s.id == id);

    match snapshot {
        Some(snap) => {
            if snap.status == SnapshotStatus::RolledBack {
                warn!("Snapshot {} already rolled back", id);
                return Ok(false);
            }

            // Restore files from snapshot
            for (component, changes) in &snap.version_changes {
                for change in changes {
                    let component_path = dest.join(component);
                    let file_path = component_path.join(&change.file);

                    if file_path.exists() {
                        // Read current content
                        let content = std::fs::read_to_string(&file_path)?;

                        // Restore previous version
                        let restored = apply_version_change(&content, change);
                        std::fs::write(&file_path, restored)?;
                        info!("Restored: {}", file_path.display());
                    }
                }
            }

            snap.status = SnapshotStatus::RolledBack;
            save(dest, &file)?;
            info!("Rolled back to snapshot {}", id);
            Ok(true)
        }
        None => {
            warn!("Snapshot {} not found", id);
            Ok(false)
        }
    }
}

/// Apply a version change to file content.
fn apply_version_change(content: &str, change: &FileVersionChange) -> String {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(content).unwrap_or_default();

    match change.kind {
        VersionChangeKind::HelmChart => {
            // Find and update helm chart version
            if let Some(helm_charts) = doc.get_mut("helmCharts").and_then(|v| v.as_sequence_mut()) {
                for chart in helm_charts {
                    if let Some(name) = chart.get("name").and_then(|v| v.as_str())
                        && name == change.name
                        && let Some(version) = chart.get_mut("version")
                    {
                        *version = serde_yaml::Value::String(change.before.clone());
                    }
                }
            }

            // Also check dependencies
            if let Some(deps) = doc
                .get_mut("dependencies")
                .and_then(|v| v.as_sequence_mut())
            {
                for dep in deps {
                    if let Some(name) = dep.get("name").and_then(|v| v.as_str())
                        && name == change.name
                        && let Some(version) = dep.get_mut("version")
                    {
                        *version = serde_yaml::Value::String(change.before.clone());
                    }
                }
            }
        }
        VersionChangeKind::ContainerImage => {
            // Update container image tag
            apply_image_tag_change(&mut doc, &change.name, &change.before);
        }
    }

    serde_yaml::to_string(&doc).unwrap_or_default()
}

fn apply_image_tag_change(value: &mut serde_yaml::Value, image_name: &str, new_tag: &str) {
    if let serde_yaml::Value::Mapping(map) = value {
        // Check for image structure
        if let (Some(repo), Some(_)) = (
            map.get(serde_yaml::Value::String("repository".to_string()))
                .and_then(|v| v.as_str()),
            map.get(serde_yaml::Value::String("tag".to_string())),
        ) && repo == image_name
        {
            map.insert(
                serde_yaml::Value::String("tag".to_string()),
                serde_yaml::Value::String(new_tag.to_string()),
            );
        }

        // Recurse into nested structures
        for (_, val) in map.iter_mut() {
            apply_image_tag_change(val, image_name, new_tag);
        }
    } else if let serde_yaml::Value::Sequence(seq) = value {
        for item in seq.iter_mut() {
            apply_image_tag_change(item, image_name, new_tag);
        }
    }
}

/// Delete a snapshot by ID.
pub fn delete(dest: &Path, id: &str) -> AppResult<bool> {
    let mut file = load(dest)?;
    let initial_len = file.snapshots.len();
    file.snapshots.retain(|s| s.id != id);

    if file.snapshots.len() < initial_len {
        save(dest, &file)?;
        info!("Deleted snapshot {}", id);
        Ok(true)
    } else {
        warn!("Snapshot {} not found", id);
        Ok(false)
    }
}
