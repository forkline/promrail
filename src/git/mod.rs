pub mod diff;
pub mod repo;

pub use diff::{compute_diff, format_colored_diff, FileDiff};
pub use repo::GitRepo;
