use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use console::style;
use log::{debug, info, warn};

use crate::commands::diff::{self, DiffArgs};
use crate::commands::{default_filter, print_promotion_summary};
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
    pub confirm: bool,
    pub show_diff: bool,
    pub include_protected: bool,
    pub allow_duplicates: bool,
    pub only_existing: bool,
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
        config: &Config,
    ) -> AppResult<Self> {
        let sources = if source_vec.is_empty() {
            config
                .default_source
                .clone()
                .map(|s| vec![s])
                .ok_or_else(|| {
                    PromrailError::ConfigInvalid(
                        "no source specified and no default_source in config".to_string(),
                    )
                })?
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

    let mut source_paths: Vec<(String, PathBuf)> = Vec::new();
    for source in &args.sources {
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

    // Create file selector
    let selector = FileSelector::from_config(config)?;
    let discovery = FileDiscovery::new(selector);

    // Collect all files from all sources
    let mut all_files: HashMap<PathBuf, (String, PathBuf)> = HashMap::new();
    let mut duplicates: Vec<(PathBuf, Vec<String>)> = Vec::new();

    for (source_name, source_path) in &source_paths {
        debug!("Discovering files in: {}", source_path.display());
        let discovered = discovery.discover(source_path, &args.filter, args.include_protected)?;
        debug!("  Found {} files", discovered.files.len());

        for file in &discovered.files {
            // file is already a relative path from discover()
            let relative = file.clone();
            let absolute = source_path.join(&relative);

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
                all_files.insert(relative.clone(), (source_name.clone(), absolute));
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
        // file is already relative from discover()
        let relative = file.clone();

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

    if args.sources.len() == 1 {
        println!("Comparing {} -> {}", args.sources[0], args.dest);
    } else {
        println!("Comparing {} sources -> {}", args.sources.len(), args.dest);
    }
    println!();

    if total_copies == 0 && total_deletes == 0 {
        println!("No changes to promote");
        return Ok(());
    }

    for relative in all_files.keys() {
        println!("~ {}", relative.display());
    }

    println!();
    println!("Summary:");
    println!("  {} files to copy", style(total_copies).green());
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

    let snapshot_id = create_multi_source_snapshot(&dest_path)?;
    info!("Created snapshot: {}", snapshot_id);

    for (relative, (source_name, source_file)) in &all_files {
        let dest_file = dest_path.join(relative);

        if is_protected(relative, &config.protected_dirs) && !args.include_protected {
            continue;
        }

        if let Some(parent) = dest_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(source_file, &dest_file)?;
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

    save_multi_source_snapshot(&dest_path, &snapshot_id, &all_files, &files_to_delete)?;

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

fn get_component(path: &Path) -> String {
    let parts: Vec<_> = path.components().take(2).collect();
    parts
        .iter()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn is_protected(path: &Path, protected_dirs: &[String]) -> bool {
    path.components().any(|component| {
        protected_dirs
            .iter()
            .any(|p| component.as_os_str() == std::ffi::OsStr::new(p))
    })
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

    entries.push(serde_yaml::Value::Mapping({
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
    }));

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
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping({
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
                entry
            })]),
        );
        map
    });

    std::fs::write(log_path, serde_yaml::to_string(&doc)?)?;
    Ok(())
}
