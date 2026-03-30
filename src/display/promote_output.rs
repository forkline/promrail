use std::collections::BTreeMap;
use std::path::PathBuf;

use console::style;

use crate::config::OutputLevel;
use crate::versions::models::{VersionChangeKind, VersionChangeSummary};

pub struct PromotionOutputData {
    pub copied_files: Vec<PathBuf>,
    pub deleted_files: Vec<PathBuf>,
    pub version_changes: Vec<VersionChangeSummary>,
    pub sources: Vec<String>,
    pub dest: String,
    pub dry_run: bool,
}

pub fn print_promotion_result(data: &PromotionOutputData, level: OutputLevel) {
    print_header(data);

    if data.copied_files.is_empty()
        && data.deleted_files.is_empty()
        && data.version_changes.is_empty()
    {
        println!();
        println!("No changes to promote");
        return;
    }

    for file in &data.copied_files {
        println!("~ {}", file.display());
    }
    println!();

    print_summary(data, level);

    if data.dry_run {
        println!();
        println!("Dry run complete. No files were modified.");
    }
}

fn print_header(data: &PromotionOutputData) {
    if data.sources.len() == 1 {
        println!(
            "Comparing {} -> {}",
            style(&data.sources[0]).cyan(),
            style(&data.dest).yellow()
        );
    } else {
        println!(
            "Comparing {} sources -> {}",
            style(data.sources.len()).cyan(),
            style(&data.dest).yellow()
        );
    }
}

fn print_summary(data: &PromotionOutputData, level: OutputLevel) {
    match level {
        OutputLevel::Minimal => print_minimal_summary(data),
        OutputLevel::Normal => print_normal_summary(data),
        OutputLevel::Verbose => print_verbose_summary(data),
    }
}

fn print_minimal_summary(data: &PromotionOutputData) {
    println!("Summary:");
    if !data.copied_files.is_empty() {
        println!("  {} files copied", style(data.copied_files.len()).green());
    }
    if !data.version_changes.is_empty() {
        println!(
            "  {} versions updated",
            style(data.version_changes.len()).cyan()
        );
    }
    if !data.deleted_files.is_empty() {
        println!("  {} files deleted", style(data.deleted_files.len()).red());
    }
}

fn print_normal_summary(data: &PromotionOutputData) {
    print_minimal_summary(data);

    if !data.version_changes.is_empty() {
        println!();
        println!("Version updates:");
        for change in &data.version_changes {
            println!(
                "  {}: {} {} {}",
                style(&change.name).white(),
                style(&change.before).red(),
                style("→").cyan(),
                style(&change.after).green()
            );
        }
    }
}

fn print_verbose_summary(data: &PromotionOutputData) {
    let mut by_component: BTreeMap<&str, Vec<&VersionChangeSummary>> = BTreeMap::new();
    for change in &data.version_changes {
        by_component
            .entry(&change.component)
            .or_default()
            .push(change);
    }

    if !by_component.is_empty() {
        println!("Structured Version Updates:");
        for (component, changes) in &by_component {
            println!("  {}:", style(component).yellow());
            for change in changes {
                let kind_label = match change.kind {
                    VersionChangeKind::HelmChart => "chart",
                    VersionChangeKind::ContainerImage => "image",
                };
                println!(
                    "    {} ({}): {} {} {}",
                    style(&change.name).white(),
                    kind_label,
                    style(&change.before).red(),
                    style("→").cyan(),
                    style(&change.after).green()
                );
            }
        }
        println!();
    }

    if !data.copied_files.is_empty() {
        println!("Copied:");
        for file in &data.copied_files {
            println!("  {}", style(file.display()).green());
        }
        println!();
    }

    println!("Summary:");
    if !data.copied_files.is_empty() {
        println!("  {} files copied", style(data.copied_files.len()).green());
    }
    if !data.version_changes.is_empty() {
        println!(
            "  {} versions updated",
            style(data.version_changes.len()).cyan()
        );
    }
    if !data.deleted_files.is_empty() {
        println!("  {} files deleted", style(data.deleted_files.len()).red());
    }
}
