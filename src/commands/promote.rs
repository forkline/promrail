use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use console::style;
use log::{debug, info, warn};

use crate::commands::diff::{self, DiffArgs};
use crate::commands::{default_filter, print_promotion_summary};
use crate::config::{Config, PromotionAction};
use crate::error::{AppResult, PromrailError};
use crate::files::{FileDiscovery, FileSelector};
use crate::git::{FileDiff, GitRepo};
use crate::review::analyze::is_version_managed_file;
use crate::review::analyze::matching_preserve_paths;
use crate::review::{
    ReviewArtifact, ReviewArtifactStatus, analyze_multi_source_promotion, apply_review_decisions,
    artifact_from_analysis, artifact_path, artifact_ready_for_apply, load_artifact, save_artifact,
};
use crate::versions;

/// Collected files from sources: relative path -> (source_name, absolute_path)
type CollectedFiles = HashMap<PathBuf, (String, PathBuf)>;

/// Duplicate files: (relative_path, [source_names])
type DuplicateFiles = Vec<(PathBuf, Vec<String>)>;

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
        info!("No changes to promote");
        return Ok(());
    }

    if args.dry_run {
        info!("Dry run complete. No files were modified.");
        return Ok(());
    }

    if args.confirm && !confirm_prompt("Proceed with promotion?")? {
        info!("Promotion cancelled");
        return Ok(());
    }

    info!("Applying changes...");

    for file_diff in &result.copied {
        let source_file = source_path.join(&file_diff.path);
        let dest_file = dest_path.join(&file_diff.path);

        repo.copy_file(&source_file, &dest_file)?;
        println!(
            "{}",
            style(format!("Copied: {}", file_diff.path.display())).green()
        );
    }

    if should_delete {
        for file in &result.deleted {
            let dest_file = dest_path.join(file);
            repo.delete_file(&dest_file)?;
            println!("{}", style(format!("Deleted: {}", file.display())).red());
        }
    }

    println!();
    println!("{}", style("Promotion complete!").bold());
    print_promotion_summary(result.copied.len(), result.deleted.len(), should_delete);

    if config.audit.enabled {
        write_audit_log(
            config,
            repo,
            std::slice::from_ref(source),
            &args.dest,
            &result,
        )?;
    }

    Ok(())
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
    let total_copies = all_files.len();
    let total_deletes = files_to_delete.len();
    let total_retained = analysis.retained_paths.len();

    if args.sources.len() == 1 {
        println!("Comparing {} -> {}", args.sources[0], args.dest);
    } else {
        println!("Comparing {} sources -> {}", args.sources.len(), args.dest);
    }
    println!();

    if total_copies == 0 && total_deletes == 0 && total_retained == 0 {
        println!("No changes to promote");
        return Ok(());
    }

    for relative in all_files.keys() {
        println!("~ {}", relative.display());
    }

    println!();
    println!("Summary:");
    println!("  {} files to copy", style(total_copies).green());
    if total_retained > 0 {
        println!(
            "  {} files preserved for structured version merge",
            style(total_retained).cyan()
        );
    }
    if should_delete && total_deletes > 0 {
        println!("  {} files to delete", style(total_deletes).red());
    }

    if args.dry_run {
        println!();
        println!("Dry run complete. No files were modified.");
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

    for (relative, (source_name, source_file)) in &all_files {
        let dest_file = dest_path.join(relative);

        if is_protected(relative, &config.protected_dirs) && !args.include_protected {
            continue;
        }

        if let Some(parent) = dest_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if !apply_preserve_rules(config, relative, source_file, &dest_file)? {
            std::fs::copy(source_file, &dest_file)?;
        }
        println!(
            "{}",
            style(format!(
                "Copied: {} (from {})",
                relative.display(),
                source_name
            ))
            .green()
        );
    }

    if should_delete {
        for relative in &files_to_delete {
            let dest_file = dest_path.join(relative);
            std::fs::remove_file(&dest_file)?;
            println!(
                "{}",
                style(format!("Deleted: {}", relative.display())).red()
            );
        }
    }

    let version_updated = apply_merged_versions(config, &source_paths, &dest_path, &analysis)?;

    save_multi_source_snapshot(
        &dest_path,
        &snapshot_id,
        &all_files,
        &files_to_delete,
        &version_updated,
    )?;

    if let Some(mut artifact) = applied_review_artifact {
        artifact.status = ReviewArtifactStatus::Applied;
        artifact.updated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        save_artifact(&repo.path, &artifact)?;
    }

    println!();
    if args.sources.len() == 1 {
        println!("{}", style("Promotion complete!").bold());
    } else {
        println!("{}", style("Multi-source promotion complete!").bold());
    }
    print_promotion_summary(total_copies, total_deletes, should_delete);

    if config.audit.enabled {
        let result = diff::PromotionResult {
            copied: all_files
                .keys()
                .map(|p| FileDiff::added(p.clone(), String::new()))
                .collect(),
            deleted: files_to_delete.clone(),
            skipped: vec![],
            protected: vec![],
        };
        write_audit_log(config, repo, &args.sources, &args.dest, &result)?;
    }

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
) -> AppResult<Vec<PathBuf>> {
    if source_paths.is_empty() {
        return Ok(Vec::new());
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
        return Ok(Vec::new());
    }

    let mut sources = Vec::new();
    for (source_name, source_path) in source_paths {
        let report = versions::extract_versions(source_path, &[])?;
        sources.push((source_name.clone(), report));
    }

    let merged = versions::merge_versions(&sources, &config.rules)?;
    if merged.report.components.is_empty() {
        return Ok(Vec::new());
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

    Ok(result.updated_files)
}

fn apply_preserve_rules(
    config: &Config,
    relative: &Path,
    source_file: &Path,
    dest_file: &Path,
) -> AppResult<bool> {
    if !dest_file.exists() {
        return Ok(false);
    }

    let component = get_component(relative);
    let Some(component_rule) = config.rules.get_component_rule(&component) else {
        return Ok(false);
    };
    let Some(paths) = matching_preserve_paths(component_rule, &component, relative) else {
        return Ok(false);
    };
    if paths.is_empty() {
        return Ok(false);
    }

    preserve_destination_paths(source_file, dest_file, &paths)?;
    Ok(true)
}

fn preserve_destination_paths(
    source_file: &Path,
    dest_file: &Path,
    paths: &[String],
) -> AppResult<()> {
    let extension = source_file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();

    if extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml") {
        preserve_yaml_with_python(source_file, dest_file, paths)?;
        return Ok(());
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

    write_serialized_value(dest_file, &source_doc)?;
    Ok(())
}

fn preserve_yaml_with_python(
    source_file: &Path,
    dest_file: &Path,
    paths: &[String],
) -> AppResult<()> {
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

with dest_path.open('w', encoding='utf-8') as fh:
    yaml.dump(source_doc, fh)
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

    Ok(())
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

fn write_serialized_value(path: &Path, value: &serde_json::Value) -> AppResult<()> {
    let content = serde_json::to_string_pretty(value)? + "\n";

    std::fs::write(path, content)?;
    Ok(())
}

fn write_audit_log(
    config: &Config,
    repo: &GitRepo,
    sources: &[String],
    dest: &str,
    result: &diff::PromotionResult,
) -> AppResult<()> {
    let log_path = repo.path.join(&config.audit.log_file);

    let mut entries: Vec<serde_yaml::Value> = if log_path.exists() {
        let content = match std::fs::read_to_string(&log_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Could not read audit log, starting fresh: {}", e);
                return write_new_audit_log(&log_path, sources, dest, result);
            }
        };

        match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(doc) => doc
                .get("promotions")
                .and_then(|v| v.as_sequence())
                .cloned()
                .unwrap_or_default(),
            Err(e) => {
                warn!("Audit log corrupted, backing up and starting fresh: {}", e);
                let backup_path = log_path.with_extension("yaml.bak");
                if let Err(backup_err) = std::fs::rename(&log_path, &backup_path) {
                    warn!("Could not backup corrupted audit log: {}", backup_err);
                }
                return write_new_audit_log(&log_path, sources, dest, result);
            }
        }
    } else {
        vec![]
    };

    entries.push(create_audit_entry(sources, dest, result));

    let doc = serde_yaml::Value::Mapping({
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("promotions".to_string()),
            serde_yaml::Value::Sequence(entries),
        );
        map
    });

    std::fs::write(&log_path, serde_yaml::to_string(&doc)?)?;

    Ok(())
}

fn write_new_audit_log(
    log_path: &std::path::Path,
    sources: &[String],
    dest: &str,
    result: &diff::PromotionResult,
) -> AppResult<()> {
    let doc = serde_yaml::Value::Mapping({
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("promotions".to_string()),
            serde_yaml::Value::Sequence(vec![create_audit_entry(sources, dest, result)]),
        );
        map
    });

    std::fs::write(log_path, serde_yaml::to_string(&doc)?)?;
    Ok(())
}

fn create_audit_entry(
    sources: &[String],
    dest: &str,
    result: &diff::PromotionResult,
) -> serde_yaml::Value {
    let mut entry = serde_yaml::Mapping::new();
    entry.insert(
        serde_yaml::Value::String("timestamp".to_string()),
        serde_yaml::Value::String(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ),
    );
    entry.insert(
        serde_yaml::Value::String("sources".to_string()),
        serde_yaml::Value::Sequence(
            sources
                .iter()
                .map(|s| serde_yaml::Value::String(s.clone()))
                .collect(),
        ),
    );
    entry.insert(
        serde_yaml::Value::String("destination".to_string()),
        serde_yaml::Value::String(dest.to_string()),
    );
    entry.insert(
        serde_yaml::Value::String("files_copied".to_string()),
        serde_yaml::Value::Number(result.copied.len().into()),
    );
    entry.insert(
        serde_yaml::Value::String("files_deleted".to_string()),
        serde_yaml::Value::Number(result.deleted.len().into()),
    );
    serde_yaml::Value::Mapping(entry)
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
