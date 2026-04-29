/// Errors produced by vminit package installation operations.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("package archive not found: {path:?}")]
    ArchiveNotFound { path: String },

    #[error("extraction failed for {name:?}: {reason}")]
    ExtractionFailed { name: String, reason: String },

    #[error("manifest parse error: {0}")]
    ManifestParse(String),

    #[error("package not found in manifest: {name:?}")]
    NotInManifest { name: String },

    #[error("network fetch failed for {name:?}: {reason}")]
    NetworkFailed { name: String, reason: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
