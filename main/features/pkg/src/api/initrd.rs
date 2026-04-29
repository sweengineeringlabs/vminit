use std::path::PathBuf;

/// Describes how a package source resolves a named package to an on-disk archive path.
pub trait PackagesSource {
    fn archive_path(&self, name: &str) -> PathBuf;
}
