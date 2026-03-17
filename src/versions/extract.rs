//! Version extraction from Kubernetes manifests.

use std::path::Path;

use tracing::info;
use walkdir::WalkDir;

use crate::error::AppResult;
use crate::versions::models::{
    ComponentVersions, ContainerImageVersion, HelmChartVersion, VersionReport,
};

/// Extract versions from a repository path.
pub fn extract_versions(root: &Path, filters: &[String]) -> AppResult<VersionReport> {
    let mut report = VersionReport::new(&root.display().to_string());

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative_str = relative.to_string_lossy();

        // Skip non-YAML files
        if !relative_str.ends_with(".yaml") && !relative_str.ends_with(".yml") {
            continue;
        }

        // Apply filters
        if !filters.is_empty() && !filters.iter().any(|f| relative_str.contains(f)) {
            continue;
        }

        // Skip common non-version files
        if should_skip_file(&relative_str) {
            continue;
        }

        let component_path = get_component_path(relative);

        if let Ok(content) = std::fs::read_to_string(path) {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();

            let versions = match file_name.as_ref() {
                "kustomization.yaml" | "kustomization.yml" => {
                    extract_from_kustomization(&content, &relative_str)
                }
                "Chart.yaml" | "Chart.yml" => extract_from_chart(&content, &relative_str),
                "values.yaml" | "values.yml" | "values-images.yaml" | "values-images.yml" => {
                    extract_from_values(&content, &relative_str)
                }
                _ => continue,
            };

            if !versions.helm_charts.is_empty() || !versions.container_images.is_empty() {
                let entry = report.components.entry(component_path.clone());
                let component = entry.or_insert_with(|| ComponentVersions {
                    path: component_path,
                    ..Default::default()
                });
                component.helm_charts.extend(versions.helm_charts);
                component.container_images.extend(versions.container_images);
            }
        }
    }

    info!(
        "Extracted versions from {} components",
        report.components.len()
    );
    Ok(report)
}

/// Check if file should be skipped.
fn should_skip_file(path: &str) -> bool {
    let skip_patterns = [
        "/custom/",
        "/test/",
        "/tests/",
        "/templates/",
        "secrets",
        "secret",
    ];

    skip_patterns.iter().any(|p| path.contains(p))
}

/// Get component path (parent directory of the file).
fn get_component_path(file_path: &Path) -> String {
    file_path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

/// Extract versions from kustomization.yaml helmCharts.
fn extract_from_kustomization(content: &str, source_file: &str) -> ComponentVersions {
    let mut versions = ComponentVersions::default();

    // Parse as generic YAML to extract helmCharts
    if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        if let Some(helm_charts) = doc.get("helmCharts").and_then(|v| v.as_sequence()) {
            for chart in helm_charts {
                if let (Some(name), Some(version)) = (
                    chart.get("name").and_then(|v| v.as_str()),
                    chart.get("version").and_then(|v| v.as_str()),
                ) {
                    let repo = chart
                        .get("repo")
                        .and_then(|v| v.as_str())
                        .or_else(|| chart.get("repository").and_then(|v| v.as_str()))
                        .map(|s| s.to_string());

                    versions.helm_charts.push(HelmChartVersion {
                        name: name.to_string(),
                        version: version.to_string(),
                        repository: repo,
                        source_file: source_file.to_string(),
                    });
                }
            }
        }
    }

    versions
}

/// Extract versions from Chart.yaml dependencies.
fn extract_from_chart(content: &str, source_file: &str) -> ComponentVersions {
    let mut versions = ComponentVersions::default();

    #[derive(Debug, serde::Deserialize)]
    struct Chart {
        #[serde(default)]
        dependencies: Vec<Dependency>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct Dependency {
        name: String,
        version: String,
        #[serde(default)]
        repository: String,
    }

    if let Ok(chart) = serde_yaml::from_str::<Chart>(content) {
        for dep in chart.dependencies {
            versions.helm_charts.push(HelmChartVersion {
                name: dep.name,
                version: dep.version,
                repository: if dep.repository.is_empty() {
                    None
                } else {
                    Some(dep.repository)
                },
                source_file: source_file.to_string(),
            });
        }
    }

    versions
}

/// Extract container image tags from values files.
fn extract_from_values(content: &str, source_file: &str) -> ComponentVersions {
    let mut versions = ComponentVersions::default();

    if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        extract_images_recursive(&doc, source_file, "", &mut versions.container_images);
    }

    versions
}

/// Recursively extract image references from YAML structure.
fn extract_images_recursive(
    value: &serde_yaml::Value,
    source_file: &str,
    path: &str,
    images: &mut Vec<ContainerImageVersion>,
) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            // Check for image structure: { repository: "...", tag: "..." }
            if let (Some(repo), Some(tag)) = (
                map.get(serde_yaml::Value::String("repository".to_string()))
                    .and_then(|v| v.as_str()),
                map.get(serde_yaml::Value::String("tag".to_string()))
                    .and_then(|v| v.as_str()),
            ) {
                // Only include if tag looks like a version (not latest, not empty)
                if !tag.is_empty() && tag != "latest" {
                    images.push(ContainerImageVersion {
                        name: repo.to_string(),
                        tag: tag.to_string(),
                        source_file: source_file.to_string(),
                        json_path: if path.is_empty() {
                            "image".to_string()
                        } else {
                            format!("{}.image", path)
                        },
                    });
                }
            }

            // Also check for just "image: repo:tag" format
            if let Some(image) = map.get(serde_yaml::Value::String("image".to_string())) {
                if let Some(image_str) = image.as_str() {
                    if let Some((repo, tag)) = parse_image_string(image_str) {
                        images.push(ContainerImageVersion {
                            name: repo,
                            tag,
                            source_file: source_file.to_string(),
                            json_path: if path.is_empty() {
                                "image".to_string()
                            } else {
                                format!("{}.image", path)
                            },
                        });
                    }
                }
            }

            // Recurse into nested structures
            for (key, val) in map {
                if let Some(key_str) = key.as_str() {
                    let new_path = if path.is_empty() {
                        key_str.to_string()
                    } else {
                        format!("{}.{}", path, key_str)
                    };
                    extract_images_recursive(val, source_file, &new_path, images);
                }
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for (i, item) in seq.iter().enumerate() {
                let new_path = format!("{}[{}]", path, i);
                extract_images_recursive(item, source_file, &new_path, images);
            }
        }
        _ => {}
    }
}

/// Parse image string "repo:tag" or "repo@digest".
fn parse_image_string(s: &str) -> Option<(String, String)> {
    // Handle digest format: repo@sha256:xxx
    if let Some(at_pos) = s.find('@') {
        let repo = &s[..at_pos];
        let digest = &s[at_pos + 1..];
        return Some((repo.to_string(), digest.to_string()));
    }

    // Handle tag format: repo:tag
    // Find last : that's not part of a port number or registry
    let last_colon = s.rfind(':')?;

    // Check if it looks like a port (followed by digits only, or part of registry)
    let after_colon = &s[last_colon + 1..];
    if after_colon.chars().all(|c| c.is_ascii_digit()) && after_colon.len() <= 5 {
        // Looks like a port, check if there's another colon
        let before = &s[..last_colon];
        if let Some(second_colon) = before.rfind(':') {
            let repo = &s[..second_colon];
            let tag = &s[second_colon + 1..];
            return Some((repo.to_string(), tag.to_string()));
        }
        return None;
    }

    let repo = &s[..last_colon];
    let tag = &s[last_colon + 1..];

    if !repo.is_empty() && !tag.is_empty() {
        Some((repo.to_string(), tag.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_string() {
        assert_eq!(
            parse_image_string("nginx:1.25"),
            Some(("nginx".to_string(), "1.25".to_string()))
        );
        assert_eq!(
            parse_image_string("ghcr.io/home-operations/home-assistant:2026.3.1"),
            Some((
                "ghcr.io/home-operations/home-assistant".to_string(),
                "2026.3.1".to_string()
            ))
        );
        assert_eq!(
            parse_image_string("ghcr.io/repo/image@sha256:abc123"),
            Some((
                "ghcr.io/repo/image".to_string(),
                "sha256:abc123".to_string()
            ))
        );
    }

    #[test]
    fn test_extract_from_kustomization() {
        let content = r#"
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
helmCharts:
  - name: postgres-operator
    version: 1.15.1
    repo: https://opensource.zalando.com/postgres-operator/charts/postgres-operator/
"#;
        let versions = extract_from_kustomization(content, "test.yaml");
        assert_eq!(versions.helm_charts.len(), 1);
        assert_eq!(versions.helm_charts[0].name, "postgres-operator");
        assert_eq!(versions.helm_charts[0].version, "1.15.1");
    }

    #[test]
    fn test_extract_from_chart() {
        let content = r#"
apiVersion: v2
name: vault
version: 0.0.0
dependencies:
  - name: vault-operator
    version: 1.23.4
    repository: oci://ghcr.io/bank-vaults/helm-charts
"#;
        let versions = extract_from_chart(content, "Chart.yaml");
        assert_eq!(versions.helm_charts.len(), 1);
        assert_eq!(versions.helm_charts[0].name, "vault-operator");
        assert_eq!(versions.helm_charts[0].version, "1.23.4");
    }

    #[test]
    fn test_extract_from_values() {
        let content = r#"
image:
  repository: ghcr.io/jellyfin/jellyfin
  tag: 10.11.6
controllers:
  main:
    containers:
      main:
        image:
          repository: ghcr.io/other/app
          tag: 1.2.3
"#;
        let versions = extract_from_values(content, "values.yaml");
        assert_eq!(versions.container_images.len(), 2);
    }
}
