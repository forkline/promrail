use std::path::PathBuf;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod config;
mod error;
mod files;
mod git;

use cli::{Cli, Commands, LogLevel};
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

fn main() -> AppResult<()> {
    let args = Cli::parse();

    setup_logging(args.log_level);

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
    }

    Ok(())
}
