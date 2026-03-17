pub mod diff;
pub mod repo;

pub use diff::{FileDiff, compute_diff, format_colored_diff};
pub use repo::GitRepo;
