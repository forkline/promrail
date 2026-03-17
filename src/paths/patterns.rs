use globset::{Glob, GlobSetBuilder};

pub fn build_glob_set(patterns: &[String]) -> Result<globset::GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    builder.build()
}

pub fn matches_any(path: &std::path::Path, globset: &globset::GlobSet) -> bool {
    let path_str = path.to_string_lossy();
    globset.is_match(path_str.as_ref())
}
