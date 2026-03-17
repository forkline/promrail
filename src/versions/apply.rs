//! Version application to repository files.

use std::path::Path;

use tracing::{info, warn};

use crate::error::AppResult;
use crate::versions::models::{ComponentVersions, VersionReport};

/// Apply versions from a report to a destination repository.
pub fn apply_versions(report: &VersionReport, dest: &Path, dry_run: bool) -> AppResult<usize> {
    let mut updated = 0;

    for (component_path, versions) in &report.components {
        let component_dir = dest.join(component_path);

        if !component_dir.exists() {
            warn!("Component directory not found: {}", component_dir.display());
            continue;
        }

        // Update kustomization.yaml
        if !versions.helm_charts.is_empty() {
            let kustomization_path = component_dir.join("kustomization.yaml");
            if kustomization_path.exists()
                && update_kustomization_versions(
                    &kustomization_path,
                    &versions.helm_charts,
                    dry_run,
                )?
            {
                info!(
                    "Updated helm chart versions in {}",
                    kustomization_path.display()
                );
                updated += 1;
            }
        }

        // Update Chart.yaml
        if !versions.helm_charts.is_empty() {
            let chart_path = component_dir.join("Chart.yaml");
            if chart_path.exists()
                && update_chart_versions(&chart_path, &versions.helm_charts, dry_run)?
            {
                info!("Updated chart dependencies in {}", chart_path.display());
                updated += 1;
            }
        }

        // Update values.yaml
        if !versions.container_images.is_empty() {
            let values_path = component_dir.join("values.yaml");
            if values_path.exists()
                && update_values_images(&values_path, &versions.container_images, dry_run)?
            {
                info!("Updated image tags in {}", values_path.display());
                updated += 1;
            }
        }
    }

    Ok(updated)
}

/// Update helm chart versions in kustomization.yaml.
fn update_kustomization_versions(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    dry_run: bool,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let mut changed = false;

    if let Some(helm_charts) = doc.get_mut("helmCharts").and_then(|v| v.as_sequence_mut()) {
        for chart in helm_charts {
            if let Some(name) = chart.get("name").and_then(|v| v.as_str()) {
                if let Some(new_version) = charts.iter().find(|c| c.name == name) {
                    if let Some(version) = chart.get_mut("version") {
                        let current = version.as_str().unwrap_or("");
                        if current != new_version.version {
                            *version = serde_yaml::Value::String(new_version.version.clone());
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    if changed && !dry_run {
        let new_content = serde_yaml::to_string(&doc)?;
        std::fs::write(path, new_content)?;
    }

    Ok(changed)
}

/// Update chart dependency versions in Chart.yaml.
fn update_chart_versions(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    dry_run: bool,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let mut changed = false;

    if let Some(dependencies) = doc
        .get_mut("dependencies")
        .and_then(|v| v.as_sequence_mut())
    {
        for dep in dependencies {
            if let Some(name) = dep.get("name").and_then(|v| v.as_str()) {
                if let Some(new_version) = charts.iter().find(|c| c.name == name) {
                    if let Some(version) = dep.get_mut("version") {
                        let current = version.as_str().unwrap_or("");
                        if current != new_version.version {
                            *version = serde_yaml::Value::String(new_version.version.clone());
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    if changed && !dry_run {
        let new_content = serde_yaml::to_string(&doc)?;
        std::fs::write(path, new_content)?;
    }

    Ok(changed)
}

/// Update container image tags in values.yaml.
fn update_values_images(
    path: &Path,
    images: &[crate::versions::models::ContainerImageVersion],
    dry_run: bool,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let changed = update_images_recursive(&mut doc, images, "");

    if changed && !dry_run {
        let new_content = serde_yaml::to_string(&doc)?;
        std::fs::write(path, new_content)?;
    }

    Ok(changed)
}

/// Recursively update image tags in YAML structure.
fn update_images_recursive(
    value: &mut serde_yaml::Value,
    images: &[crate::versions::models::ContainerImageVersion],
    _path: &str,
) -> bool {
    let mut changed = false;

    if let serde_yaml::Value::Mapping(ref mut map) = value {
        // Check for image structure: { repository: "...", tag: "..." }
        if let (Some(repo), Some(current_tag)) = (
            map.get(serde_yaml::Value::String("repository".to_string()))
                .and_then(|v| v.as_str()),
            map.get(serde_yaml::Value::String("tag".to_string()))
                .and_then(|v| v.as_str()),
        ) {
            if let Some(new_image) = images.iter().find(|i| i.name == repo) {
                if current_tag != new_image.tag {
                    map.insert(
                        serde_yaml::Value::String("tag".to_string()),
                        serde_yaml::Value::String(new_image.tag.clone()),
                    );
                    changed = true;
                }
            }
        }

        // Recurse into nested structures
        for (_, val) in map.iter_mut() {
            if update_images_recursive(val, images, "") {
                changed = true;
            }
        }
    } else if let serde_yaml::Value::Sequence(ref mut seq) = value {
        for item in seq.iter_mut() {
            if update_images_recursive(item, images, "") {
                changed = true;
            }
        }
    }

    changed
}

/// Compare versions between two reports.
pub fn diff_versions(
    source: &VersionReport,
    dest: &VersionReport,
) -> Vec<crate::versions::models::VersionDiff> {
    let mut diffs = Vec::new();

    for (component, source_versions) in &source.components {
        if let Some(dest_versions) = dest.components.get(component) {
            let diff = compare_components(component, source_versions, dest_versions);
            if !diff.helm_charts.is_empty() || !diff.container_images.is_empty() {
                diffs.push(diff);
            }
        }
    }

    diffs
}

fn compare_components(
    component: &str,
    source: &ComponentVersions,
    dest: &ComponentVersions,
) -> crate::versions::models::VersionDiff {
    use crate::versions::models::{ContainerImageDiff, HelmChartDiff, VersionDiff};

    let mut helm_chart_diffs = Vec::new();

    for src_chart in &source.helm_charts {
        if let Some(dest_chart) = dest.helm_charts.iter().find(|c| c.name == src_chart.name) {
            if src_chart.version != dest_chart.version {
                helm_chart_diffs.push(HelmChartDiff {
                    name: src_chart.name.clone(),
                    source_version: src_chart.version.clone(),
                    dest_version: dest_chart.version.clone(),
                    changed: true,
                });
            }
        }
    }

    let mut container_image_diffs = Vec::new();

    for src_image in &source.container_images {
        if let Some(dest_image) = dest
            .container_images
            .iter()
            .find(|i| i.name == src_image.name)
        {
            if src_image.tag != dest_image.tag {
                container_image_diffs.push(ContainerImageDiff {
                    name: src_image.name.clone(),
                    source_tag: src_image.tag.clone(),
                    dest_tag: dest_image.tag.clone(),
                    changed: true,
                });
            }
        }
    }

    VersionDiff {
        component: component.to_string(),
        helm_charts: helm_chart_diffs,
        container_images: container_image_diffs,
    }
}
