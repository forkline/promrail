//! Version data models.

use std::collections::HashMap;

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

/// Version difference between source and destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub component: String,
    pub helm_charts: Vec<HelmChartDiff>,
    pub container_images: Vec<ContainerImageDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmChartDiff {
    pub name: String,
    pub source_version: String,
    pub dest_version: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerImageDiff {
    pub name: String,
    pub source_tag: String,
    pub dest_tag: String,
    pub changed: bool,
}

/// Parsed kustomization.yaml helmCharts entry.
#[derive(Debug, Clone, Deserialize)]
pub struct HelmChartEntry {
    pub name: Option<String>,
    pub version: Option<String>,
    pub repo: Option<String>,
    pub repository: Option<String>,
}

/// Parsed Chart.yaml dependency entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ChartDependency {
    pub name: String,
    pub version: String,
    pub repository: String,
}

/// Parsed image reference from values files.
#[derive(Debug, Clone)]
pub struct ImageRef {
    pub repository: String,
    pub tag: String,
    pub json_path: String,
}
