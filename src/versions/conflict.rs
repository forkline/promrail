//! Conflict detection for version apply operations.

use crate::versions::models::{ComponentVersions, Conflict, ConflictKind, VersionReport};

/// Detect conflicts between source versions and destination versions.
pub fn detect_conflicts(source: &VersionReport, dest: &VersionReport) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    for (component, source_versions) in &source.components {
        if let Some(dest_versions) = dest.components.get(component) {
            check_component_conflicts(component, source_versions, dest_versions, &mut conflicts);
        }
        // MissingInDest is not a conflict - it's just a new component
    }

    conflicts
}

fn check_component_conflicts(
    component: &str,
    source: &ComponentVersions,
    dest: &ComponentVersions,
    conflicts: &mut Vec<Conflict>,
) {
    // Check helm chart version conflicts
    for src_chart in &source.helm_charts {
        if let Some(dest_chart) = dest.helm_charts.iter().find(|c| c.name == src_chart.name)
            && let Some(ordering) = compare_versions(&src_chart.version, &dest_chart.version)
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

    // Check container image tag conflicts
    for src_image in &source.container_images {
        if let Some(dest_image) = dest
            .container_images
            .iter()
            .find(|i| i.name == src_image.name)
            && let Some(ordering) = compare_versions(&src_image.tag, &dest_image.tag)
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
    }
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
}
