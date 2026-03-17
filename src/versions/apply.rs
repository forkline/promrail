//! Version application to repository files.

use std::path::Path;

use tracing::{info, warn};

use crate::error::AppResult;
use crate::versions::conflict::detect_conflicts;
use crate::versions::models::{
    ApplyOptions, ApplyResult, ComponentVersions, FileVersionChange, Snapshot, VersionChangeKind,
    VersionReport,
};
use crate::versions::snapshot;

/// Apply versions from a report to a destination repository.
pub fn apply_versions(
    report: &VersionReport,
    dest: &Path,
    options: &ApplyOptions,
) -> AppResult<ApplyResult> {
    let mut result = ApplyResult::default();

    // Check conflicts if requested
    if options.check_conflicts && !options.dry_run {
        let dest_report = super::extract::extract_versions(dest, &[])?;
        result.conflicts = detect_conflicts(report, &dest_report);

        if !result.conflicts.is_empty() {
            warn!("Conflicts detected!");
            for conflict in &result.conflicts {
                warn!(
                    "  {} in {}: {}",
                    conflict.component, conflict.file, conflict.details
                );
            }
            return Ok(result);
        }
    }

    // Create snapshot if requested
    let mut snapshot_obj = if options.create_snapshot && !options.dry_run {
        Some(snapshot::create(
            dest,
            report.source_path.clone(),
            dest.display().to_string(),
            vec![],
        )?)
    } else {
        None
    };

    // Filter components
    let components_to_apply: Vec<&String> = if options.components.is_empty() {
        report.components.keys().collect()
    } else {
        report
            .components
            .keys()
            .filter(|k| {
                options
                    .components
                    .iter()
                    .any(|f| k.contains(f) || f.contains(k.as_str()))
            })
            .collect()
    };

    info!("Applying {} components", components_to_apply.len());

    for component_path in components_to_apply {
        if let Some(versions) = report.components.get(component_path) {
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
                        options.dry_run,
                        component_path,
                        snapshot_obj.as_mut(),
                    )?
                {
                    info!(
                        "Updated helm chart versions in {}",
                        kustomization_path.display()
                    );
                    result.updated_files.push(kustomization_path);
                }
            }

            // Update Chart.yaml
            if !versions.helm_charts.is_empty() {
                let chart_path = component_dir.join("Chart.yaml");
                if chart_path.exists()
                    && update_chart_versions(
                        &chart_path,
                        &versions.helm_charts,
                        options.dry_run,
                        component_path,
                        snapshot_obj.as_mut(),
                    )?
                {
                    info!("Updated chart dependencies in {}", chart_path.display());
                    result.updated_files.push(chart_path);
                }
            }

            // Update values.yaml
            if !versions.container_images.is_empty() {
                let values_path = component_dir.join("values.yaml");
                if values_path.exists()
                    && update_values_images(
                        &values_path,
                        &versions.container_images,
                        options.dry_run,
                        component_path,
                        snapshot_obj.as_mut(),
                    )?
                {
                    info!("Updated image tags in {}", values_path.display());
                    result.updated_files.push(values_path);
                }
            }
        }
    }

    // Save snapshot if created
    if let Some(snap) = &snapshot_obj {
        result.snapshot_id = Some(snap.id.clone());
        if !options.dry_run {
            snapshot::add(dest, snap.clone())?;
            info!("Created snapshot: {}", snap.id);
        }
    }

    Ok(result)
}

/// Update helm chart versions in kustomization.yaml.
fn update_kustomization_versions(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    dry_run: bool,
    component: &str,
    snapshot: Option<&mut Snapshot>,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let mut changed = false;
    let mut changes = Vec::new();

    if let Some(helm_charts) = doc.get_mut("helmCharts").and_then(|v| v.as_sequence_mut()) {
        for chart in helm_charts {
            let name = chart
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(name) = name
                && let Some(new_version) = charts.iter().find(|c| c.name == name)
                && let Some(version) = chart.get_mut("version")
            {
                let current = version.as_str().unwrap_or("");
                if current != new_version.version {
                    changes.push(FileVersionChange {
                        file: "kustomization.yaml".to_string(),
                        kind: VersionChangeKind::HelmChart,
                        name: name.clone(),
                        before: current.to_string(),
                        after: new_version.version.clone(),
                    });
                    *version = serde_yaml::Value::String(new_version.version.clone());
                    changed = true;
                }
            }
        }
    }

    if changed {
        if let Some(snap) = snapshot {
            snap.version_changes
                .entry(component.to_string())
                .or_default()
                .extend(changes);
            snap.files_modified.push(path.display().to_string());
        }

        if !dry_run {
            let new_content = serde_yaml::to_string(&doc)?;
            std::fs::write(path, new_content)?;
        }
    }

    Ok(changed)
}

/// Update chart dependency versions in Chart.yaml.
fn update_chart_versions(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    dry_run: bool,
    component: &str,
    snapshot: Option<&mut Snapshot>,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let mut changed = false;
    let mut changes = Vec::new();

    if let Some(dependencies) = doc
        .get_mut("dependencies")
        .and_then(|v| v.as_sequence_mut())
    {
        for dep in dependencies {
            let name = dep.get("name").and_then(|v| v.as_str()).map(str::to_string);
            if let Some(name) = name
                && let Some(new_version) = charts.iter().find(|c| c.name == name)
                && let Some(version) = dep.get_mut("version")
            {
                let current = version.as_str().unwrap_or("");
                if current != new_version.version {
                    changes.push(FileVersionChange {
                        file: "Chart.yaml".to_string(),
                        kind: VersionChangeKind::HelmChart,
                        name: name.clone(),
                        before: current.to_string(),
                        after: new_version.version.clone(),
                    });
                    *version = serde_yaml::Value::String(new_version.version.clone());
                    changed = true;
                }
            }
        }
    }

    if changed {
        if let Some(snap) = snapshot {
            snap.version_changes
                .entry(component.to_string())
                .or_default()
                .extend(changes);
            snap.files_modified.push(path.display().to_string());
        }

        if !dry_run {
            let new_content = serde_yaml::to_string(&doc)?;
            std::fs::write(path, new_content)?;
        }
    }

    Ok(changed)
}

/// Update container image tags in values.yaml.
fn update_values_images(
    path: &Path,
    images: &[crate::versions::models::ContainerImageVersion],
    dry_run: bool,
    component: &str,
    snapshot: Option<&mut Snapshot>,
) -> AppResult<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let changes = update_images_recursive(&mut doc, images);
    let changed = !changes.is_empty();

    if changed {
        if let Some(snap) = snapshot {
            snap.version_changes
                .entry(component.to_string())
                .or_default()
                .extend(changes);
            snap.files_modified.push(path.display().to_string());
        }

        if !dry_run {
            let new_content = serde_yaml::to_string(&doc)?;
            std::fs::write(path, new_content)?;
        }
    }

    Ok(changed)
}

/// Recursively update image tags in YAML structure.
fn update_images_recursive(
    value: &mut serde_yaml::Value,
    images: &[crate::versions::models::ContainerImageVersion],
) -> Vec<FileVersionChange> {
    let mut changes = Vec::new();

    if let serde_yaml::Value::Mapping(map) = value {
        // Check for image structure: { repository: "...", tag: "..." }
        if let (Some(repo), Some(current_tag)) = (
            map.get(serde_yaml::Value::String("repository".to_string()))
                .and_then(|v| v.as_str()),
            map.get(serde_yaml::Value::String("tag".to_string()))
                .and_then(|v| v.as_str()),
        ) && let Some(new_image) = images.iter().find(|i| i.name == repo)
            && current_tag != new_image.tag
        {
            changes.push(FileVersionChange {
                file: "values.yaml".to_string(),
                kind: VersionChangeKind::ContainerImage,
                name: repo.to_string(),
                before: current_tag.to_string(),
                after: new_image.tag.clone(),
            });
            map.insert(
                serde_yaml::Value::String("tag".to_string()),
                serde_yaml::Value::String(new_image.tag.clone()),
            );
        }

        // Recurse into nested structures
        for (_, val) in map.iter_mut() {
            changes.extend(update_images_recursive(val, images));
        }
    } else if let serde_yaml::Value::Sequence(seq) = value {
        for item in seq.iter_mut() {
            changes.extend(update_images_recursive(item, images));
        }
    }

    changes
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
        if let Some(dest_chart) = dest.helm_charts.iter().find(|c| c.name == src_chart.name)
            && src_chart.version != dest_chart.version
        {
            helm_chart_diffs.push(HelmChartDiff {
                name: src_chart.name.clone(),
                source_version: src_chart.version.clone(),
                dest_version: dest_chart.version.clone(),
                changed: true,
            });
        }
    }

    let mut container_image_diffs = Vec::new();

    for src_image in &source.container_images {
        if let Some(dest_image) = dest
            .container_images
            .iter()
            .find(|i| i.name == src_image.name)
            && src_image.tag != dest_image.tag
        {
            container_image_diffs.push(ContainerImageDiff {
                name: src_image.name.clone(),
                source_tag: src_image.tag.clone(),
                dest_tag: dest_image.tag.clone(),
                changed: true,
            });
        }
    }

    VersionDiff {
        component: component.to_string(),
        helm_charts: helm_chart_diffs,
        container_images: container_image_diffs,
    }
}
