use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;

use crate::api::error::PackageError;
use crate::api::installer::PackageInstaller;

/// Extracts `/packages/<name>.tar.gz` into `dest_dir` using pure-Rust
/// `tar` + `flate2`.  No subprocess is spawned; safe for PID-1 environments
/// that may not have a working `execve`.
///
/// Every archive entry path is validated with `swe_justpkg_pkg::safe_path_join`
/// before any write occurs.  Entries that escape the destination are rejected
/// with `PackageError::ExtractionFailed`.
pub(crate) struct InitrdInstaller {
    /// Directory that holds `<name>.tar.gz` archives.  Defaults to `/packages`.
    pub(crate) packages_dir: PathBuf,
}

impl InitrdInstaller {
    pub(crate) fn new(packages_dir: PathBuf) -> Self {
        Self { packages_dir }
    }
}

impl PackageInstaller for InitrdInstaller {
    fn install(&self, name: &str, dest_dir: &Path) -> Result<(), PackageError> {
        let archive_path = self.packages_dir.join(format!("{name}.tar.gz"));

        let file = std::fs::File::open(&archive_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PackageError::ArchiveNotFound {
                    path: archive_path.to_string_lossy().into_owned(),
                }
            } else {
                PackageError::ExtractionFailed {
                    name: name.to_string(),
                    reason: e.to_string(),
                }
            }
        })?;

        extract_tar_gz(name, file, dest_dir)
    }
}

/// Extract a `.tar.gz` from any `Read` source into `dest_dir`.
/// Validates every entry path via `safe_path_join` before writing.
pub(crate) fn extract_tar_gz<R: std::io::Read>(
    pkg_name: &str,
    reader: R,
    dest_dir: &Path,
) -> Result<(), PackageError> {
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);

    for entry_result in archive.entries().map_err(|e| PackageError::ExtractionFailed {
        name: pkg_name.to_string(),
        reason: e.to_string(),
    })? {
        let mut entry = entry_result.map_err(|e| PackageError::ExtractionFailed {
            name: pkg_name.to_string(),
            reason: e.to_string(),
        })?;

        let raw_path = entry
            .path()
            .map_err(|e| PackageError::ExtractionFailed {
                name: pkg_name.to_string(),
                reason: e.to_string(),
            })?
            .to_string_lossy()
            .into_owned();

        // Validate: reject path traversal, absolute paths, null bytes.
        let dest_path =
            swe_justpkg_pkg::safe_path_join(dest_dir, &raw_path).map_err(|e| {
                PackageError::ExtractionFailed {
                    name: pkg_name.to_string(),
                    reason: e.to_string(),
                }
            })?;

        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            std::fs::create_dir_all(&dest_path).map_err(|e| PackageError::ExtractionFailed {
                name: pkg_name.to_string(),
                reason: e.to_string(),
            })?;
        } else if entry_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| PackageError::ExtractionFailed {
                    name: pkg_name.to_string(),
                    reason: e.to_string(),
                })?;
            }
            let mut out =
                std::fs::File::create(&dest_path).map_err(|e| PackageError::ExtractionFailed {
                    name: pkg_name.to_string(),
                    reason: e.to_string(),
                })?;
            std::io::copy(&mut entry, &mut out).map_err(|e| PackageError::ExtractionFailed {
                name: pkg_name.to_string(),
                reason: e.to_string(),
            })?;
        }
        // Symlinks and other special types are silently skipped — not needed
        // in the initrd package set and their handling would require unsafe ops.
    }

    Ok(())
}
