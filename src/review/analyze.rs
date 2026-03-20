use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::commands::promote::{
    PromoteArgs, component_exists_in_dest, get_component, is_protected,
};
use crate::config::{ComponentRule, Config, ConfigStrategy, PromotionAction, PromotionRules};
use crate::error::AppResult;
use crate::files::{FileDiscovery, FileSelector};
use crate::review::models::{
    ReviewArtifact, ReviewArtifactStatus, ReviewItem, ReviewItemKind, ReviewSummary,
};

#[derive(Debug, Clone)]
pub struct PromotionAnalysis {
    pub route_key: String,
    pub fingerprint: String,
    pub auto_files: HashMap<PathBuf, (String, PathBuf)>,
    pub retained_paths: HashSet<PathBuf>,
    pub review_items: Vec<ReviewItem>,
    pub source_lookup: HashMap<(PathBuf, String), PathBuf>,
}

#[derive(Debug, Clone)]
struct FileCandidate {
    source_name: String,
    absolute_path: PathBuf,
    component: String,
    existing_component: bool,
    content_hash: u64,
    version_managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReviewKey {
    kind: ReviewItemKind,
    component: String,
}

pub fn analyze_multi_source_promotion(
    config: &Config,
    source_paths: &[(String, PathBuf)],
    dest_path: &Path,
    args: &PromoteArgs,
) -> AppResult<PromotionAnalysis> {
    let selector = FileSelector::from_config(config)?;
    let discovery = FileDiscovery::new(selector);
    let mut grouped: BTreeMap<PathBuf, Vec<FileCandidate>> = BTreeMap::new();
    let mut source_lookup = HashMap::new();
    let mut fingerprint_parts = Vec::new();

    for (source_name, source_path) in source_paths {
        let discovered = discovery.discover(
            source_path,
            &args.filter,
            args.include_protected,
            args.ignore_gitignore,
        )?;

        for relative in discovered.files {
            if is_protected(&relative, &config.protected_dirs) && !args.include_protected {
                continue;
            }

            let component = get_component(&relative);
            if config.rules.has_rules() {
                if !config.rules.source_includes(source_name, &component) {
                    continue;
                }

                if config.rules.get_action(&component) == PromotionAction::Never {
                    continue;
                }
            }

            let existing_component = component_exists_in_dest(dest_path, &component);
            if (args.only_existing || config.rules.global.promote_options.only_existing)
                && !existing_component
            {
                continue;
            }

            let absolute_path = source_path.join(&relative);
            let content = std::fs::read(&absolute_path)?;
            let content_hash = hash_bytes(&content);
            let version_managed = is_version_managed_file(&relative);

            fingerprint_parts.push(format!(
                "src:{}:{}:{}",
                source_name,
                relative.display(),
                content_hash
            ));

            source_lookup.insert(
                (relative.clone(), source_name.clone()),
                absolute_path.clone(),
            );
            grouped.entry(relative).or_default().push(FileCandidate {
                source_name: source_name.clone(),
                absolute_path,
                component,
                existing_component,
                content_hash,
                version_managed,
            });
        }
    }

    let dest_discovered = discovery.discover(
        dest_path,
        &args.filter,
        args.include_protected,
        args.ignore_gitignore,
    )?;
    for relative in dest_discovered.files {
        let absolute_path = dest_path.join(&relative);
        let content_hash = hash_bytes(&std::fs::read(&absolute_path)?);
        fingerprint_parts.push(format!("dst:{}:{}", relative.display(), content_hash));
    }

    fingerprint_parts.sort();

    let mut auto_files = HashMap::new();
    let mut retained_paths = HashSet::new();
    let mut review_map: BTreeMap<String, ReviewItem> = BTreeMap::new();

    for (relative, candidates) in &grouped {
        let component = candidates[0].component.clone();
        let existing_component = candidates[0].existing_component;
        let version_managed = candidates[0].version_managed;
        let component_action = config.rules.get_action(&component);
        let component_rule = config.rules.get_component_rule(&component);
        let candidate_sources = unique_sources(candidates);
        let identical = candidates
            .iter()
            .all(|candidate| candidate.content_hash == candidates[0].content_hash);

        if !existing_component {
            retained_paths.insert(relative.clone());
            upsert_review_item(
                &mut review_map,
                ReviewItemKind::NewComponent,
                &component,
                relative,
                &candidate_sources,
                "Component does not exist in destination; review before promoting",
            );
            continue;
        }

        if version_managed {
            if dest_path.join(relative).exists() {
                retained_paths.insert(relative.clone());
            } else {
                let selected = &candidates[0];
                auto_files.insert(
                    relative.clone(),
                    (selected.source_name.clone(), selected.absolute_path.clone()),
                );
            }
            continue;
        }

        if component_action == PromotionAction::Review {
            retained_paths.insert(relative.clone());
            upsert_review_item(
                &mut review_map,
                ReviewItemKind::RuleReview,
                &component,
                relative,
                &candidate_sources,
                "Component is marked action: review; classify before promoting non-version files",
            );
            continue;
        }

        if let Some(selected_source) = should_auto_resolve_conflict(
            &config.rules,
            component_rule,
            &component,
            relative,
            &candidate_sources,
        ) {
            let selected = candidates
                .iter()
                .find(|candidate| candidate.source_name == selected_source)
                .unwrap_or(&candidates[0]);
            auto_files.insert(
                relative.clone(),
                (selected.source_name.clone(), selected.absolute_path.clone()),
            );
            continue;
        }

        if candidates.len() == 1 || identical {
            let selected = &candidates[0];
            auto_files.insert(
                relative.clone(),
                (selected.source_name.clone(), selected.absolute_path.clone()),
            );
            continue;
        }

        retained_paths.insert(relative.clone());
        upsert_review_item(
            &mut review_map,
            ReviewItemKind::ConflictingFile,
            &component,
            relative,
            &candidate_sources,
            "Conflicting file content across sources; choose the source to promote",
        );
    }

    let review_items: Vec<ReviewItem> = review_map.into_values().collect();
    Ok(PromotionAnalysis {
        route_key: build_route_key(source_paths, &args.dest, &args.filter),
        fingerprint: hash_string(&fingerprint_parts.join("\n")),
        auto_files,
        retained_paths,
        review_items: review_items
            .into_iter()
            .map(|mut item| {
                item.id = build_review_item_id(&item.kind, &item.component, &item.files);
                item
            })
            .collect(),
        source_lookup,
    })
}

pub fn artifact_from_analysis(
    analysis: &PromotionAnalysis,
    sources: &[String],
    dest: &str,
    filters: &[String],
) -> ReviewArtifact {
    let timestamp = now_rfc3339();
    ReviewArtifact {
        version: 1,
        id: analysis.route_key.clone(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        status: ReviewArtifactStatus::Pending,
        sources: sources.to_vec(),
        dest: dest.to_string(),
        filters: filters.to_vec(),
        route_key: analysis.route_key.clone(),
        fingerprint: analysis.fingerprint.clone(),
        summary: ReviewSummary {
            auto_files: analysis.auto_files.len(),
            retained_files: analysis.retained_paths.len(),
            review_items: analysis.review_items.len(),
            new_components: analysis
                .review_items
                .iter()
                .filter(|item| item.kind == ReviewItemKind::NewComponent)
                .count(),
            conflicting_files: analysis
                .review_items
                .iter()
                .filter(|item| item.kind == ReviewItemKind::ConflictingFile)
                .count(),
        },
        items: analysis.review_items.clone(),
    }
}

pub(crate) fn is_version_managed_file(relative: &Path) -> bool {
    matches!(
        relative.file_name().and_then(|name| name.to_str()),
        Some(
            "kustomization.yaml"
                | "kustomization.yml"
                | "Chart.yaml"
                | "Chart.yml"
                | "values.yaml"
                | "values.yml"
                | "values-images.yaml"
                | "values-images.yml"
        )
    )
}

fn upsert_review_item(
    review_map: &mut BTreeMap<String, ReviewItem>,
    kind: ReviewItemKind,
    component: &str,
    relative: &Path,
    candidate_sources: &[String],
    reason: &str,
) {
    let key = format!("{:?}:{}", kind, component);
    let entry = review_map.entry(key).or_insert_with(|| ReviewItem {
        id: String::new(),
        kind: kind.clone(),
        component: component.to_string(),
        files: Vec::new(),
        candidate_sources: candidate_sources.to_vec(),
        reason: reason.to_string(),
        decision: None,
        selected_source: None,
        notes: None,
    });

    let relative_str = relative.display().to_string();
    if !entry.files.contains(&relative_str) {
        entry.files.push(relative_str);
        entry.files.sort();
    }
    for source in candidate_sources {
        if !entry.candidate_sources.contains(source) {
            entry.candidate_sources.push(source.clone());
        }
    }
    entry.candidate_sources.sort();
}

fn unique_sources(candidates: &[FileCandidate]) -> Vec<String> {
    let mut sources = BTreeSet::new();
    for candidate in candidates {
        sources.insert(candidate.source_name.clone());
    }
    sources.into_iter().collect()
}

pub(crate) fn matching_preserve_paths(
    component_rule: &ComponentRule,
    component: &str,
    relative: &Path,
) -> Option<Vec<String>> {
    let component_prefix = Path::new(component);
    let relative_within_component = relative
        .strip_prefix(component_prefix)
        .ok()
        .unwrap_or(relative)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    let relative_full = relative.to_string_lossy().to_string();

    for preserve in &component_rule.preserve {
        if preserve.file.is_empty() {
            continue;
        }

        if glob_match::glob_match(&preserve.file, &relative_within_component)
            || glob_match::glob_match(&preserve.file, &relative_full)
            || preserve.file == relative_within_component
            || preserve.file == relative_full
        {
            return Some(preserve.paths.clone());
        }
    }

    None
}

fn should_auto_resolve_conflict(
    rules: &PromotionRules,
    component_rule: Option<&ComponentRule>,
    component: &str,
    relative: &Path,
    candidate_sources: &[String],
) -> Option<String> {
    if candidate_sources.is_empty() {
        return None;
    }

    if candidate_sources.len() == 1 {
        return Some(candidate_sources[0].clone());
    }

    if rules.conflict_resolution.config_strategy != ConfigStrategy::SourcePriority {
        return None;
    }

    let component_rule = component_rule?;
    let has_preserve = matching_preserve_paths(component_rule, component, relative)
        .map(|paths| !paths.is_empty())
        .unwrap_or(false);

    if component_rule.action == PromotionAction::Always || has_preserve {
        return rules.resolve_config_source(candidate_sources);
    }

    None
}

fn build_route_key(source_paths: &[(String, PathBuf)], dest: &str, filters: &[String]) -> String {
    let mut parts = source_paths
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    parts.push(dest.to_string());
    parts.extend(filters.iter().cloned());
    let raw = parts.join("__");
    let sanitized: String = raw
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    if sanitized.len() > 96 {
        format!("review_{}", &hash_string(&sanitized)[..16])
    } else {
        sanitized
    }
}

fn build_review_item_id(kind: &ReviewItemKind, component: &str, files: &[String]) -> String {
    let mut payload = vec![format!("{:?}", kind), component.to_string()];
    payload.extend(files.iter().cloned());
    hash_string(&payload.join("\n"))
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn hash_string(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{build_review_item_id, is_version_managed_file};
    use crate::review::models::ReviewItemKind;
    use std::path::Path;

    #[test]
    fn version_managed_files_are_detected() {
        assert!(is_version_managed_file(Path::new(
            "platform/app/values.yaml"
        )));
        assert!(is_version_managed_file(Path::new(
            "platform/app/kustomization.yaml"
        )));
        assert!(!is_version_managed_file(Path::new(
            "platform/app/config.yaml"
        )));
    }

    #[test]
    fn review_item_ids_are_stable() {
        let files = vec!["a.yaml".to_string(), "b.yaml".to_string()];
        assert_eq!(
            build_review_item_id(&ReviewItemKind::NewComponent, "apps/demo", &files),
            build_review_item_id(&ReviewItemKind::NewComponent, "apps/demo", &files)
        );
    }
}
