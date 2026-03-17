//! Configuration diff between directories.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::AppResult;
use crate::versions::models::{
    ConfigDiff, DiffHunk, DiffLine, DiffLineKind, DiffSummary, FileConfigDiff,
};

/// Compare configuration files between two directories.
pub fn diff_configs(
    source: &Path,
    dest: &Path,
    files: Option<Vec<String>>,
) -> AppResult<ConfigDiff> {
    let mut file_diffs = Vec::new();

    // Get all YAML files from source
    let source_files = find_yaml_files(source);

    for source_file in &source_files {
        let relative: &Path = source_file
            .strip_prefix(source)
            .unwrap_or(source_file.as_path());
        let dest_file = dest.join(relative);

        // Filter by specified files
        if let Some(ref filter) = files {
            let rel_str = relative.to_string_lossy();
            if !filter.iter().any(|f| rel_str.ends_with(f)) {
                continue;
            }
        }

        if dest_file.exists() {
            let diff = diff_file(source_file, &dest_file, relative)?;
            if !diff.summary.is_empty() {
                file_diffs.push(diff);
            }
        }
    }

    // Check for files in dest that don't exist in source
    let dest_files = find_yaml_files(dest);
    for dest_file in &dest_files {
        let relative: &Path = dest_file.strip_prefix(dest).unwrap_or(dest_file.as_path());
        let source_file = source.join(relative);

        if let Some(ref filter) = files {
            let rel_str = relative.to_string_lossy();
            if !filter.iter().any(|f| rel_str.ends_with(f)) {
                continue;
            }
        }

        if !source_file.exists() {
            // File only exists in dest
            let content = std::fs::read_to_string(dest_file)?;
            let diff = create_removal_diff(relative, &content);
            file_diffs.push(diff);
        }
    }

    Ok(ConfigDiff {
        source_path: source.display().to_string(),
        dest_path: dest.display().to_string(),
        file_diffs,
    })
}

/// Find all YAML files in a directory.
fn find_yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().map(|e| e.to_string_lossy());
            if ext
                .as_ref()
                .map(|e| *e == "yaml" || *e == "yml")
                .unwrap_or(false)
            {
                // Skip custom, test, secret files
                let path_str = path.to_string_lossy();
                if !path_str.contains("/custom/")
                    && !path_str.contains("/test/")
                    && !path_str.contains("secret")
                {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    files
}

/// Diff two files.
fn diff_file(source: &Path, dest: &Path, relative: &Path) -> AppResult<FileConfigDiff> {
    let source_content = std::fs::read_to_string(source)?;
    let dest_content = std::fs::read_to_string(dest)?;

    let source_lines: Vec<&str> = source_content.lines().collect();
    let dest_lines: Vec<&str> = dest_content.lines().collect();

    let hunks = compute_unified_diff(&source_lines, &dest_lines);

    let summary = DiffSummary {
        additions: hunks
            .iter()
            .map(|h| {
                h.lines
                    .iter()
                    .filter(|l| matches!(l.kind, DiffLineKind::Addition))
                    .count()
            })
            .sum(),
        removals: hunks
            .iter()
            .map(|h| {
                h.lines
                    .iter()
                    .filter(|l| matches!(l.kind, DiffLineKind::Removal))
                    .count()
            })
            .sum(),
        modifications: 0,
    };

    Ok(FileConfigDiff {
        relative_path: relative.display().to_string(),
        hunks,
        summary,
    })
}

/// Create a diff for a file that was removed.
fn create_removal_diff(relative: &Path, content: &str) -> FileConfigDiff {
    let lines: Vec<&str> = content.lines().collect();

    let diff_lines: Vec<DiffLine> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| DiffLine {
            kind: DiffLineKind::Removal,
            content: line.to_string(),
            old_line: Some(i + 1),
            new_line: None,
        })
        .collect();

    let summary = DiffSummary {
        additions: 0,
        removals: lines.len(),
        modifications: 0,
    };

    FileConfigDiff {
        relative_path: relative.display().to_string(),
        hunks: vec![DiffHunk {
            old_start: 1,
            new_start: 0,
            lines: diff_lines,
        }],
        summary,
    }
}

/// Compute unified diff between two sets of lines.
fn compute_unified_diff(old_lines: &[&str], new_lines: &[&str]) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();

    // Simple LCS-based diff
    let mut old_idx = 0;
    let mut new_idx = 0;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_hunk_start = 0;
    let mut new_hunk_start = 0;

    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        if old_idx < old_lines.len()
            && new_idx < new_lines.len()
            && old_lines[old_idx] == new_lines[new_idx]
        {
            // Lines match
            if current_hunk.is_some() {
                hunks.push(current_hunk.take().unwrap());
            }
            old_idx += 1;
            new_idx += 1;
            old_hunk_start = old_idx;
            new_hunk_start = new_idx;
        } else {
            // Lines differ
            if current_hunk.is_none() {
                current_hunk = Some(DiffHunk {
                    old_start: old_hunk_start + 1,
                    new_start: new_hunk_start + 1,
                    lines: Vec::new(),
                });
            }

            if new_idx < new_lines.len()
                && (old_idx >= old_lines.len()
                    || (old_idx + 1 < old_lines.len()
                        && new_idx + 1 < new_lines.len()
                        && old_lines[old_idx + 1] == new_lines[new_idx + 1]))
            {
                // Addition
                current_hunk.as_mut().unwrap().lines.push(DiffLine {
                    kind: DiffLineKind::Addition,
                    content: new_lines[new_idx].to_string(),
                    old_line: None,
                    new_line: Some(new_idx + 1),
                });
                new_idx += 1;
            } else if old_idx < old_lines.len() {
                // Removal
                current_hunk.as_mut().unwrap().lines.push(DiffLine {
                    kind: DiffLineKind::Removal,
                    content: old_lines[old_idx].to_string(),
                    old_line: Some(old_idx + 1),
                    new_line: None,
                });
                old_idx += 1;
            } else if new_idx < new_lines.len() {
                // Addition at end
                current_hunk.as_mut().unwrap().lines.push(DiffLine {
                    kind: DiffLineKind::Addition,
                    content: new_lines[new_idx].to_string(),
                    old_line: None,
                    new_line: Some(new_idx + 1),
                });
                new_idx += 1;
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

/// Format diff output as unified diff.
pub fn format_unified_diff(diff: &ConfigDiff) -> String {
    use console::style;

    let mut output = String::new();

    output.push_str(&format!(
        "Comparing: {} -> {}\n\n",
        style(&diff.source_path).cyan(),
        style(&diff.dest_path).yellow()
    ));

    if diff.file_diffs.is_empty() {
        output.push_str("No configuration differences found.\n");
        return output;
    }

    let mut total_additions = 0;
    let mut total_removals = 0;

    for file_diff in &diff.file_diffs {
        output.push_str(&format!(
            "diff --git a/{} b/{}\n",
            file_diff.relative_path, file_diff.relative_path
        ));
        output.push_str(&format!(
            "--- {}/{}\n",
            style("a").dim(),
            style(&file_diff.relative_path).dim()
        ));
        output.push_str(&format!(
            "+++ {}/{}\n",
            style("b").dim(),
            style(&file_diff.relative_path).dim()
        ));

        for hunk in &file_diff.hunks {
            output.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start,
                hunk.lines
                    .iter()
                    .filter(|l| matches!(l.kind, DiffLineKind::Removal))
                    .max_by_key(|l| l.old_line.unwrap_or(0))
                    .map(|l| l.old_line.unwrap())
                    .unwrap_or(0),
                hunk.new_start,
                hunk.lines
                    .iter()
                    .filter(|l| matches!(l.kind, DiffLineKind::Addition))
                    .max_by_key(|l| l.new_line.unwrap_or(0))
                    .map(|l| l.new_line.unwrap())
                    .unwrap_or(0)
            ));

            for line in &hunk.lines {
                let styled = match line.kind {
                    DiffLineKind::Addition => {
                        total_additions += 1;
                        style(format!("+{}", line.content)).green()
                    }
                    DiffLineKind::Removal => {
                        total_removals += 1;
                        style(format!("-{}", line.content)).red()
                    }
                    DiffLineKind::Context => style(format!(" {}", line.content)).dim(),
                };
                output.push_str(&format!("{}\n", styled));
            }
        }

        output.push('\n');
    }

    output.push_str(&format!(
        "Summary: {} additions, {} removals across {} files\n",
        style(total_additions).green(),
        style(total_removals).red(),
        style(diff.file_diffs.len()).cyan()
    ));

    output
}
