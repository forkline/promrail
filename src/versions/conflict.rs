//! Conflict detection for version apply operations.

use crate::config::{PromotionRules, SourceChangeHandling};
use crate::versions::models::{ComponentVersions, Conflict, ConflictKind, VersionReport};
use log::warn;

/// Detect conflicts between source versions and destination versions.
pub fn detect_conflicts(source: &VersionReport, dest: &VersionReport) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    for (component, source_versions) in &source.components {
        if let Some(dest_versions) = dest.components.get(component) {
            check_component_conflicts(component, source_versions, dest_versions, &mut conflicts);
        }
    }

    conflicts
}

/// Filter source change conflicts based on component rules.
/// Returns filtered conflicts and a flag indicating if any blocking conflicts exist.
pub fn filter_source_change_conflicts(
    conflicts: Vec<Conflict>,
    rules: &PromotionRules,
) -> (Vec<Conflict>, bool) {
    let mut filtered = Vec::new();
    let mut has_blocking = false;

    for conflict in conflicts {
        match &conflict.kind {
            ConflictKind::RepositoryChange { .. } | ConflictKind::RegistryChange { .. } => {
                let handling = get_source_change_handling(rules, &conflict.component);

                match handling {
                    SourceChangeHandling::Ignore => {
                        continue;
                    }
                    SourceChangeHandling::Warn => {
                        warn!(
                            "Source change detected for {}: {}",
                            conflict.component, conflict.details
                        );
                        filtered.push(conflict);
                    }
                    SourceChangeHandling::Review => {
                        warn!(
                            "Source change requires review for {}: {}",
                            conflict.component, conflict.details
                        );
                        filtered.push(conflict);
                        has_blocking = true;
                    }
                    SourceChangeHandling::Block => {
                        warn!(
                            "Source change blocked for {}: {}",
                            conflict.component, conflict.details
                        );
                        filtered.push(conflict);
                        has_blocking = true;
                    }
                }
            }
            _ => {
                filtered.push(conflict);
            }
        }
    }

    (filtered, has_blocking)
}

/// Get source change handling for a component.
/// Returns the handling policy from component rules, or default (Warn) if not specified.
fn get_source_change_handling(rules: &PromotionRules, component: &str) -> SourceChangeHandling {
    rules
        .get_component_rule(component)
        .map(|r| r.source_change_handling.clone())
        .unwrap_or_default()
}

fn check_component_conflicts(
    component: &str,
    source: &ComponentVersions,
    dest: &ComponentVersions,
    conflicts: &mut Vec<Conflict>,
) {
    // Check helm chart version conflicts
    for src_chart in &source.helm_charts {
        if let Some(dest_chart) = dest.helm_charts.iter().find(|c| c.name == src_chart.name) {
            // Check for repository change first
            if let (Some(src_repo), Some(dest_repo)) =
                (&src_chart.repository, &dest_chart.repository)
            {
                if !repositories_equal(src_repo, dest_repo) {
                    conflicts.push(Conflict {
                        component: component.to_string(),
                        file: src_chart.source_file.clone(),
                        kind: ConflictKind::RepositoryChange {
                            chart_name: src_chart.name.clone(),
                            from_repo: dest_repo.clone(),
                            to_repo: src_repo.clone(),
                        },
                        details: format!(
                            "Repository change for {}: {} -> {}",
                            src_chart.name, dest_repo, src_repo
                        ),
                    });
                }
            }

            // Check for version downgrade
            if let Some(ordering) = compare_versions(&src_chart.version, &dest_chart.version)
                && ordering == std::cmp::Ordering::Less
            {
                conflicts.push(Conflict {
                    component: component.to_string(),
                    file: src_chart.source_file.clone(),
                    kind: ConflictKind::VersionDowngrade {
                        chart_name: src_chart.name.clone(),
                        from: dest_chart.version.clone(),
                        to: src_chart.version.clone(),
                    },
                    details: format!(
                        "Downgrading {} from {} to {}",
                        src_chart.name, dest_chart.version, src_chart.version
                    ),
                });
            }
        }
    }

    // Check container image tag conflicts
    for src_image in &source.container_images {
        // First try exact name match
        if let Some(dest_image) = dest
            .container_images
            .iter()
            .find(|i| i.name == src_image.name)
        {
            // Check for registry change first (only relevant when names are identical
            // but one has implicit registry and one explicit)
            let src_registry = extract_registry(&src_image.name);
            let dest_registry = extract_registry(&dest_image.name);
            if src_registry != dest_registry {
                let details = format!(
                    "Registry change for {}: {} -> {}",
                    src_image.name, dest_registry, src_registry
                );
                conflicts.push(Conflict {
                    component: component.to_string(),
                    file: src_image.source_file.clone(),
                    kind: ConflictKind::RegistryChange {
                        image_name: src_image.name.clone(),
                        from_registry: dest_registry,
                        to_registry: src_registry,
                    },
                    details,
                });
            }

            // Check for tag downgrade
            if let Some(ordering) = compare_versions(&src_image.tag, &dest_image.tag)
                && ordering == std::cmp::Ordering::Less
            {
                conflicts.push(Conflict {
                    component: component.to_string(),
                    file: src_image.source_file.clone(),
                    kind: ConflictKind::ImageDowngrade {
                        image_name: src_image.name.clone(),
                        from: dest_image.tag.clone(),
                        to: src_image.tag.clone(),
                    },
                    details: format!(
                        "Downgrading {} from {} to {}",
                        src_image.name, dest_image.tag, src_image.tag
                    ),
                });
            }
        } else {
            // No exact match - check for registry change by comparing core image paths
            let src_core = extract_image_path(&src_image.name);
            if let Some(dest_image) = dest
                .container_images
                .iter()
                .find(|i| extract_image_path(&i.name) == src_core)
            {
                let src_registry = extract_registry(&src_image.name);
                let dest_registry = extract_registry(&dest_image.name);
                let details = format!(
                    "Registry change for {}: {} ({}) -> {} ({})",
                    src_core, dest_image.name, dest_registry, src_image.name, src_registry
                );
                conflicts.push(Conflict {
                    component: component.to_string(),
                    file: src_image.source_file.clone(),
                    kind: ConflictKind::RegistryChange {
                        image_name: src_image.name.clone(),
                        from_registry: dest_registry,
                        to_registry: src_registry,
                    },
                    details,
                });
            }
        }
    }
}

/// Normalize and compare repository URLs.
/// Handles protocol differences and trailing slashes.
fn repositories_equal(a: &str, b: &str) -> bool {
    normalize_repository_url(a) == normalize_repository_url(b)
}

/// Normalize a repository URL for comparison.
/// Strips protocol prefixes and trailing slashes.
fn normalize_repository_url(url: &str) -> String {
    let url = url.trim();

    // Strip protocol prefixes
    let url = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("oci://"))
        .unwrap_or(url);

    // Strip trailing slash
    url.trim_end_matches('/').to_lowercase()
}

/// Extract registry from container image name.
/// Returns the registry portion or "docker.io" if implicit.
fn extract_registry(image_name: &str) -> String {
    // Image names can be:
    // - nginx (implicit docker.io/library/nginx)
    // - library/nginx (implicit docker.io/library/nginx)
    // - docker.io/nginx (explicit registry)
    // - ghcr.io/org/image (explicit registry)
    // - registry.example.com:5000/image (explicit with port)

    // Check if there's an explicit registry (contains '/' before first '/' after potential port)
    if image_name.contains('/') {
        let first_slash = image_name.find('/').unwrap();
        let potential_registry = &image_name[..first_slash];

        // Check if it looks like a registry (has '.' or ':' or is 'localhost')
        if potential_registry.contains('.')
            || potential_registry.contains(':')
            || potential_registry == "localhost"
        {
            return potential_registry.to_string();
        }

        // Could be docker.io official image like "library/nginx" or just "nginx"
        // If first segment doesn't look like a registry, it's implicit docker.io
    }

    // Default registry
    "docker.io".to_string()
}

/// Extract the core image path (image name without registry).
/// Used for comparing images that may have different registries.
fn extract_image_path(image_name: &str) -> String {
    // Strip the registry prefix if present
    if image_name.contains('/') {
        let first_slash = image_name.find('/').unwrap();
        let potential_registry = &image_name[..first_slash];

        // Check if it looks like a registry (has '.' or ':' or is 'localhost')
        if potential_registry.contains('.')
            || potential_registry.contains(':')
            || potential_registry == "localhost"
        {
            return image_name[first_slash + 1..].to_string();
        }
    }

    // No registry prefix, return the full name
    image_name.to_string()
}

/// Compare two version strings using semantic versioning.
/// Returns None if versions are not comparable.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    // Try semver comparison first
    if let (Ok(va), Ok(vb)) = (parse_semver(a), parse_semver(b)) {
        return Some(va.cmp(&vb));
    }

    // Fall back to string comparison for non-semver versions
    Some(a.cmp(b))
}

/// Parse a version string into comparable components.
fn parse_semver(v: &str) -> Result<Vec<u64>, ()> {
    let v = v.trim_start_matches('v').trim_start_matches('V');
    let parts: Vec<&str> = v.split('.').collect();

    if parts.is_empty() {
        return Err(());
    }

    let mut result = Vec::new();
    for part in parts {
        // Handle pre-release suffixes (e.g., "1.2.3-alpha" -> just take "1.2.3")
        let numeric_part = part.split('-').next().unwrap_or(part);
        if let Ok(n) = numeric_part.parse::<u64>() {
            result.push(n);
        } else {
            return Err(());
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("1.15.1", "1.15.0"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_versions("1.15.0", "1.15.1"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            compare_versions("1.15.1", "1.15.1"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(
            compare_versions("2.0.0", "1.99.99"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_versions("v1.0.0", "1.0.0"),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn test_normalize_repository_url() {
        // Protocol normalization
        assert_eq!(
            normalize_repository_url("https://grafana.github.io/helm-charts"),
            "grafana.github.io/helm-charts"
        );
        assert_eq!(
            normalize_repository_url("https://grafana.github.io/helm-charts/"),
            "grafana.github.io/helm-charts"
        );
        assert_eq!(
            normalize_repository_url("http://charts.example.com"),
            "charts.example.com"
        );
        assert_eq!(
            normalize_repository_url("oci://ghcr.io/charts"),
            "ghcr.io/charts"
        );

        // Case normalization
        assert_eq!(
            normalize_repository_url("https://GRAFANA.GITHUB.IO/helm-charts"),
            "grafana.github.io/helm-charts"
        );
    }

    #[test]
    fn test_repositories_equal() {
        // Same URL, different protocols
        assert!(repositories_equal(
            "https://grafana.github.io/helm-charts",
            "https://grafana.github.io/helm-charts/"
        ));
        assert!(repositories_equal(
            "https://grafana.github.io/helm-charts",
            "http://grafana.github.io/helm-charts"
        ));

        // Different URLs
        assert!(!repositories_equal(
            "https://grafana.github.io/helm-charts",
            "https://charts.loki.io"
        ));
        assert!(!repositories_equal(
            "oci://ghcr.io/charts",
            "https://ghcr.io/other-charts"
        ));
    }

    #[test]
    fn test_repository_change_detection() {
        use crate::versions::models::{HelmChartVersion, ContainerImageVersion};

        let source = ComponentVersions {
            path: "system/loki".to_string(),
            helm_charts: vec![HelmChartVersion {
                name: "loki".to_string(),
                version: "5.0.0".to_string(),
                repository: Some("https://grafana.github.io/helm-charts".to_string()),
                source_file: "Chart.yaml".to_string(),
            }],
            container_images: vec![],
        };

        let dest = ComponentVersions {
            path: "system/loki".to_string(),
            helm_charts: vec![HelmChartVersion {
                name: "loki".to_string(),
                version: "2.9.0".to_string(),
                repository: Some("https://charts.loki.io".to_string()),
                source_file: "Chart.yaml".to_string(),
            }],
            container_images: vec![],
        };

        let mut conflicts = Vec::new();
        check_component_conflicts("system/loki", &source, &dest, &mut conflicts);

        // Should detect repository change
        assert!(conflicts.iter().any(|c| matches!(
            &c.kind,
            ConflictKind::RepositoryChange { .. }
        )));

        let repo_conflict = conflicts
            .iter()
            .find(|c| matches!(&c.kind, ConflictKind::RepositoryChange { .. }))
            .unwrap();
        if let ConflictKind::RepositoryChange {
            chart_name,
            from_repo,
            to_repo,
        } = &repo_conflict.kind
        {
            assert_eq!(chart_name, "loki");
            assert_eq!(from_repo, "https://charts.loki.io");
            assert_eq!(to_repo, "https://grafana.github.io/helm-charts");
        } else {
            panic!("Expected RepositoryChange");
        }
    }

    #[test]
    fn test_extract_registry() {
        // Explicit registries
        assert_eq!(extract_registry("ghcr.io/home-operations/app"), "ghcr.io");
        assert_eq!(extract_registry("docker.io/library/nginx"), "docker.io");
        assert_eq!(
            extract_registry("registry.example.com:5000/image"),
            "registry.example.com:5000"
        );
        assert_eq!(extract_registry("localhost:5000/myimage"), "localhost:5000");
        assert_eq!(extract_registry("localhost/myimage"), "localhost");

        // Implicit docker.io
        assert_eq!(extract_registry("nginx"), "docker.io");
        assert_eq!(extract_registry("library/nginx"), "docker.io");
        assert_eq!(extract_registry("myorg/myimage"), "docker.io");
    }

    #[test]
    fn test_extract_image_path() {
        // Explicit registries - strip them
        assert_eq!(extract_image_path("ghcr.io/grafana/loki"), "grafana/loki");
        assert_eq!(extract_image_path("docker.io/grafana/loki"), "grafana/loki");
        assert_eq!(
            extract_image_path("registry.example.com:5000/grafana/loki"),
            "grafana/loki"
        );

        // Implicit registry - keep full name
        assert_eq!(extract_image_path("grafana/loki"), "grafana/loki");
        assert_eq!(extract_image_path("nginx"), "nginx");
        assert_eq!(extract_image_path("library/nginx"), "library/nginx");
    }

    #[test]
    fn test_registry_change_detection() {
        use crate::versions::models::ContainerImageVersion;

        // Scenario: same core image path but different registries
        // Source has ghcr.io registry, destination has docker.io registry
        let source = ComponentVersions {
            path: "system/loki".to_string(),
            helm_charts: vec![],
            container_images: vec![ContainerImageVersion {
                name: "ghcr.io/grafana/loki".to_string(),
                tag: "3.0.0".to_string(),
                source_file: "values.yaml".to_string(),
                json_path: "$.image.tag".to_string(),
            }],
        };

        let dest = ComponentVersions {
            path: "system/loki".to_string(),
            helm_charts: vec![],
            container_images: vec![ContainerImageVersion {
                name: "docker.io/grafana/loki".to_string(),
                tag: "2.9.0".to_string(),
                source_file: "values.yaml".to_string(),
                json_path: "$.image.tag".to_string(),
            }],
        };

        let mut conflicts = Vec::new();
        check_component_conflicts("system/loki", &source, &dest, &mut conflicts);

        // Should detect registry change when core paths match (grafana/loki)
        assert!(conflicts.iter().any(|c| matches!(
            &c.kind,
            ConflictKind::RegistryChange { .. }
        )));

        let registry_conflict = conflicts
            .iter()
            .find(|c| matches!(&c.kind, ConflictKind::RegistryChange { .. }))
            .unwrap();
        if let ConflictKind::RegistryChange {
            image_name,
            from_registry,
            to_registry,
        } = &registry_conflict.kind
        {
            assert_eq!(image_name, "ghcr.io/grafana/loki");
            assert_eq!(from_registry, "docker.io");
            assert_eq!(to_registry, "ghcr.io");
        } else {
            panic!("Expected RegistryChange");
        }

        // Verify the details message
        assert!(registry_conflict.details.contains("Registry change"));
        assert!(registry_conflict.details.contains("grafana/loki"));
    }
}
