//! Version data models.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Extracted versions from a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionReport {
    pub extracted_at: String,
    pub source_path: String,
    pub components: HashMap<String, ComponentVersions>,
}

impl VersionReport {
    pub fn new(source_path: &str) -> Self {
        Self {
            extracted_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            source_path: source_path.to_string(),
            components: HashMap::new(),
        }
    }
}

/// Versions for a single component.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentVersions {
    pub path: String,
    pub helm_charts: Vec<HelmChartVersion>,
    pub container_images: Vec<ContainerImageVersion>,
}

/// Helm chart version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmChartVersion {
    pub name: String,
    pub version: String,
    pub repository: Option<String>,
    pub source_file: String,
}

/// Container image version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerImageVersion {
    pub name: String,
    pub tag: String,
    pub source_file: String,
    pub json_path: String,
}

// =============================================================================
// APPLY OPTIONS
// =============================================================================

/// Options for version apply operation.
#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    pub components: Vec<String>,
    pub dry_run: bool,
    pub check_conflicts: bool,
    pub create_snapshot: bool,
}

/// Result of version apply operation.
#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    pub updated_files: Vec<PathBuf>,
    pub skipped_files: Vec<PathBuf>,
    pub conflicts: Vec<Conflict>,
    pub snapshot_id: Option<String>,
}

/// Conflict detected during version apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub component: String,
    pub file: String,
    pub kind: ConflictKind,
    pub details: String,
}

/// Type of conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictKind {
    VersionDowngrade {
        chart_name: String,
        from: String,
        to: String,
    },
    ImageDowngrade {
        image_name: String,
        from: String,
        to: String,
    },
    MissingInDest,
    MissingInSource,
}

// =============================================================================
// SNAPSHOT
// =============================================================================

/// Snapshot file containing all snapshots for a destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub version: u32,
    pub snapshots: Vec<Snapshot>,
}

impl Default for SnapshotFile {
    fn default() -> Self {
        Self {
            version: 1,
            snapshots: Vec::new(),
        }
    }
}

/// A single promotion snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub created_at: String,
    pub source_path: String,
    pub dest_path: String,
    pub filters: Vec<String>,
    pub version_changes: HashMap<String, Vec<FileVersionChange>>,
    pub files_modified: Vec<String>,
    pub status: SnapshotStatus,
}

impl Snapshot {
    pub fn new(source_path: String, dest_path: String, filters: Vec<String>) -> Self {
        let timestamp = time::OffsetDateTime::now_utc()
            .format(
                &time::format_description::parse("[year][month][day][hour][minute][second]")
                    .unwrap(),
            )
            .unwrap_or_default();

        Self {
            id: format!("snap-{}", timestamp),
            created_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            source_path,
            dest_path,
            filters,
            version_changes: HashMap::new(),
            files_modified: Vec::new(),
            status: SnapshotStatus::Applied,
        }
    }
}

/// File version change record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersionChange {
    pub file: String,
    pub kind: VersionChangeKind,
    pub name: String,
    pub before: String,
    pub after: String,
}

/// Kind of version change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionChangeKind {
    HelmChart,
    ContainerImage,
}

/// Snapshot status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnapshotStatus {
    Applied,
    RolledBack,
    Pending,
}
