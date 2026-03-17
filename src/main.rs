use std::path::PathBuf;

use clap::Parser;
use console::style;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod config;
mod error;
mod files;
mod git;
mod versions;

use cli::{Cli, Commands, ConfigCommands, LogLevel, SnapshotCommands, VersionsCommands};
use config::Config;
use error::AppResult;
use git::GitRepo;

fn setup_logging(level: LogLevel) {
    let filter = match level {
        LogLevel::Error => EnvFilter::new("error"),
        LogLevel::Warn => EnvFilter::new("warn"),
        LogLevel::Info => EnvFilter::new("info"),
        LogLevel::Debug => EnvFilter::new("debug"),
        LogLevel::Trace => EnvFilter::new("trace"),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}

fn find_config_path(config_arg: Option<&str>) -> AppResult<PathBuf> {
    if let Some(path) = config_arg {
        return Ok(PathBuf::from(path));
    }

    let candidates = [
        "promrail.yaml",
        "promrail.yml",
        ".promrail.yaml",
        ".promrail.yml",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(error::PromrailError::ConfigNotFound(
        "promrail.yaml (set PROMRAIL_CONFIG env var or use --config)".to_string(),
    ))
}

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        return PathBuf::from(path.replacen('~', &home.display().to_string(), 1));
    }
    PathBuf::from(path)
}

fn main() -> AppResult<()> {
    let args = Cli::parse();

    setup_logging(args.log_level);

    match args.command {
        Commands::Versions { command } => handle_versions_command(command),
        Commands::Snapshot { command } => handle_snapshot_command(command),
        Commands::Config { command } => handle_config_command(command),
        _ => handle_repo_command(args),
    }
}

fn handle_versions_command(command: VersionsCommands) -> AppResult<()> {
    match command {
        VersionsCommands::Extract {
            path,
            output,
            filter_vec,
        } => {
            let repo_path = expand_path(&path);
            info!("Extracting versions from {}", repo_path.display());

            let filters = if filter_vec.is_empty() {
                vec![]
            } else {
                filter_vec
            };

            let report = versions::extract_versions(&repo_path, &filters)?;

            let json = serde_json::to_string_pretty(&report)?;

            if let Some(output_path) = output {
                std::fs::write(&output_path, &json)?;
                info!("Written to {}", output_path);
            } else {
                println!("{}", json);
            }

            info!(
                "Extracted {} components",
                style(report.components.len()).cyan()
            );
        }
        VersionsCommands::Apply {
            file,
            path,
            component,
            check_conflicts,
            snapshot,
            dry_run,
        } => {
            let dest_path = expand_path(&path);
            let versions_path = PathBuf::from(&file);

            info!("Loading versions from {}", versions_path.display());
            let report: versions::VersionReport =
                serde_json::from_str(&std::fs::read_to_string(&versions_path)?)?;

            let options = versions::ApplyOptions {
                components: component
                    .as_ref()
                    .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default(),
                dry_run,
                check_conflicts,
                create_snapshot: snapshot,
            };

            info!("Applying versions to {}", dest_path.display());

            let result = versions::apply_versions(&report, &dest_path, &options)?;

            if !result.conflicts.is_empty() {
                println!("\n{} Conflicts detected:\n", style("ERROR:").red());
                for conflict in &result.conflicts {
                    println!(
                        "  {} in {}: {}",
                        style(&conflict.component).yellow(),
                        style(&conflict.file).dim(),
                        conflict.details
                    );
                }
                println!("\nUse without --check-conflicts to proceed anyway.");
                return Ok(());
            }

            if dry_run {
                info!(
                    "Dry run: would update {} files",
                    style(result.updated_files.len()).yellow()
                );
            } else {
                info!(
                    "Updated {} files",
                    style(result.updated_files.len()).green()
                );

                if let Some(snapshot_id) = &result.snapshot_id {
                    info!("Created snapshot: {}", style(snapshot_id).cyan());
                }
            }
        }
        VersionsCommands::Diff {
            source,
            dest,
            filter_vec,
        } => {
            let source_path = expand_path(&source);
            let dest_path = expand_path(&dest);

            info!("Comparing versions: {} -> {}", source, dest);

            let filters = if filter_vec.is_empty() {
                vec![]
            } else {
                filter_vec
            };

            let source_report = versions::extract_versions(&source_path, &filters)?;
            let dest_report = versions::extract_versions(&dest_path, &filters)?;

            let diffs = versions::diff_versions(&source_report, &dest_report);

            if diffs.is_empty() {
                info!("No version differences found");
            } else {
                println!("\nVersion Differences:\n");
                for diff in &diffs {
                    println!("{}", style(&diff.component).cyan().bold());

                    for chart in &diff.helm_charts {
                        println!(
                            "  {} {} -> {}",
                            style(&chart.name).yellow(),
                            style(&chart.source_version).red(),
                            style(&chart.dest_version).green()
                        );
                    }

                    for image in &diff.container_images {
                        println!(
                            "  {} {} -> {}",
                            style(&image.name).yellow(),
                            style(&image.source_tag).red(),
                            style(&image.dest_tag).green()
                        );
                    }
                    println!();
                }
            }
        }
        VersionsCommands::Merge {
            source_vec,
            output,
            explain,
        } => {
            if source_vec.len() < 2 {
                return Err(error::PromrailError::ConfigInvalid(
                    "merge requires at least 2 sources".to_string(),
                ));
            }

            info!("Merging versions from {} sources", source_vec.len());

            // Extract versions from all sources
            let mut sources: Vec<(String, versions::VersionReport)> = Vec::new();
            for source in &source_vec {
                let source_path = expand_path(source);
                let report = versions::extract_versions(&source_path, &[])?;
                sources.push((source.clone(), report));
            }

            // Load config to get rules
            let config_path = find_config_path(None)?;
            let config = Config::load(&config_path)?;

            // Merge with rules
            let result = versions::merge_versions(&sources, &config.rules)?;

            // Show warnings
            for warning in &result.warnings {
                warn!("{}", warning);
            }

            // Output explanation if requested
            if explain {
                println!("{}", versions::explain_merge(&result));
            }

            // Output JSON
            if let Some(output_path) = output {
                let json = serde_json::to_string_pretty(&result.report)?;
                std::fs::write(&output_path, &json)?;
                info!("Written merged versions to {}", output_path);
            } else if !explain {
                let json = serde_json::to_string_pretty(&result.report)?;
                println!("{}", json);
            }

            // Summary
            info!(
                "Merged {} components from {} sources",
                result.report.components.len(),
                sources.len()
            );

            if !result.removed.is_empty() {
                warn!("Removed {} components due to rules", result.removed.len());
            }
        }
    }

    Ok(())
}

fn handle_snapshot_command(command: SnapshotCommands) -> AppResult<()> {
    match command {
        SnapshotCommands::List { path } => {
            let dest_path = expand_path(&path);
            let snapshots = versions::list(&dest_path)?;

            if snapshots.is_empty() {
                info!("No snapshots found");
            } else {
                println!("\nSnapshots:\n");
                for snap in &snapshots {
                    let status = match snap.status {
                        versions::SnapshotStatus::Applied => style("applied").green(),
                        versions::SnapshotStatus::RolledBack => style("rolled back").yellow(),
                        versions::SnapshotStatus::Pending => style("pending").dim(),
                    };
                    println!(
                        "{} {} - {} files modified ({})",
                        style(&snap.id).cyan(),
                        status,
                        style(snap.files_modified.len()).dim(),
                        style(&snap.created_at).dim()
                    );
                }
            }
        }
        SnapshotCommands::Show { id, path } => {
            let dest_path = expand_path(&path);

            if let Some(snap) = versions::get(&dest_path, &id)? {
                println!(
                    "\n{} {}\n",
                    style("Snapshot:").bold(),
                    style(&snap.id).cyan()
                );
                println!("Created: {}", style(&snap.created_at).dim());
                println!("Source: {}", style(&snap.source_path).dim());
                println!("Status: {:?}", snap.status);
                println!("Files modified: {}", snap.files_modified.len());

                if !snap.version_changes.is_empty() {
                    println!("\nVersion changes:");
                    for (component, changes) in &snap.version_changes {
                        println!("\n  {}:", style(component).yellow());
                        for change in changes {
                            println!(
                                "    {} {} -> {}",
                                style(&change.name).dim(),
                                style(&change.before).red(),
                                style(&change.after).green()
                            );
                        }
                    }
                }
            } else {
                println!("Snapshot '{}' not found", style(&id).red());
            }
        }
        SnapshotCommands::Rollback { id, path } => {
            let dest_path = expand_path(&path);

            info!("Rolling back to snapshot {}...", style(&id).cyan());

            if versions::rollback(&dest_path, &id)? {
                info!("Rollback complete");
            } else {
                info!("Rollback failed or snapshot already rolled back");
            }
        }
        SnapshotCommands::Delete { id, path } => {
            let dest_path = expand_path(&path);

            if versions::delete(&dest_path, &id)? {
                info!("Deleted snapshot {}", style(&id).cyan());
            } else {
                info!("Snapshot '{}' not found", style(&id).red());
            }
        }
    }

    Ok(())
}

fn handle_config_command(command: ConfigCommands) -> AppResult<()> {
    match command {
        ConfigCommands::Show {} => {
            println!("{}", Config::generate_full_docs());
        }
        ConfigCommands::Example { output } => {
            let example = Config::generate_full_example();
            match output {
                Some(path) => {
                    std::fs::write(&path, example)?;
                    info!("Written example config to {}", path);
                }
                None => println!("{}", example),
            }
        }
        ConfigCommands::Diff { source, dest, file } => {
            let source_path = expand_path(&source);
            let dest_path = expand_path(&dest);

            let files = file.map(|f| f.split(',').map(|s| s.trim().to_string()).collect());

            info!("Comparing config: {} -> {}", source, dest);

            let diff = versions::diff_configs(&source_path, &dest_path, files)?;

            println!("{}", versions::format_unified_diff(&diff));
        }
    }

    Ok(())
}

fn handle_repo_command(args: cli::Cli) -> AppResult<()> {
    let config_path = find_config_path(args.config.as_deref())?;
    info!("Loading config from {}", config_path.display());

    let config = Config::load(&config_path)?;

    let (_, repo_config) = config.get_repo(args.repo.as_deref())?;
    let repo_path = repo_config.resolved_path();

    let repo = GitRepo::discover(&repo_path)?;

    match args.command {
        Commands::Diff {
            source,
            dest,
            filter_vec,
            no_delete,
            dest_based,
            include_protected,
        } => {
            let diff_args = commands::DiffArgs {
                source,
                dest,
                filter: if filter_vec.is_empty() {
                    vec![".*".to_string()]
                } else {
                    filter_vec
                },
                delete: !no_delete,
                dest_based,
                include_protected,
            };
            commands::diff::execute(&config, &repo, &diff_args, true)?;
        }
        Commands::Promote {
            source_vec,
            dest,
            filter_vec,
            no_delete,
            dest_based,
            dry_run,
            yes,
            diff,
            include_protected,
            force,
            allow_duplicates,
            only_existing,
        } => {
            if config.git.require_clean_tree && !force && !repo.is_clean()? {
                return Err(error::PromrailError::DirtyTree);
            }

            let promote_args = commands::PromoteArgs {
                sources: source_vec,
                dest,
                filter: if filter_vec.is_empty() {
                    vec![".*".to_string()]
                } else {
                    filter_vec
                },
                delete: !no_delete,
                dest_based,
                dry_run,
                yes,
                show_diff: diff,
                include_protected,
                allow_duplicates,
                only_existing,
            };
            commands::promote::execute(&config, &repo, &promote_args)?;
        }
        Commands::Validate {} => {
            commands::validate::execute(&config, &repo)?;
        }
        Commands::Versions { .. } | Commands::Snapshot { .. } | Commands::Config { .. } => {
            unreachable!()
        }
    }

    Ok(())
}
