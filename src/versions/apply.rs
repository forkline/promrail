//! Version application to repository files.

use std::path::Path;
use std::process::Command;

use log::{info, warn};

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
            write_kustomization_yaml(path, charts, &doc)?;
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
            write_chart_yaml(path, charts, &doc)?;
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
            write_values_yaml(path, images, &doc)?;
        }
    }

    Ok(changed)
}

fn write_kustomization_yaml(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    fallback_doc: &serde_yaml::Value,
) -> AppResult<()> {
    let updates: Vec<_> = charts
        .iter()
        .map(|chart| serde_json::json!({"name": chart.name, "version": chart.version}))
        .collect();
    write_yaml_roundtrip(path, "kustomization", updates, fallback_doc)
}

fn write_chart_yaml(
    path: &Path,
    charts: &[crate::versions::models::HelmChartVersion],
    fallback_doc: &serde_yaml::Value,
) -> AppResult<()> {
    let updates: Vec<_> = charts
        .iter()
        .map(|chart| serde_json::json!({"name": chart.name, "version": chart.version}))
        .collect();
    write_yaml_roundtrip(path, "chart", updates, fallback_doc)
}

fn write_values_yaml(
    path: &Path,
    images: &[crate::versions::models::ContainerImageVersion],
    fallback_doc: &serde_yaml::Value,
) -> AppResult<()> {
    let updates: Vec<_> = images
        .iter()
        .map(|image| serde_json::json!({"name": image.name, "tag": image.tag}))
        .collect();
    write_yaml_roundtrip(path, "values", updates, fallback_doc)
}

fn write_yaml_roundtrip(
    path: &Path,
    mode: &str,
    updates: Vec<serde_json::Value>,
    fallback_doc: &serde_yaml::Value,
) -> AppResult<()> {
    match roundtrip_update_yaml(path, mode, updates) {
        Ok(content) => {
            std::fs::write(path, content)?;
            Ok(())
        }
        Err(crate::error::PromrailError::ReviewArtifactInvalid(message))
            if message.contains("No module named 'ruamel'")
                || message.contains("No module named \"ruamel\"") =>
        {
            std::fs::write(path, serde_yaml::to_string(fallback_doc)?)?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn roundtrip_update_yaml(
    path: &Path,
    mode: &str,
    updates: Vec<serde_json::Value>,
) -> AppResult<Vec<u8>> {
    let updates_json = serde_json::to_string(&updates)?;
    let script = r#"
import json
import sys
from pathlib import Path
from ruamel.yaml import YAML


def update_kustomization(doc, updates):
    mapping = {item['name']: item['version'] for item in updates}
    for chart in doc.get('helmCharts', []) or []:
        name = chart.get('name')
        if name in mapping:
            chart['version'] = mapping[name]


def update_chart(doc, updates):
    mapping = {item['name']: item['version'] for item in updates}
    for dep in doc.get('dependencies', []) or []:
        name = dep.get('name')
        if name in mapping:
            dep['version'] = mapping[name]


def update_values(node, mapping):
    if isinstance(node, dict):
        repo = node.get('repository')
        tag = node.get('tag')
        if repo in mapping and tag != mapping[repo]:
            node['tag'] = mapping[repo]
        for value in node.values():
            update_values(value, mapping)
    elif isinstance(node, list):
        for item in node:
            update_values(item, mapping)


path = Path(sys.argv[1])
mode = sys.argv[2]
updates = json.loads(sys.argv[3])

yaml = YAML()
yaml.preserve_quotes = True
yaml.indent(mapping=2, sequence=4, offset=2)
yaml.width = 4096

text = path.read_text(encoding='utf-8')
yaml.explicit_start = text.lstrip().startswith('---')
doc = yaml.load(text)

if mode == 'kustomization':
    update_kustomization(doc, updates)
elif mode == 'chart':
    update_chart(doc, updates)
elif mode == 'values':
    update_values(doc, {item['name']: item['tag'] for item in updates})
else:
    raise SystemExit(f'unknown mode: {mode}')

yaml.dump(doc, sys.stdout)
"#;

    let output = Command::new("python")
        .arg("-c")
        .arg(script)
        .arg(path)
        .arg(mode)
        .arg(updates_json)
        .output()
        .map_err(|err| {
            crate::error::PromrailError::ReviewArtifactInvalid(format!(
                "failed to run python yaml roundtrip helper: {}",
                err
            ))
        })?;

    if !output.status.success() {
        return Err(crate::error::PromrailError::ReviewArtifactInvalid(format!(
            "python yaml roundtrip helper failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(output.stdout)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_update_kustomization_versions_preserves_formatting_when_ruamel_available() {
        let has_ruamel = Command::new("python")
            .args(["-c", "import ruamel.yaml"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("kustomization.yaml");
        let content = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: external-secrets

resources:
  - resources/clustersecretstore.yaml

helmCharts:
  - includeCRDs: true
    name: external-secrets
    namespace: external-secrets
    releaseName: external-secrets
    repo: https://charts.external-secrets.io
    valuesFile: values.yaml
    version: 2.1.0

patches:
  - patch: |-
      - op: add
        path: "/metadata/annotations/argocd.argoproj.io~1sync-options"
        value: "Replace=true"
    target:
      group: apiextensions.k8s.io
      kind: CustomResourceDefinition
"#;
        fs::write(&path, content).expect("write kustomization");

        let charts = vec![crate::versions::models::HelmChartVersion {
            name: "external-secrets".to_string(),
            version: "2.4.0".to_string(),
            repository: Some("https://charts.external-secrets.io".to_string()),
            source_file: "kustomization.yaml".to_string(),
        }];

        let changed =
            update_kustomization_versions(&path, &charts, false, "platform/external-secrets", None)
                .expect("update should succeed");
        assert!(changed);

        let updated = fs::read_to_string(&path).expect("read updated kustomization");
        assert!(updated.contains("version: 2.4.0"));
        if has_ruamel {
            assert!(updated.contains(
                "resources:\n  - resources/clustersecretstore.yaml\n\nhelmCharts:\n  - includeCRDs: true"
            ));
            assert!(updated.contains("patches:\n  - patch: |-\n      - op: add"));
        }
    }
}
