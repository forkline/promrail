//! Version extraction and manipulation for GitOps repositories.
//!
//! This module provides functionality to:
//! - Extract versions from kustomization.yaml, Chart.yaml, and values.yaml
//! - Apply versions to destination repositories
//! - Compare versions between repositories

pub mod apply;
pub mod extract;
pub mod models;

pub use apply::{apply_versions, diff_versions};
pub use extract::extract_versions;
pub use models::VersionReport;
