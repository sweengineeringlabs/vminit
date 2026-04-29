use std::collections::HashMap;
use std::path::Path;

use swe_justpkg_nix::{FlakeLock, NixFetcher};

use crate::api::error::PackageError;
use crate::api::installer::PackageInstaller;

/// A parsed manifest: `{"packages": {"name": "sha256-<sri>", ...}}`.
#[derive(Debug)]
pub(crate) struct NetworkManifest {
    /// Maps package name → SRI hash (`sha256-<base64>`).
    pub(crate) entries: HashMap<String, String>,
}

impl NetworkManifest {
    /// Parse a JSON manifest of the form `{"packages":{"name":"sha256-xxx",...}}`.
    pub(crate) fn parse(text: &str) -> Result<Self, PackageError> {
        // Parse via serde_json::Value to avoid adding serde as a direct dep —
        // serde_json is already a dependency and serde_json::Value works without
        // the serde derive attribute.
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| PackageError::ManifestParse(e.to_string()))?;

        let packages_obj = value
            .get("packages")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                PackageError::ManifestParse(
                    "missing or non-object \"packages\" field".to_string(),
                )
            })?;

        let mut entries = HashMap::with_capacity(packages_obj.len());
        for (k, v) in packages_obj {
            let sri = v.as_str().ok_or_else(|| {
                PackageError::ManifestParse(format!(
                    "package {k:?} has non-string SRI value"
                ))
            })?;
            entries.insert(k.clone(), sri.to_string());
        }

        Ok(Self { entries })
    }
}

/// Fetches a NAR from `cache.nixos.org` using [`NixFetcher`].
///
/// A manifest JSON of the form `{"packages":{"name":"sha256-xxx",...}}` maps
/// package names to their SRI `narHash`.  For each install request the fetcher
/// constructs a minimal synthetic [`FlakeLock`] with one locked node and
/// delegates to `NixFetcher::build`.
pub(crate) struct NetworkInstaller<'a> {
    pub(crate) http: &'a dyn swe_justpkg_pkg::HttpClient,
    pub(crate) manifest: NetworkManifest,
}

impl<'a> PackageInstaller for NetworkInstaller<'a> {
    fn install(&self, name: &str, dest_dir: &Path) -> Result<(), PackageError> {
        let sri = self
            .manifest
            .entries
            .get(name)
            .ok_or_else(|| PackageError::NotInManifest {
                name: name.to_string(),
            })?
            .clone();

        // Build a minimal synthetic flake.lock JSON with one locked node
        // and parse it via FlakeLock::from_json — the structs' fields are not
        // re-exported from swe_justpkg_nix, so direct construction is not possible.
        let lock_json = build_single_node_lock_json(name, &sri);
        let lock = FlakeLock::from_json(&lock_json).map_err(|e| PackageError::NetworkFailed {
            name: name.to_string(),
            reason: format!("synthetic FlakeLock parse failed: {e}"),
        })?;

        NixFetcher { http: self.http }
            .build(&lock, dest_dir)
            .map_err(|e| PackageError::NetworkFailed {
                name: name.to_string(),
                reason: e.to_string(),
            })
    }
}

/// Produce a minimal flake.lock v7 JSON with a single tarball node.
///
/// The resulting JSON looks like:
/// ```json
/// {
///   "nodes": {
///     "<name>": {
///       "locked": {
///         "narHash": "<sri>",
///         "type": "tarball",
///         "url": "https://cache.nixos.org/<name>.tar.gz"
///       },
///       "inputs": {}
///     },
///     "root": { "inputs": {} }
///   },
///   "root": "root",
///   "version": 7
/// }
/// ```
fn build_single_node_lock_json(name: &str, sri: &str) -> String {
    // Use serde_json::json! macro to avoid manual escaping.
    serde_json::json!({
        "nodes": {
            name: {
                "locked": {
                    "narHash": sri,
                    "type": "tarball",
                    "url": format!("https://cache.nixos.org/{}.tar.gz", name)
                },
                "inputs": {}
            },
            "root": {
                "inputs": {}
            }
        },
        "root": "root",
        "version": 7
    })
    .to_string()
}
