use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffAction {
    Added,
    Modified,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: PathBuf,
    pub action: DiffAction,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}

impl FileDiff {
    pub fn added(path: PathBuf, content: String) -> Self {
        Self {
            path,
            action: DiffAction::Added,
            old_content: None,
            new_content: Some(content),
        }
    }

    pub fn removed(path: PathBuf, content: String) -> Self {
        Self {
            path,
            action: DiffAction::Removed,
            old_content: Some(content),
            new_content: None,
        }
    }

    pub fn modified(path: PathBuf, old: String, new: String) -> Self {
        Self {
            path,
            action: DiffAction::Modified,
            old_content: Some(old),
            new_content: Some(new),
        }
    }

    pub fn unchanged(path: PathBuf, content: String) -> Self {
        Self {
            path,
            action: DiffAction::Unchanged,
            old_content: Some(content.clone()),
            new_content: Some(content),
        }
    }
}

pub fn compute_diff(old: Option<&str>, new: Option<&str>) -> Vec<String> {
    let mut output = Vec::new();

    match (old, new) {
        (None, Some(new_content)) => {
            for line in new_content.lines() {
                output.push(format!("+{}", line));
            }
        }
        (Some(old_content), None) => {
            for line in old_content.lines() {
                output.push(format!("-{}", line));
            }
        }
        (Some(old_content), Some(new_content)) => {
            let old_lines: Vec<&str> = old_content.lines().collect();
            let new_lines: Vec<&str> = new_content.lines().collect();

            output.push("--- old".to_string());
            output.push("+++ new".to_string());

            let diff = simple_diff(&old_lines, &new_lines);
            output.extend(diff);
        }
        (None, None) => {}
    }

    output
}

fn simple_diff(old: &[&str], new: &[&str]) -> Vec<String> {
    let mut result = Vec::new();

    let mut old_idx = 0;
    let mut new_idx = 0;

    while old_idx < old.len() || new_idx < new.len() {
        if old_idx < old.len() && new_idx < new.len() && old[old_idx] == new[new_idx] {
            result.push(format!(" {}", old[old_idx]));
            old_idx += 1;
            new_idx += 1;
        } else if new_idx < new.len() && (old_idx >= old.len() || !old.contains(&new[new_idx])) {
            result.push(format!("+{}", new[new_idx]));
            new_idx += 1;
        } else if old_idx < old.len() && (new_idx >= new.len() || !new.contains(&old[old_idx])) {
            result.push(format!("-{}", old[old_idx]));
            old_idx += 1;
        } else if old_idx < old.len() && new_idx < new.len() {
            result.push(format!("-{}", old[old_idx]));
            result.push(format!("+{}", new[new_idx]));
            old_idx += 1;
            new_idx += 1;
        } else if old_idx < old.len() {
            result.push(format!("-{}", old[old_idx]));
            old_idx += 1;
        } else if new_idx < new.len() {
            result.push(format!("+{}", new[new_idx]));
            new_idx += 1;
        }
    }

    result
}

pub fn format_colored_diff(lines: &[String]) -> String {
    use console::style;

    let mut output = String::new();

    for line in lines {
        let styled = if line.starts_with('+') && !line.starts_with("++") {
            style(line).green().to_string()
        } else if line.starts_with('-') && !line.starts_with("--") {
            style(line).red().to_string()
        } else if line.starts_with("@@") {
            style(line).cyan().to_string()
        } else {
            line.to_string()
        };
        output.push_str(&styled);
        output.push('\n');
    }

    output
}
