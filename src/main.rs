use std::path::PathBuf;

use clap::Parser;
use console::style;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod config;
mod error;
mod files;
mod git;
mod versions;

use cli::{Cli, Commands, LogLevel, VersionsCommands};
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
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return PathBuf::from(path.replacen('~', &home.display().to_string(), 1));
        }
    }
    PathBuf::from(path)
}

fn main() -> AppResult<()> {
    let args = Cli::parse();

    setup_logging(args.log_level);

    match args.command {
        Commands::Versions { command } => handle_versions_command(command),
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
            dry_run,
        } => {
            let dest_path = expand_path(&path);
            let versions_path = PathBuf::from(&file);

            info!("Loading versions from {}", versions_path.display());
            let report: versions::VersionReport =
                serde_json::from_str(&std::fs::read_to_string(&versions_path)?)?;

            info!("Applying versions to {}", dest_path.display());

            let updated = versions::apply_versions(&report, &dest_path, dry_run)?;

            if dry_run {
                info!("Dry run: would update {} files", style(updated).yellow());
            } else {
                info!("Updated {} files", style(updated).green());
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
            source,
            dest,
            filter_vec,
            no_delete,
            dest_based,
            dry_run,
            yes,
            diff,
            include_protected,
        } => {
            if config.git.require_clean_tree && !repo.is_clean()? {
                return Err(error::PromrailError::DirtyTree);
            }

            let promote_args = commands::PromoteArgs {
                source,
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
            };
            commands::promote::execute(&config, &repo, &promote_args)?;
        }
        Commands::Validate {} => {
            commands::validate::execute(&config, &repo)?;
        }
        Commands::Versions { .. } => unreachable!(),
    }

    Ok(())
}
