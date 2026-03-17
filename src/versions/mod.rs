//! Version extraction and manipulation for GitOps repositories.
//!
//! This module provides functionality to:
//! - Extract versions from kustomization.yaml, Chart.yaml, and values.yaml
//! - Apply versions to destination repositories
//! - Compare versions between repositories
//! - Detect conflicts (downgrades, missing components)
//! - Create and manage snapshots
//! - Diff configuration files

pub mod apply;
pub mod config_diff;
pub mod conflict;
pub mod extract;
pub mod models;
pub mod snapshot;

pub use apply::{apply_versions, diff_versions};
pub use config_diff::{diff_configs, format_unified_diff};
pub use extract::extract_versions;
pub use models::{ApplyOptions, SnapshotStatus, VersionReport};
pub use snapshot::{delete, get, list, rollback};
