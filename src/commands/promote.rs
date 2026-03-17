use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use console::style;
use tracing::{info, warn};

use crate::commands::diff::{self, DiffArgs};
use crate::config::{Config, PromotionAction};
use crate::error::{AppResult, PromrailError};
use crate::files::{FileDiscovery, FileSelector};
use crate::git::{FileDiff, GitRepo};

pub struct PromoteArgs {
    pub sources: Vec<String>,
    pub dest: String,
    pub filter: Vec<String>,
    pub delete: bool,
    pub dest_based: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub show_diff: bool,
    pub include_protected: bool,
    pub allow_duplicates: bool,
    pub only_existing: bool,
}

pub fn execute(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let is_multi_source = args.sources.len() > 1;

    if is_multi_source {
        execute_multi_source(config, repo, args)
    } else {
        execute_single_source(config, repo, args)
    }
}

fn execute_single_source(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let source = &args.sources[0];
    let (_, repo_config) = config.get_repo(None)?;

    let source_env =
        repo_config
            .environments
            .get(source)
            .ok_or_else(|| PromrailError::EnvironmentNotFound {
                repo: config.default_repo.clone(),
                env: source.clone(),
            })?;

    let dest_env = repo_config.environments.get(&args.dest).ok_or_else(|| {
        PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.dest.clone(),
        }
    })?;

    let source_path = PathBuf::from(&source_env.path);
    let dest_path = PathBuf::from(&dest_env.path);

    let diff_args = DiffArgs {
        source: source.clone(),
        dest: args.dest.clone(),
        filter: args.filter.clone(),
        delete: args.delete,
        dest_based: args.dest_based,
        include_protected: args.include_protected,
    };

    let result = diff::execute(config, repo, &diff_args, args.show_diff)?;

    if result.copied.is_empty() && result.deleted.is_empty() {
        info!("No changes to promote");
        return Ok(());
    }

    if args.dry_run {
        info!("Dry run complete. No files were modified.");
        return Ok(());
    }

    if !args.yes {
        print!("Proceed with promotion? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().to_lowercase().starts_with('y') {
            info!("Promotion cancelled");
            return Ok(());
        }
    }

    info!("Applying changes...");

    for file_diff in &result.copied {
        let source_file = source_path.join(&file_diff.path);
        let dest_file = dest_path.join(&file_diff.path);

        repo.copy_file(&source_file, &dest_file)?;
        info!("Copied: {}", style(&file_diff.path.display()).green());
    }

    if args.delete {
        for file in &result.deleted {
            let dest_file = dest_path.join(file);
            repo.delete_file(&dest_file)?;
            info!("Deleted: {}", style(file.display()).red());
        }
    }

    info!("Promotion complete!");
    info!("  {} files copied", result.copied.len());
    if args.delete {
        info!("  {} files deleted", result.deleted.len());
    }

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
    let (_, repo_config) = config.get_repo(None)?;

    let dest_env = repo_config.environments.get(&args.dest).ok_or_else(|| {
        PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.dest.clone(),
        }
    })?;

    // Destination is within the current repo
    let dest_path = repo.path.join(&dest_env.path);

    info!(
        "Multi-source promotion from {} sources to {}",
        args.sources.len(),
        args.dest
    );

    // Resolve source paths - can be either environment names or repo names
    let mut source_paths: Vec<(String, PathBuf)> = Vec::new();
    for source in &args.sources {
        // First, try to resolve as an environment in the current repo
        if let Some(source_env) = repo_config.environments.get(source) {
            let source_path = repo.path.join(&source_env.path);
            source_paths.push((source.clone(), source_path));
        }
        // Second, try to resolve as a separate repo
        else if let Some((repo_name, other_repo_config)) = config.repos.get_key_value(source) {
            let repo_path = other_repo_config.resolved_path();
            source_paths.push((repo_name.clone(), repo_path));
        } else {
            return Err(PromrailError::EnvironmentNotFound {
                repo: config.default_repo.clone(),
                env: source.clone(),
            });
        }
    }

    // Create file selector
    let selector = FileSelector::from_config(config)?;
    let discovery = FileDiscovery::new(selector);

    // Collect all files from all sources
    let mut all_files: HashMap<PathBuf, (String, PathBuf)> = HashMap::new(); // relative -> (source_name, absolute)
    let mut duplicates: Vec<(PathBuf, Vec<String>)> = Vec::new(); // (file, [sources])

    for (source_name, source_path) in &source_paths {
        let discovered = discovery.discover(source_path, &args.filter, args.include_protected)?;

        for file in &discovered.files {
            let relative = match file.strip_prefix(source_path) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };

            // Check if this file is in a protected directory
            if is_protected(&relative, &config.protected_dirs) && !args.include_protected {
                continue;
            }

            // Check if source includes this component (from rules)
            if config.rules.has_rules() {
                let component = get_component(&relative);
                if !config.rules.source_includes(source_name, &component) {
                    continue;
                }

                // Check component action
                let action = config.rules.get_action(&component);
                if action == PromotionAction::Never {
                    info!("Skipping {} (action: never)", component);
                    continue;
                }
            }

            // Check only_existing
            if args.only_existing || config.rules.global.promote_options.only_existing {
                let component = get_component(&relative);
                if !component_exists_in_dest(&dest_path, &component) {
                    info!("Skipping new component: {} (only_existing)", component);
                    continue;
                }
            }

            // Check for duplicates
            if let Some((existing_source, _)) = all_files.get(&relative) {
                // Duplicate found
                let existing = existing_source.clone();
                if let Some(pos) = duplicates.iter().position(|(p, _)| p == &relative) {
                    duplicates[pos].1.push(source_name.clone());
                } else {
                    duplicates.push((relative.clone(), vec![existing, source_name.clone()]));
                }
            } else {
                all_files.insert(relative.clone(), (source_name.clone(), file.clone()));
            }
        }
    }

    // Handle duplicates
    if !duplicates.is_empty()
        && !args.allow_duplicates
        && !config.rules.global.promote_options.allow_duplicates
    {
        return Err(PromrailError::DuplicateFiles(
            duplicates
                .iter()
                .map(|(p, s)| format!("{} (in: {})", p.display(), s.join(", ")))
                .collect(),
        ));
    }

    if !duplicates.is_empty() {
        for (file, sources) in &duplicates {
            warn!(
                "Duplicate file {} found in sources: {} (using first source)",
                file.display(),
                sources.join(", ")
            );
        }
    }

    // Discover destination files
    let dest_discovered = discovery.discover(&dest_path, &args.filter, args.include_protected)?;

    // Calculate files to delete (not in any source)
    let mut files_to_delete: Vec<PathBuf> = Vec::new();
    let all_relative: HashSet<PathBuf> = all_files.keys().cloned().collect();

    for file in &dest_discovered.files {
        let relative = match file.strip_prefix(&dest_path) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };

        if is_protected(&relative, &config.protected_dirs) {
            continue;
        }

        if !all_relative.contains(&relative) {
            files_to_delete.push(relative);
        }
    }

    // Show summary
    let total_copies = all_files.len();
    let total_deletes = files_to_delete.len();

    if total_copies == 0 && total_deletes == 0 {
        info!("No changes to promote");
        return Ok(());
    }

    info!(
        "Changes: {} files to copy, {} files to delete",
        total_copies, total_deletes
    );

    if args.dry_run {
        info!("Dry run complete. No files were modified.");
        return Ok(());
    }

    if !args.yes {
        print!("Proceed with multi-source promotion? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().to_lowercase().starts_with('y') {
            info!("Promotion cancelled");
            return Ok(());
        }
    }

    // Create snapshot before applying changes
    let snapshot_id = create_multi_source_snapshot(&dest_path)?;
    info!("Created snapshot: {}", style(&snapshot_id).cyan());

    // Apply changes
    info!("Applying changes...");

    for (relative, (source_name, source_file)) in &all_files {
        let dest_file = dest_path.join(relative);

        // Skip protected directories
        if is_protected(relative, &config.protected_dirs) && !args.include_protected {
            continue;
        }

        if let Some(parent) = dest_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(source_file, &dest_file)?;
        info!(
            "Copied: {} (from {})",
            style(relative.display()).green(),
            style(source_name).dim()
        );
    }

    if args.delete {
        for relative in &files_to_delete {
            let dest_file = dest_path.join(relative);
            std::fs::remove_file(&dest_file)?;
            info!("Deleted: {}", style(relative.display()).red());
        }
    }

    info!("Multi-source promotion complete!");
    info!("  {} files copied", total_copies);
    if args.delete {
        info!("  {} files deleted", total_deletes);
    }

    // Save snapshot
    save_multi_source_snapshot(&dest_path, &snapshot_id, &all_files, &files_to_delete)?;

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

fn get_component(path: &Path) -> String {
    let parts: Vec<_> = path.components().take(2).collect();
    parts
        .iter()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn is_protected(path: &Path, protected_dirs: &[String]) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if protected_dirs.contains(&name.to_string()) {
            return true;
        }
    }
    false
}

fn component_exists_in_dest(dest: &Path, component: &str) -> bool {
    dest.join(component).exists()
}

fn create_multi_source_snapshot(dest_path: &Path) -> AppResult<String> {
    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
        .replace([':', '-'], "")
        .split('.')
        .next()
        .unwrap_or("unknown")
        .to_string();

    let snapshot_id = format!("snap_{}", timestamp);

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
                map.insert(
                    serde_yaml::Value::String("files_deleted".to_string()),
                    serde_yaml::Value::Sequence(
                        deleted
                            .iter()
                            .map(|p| serde_yaml::Value::String(p.display().to_string()))
                            .collect(),
                    ),
                );
                break;
            }
        }
    }

    std::fs::write(&snapshot_file, serde_yaml::to_string(&doc)?)?;

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

    let entry = serde_yaml::to_string(&serde_yaml::Value::Mapping({
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("timestamp".to_string()),
            serde_yaml::Value::String(
                time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            ),
        );
        map.insert(
            serde_yaml::Value::String("sources".to_string()),
            serde_yaml::Value::Sequence(
                sources
                    .iter()
                    .map(|s| serde_yaml::Value::String(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            serde_yaml::Value::String("destination".to_string()),
            serde_yaml::Value::String(dest.to_string()),
        );
        map.insert(
            serde_yaml::Value::String("files_copied".to_string()),
            serde_yaml::Value::Number(result.copied.len().into()),
        );
        map.insert(
            serde_yaml::Value::String("files_deleted".to_string()),
            serde_yaml::Value::Number(result.deleted.len().into()),
        );
        map
    }))?;

    let existing = if log_path.exists() {
        std::fs::read_to_string(&log_path)?
    } else {
        "promotions:\n".to_string()
    };

    let new_content = format!("{}  - {}", existing, entry.replace("---\n", "").trim());
    std::fs::write(&log_path, new_content)?;

    Ok(())
}
