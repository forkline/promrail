use std::collections::HashSet;
use std::path::PathBuf;

use console::style;
use log::debug;

use crate::config::Config;
use crate::error::{AppResult, PromrailError};
use crate::files::{FileDiscovery, FileSelector};
use crate::git::{FileDiff, GitRepo, compute_diff, format_colored_diff};

pub struct DiffArgs {
    pub source: String,
    pub dest: String,
    pub filter: Vec<String>,
    pub delete: bool,
    pub dest_based: bool,
    pub include_protected: bool,
}

pub struct PromotionResult {
    pub copied: Vec<FileDiff>,
    pub deleted: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub protected: Vec<PathBuf>,
}

pub fn execute(
    config: &Config,
    repo: &GitRepo,
    args: &DiffArgs,
    show_diff: bool,
    quiet_summary: bool,
) -> AppResult<PromotionResult> {
    let (_, repo_config) = config.get_repo(None)?;

    let source_env = repo_config.environments.get(&args.source).ok_or_else(|| {
        PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.source.clone(),
        }
    })?;

    let dest_env = repo_config.environments.get(&args.dest).ok_or_else(|| {
        PromrailError::EnvironmentNotFound {
            repo: config.default_repo.clone(),
            env: args.dest.clone(),
        }
    })?;

    if args.source == args.dest {
        return Err(PromrailError::SameEnvironment(args.source.clone()));
    }

    let source_path = PathBuf::from(&source_env.path);
    let dest_path = PathBuf::from(&dest_env.path);

    println!(
        "Comparing {} -> {}",
        style(&args.source).cyan(),
        style(&args.dest).yellow()
    );
    println!();

    let selector = FileSelector::from_config(config)?;
    let discovery = FileDiscovery::new(selector);

    let source_files = discovery.discover(
        &repo.path.join(&source_path),
        &args.filter,
        args.include_protected,
    )?;
    let dest_files = discovery.discover(
        &repo.path.join(&dest_path),
        &args.filter,
        args.include_protected,
    )?;

    let source_set: HashSet<_> = source_files.files.iter().cloned().collect();
    let dest_set: HashSet<_> = dest_files.files.iter().cloned().collect();

    let dest_subdirs = if args.dest_based {
        crate::files::discovery::get_subdirs_recursive(&repo.path.join(&dest_path), false)
    } else {
        HashSet::new()
    };

    let mut copied = Vec::new();
    let skipped = Vec::new();
    let protected = Vec::new();

    for file in &source_files.files {
        if args.dest_based
            && let Some(parent) = file.parent()
            && !dest_subdirs.contains(parent)
            && !dest_set.iter().any(|f| f.starts_with(parent))
        {
            debug!(
                "Skipping copy (dest-based, dir not in dest): {}",
                file.display()
            );
            continue;
        }

        let dest_file = dest_path.join(file);
        let source_file = source_path.join(file);

        let source_content = repo.read_file(&source_file)?;
        let dest_content = repo.read_file(&dest_file)?;

        match (source_content, dest_content) {
            (Some(src), None) => {
                println!("{}", style(format!("+ {}", file.display())).green());
                let diff = FileDiff::added(file.clone(), src);
                if show_diff {
                    display_file_diff(&diff);
                }
                copied.push(diff);
            }
            (Some(src), Some(dst)) => {
                if src != dst {
                    println!("{}", style(format!("~ {}", file.display())).yellow());
                    let diff = FileDiff::modified(file.clone(), dst, src);
                    if show_diff {
                        display_file_diff(&diff);
                    }
                    copied.push(diff);
                } else {
                    debug!("= {} (unchanged)", file.display());
                }
            }
            (None, _) => {
                debug!("? {} (source missing)", file.display());
            }
        }
    }

    let mut deleted = Vec::new();

    if args.delete {
        let source_subdirs =
            crate::files::discovery::get_subdirs_recursive(&repo.path.join(&source_path), false);

        for file in &dest_files.files {
            if !source_set.contains(file) {
                if args.dest_based
                    && let Some(parent) = file.parent()
                    && !source_subdirs.contains(parent)
                {
                    debug!("Skipping deletion (dest-based): {}", file.display());
                    continue;
                }

                println!("{}", style(format!("- {}", file.display())).red());
                deleted.push(file.clone());
            }
        }
    }

    println!();
    if !quiet_summary {
        println!("Summary:");
        println!("  {} files to copy", style(copied.len()).green());
        if args.delete {
            println!("  {} files to delete", style(deleted.len()).red());
        }
        println!();
    }

    Ok(PromotionResult {
        copied,
        deleted,
        skipped,
        protected,
    })
}

pub fn display_file_diff(diff: &FileDiff) {
    let lines = match (&diff.old_content, &diff.new_content) {
        (None, Some(new)) => new.lines().map(|l| format!("+{}", l)).collect(),
        (Some(old), None) => old.lines().map(|l| format!("-{}", l)).collect(),
        (Some(old), Some(new)) => compute_diff(Some(old), Some(new)),
        (None, None) => vec![],
    };

    if !lines.is_empty() {
        println!("{}", format_colored_diff(&lines));
    }
}
