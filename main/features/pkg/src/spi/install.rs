use std::path::Path;

use crate::api::error::PackageError;
use crate::api::installer::PackageInstaller;
use crate::core::initrd::InitrdInstaller;
use crate::core::network::{NetworkInstaller, NetworkManifest};

/// PID-1 safe package installation step.
///
/// Attempts each package with [`InitrdInstaller`] first (no network dependency).
/// If the initrd archive is absent **and** a manifest + HTTP client are provided,
/// falls back to [`NetworkInstaller`].
///
/// The function is intentionally infallible (`-> ()`) so that PID 1 never
/// panics due to a package installation failure.  All errors are logged to
/// stderr via `eprintln!`.
pub fn run_install_step(
    packages: &[String],
    packages_dir: &Path,
    manifest_text: Option<&str>,
    http: Option<&dyn swe_justpkg_pkg::HttpClient>,
    dest_dir: Option<&Path>,
) {
    if packages.is_empty() {
        return;
    }

    let effective_dest = match dest_dir {
        Some(d) => d.to_path_buf(),
        None => std::path::PathBuf::from("/"),
    };

    let initrd = InitrdInstaller::new(packages_dir.to_path_buf());

    // Parse manifest once (if present) so we fail fast on bad JSON, not per-package.
    let manifest_opt: Option<NetworkManifest> = match manifest_text {
        Some(text) => match NetworkManifest::parse(text) {
            Ok(m) => Some(m),
            Err(PackageError::ManifestParse(ref msg)) => {
                eprintln!("vminit-pkg: manifest parse error — {msg}; network fallback disabled");
                None
            }
            Err(e) => {
                eprintln!("vminit-pkg: unexpected manifest error — {e}; network fallback disabled");
                None
            }
        },
        None => None,
    };

    for pkg in packages {
        match initrd.install(pkg, &effective_dest) {
            Ok(()) => {
                eprintln!("vminit-pkg: installed {pkg:?} from initrd");
                continue;
            }
            Err(PackageError::ArchiveNotFound { .. }) => {
                // Not in initrd — try network fallback below.
            }
            Err(e) => {
                eprintln!("vminit-pkg: initrd install failed for {pkg:?}: {e}");
                continue;
            }
        }

        // Network fallback — requires both manifest and http client.
        match (manifest_opt.as_ref(), http) {
            (Some(manifest), Some(http_client)) => {
                let network = NetworkInstaller {
                    http: http_client,
                    manifest: NetworkManifest {
                        entries: manifest.entries.clone(),
                    },
                };
                match network.install(pkg, &effective_dest) {
                    Ok(()) => {
                        eprintln!("vminit-pkg: installed {pkg:?} via network");
                    }
                    Err(e) => {
                        eprintln!("vminit-pkg: network install failed for {pkg:?}: {e}");
                    }
                }
            }
            (None, _) => {
                eprintln!(
                    "vminit-pkg: {pkg:?} not in initrd and no manifest available — skipping"
                );
            }
            (_, None) => {
                eprintln!(
                    "vminit-pkg: {pkg:?} not in initrd and no HTTP client available — skipping"
                );
            }
        }
    }
}
