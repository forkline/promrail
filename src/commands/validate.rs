use console::style;
use tracing::info;

use crate::config::Config;
use crate::error::AppResult;
use crate::git::GitRepo;

pub struct ValidateArgs;

pub fn execute(config: &Config, repo: &GitRepo) -> AppResult<()> {
    info!("Validating configuration...");

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if config.allowlist.is_empty() {
        warnings.push("No allowlist patterns defined (will match all files)".to_string());
    }

    if config.repos.is_empty() {
        errors.push("No repositories defined".to_string());
    }

    for (name, repo_config) in &config.repos {
        let path = repo_config.resolved_path();
        if !path.exists() {
            errors.push(format!(
                "Repository '{}' path does not exist: {}",
                name,
                path.display()
            ));
        }

        for (env_name, env_config) in &repo_config.environments {
            let env_path = path.join(&env_config.path);
            if !env_path.exists() {
                errors.push(format!(
                    "Environment '{}' in repo '{}' does not exist: {}",
                    env_name,
                    name,
                    env_path.display()
                ));
            }
        }
    }

    if config.git.require_clean_tree {
        match repo.is_clean() {
            Ok(true) => info!("Git tree is clean"),
            Ok(false) => warnings.push("Git tree has uncommitted changes".to_string()),
            Err(e) => warnings.push(format!("Could not check git status: {}", e)),
        }
    }

    println!();

    if !errors.is_empty() {
        println!("{}", style("Errors:").red());
        for error in &errors {
            println!("  {} {}", style("✗").red(), error);
        }
    }

    if !warnings.is_empty() {
        println!("{}", style("Warnings:").yellow());
        for warning in &warnings {
            println!("  {} {}", style("!").yellow(), warning);
        }
    }

    if errors.is_empty() {
        println!("{} Configuration is valid", style("✓").green());
        Ok(())
    } else {
        Err(crate::error::PromrailError::ConfigInvalid(format!(
            "{} validation error(s)",
            errors.len()
        )))
    }
}
