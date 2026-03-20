//! Version extraction and manipulation for GitOps repositories.
//!
//! This module provides functionality to:
//! - Extract versions from kustomization.yaml, Chart.yaml, and values.yaml
//! - Apply versions to destination repositories
//! - Detect conflicts (downgrades, missing components)
//! - Create and manage snapshots
//! - Merge versions from multiple sources

pub mod apply;
pub mod conflict;
pub mod extract;
pub mod merge;
pub mod models;
pub mod snapshot;

pub use apply::apply_versions;
pub use extract::extract_versions;
pub use merge::{explain_merge, merge_versions};
pub use models::{ApplyOptions, SnapshotStatus, VersionReport};
pub use snapshot::{get, list, rollback};
