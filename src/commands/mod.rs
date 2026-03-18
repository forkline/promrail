pub mod diff;
pub mod promote;
pub mod validate;

pub use diff::DiffArgs;
pub use promote::PromoteArgs;

/// Default filter pattern when none specified.
pub const DEFAULT_FILTER: &str = ".*";

/// Returns filter vec with default if empty.
pub fn default_filter(filter_vec: Vec<String>) -> Vec<String> {
    if filter_vec.is_empty() {
        vec![DEFAULT_FILTER.to_string()]
    } else {
        filter_vec
    }
}

/// Prints promotion summary with proper singular/plural handling.
pub fn print_promotion_summary(copied: usize, deleted: usize, should_delete: bool) {
    match copied {
        1 => println!("  1 file copied"),
        n => println!("  {} files copied", console::style(n).green()),
    }
    if should_delete && deleted > 0 {
        match deleted {
            1 => println!("  1 file deleted"),
            n => println!("  {} files deleted", console::style(n).red()),
        }
    }
}
