//! Version merging for multi-source promotion.
//!
//! Merges versions from multiple sources according to rules.

use std::collections::HashMap;

use tracing::{info, warn};

use crate::config::{PromotionAction, PromotionRules};
use crate::error::AppResult;
use crate::versions::models::{ComponentVersions, VersionReport};

/// Merge result with decisions and warnings.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Merged version report.
    pub report: VersionReport,
    /// Merge decisions for each component.
    pub decisions: Vec<MergeDecision>,
    /// Warnings generated during merge.
    pub warnings: Vec<String>,
    /// Components removed due to rules.
    pub removed: Vec<RemovedComponent>,
}

/// Decision made for a component during merge.
#[derive(Debug, Clone)]
pub struct MergeDecision {
    pub component: String,
    pub action: PromotionAction,
    pub source: Option<String>,
    pub reason: String,
}

/// Component removed due to rules.
#[derive(Debug, Clone)]
pub struct RemovedComponent {
    pub component: String,
    pub source: String,
    pub reason: String,
}

/// Merge versions from multiple sources.
pub fn merge_versions(
    sources: &[(String, VersionReport)], // (source_name, report)
    rules: &PromotionRules,
) -> AppResult<MergeResult> {
    let mut merged = VersionReport::new("merged");
    merged.source_path = "merged".to_string();

    let mut decisions = Vec::new();
    let mut warnings = Vec::new();
    let mut removed = Vec::new();

    // Collect all components from all sources
    let mut component_sources: HashMap<String, Vec<(String, ComponentVersions)>> = HashMap::new();

    for (source_name, report) in sources {
        for (component_path, versions) in &report.components {
            // Check if source should include this component
            if !rules.source_includes(source_name, component_path) {
                info!(
                    "Source {} excludes component {}",
                    source_name, component_path
                );
                continue;
            }

            component_sources
                .entry(component_path.clone())
                .or_default()
                .push((source_name.clone(), versions.clone()));
        }
    }

    // Process each component
    for (component_path, sources_versions) in &component_sources {
        // Check component action
        let action = rules.get_action(component_path);

        match action {
            PromotionAction::Never => {
                removed.push(RemovedComponent {
                    component: component_path.clone(),
                    source: sources_versions
                        .iter()
                        .map(|(s, _)| s.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    reason: "Component marked as never promote".to_string(),
                });
                continue;
            }
            PromotionAction::Review => {
                warnings.push(format!(
                    "Component {} requires review (action: review)",
                    component_path
                ));
            }
            PromotionAction::Always => {}
        }

        // Merge versions from multiple sources
        let merged_versions = merge_component_versions(component_path, sources_versions, rules);

        if let Some((source, versions)) = merged_versions {
            merged.components.insert(component_path.clone(), versions);

            decisions.push(MergeDecision {
                component: component_path.clone(),
                action,
                source: Some(source.clone()),
                reason: format!("Selected from source {}", source),
            });
        }
    }

    // Generate summary warnings
    if !removed.is_empty() {
        warn!("Removed {} components due to rules", removed.len());
    }

    Ok(MergeResult {
        report: merged,
        decisions,
        warnings,
        removed,
    })
}

/// Merge versions for a single component from multiple sources.
fn merge_component_versions(
    component_path: &str,
    sources: &[(String, ComponentVersions)],
    rules: &PromotionRules,
) -> Option<(String, ComponentVersions)> {
    if sources.is_empty() {
        return None;
    }

    if sources.len() == 1 {
        return Some(sources[0].clone());
    }

    // Multiple sources - need to merge
    let mut merged = ComponentVersions {
        path: component_path.to_string(),
        helm_charts: Vec::new(),
        container_images: Vec::new(),
    };

    // Merge helm charts
    let mut chart_versions: HashMap<String, Vec<(String, String)>> = HashMap::new(); // chart_name -> [(source, version)]

    for (source, versions) in sources {
        for chart in &versions.helm_charts {
            chart_versions
                .entry(chart.name.clone())
                .or_default()
                .push((source.clone(), chart.version.clone()));
        }
    }

    for (chart_name, versions) in chart_versions {
        // Resolve conflict
        if let Some((source, version)) = rules.resolve_version_conflict(&versions) {
            // Find the full chart info from the selected source
            for (s, v) in sources {
                if s == &source
                    && let Some(chart) = v.helm_charts.iter().find(|c| c.name == chart_name)
                {
                    let mut merged_chart = chart.clone();
                    merged_chart.version = version;
                    merged.helm_charts.push(merged_chart);
                    break;
                }
            }
        }
    }

    // Merge container images
    let mut image_versions: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for (source, versions) in sources {
        for image in &versions.container_images {
            image_versions
                .entry(image.name.clone())
                .or_default()
                .push((source.clone(), image.tag.clone()));
        }
    }

    for (image_name, versions) in image_versions {
        if let Some((source, tag)) = rules.resolve_version_conflict(&versions) {
            for (s, v) in sources {
                if s == &source
                    && let Some(image) = v.container_images.iter().find(|i| i.name == image_name)
                {
                    let mut merged_image = image.clone();
                    merged_image.tag = tag;
                    merged.container_images.push(merged_image);
                    break;
                }
            }
        }
    }

    // Determine primary source (highest priority)
    let primary_source = sources
        .iter()
        .max_by_key(|(s, _)| rules.get_source_priority(s))
        .map(|(s, _)| s.clone())
        .unwrap_or_else(|| sources[0].0.clone());

    Some((primary_source, merged))
}

/// Generate human-readable explanation of merge decisions.
pub fn explain_merge(result: &MergeResult) -> String {
    let mut output = String::new();

    output.push_str("=== Merge Summary ===\n\n");

    // Applied changes
    let applied: Vec<_> = result
        .decisions
        .iter()
        .filter(|d| d.action != PromotionAction::Never)
        .collect();

    if !applied.is_empty() {
        output.push_str(&format!("Applied Changes ({})\n", applied.len()));
        output.push_str("-------------------\n");
        for decision in &applied {
            let source = decision.source.as_deref().unwrap_or("unknown");
            output.push_str(&format!(
                "- {} (from {}, {})\n",
                decision.component, source, decision.reason
            ));
        }
        output.push('\n');
    }

    // Removed changes
    if !result.removed.is_empty() {
        output.push_str(&format!("Removed Changes ({})\n", result.removed.len()));
        output.push_str("-------------------\n");
        for removed in &result.removed {
            output.push_str(&format!("- {} ({})\n", removed.component, removed.reason));
        }
        output.push('\n');
    }

    // Warnings
    if !result.warnings.is_empty() {
        output.push_str(&format!("Warnings ({})\n", result.warnings.len()));
        output.push_str("---------\n");
        for warning in &result.warnings {
            output.push_str(&format!("- {}\n", warning));
        }
        output.push('\n');
    }

    // Components needing review
    let review_needed: Vec<_> = result
        .decisions
        .iter()
        .filter(|d| d.action == PromotionAction::Review)
        .collect();

    if !review_needed.is_empty() {
        output.push_str(&format!("Needs Review ({})\n", review_needed.len()));
        output.push_str("-------------\n");
        for decision in &review_needed {
            output.push_str(&format!(
                "- {} (check before committing)\n",
                decision.component
            ));
        }
    }

    output
}
