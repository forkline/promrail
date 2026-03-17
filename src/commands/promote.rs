use std::io::{self, Write};
use std::path::PathBuf;

use console::style;
use tracing::info;

use crate::commands::diff::{self, DiffArgs};
use crate::config::Config;
use crate::error::AppResult;
use crate::git::GitRepo;

pub struct PromoteArgs {
    pub source: String,
    pub dest: String,
    pub filter: Vec<String>,
    pub delete: bool,
    pub dest_based: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub show_diff: bool,
    pub include_protected: bool,
}

pub fn execute(config: &Config, repo: &GitRepo, args: &PromoteArgs) -> AppResult<()> {
    let (_, repo_config) = config.get_repo(None)?;

    let source_env = repo_config.environments.get(&args.source).ok_or_else(|| {
        crate::error::PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.source.clone(),
        }
    })?;

    let dest_env = repo_config.environments.get(&args.dest).ok_or_else(|| {
        crate::error::PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.dest.clone(),
        }
    })?;

    let source_path = PathBuf::from(&source_env.path);
    let dest_path = PathBuf::from(&dest_env.path);

    let diff_args = DiffArgs {
        source: args.source.clone(),
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
        write_audit_log(config, repo, &args.source, &args.dest, &result)?;
    }

    Ok(())
}

fn write_audit_log(
    config: &Config,
    repo: &GitRepo,
    source: &str,
    dest: &str,
    result: &crate::commands::diff::PromotionResult,
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
            serde_yaml::Value::String("source".to_string()),
            serde_yaml::Value::String(source.to_string()),
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
