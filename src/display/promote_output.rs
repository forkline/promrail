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
    if data.copied_files.is_empty()
        && data.deleted_files.is_empty()
        && data.version_changes.is_empty()
    {
        println!("No changes to promote");
        return;
    }

    match level {
        OutputLevel::Minimal => print_minimal(data),
        OutputLevel::Normal => print_normal(data),
        OutputLevel::Verbose => print_verbose(data),
    }

    if data.dry_run {
        println!();
        println!("Dry run complete. No files were modified.");
    }
}

fn print_minimal(data: &PromotionOutputData) {
    print_summary(data);
}

fn print_normal(data: &PromotionOutputData) {
    print_copied_files(data);
    println!();
    print_summary(data);

    if !data.version_changes.is_empty() {
        println!();
        print_version_updates(&data.version_changes);
    }
}

fn print_verbose(data: &PromotionOutputData) {
    print_header(data);
    println!();

    print_copied_files(data);
    println!();

    print_structured_version_updates(&data.version_changes);

    print_summary(data);
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

fn print_copied_files(data: &PromotionOutputData) {
    if !data.copied_files.is_empty() {
        println!("Copied:");
        for file in &data.copied_files {
            println!("  {}", style(file.display()).green());
        }
    }
}

fn print_summary(data: &PromotionOutputData) {
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

fn print_version_updates(changes: &[VersionChangeSummary]) {
    println!("Version updates:");
    for change in changes {
        println!(
            "  {}: {} {} {}",
            style(&change.name).white(),
            style(&change.before).red(),
            style("→").cyan(),
            style(&change.after).green()
        );
    }
}

fn print_structured_version_updates(changes: &[VersionChangeSummary]) {
    if changes.is_empty() {
        return;
    }

    let mut by_component: BTreeMap<&str, Vec<&VersionChangeSummary>> = BTreeMap::new();
    for change in changes {
        by_component
            .entry(&change.component)
            .or_default()
            .push(change);
    }

    println!("Structured Version Updates:");
    for (component, component_changes) in &by_component {
        println!("  {}:", style(component).yellow());
        for change in component_changes {
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
