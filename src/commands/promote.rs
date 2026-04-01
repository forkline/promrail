use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use console::style;
use log::{debug, info, warn};

use crate::commands::default_filter;
use crate::commands::diff::{self, DiffArgs};
use crate::config::{Config, PromotionAction, VersionHandling};
use crate::display::{PromotionOutputData, print_promotion_result};
use crate::error::{AppResult, PromrailError};
use crate::files::{FileDiscovery, FileSelector};
use crate::git::GitRepo;
use crate::review::analyze::is_version_managed_file;
use crate::review::analyze::matching_preserve_paths;
use crate::review::{
    ReviewArtifact, ReviewArtifactStatus, analyze_multi_source_promotion, apply_review_decisions,
    artifact_from_analysis, artifact_path, artifact_ready_for_apply, load_artifact, save_artifact,
};
use crate::versions;
use crate::versions::models::VersionChangeSummary;

/// Collected files from sources: relative path -> (source_name, absolute_path)
type CollectedFiles = HashMap<PathBuf, (String, PathBuf)>;

/// Duplicate files: (relative_path, [source_names])
type DuplicateFiles = Vec<(PathBuf, Vec<String>)>;

#[derive(Debug, Clone)]
struct PreparedCopy {
    relative: PathBuf,
    source_name: String,
    source_file: PathBuf,
    desired_content: Vec<u8>,
}

pub struct PromoteArgs {
    pub sources: Vec<String>,
    pub dest: String,
    pub filter: Vec<String>,
    pub delete: bool,
    pub dest_based: bool,
    pub dry_run: bool,
    pub confirm: bool,
    pub show_diff: bool,
    pub include_protected: bool,
    pub allow_duplicates: bool,
    pub only_existing: bool,
    pub ignore_gitignore: bool,
    pub force: bool,
}

impl PromoteArgs {
    /// Build PromoteArgs from CLI arguments and config defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn from_cli(
        source_vec: Vec<String>,
        dest: Option<String>,
        filter_vec: Vec<String>,
        no_delete: bool,
        dest_based: bool,
        dry_run: bool,
        confirm: bool,
        show_diff: bool,
        include_protected: bool,
        force: bool,
        allow_duplicates: bool,
        only_existing: bool,
        include_gitignored: bool,
        config: &Config,
    ) -> AppResult<Self> {
        let sources = if source_vec.is_empty() {
            if config.default_sources.is_empty() {
                return Err(PromrailError::ConfigInvalid(
                    "no source specified and no default_sources in config".to_string(),
                ));
            }
            config.default_sources.clone()
        } else {
            source_vec
        };
        let dest = dest
            .or_else(|| config.default_dest.clone())
            .ok_or_else(|| {
                PromrailError::ConfigInvalid(
                    "no dest specified and no default_dest in config".to_string(),
                )
            })?;

        let ignore_gitignore = if include_gitignored {
            false
        } else {
            config.rules.global.promote_options.ignore_gitignore
        };

        Ok(Self {
            sources,
            dest,
            filter: default_filter(filter_vec),
            delete: !no_delete,
            dest_based,
            dry_run,
            confirm,
            show_diff,
            include_protected,
            allow_duplicates,
            only_existing,
            ignore_gitignore,
            force,
        })
    }
}

/// Create environment not found error with correct repo name.
fn env_not_found_error(config: &Config, env: &str) -> PromrailError {
    PromrailError::EnvironmentNotFound {
        repo: if config.is_single_repo() {
            "default".to_string()
        } else {
            config.default_repo.clone()
        },
        env: env.to_string(),
    }
}

/// Prompt user for confirmation.
fn confirm_prompt(prompt: &str) -> AppResult<bool> {
    print!("{} [y/N] ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_lowercase().starts_with('y'))
}

pub fn execute(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let source = &args.sources[0];
    let is_cross_repo =
        config.repos.contains_key(source) && !config.get_environments().contains_key(source);

    if args.sources.len() > 1 || is_cross_repo {
        info!(
            "Multi-source promotion from {} sources to {}",
            args.sources.len(),
            args.dest
        );
        execute_multi_source(config, repo, args)
    } else {
        execute_single_source(config, repo, args)
    }
}

fn execute_single_source(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let source = &args.sources[0];
    let environments = config.get_environments();

    let should_delete = args.delete && !config.rules.global.promote_options.no_delete;

    let source_env = environments
        .get(source)
        .ok_or_else(|| env_not_found_error(config, source))?;

    let dest_env = environments
        .get(&args.dest)
        .ok_or_else(|| env_not_found_error(config, &args.dest))?;

    let source_path = PathBuf::from(&source_env.path);
    let dest_path = PathBuf::from(&dest_env.path);

    let diff_args = DiffArgs {
        source: source.clone(),
        dest: args.dest.clone(),
        filter: args.filter.clone(),
        delete: should_delete,
        dest_based: args.dest_based,
        include_protected: args.include_protected,
    };

    let result = diff::execute(config, repo, &diff_args, args.show_diff, true)?;

    if result.copied.is_empty() && result.deleted.is_empty() {
        print_promotion_result(
            &PromotionOutputData {
                copied_files: vec![],
                deleted_files: vec![],
                version_changes: vec![],
                sources: vec![source.clone()],
                dest: args.dest.clone(),
                dry_run: args.dry_run,
            },
            config.output.level.clone(),
        );
        return Ok(());
    }

    // Separate version-managed files from regular copies
    // Version-managed files use structured updates only if version_handling: structured is set
    // Default behavior is whole_file (copy entire file)
    let (version_managed_copies, regular_copies): (Vec<_>, Vec<_>) =
        result.copied.into_iter().partition(|f| {
            if !is_version_managed_file(&f.path) || !dest_path.join(&f.path).exists() {
                return false;
            }
            // Check for version_handling override
            let component = get_component(&f.path);
            let component_rule = config.rules.get_component_rule(&component);
            component_rule
                .map(|r| r.version_handling == VersionHandling::Structured)
                .unwrap_or(false)
        });

    if args.dry_run {
        let version_changes =
            collect_version_changes_for_dry_run(&version_managed_copies, &source_path, &dest_path)?;

        print_promotion_result(
            &PromotionOutputData {
                copied_files: regular_copies.iter().map(|f| f.path.clone()).collect(),
                deleted_files: result.deleted,
                version_changes,
                sources: vec![source.clone()],
                dest: args.dest.clone(),
                dry_run: true,
            },
            config.output.level.clone(),
        );
        return Ok(());
    }

    if args.confirm && !confirm_prompt("Proceed with promotion?")? {
        info!("Promotion cancelled");
        return Ok(());
    }

    info!("Applying changes...");

    let mut copied_files = Vec::new();
    let mut all_version_changes = Vec::new();

    // Copy regular files
    for file_diff in &regular_copies {
        let source_file = source_path.join(&file_diff.path);
        let dest_file = dest_path.join(&file_diff.path);

        repo.copy_file(&source_file, &dest_file)?;
        copied_files.push(file_diff.path.clone());
    }

    // Apply structured version updates for version-managed files
    if !version_managed_copies.is_empty() {
        let components: Vec<String> = version_managed_copies
            .iter()
            .map(|f| get_component(&f.path))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let report = versions::extract_versions(&source_path, &[])?;
        let apply_result = versions::apply_versions(
            &report,
            &dest_path,
            &versions::ApplyOptions {
                components,
                dry_run: false,
                check_conflicts: false,
                create_snapshot: false,
            },
        )?;

        all_version_changes = apply_result.version_changes;

        // Copy version-managed files that had no version changes
        for file_diff in &version_managed_copies {
            let updated_path = dest_path.join(&file_diff.path);
            if !apply_result.updated_files.contains(&updated_path) {
                let source_file = source_path.join(&file_diff.path);
                let dest_file = dest_path.join(&file_diff.path);
                repo.copy_file(&source_file, &dest_file)?;
                copied_files.push(file_diff.path.clone());
            }
        }
    }

    let deleted_files = if should_delete {
        result.deleted.clone()
    } else {
        Vec::new()
    };

    if should_delete {
        for file in &result.deleted {
            let dest_file = dest_path.join(file);
            repo.delete_file(&dest_file)?;
        }
    }

    print_promotion_result(
        &PromotionOutputData {
            copied_files,
            deleted_files,
            version_changes: all_version_changes,
            sources: vec![source.clone()],
            dest: args.dest.clone(),
            dry_run: false,
        },
        config.output.level.clone(),
    );

    Ok(())
}

fn collect_version_changes_for_dry_run(
    version_managed_copies: &[crate::git::FileDiff],
    source_path: &Path,
    dest_path: &Path,
) -> AppResult<Vec<VersionChangeSummary>> {
    if version_managed_copies.is_empty() {
        return Ok(Vec::new());
    }

    let components: Vec<String> = version_managed_copies
        .iter()
        .map(|f| get_component(&f.path))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let report = versions::extract_versions(source_path, &[])?;
    let apply_result = versions::apply_versions(
        &report,
        dest_path,
        &versions::ApplyOptions {
            components,
            dry_run: true,
            check_conflicts: false,
            create_snapshot: false,
        },
    )?;

    Ok(apply_result.version_changes)
}

fn compute_version_changes_for_components(
    retained_paths: &HashSet<PathBuf>,
    source_paths: &[(String, PathBuf)],
    dest_path: &Path,
    rules: &crate::config::PromotionRules,
) -> AppResult<Vec<VersionChangeSummary>> {
    let components: Vec<String> = retained_paths
        .iter()
        .filter(|path| is_version_managed_file(path))
        .map(|path| get_component(path))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if components.is_empty() {
        return Ok(Vec::new());
    }

    let mut sources = Vec::new();
    for (source_name, source_path) in source_paths {
        let report = versions::extract_versions(source_path, &[])?;
        sources.push((source_name.clone(), report));
    }

    let merged = versions::merge_versions(&sources, rules)?;
    if merged.report.components.is_empty() {
        return Ok(Vec::new());
    }

    let result = versions::apply_versions(
        &merged.report,
        dest_path,
        &versions::ApplyOptions {
            components,
            dry_run: true,
            check_conflicts: false,
            create_snapshot: false,
        },
    )?;

    Ok(result.version_changes)
}

fn execute_multi_source(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let environments = config.get_environments();

    let dest_env = environments
        .get(&args.dest)
        .ok_or_else(|| env_not_found_error(config, &args.dest))?;

    let dest_path = repo.path.join(&dest_env.path);
    let should_delete = args.delete && !config.rules.global.promote_options.no_delete;

    info!(
        "Multi-source promotion from {} sources to {}",
        args.sources.len(),
        args.dest
    );

    let source_paths = resolve_source_paths(config, repo, &args.sources)?;
    let selector = FileSelector::from_config(config)?;
    let discovery = FileDiscovery::new(selector);
    let analysis = analyze_multi_source_promotion(config, &source_paths, &dest_path, args)?;
    let review_path = artifact_path(&repo.path, &analysis.route_key);
    let mut applied_review_artifact: Option<ReviewArtifact> = None;

    let all_files: CollectedFiles = if analysis.review_items.is_empty() {
        analysis.auto_files.clone()
    } else if let Some(existing_artifact) = load_artifact(&repo.path, &analysis.route_key)? {
        if artifact_ready_for_apply(&existing_artifact, &analysis)? {
            applied_review_artifact = Some(existing_artifact.clone());
            apply_review_decisions(&existing_artifact, &analysis)?
        } else {
            let artifact =
                artifact_from_analysis(&analysis, &args.sources, &args.dest, &args.filter);
            save_artifact(&repo.path, &artifact)?;
            print_review_required(&review_path, &artifact);
            return Ok(());
        }
    } else {
        let artifact = artifact_from_analysis(&analysis, &args.sources, &args.dest, &args.filter);
        save_artifact(&repo.path, &artifact)?;
        print_review_required(&review_path, &artifact);
        return Ok(());
    };

    let prepared_copies = prepare_copies(config, &dest_path, &all_files, args)?;

    let changed_files: CollectedFiles = prepared_copies
        .iter()
        .map(|prepared| {
            (
                prepared.relative.clone(),
                (prepared.source_name.clone(), prepared.source_file.clone()),
            )
        })
        .collect();

    let files_to_delete = if should_delete {
        calculate_files_to_delete(
            &discovery,
            &dest_path,
            &all_files,
            &analysis.retained_paths,
            &config.protected_dirs,
            args,
        )?
    } else {
        Vec::new()
    };

    // Show summary
    let total_copies = prepared_copies.len();
    let total_deletes = files_to_delete.len();
    let total_retained = analysis.retained_paths.len();

    let copied_files: Vec<PathBuf> = prepared_copies.iter().map(|p| p.relative.clone()).collect();

    // Early return for no changes
    if total_copies == 0 && total_deletes == 0 && total_retained == 0 {
        print_promotion_result(
            &PromotionOutputData {
                copied_files: vec![],
                deleted_files: vec![],
                version_changes: vec![],
                sources: args.sources.clone(),
                dest: args.dest.clone(),
                dry_run: args.dry_run,
            },
            config.output.level.clone(),
        );
        return Ok(());
    }

    // Handle dry-run
    if args.dry_run {
        let version_changes = compute_version_changes_for_components(
            &analysis.retained_paths,
            &source_paths,
            &dest_path,
            &config.rules,
        )?;

        print_promotion_result(
            &PromotionOutputData {
                copied_files,
                deleted_files: files_to_delete.clone(),
                version_changes,
                sources: args.sources.clone(),
                dest: args.dest.clone(),
                dry_run: true,
            },
            config.output.level.clone(),
        );
        return Ok(());
    }

    let prompt_msg = if args.sources.len() == 1 {
        format!("Proceed with promotion from {}?", args.sources[0])
    } else {
        "Proceed with multi-source promotion?".to_string()
    };

    if args.confirm && !confirm_prompt(&prompt_msg)? {
        info!("Promotion cancelled");
        return Ok(());
    }

    let snapshot_id = create_multi_source_snapshot(
        &dest_path,
        Some(review_path.as_path()).filter(|_| applied_review_artifact.is_some()),
    )?;
    info!("Created snapshot: {}", snapshot_id);

    for prepared in &prepared_copies {
        let dest_file = dest_path.join(&prepared.relative);

        if is_protected(&prepared.relative, &config.protected_dirs) && !args.include_protected {
            continue;
        }

        if let Some(parent) = dest_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&dest_file, &prepared.desired_content)?;
    }

    if should_delete {
        for relative in &files_to_delete {
            let dest_file = dest_path.join(relative);
            std::fs::remove_file(&dest_file)?;
        }
    }

    let (version_updated_paths, version_changes) =
        apply_merged_versions(config, &source_paths, &dest_path, &analysis)?;

    save_multi_source_snapshot(
        &dest_path,
        &snapshot_id,
        &changed_files,
        &files_to_delete,
        &version_updated_paths,
    )?;

    if let Some(mut artifact) = applied_review_artifact {
        artifact.status = ReviewArtifactStatus::Applied;
        artifact.updated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        save_artifact(&repo.path, &artifact)?;
    }

    // Print final results
    let final_copied: Vec<PathBuf> = prepared_copies.iter().map(|p| p.relative.clone()).collect();
    print_promotion_result(
        &PromotionOutputData {
            copied_files: final_copied,
            deleted_files: files_to_delete.clone(),
            version_changes,
            sources: args.sources.clone(),
            dest: args.dest.clone(),
            dry_run: false,
        },
        config.output.level.clone(),
    );

    Ok(())
}

pub(crate) fn get_component(path: &Path) -> String {
    let parts: Vec<_> = path.components().take(2).collect();
    parts
        .iter()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn is_protected(path: &Path, protected_dirs: &[String]) -> bool {
    path.components().any(|component| {
        protected_dirs
            .iter()
            .any(|p| component.as_os_str() == std::ffi::OsStr::new(p))
    })
}

pub(crate) fn component_exists_in_dest(dest: &Path, component: &str) -> bool {
    dest.join(component).exists()
}

fn create_multi_source_snapshot(
    dest_path: &Path,
    review_artifact: Option<&Path>,
) -> AppResult<String> {
    let snapshot_id = format!(
        "snap_{}",
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    );

    // Create snapshot file path
    let snapshot_file = dest_path.join(".promotion-snapshots.yaml");

    // Load existing snapshots or create new
    let snapshots: Vec<serde_yaml::Value> = if snapshot_file.exists() {
        let content = std::fs::read_to_string(&snapshot_file)?;
        let doc: serde_yaml::Value = serde_yaml::from_str(&content)?;
        doc.get("snapshots")
            .and_then(|v| v.as_sequence())
            .cloned()
            .unwrap_or_default()
    } else {
        vec![]
    };

    // Add new snapshot
    let mut new_snapshots = snapshots;
    new_snapshots.push(serde_yaml::Value::Mapping({
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("id".to_string()),
            serde_yaml::Value::String(snapshot_id.clone()),
        );
        map.insert(
            serde_yaml::Value::String("created_at".to_string()),
            serde_yaml::Value::String(
                time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            ),
        );
        map.insert(
            serde_yaml::Value::String("status".to_string()),
            serde_yaml::Value::String("applied".to_string()),
        );
        if let Some(review_artifact) = review_artifact {
            map.insert(
                serde_yaml::Value::String("review_artifact".to_string()),
                serde_yaml::Value::String(review_artifact.display().to_string()),
            );
        }
        map
    }));

    // Save
    let doc = serde_yaml::Value::Mapping({
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("snapshots".to_string()),
            serde_yaml::Value::Sequence(new_snapshots),
        );
        map
    });

    std::fs::write(&snapshot_file, serde_yaml::to_string(&doc)?)?;

    Ok(snapshot_id)
}

fn save_multi_source_snapshot(
    dest_path: &Path,
    snapshot_id: &str,
    files: &HashMap<PathBuf, (String, PathBuf)>,
    deleted: &[PathBuf],
    version_updated: &[PathBuf],
) -> AppResult<()> {
    let snapshot_file = dest_path.join(".promotion-snapshots.yaml");

    let content = std::fs::read_to_string(&snapshot_file)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)?;

    if let Some(snapshots) = doc.get_mut("snapshots").and_then(|v| v.as_sequence_mut()) {
        for snapshot in snapshots {
            if let Some(map) = snapshot.as_mapping_mut()
                && let Some(serde_yaml::Value::String(id)) =
                    map.get(serde_yaml::Value::String("id".to_string()))
                && id == snapshot_id
            {
                map.insert(
                    serde_yaml::Value::String("files_modified".to_string()),
                    serde_yaml::Value::Sequence(
                        files
                            .keys()
                            .map(|p| serde_yaml::Value::String(p.display().to_string()))
                            .collect(),
                    ),
                );
                if !deleted.is_empty() {
                    map.insert(
                        serde_yaml::Value::String("files_deleted".to_string()),
                        serde_yaml::Value::Sequence(
                            deleted
                                .iter()
                                .map(|p| serde_yaml::Value::String(p.display().to_string()))
                                .collect(),
                        ),
                    );
                }
                if !version_updated.is_empty() {
                    map.insert(
                        serde_yaml::Value::String("version_updated".to_string()),
                        serde_yaml::Value::Sequence(
                            version_updated
                                .iter()
                                .map(|p| serde_yaml::Value::String(p.display().to_string()))
                                .collect(),
                        ),
                    );
                }
                break;
            }
        }
    }

    std::fs::write(&snapshot_file, serde_yaml::to_string(&doc)?)?;

    Ok(())
}

fn print_review_required(review_path: &Path, artifact: &ReviewArtifact) {
    println!("Review required before promotion.");
    println!("Artifact: {}", style(review_path.display()).cyan());
    println!(
        "  {} new components, {} conflicting file groups",
        style(artifact.summary.new_components).yellow(),
        style(artifact.summary.conflicting_files).yellow()
    );
    println!();
    println!(
        "Use opencode to classify the artifact, set `status: classified`, add `decision` for each item, and set `selected_source` on promoted items."
    );
    println!("Run `prl` again after saving the artifact.");
}

fn apply_merged_versions(
    config: &Config,
    source_paths: &[(String, PathBuf)],
    dest_path: &Path,
    analysis: &crate::review::analyze::PromotionAnalysis,
) -> AppResult<(Vec<PathBuf>, Vec<VersionChangeSummary>)> {
    if source_paths.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let components: Vec<String> = analysis
        .retained_paths
        .iter()
        .filter(|path| is_version_managed_file(path))
        .map(|path| get_component(path))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if components.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut sources = Vec::new();
    for (source_name, source_path) in source_paths {
        let report = versions::extract_versions(source_path, &[])?;
        sources.push((source_name.clone(), report));
    }

    let merged = versions::merge_versions(&sources, &config.rules)?;
    if merged.report.components.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let result = versions::apply_versions(
        &merged.report,
        dest_path,
        &versions::ApplyOptions {
            components,
            dry_run: false,
            check_conflicts: false,
            create_snapshot: false,
        },
    )?;

    Ok((result.updated_files, result.version_changes))
}

fn prepare_copies(
    config: &Config,
    dest_path: &Path,
    all_files: &CollectedFiles,
    args: &PromoteArgs,
) -> AppResult<Vec<PreparedCopy>> {
    let mut prepared = Vec::new();

    for (relative, (source_name, source_file)) in all_files {
        if is_protected(relative, &config.protected_dirs) && !args.include_protected {
            continue;
        }

        let dest_file = dest_path.join(relative);
        let desired_content = desired_file_content(config, relative, source_file, &dest_file)?;

        let unchanged = dest_file.exists()
            && std::fs::read(&dest_file)
                .map(|current| current == desired_content)
                .unwrap_or(false);

        if unchanged {
            continue;
        }

        prepared.push(PreparedCopy {
            relative: relative.clone(),
            source_name: source_name.clone(),
            source_file: source_file.clone(),
            desired_content,
        });
    }

    Ok(prepared)
}

fn desired_file_content(
    config: &Config,
    relative: &Path,
    source_file: &Path,
    dest_file: &Path,
) -> AppResult<Vec<u8>> {
    if dest_file.exists() {
        let component = get_component(relative);
        if let Some(component_rule) = config.rules.get_component_rule(&component)
            && let Some(paths) = matching_preserve_paths(component_rule, &component, relative)
            && !paths.is_empty()
        {
            return preserve_destination_paths(source_file, dest_file, &paths);
        }
    }

    Ok(std::fs::read(source_file)?)
}

fn preserve_destination_paths(
    source_file: &Path,
    dest_file: &Path,
    paths: &[String],
) -> AppResult<Vec<u8>> {
    let extension = source_file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();

    if extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml") {
        return match preserve_yaml_with_python(source_file, dest_file, paths) {
            Ok(content) => Ok(content),
            Err(PromrailError::ReviewArtifactInvalid(message))
                if message.contains("No module named 'ruamel'")
                    || message.contains("No module named \"ruamel\"") =>
            {
                preserve_yaml_with_serde(source_file, dest_file, paths)
            }
            Err(err) => Err(err),
        };
    }

    let source_content = std::fs::read_to_string(source_file)?;
    let dest_content = std::fs::read_to_string(dest_file)?;

    let mut source_doc: serde_json::Value = serde_json::from_str(&source_content)?;
    let dest_doc: serde_json::Value = serde_json::from_str(&dest_content)?;

    for path in paths {
        if let Some(value) = get_value_at_path(&dest_doc, path) {
            set_value_at_path(&mut source_doc, path, value.clone());
        }
    }

    write_serialized_value(&source_doc)
}

fn preserve_yaml_with_serde(
    source_file: &Path,
    dest_file: &Path,
    paths: &[String],
) -> AppResult<Vec<u8>> {
    let source_content = std::fs::read_to_string(source_file)?;
    let dest_content = std::fs::read_to_string(dest_file)?;

    let mut source_doc: serde_yaml::Value = serde_yaml::from_str(&source_content)?;
    let dest_doc: serde_yaml::Value = serde_yaml::from_str(&dest_content)?;

    for path in paths {
        if let Some(value) = get_yaml_value_at_path(&dest_doc, path) {
            set_yaml_value_at_path(&mut source_doc, path, value.clone());
        }
    }

    Ok(serde_yaml::to_string(&source_doc)?.into_bytes())
}

fn preserve_yaml_with_python(
    source_file: &Path,
    dest_file: &Path,
    paths: &[String],
) -> AppResult<Vec<u8>> {
    let paths_json = serde_json::to_string(paths)?;
    let script = r#"
from pathlib import Path
import json
import sys
from ruamel.yaml import YAML


def parse_path(path):
    tokens = []
    for segment in path.split('.'):
        if not segment:
            continue
        try:
            tokens.append(int(segment))
        except ValueError:
            tokens.append(segment)
    return tokens


def get_value(value, tokens):
    current = value
    for token in tokens:
        current = current[token]
    return current


def set_value(value, tokens, new_value):
    current = value
    for token in tokens[:-1]:
        current = current[token]
    current[tokens[-1]] = new_value


source_path = Path(sys.argv[1])
dest_path = Path(sys.argv[2])
paths = json.loads(sys.argv[3])

yaml = YAML()
yaml.preserve_quotes = True
yaml.indent(mapping=2, sequence=4, offset=2)
yaml.width = 4096

source_text = source_path.read_text(encoding='utf-8')
dest_text = dest_path.read_text(encoding='utf-8')
yaml.explicit_start = dest_text.lstrip().startswith('---')

source_doc = yaml.load(source_text)
dest_doc = yaml.load(dest_text)

for path in paths:
    tokens = parse_path(path)
    try:
        value = get_value(dest_doc, tokens)
    except Exception:
        continue
    set_value(source_doc, tokens, value)

yaml.dump(source_doc, sys.stdout)
"#;

    let output = Command::new("python")
        .arg("-c")
        .arg(script)
        .arg(source_file)
        .arg(dest_file)
        .arg(paths_json)
        .output()
        .map_err(|err| {
            PromrailError::ReviewArtifactInvalid(format!(
                "failed to run python yaml preserve helper: {}",
                err
            ))
        })?;

    if !output.status.success() {
        return Err(PromrailError::ReviewArtifactInvalid(format!(
            "python yaml preserve helper failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(output.stdout)
}

#[derive(Debug, Clone)]
enum PathToken {
    Key(String),
    Index(usize),
}

fn parse_path(path: &str) -> Vec<PathToken> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .map(|segment| match segment.parse::<usize>() {
            Ok(index) => PathToken::Index(index),
            Err(_) => PathToken::Key(segment.to_string()),
        })
        .collect()
}

fn get_value_at_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for token in parse_path(path) {
        match token {
            PathToken::Key(key) => {
                current = current.get(key)?;
            }
            PathToken::Index(index) => {
                current = current.as_array()?.get(index)?;
            }
        }
    }
    Some(current)
}

fn get_yaml_value_at_path<'a>(
    value: &'a serde_yaml::Value,
    path: &str,
) -> Option<&'a serde_yaml::Value> {
    let mut current = value;
    for token in parse_path(path) {
        match token {
            PathToken::Key(key) => {
                current = current.as_mapping()?.get(serde_yaml::Value::String(key))?;
            }
            PathToken::Index(index) => {
                current = current.as_sequence()?.get(index)?;
            }
        }
    }
    Some(current)
}

fn set_value_at_path(value: &mut serde_json::Value, path: &str, new_value: serde_json::Value) {
    fn set_recursive(
        current: &mut serde_json::Value,
        tokens: &[PathToken],
        new_value: serde_json::Value,
    ) {
        if tokens.is_empty() {
            *current = new_value;
            return;
        }

        match &tokens[0] {
            PathToken::Key(key) => {
                if !current.is_object() {
                    *current = serde_json::Value::Object(serde_json::Map::new());
                }
                let map = current.as_object_mut().expect("object just created");
                let entry = map.entry(key.clone()).or_insert(serde_json::Value::Null);
                set_recursive(entry, &tokens[1..], new_value);
            }
            PathToken::Index(index) => {
                if !current.is_array() {
                    *current = serde_json::Value::Array(Vec::new());
                }
                let seq = current.as_array_mut().expect("array just created");
                while seq.len() <= *index {
                    seq.push(serde_json::Value::Null);
                }
                set_recursive(&mut seq[*index], &tokens[1..], new_value);
            }
        }
    }

    let tokens = parse_path(path);
    set_recursive(value, &tokens, new_value);
}

fn set_yaml_value_at_path(value: &mut serde_yaml::Value, path: &str, new_value: serde_yaml::Value) {
    fn set_recursive(
        current: &mut serde_yaml::Value,
        tokens: &[PathToken],
        new_value: serde_yaml::Value,
    ) {
        if tokens.is_empty() {
            *current = new_value;
            return;
        }

        match &tokens[0] {
            PathToken::Key(key) => {
                if !current.is_mapping() {
                    *current = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
                }
                let map = current.as_mapping_mut().expect("mapping just created");
                let entry = map
                    .entry(serde_yaml::Value::String(key.clone()))
                    .or_insert(serde_yaml::Value::Null);
                set_recursive(entry, &tokens[1..], new_value);
            }
            PathToken::Index(index) => {
                if !current.is_sequence() {
                    *current = serde_yaml::Value::Sequence(Vec::new());
                }
                let seq = current.as_sequence_mut().expect("sequence just created");
                while seq.len() <= *index {
                    seq.push(serde_yaml::Value::Null);
                }
                set_recursive(&mut seq[*index], &tokens[1..], new_value);
            }
        }
    }

    let tokens = parse_path(path);
    set_recursive(value, &tokens, new_value);
}

fn write_serialized_value(value: &serde_json::Value) -> AppResult<Vec<u8>> {
    Ok((serde_json::to_string_pretty(value)? + "\n").into_bytes())
}

/// Resolve source names to their filesystem paths.
fn resolve_source_paths(
    config: &Config,
    repo: &GitRepo,
    sources: &[String],
) -> AppResult<Vec<(String, PathBuf)>> {
    let environments = config.get_environments();
    let mut source_paths: Vec<(String, PathBuf)> = Vec::new();

    for source in sources {
        if let Some(source_env) = environments.get(source) {
            let source_path = repo.path.join(&source_env.path);
            source_paths.push((source.clone(), source_path));
        } else if let Some((repo_name, other_repo_config)) = config.repos.get_key_value(source) {
            let repo_path = other_repo_config.resolved_path();
            source_paths.push((repo_name.clone(), repo_path));
        } else {
            return Err(env_not_found_error(config, source));
        }
    }

    Ok(source_paths)
}

/// Collect files from all sources, handling rules and duplicates.
fn collect_files_from_sources(
    discovery: &FileDiscovery,
    source_paths: &[(String, PathBuf)],
    dest_path: &Path,
    config: &Config,
    args: &PromoteArgs,
) -> AppResult<(CollectedFiles, DuplicateFiles)> {
    let mut all_files: CollectedFiles = HashMap::new();
    let mut duplicates: DuplicateFiles = Vec::new();

    for (source_name, source_path) in source_paths {
        debug!("Discovering files in: {}", source_path.display());
        let discovered = discovery.discover(
            source_path,
            &args.filter,
            args.include_protected,
            args.ignore_gitignore,
        )?;
        debug!("  Found {} files", discovered.files.len());

        for file in &discovered.files {
            let relative = file.clone();
            let absolute = source_path.join(&relative);

            if is_protected(&relative, &config.protected_dirs) && !args.include_protected {
                continue;
            }

            if config.rules.has_rules() {
                let component = get_component(&relative);
                if !config.rules.source_includes(source_name, &component) {
                    continue;
                }

                let action = config.rules.get_action(&component);
                if action == PromotionAction::Never {
                    info!("Skipping {} (action: never)", component);
                    continue;
                }
            }

            if args.only_existing || config.rules.global.promote_options.only_existing {
                let component = get_component(&relative);
                if !component_exists_in_dest(dest_path, &component) {
                    info!("Skipping new component: {} (only_existing)", component);
                    continue;
                }
            }

            if let Some((existing_source, _)) = all_files.get(&relative) {
                let existing = existing_source.clone();
                if let Some(pos) = duplicates.iter().position(|(p, _)| p == &relative) {
                    duplicates[pos].1.push(source_name.clone());
                } else {
                    duplicates.push((relative.clone(), vec![existing, source_name.clone()]));
                }
            } else {
                all_files.insert(relative, (source_name.clone(), absolute));
            }
        }
    }

    Ok((all_files, duplicates))
}

/// Handle duplicate files across sources.
fn handle_duplicates(
    duplicates: &DuplicateFiles,
    args: &PromoteArgs,
    promote_options: &crate::config::PromoteOptions,
) -> AppResult<()> {
    if duplicates.is_empty() {
        return Ok(());
    }

    if !args.allow_duplicates && !promote_options.allow_duplicates {
        return Err(PromrailError::DuplicateFiles(
            duplicates
                .iter()
                .map(|(p, s)| format!("{} (in: {})", p.display(), s.join(", ")))
                .collect(),
        ));
    }

    for (file, sources) in duplicates {
        warn!(
            "Duplicate file {} found in sources: {} (using first source)",
            file.display(),
            sources.join(", ")
        );
    }

    Ok(())
}

/// Calculate files to delete from destination.
fn calculate_files_to_delete(
    discovery: &FileDiscovery,
    dest_path: &Path,
    all_files: &CollectedFiles,
    retained_paths: &HashSet<PathBuf>,
    protected_dirs: &[String],
    args: &PromoteArgs,
) -> AppResult<Vec<PathBuf>> {
    let dest_discovered = discovery.discover(
        dest_path,
        &args.filter,
        args.include_protected,
        args.ignore_gitignore,
    )?;
    let all_relative: HashSet<PathBuf> = all_files.keys().cloned().collect();

    let files_to_delete: Vec<PathBuf> = dest_discovered
        .files
        .iter()
        .filter(|file| {
            !is_protected(file, protected_dirs)
                && !all_relative.contains(*file)
                && !retained_paths.contains(*file)
        })
        .cloned()
        .collect();

    Ok(files_to_delete)
}
