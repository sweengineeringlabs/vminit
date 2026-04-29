use std::collections::HashMap;
use std::path::Path;

use swe_justpkg_nix::NixFetcher;

use crate::api::error::PackageError;
use crate::api::installer::PackageInstaller;
use crate::api::network::ManifestLookup;

/// A parsed manifest: `{"packages": {"name": "/nix/store/<hash>-<name>-<version>", ...}}`.
#[derive(Debug)]
pub(crate) struct NetworkManifest {
    /// Maps package name → absolute Nix store path (`/nix/store/<32-char-hash>-<name>`).
    pub(crate) entries: HashMap<String, String>,
}

impl ManifestLookup for NetworkManifest {
    fn get_store_path(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }
}

impl NetworkManifest {
    /// Parse a JSON manifest of the form `{"packages":{"name":"/nix/store/...","..."}}`.
    pub(crate) fn parse(text: &str) -> Result<Self, PackageError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| PackageError::ManifestParse(e.to_string()))?;

        let packages_obj = value
            .get("packages")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                PackageError::ManifestParse("missing or non-object \"packages\" field".to_string())
            })?;

        let mut entries = HashMap::with_capacity(packages_obj.len());
        for (k, v) in packages_obj {
            let store_path = v.as_str().ok_or_else(|| {
                PackageError::ManifestParse(format!("package {k:?} has non-string store path"))
            })?;
            entries.insert(k.clone(), store_path.to_string());
        }

        Ok(Self { entries })
    }
}

#[cfg(test)]
mod tests_parse {
    use super::*;

    #[test]
    fn test_parse_returns_store_paths_by_name() {
        let json = r#"{"packages":{
            "curl":"/nix/store/abc123-curl-8.5",
            "git":"/nix/store/def456-git-2.44"
        }}"#;
        let m = NetworkManifest::parse(json).unwrap();
        assert_eq!(m.get_store_path("curl"), Some("/nix/store/abc123-curl-8.5"));
        assert_eq!(m.get_store_path("git"), Some("/nix/store/def456-git-2.44"));
        assert_eq!(m.get_store_path("absent"), None, "unknown package must return None");
    }

    #[test]
    fn test_parse_malformed_json_returns_manifest_parse_error() {
        let err = NetworkManifest::parse("{not valid json}").unwrap_err();
        assert!(
            matches!(err, PackageError::ManifestParse(_)),
            "malformed JSON must return ManifestParse error"
        );
    }

    #[test]
    fn test_parse_missing_packages_field_returns_manifest_parse_error() {
        let err = NetworkManifest::parse(r#"{"other":{}}"#).unwrap_err();
        assert!(
            matches!(err, PackageError::ManifestParse(_)),
            "missing 'packages' field must return ManifestParse error"
        );
    }

    #[test]
    fn test_parse_non_string_value_returns_manifest_parse_error() {
        let err = NetworkManifest::parse(r#"{"packages":{"curl":42}}"#).unwrap_err();
        assert!(
            matches!(err, PackageError::ManifestParse(_)),
            "non-string store path must return ManifestParse error"
        );
    }
}

/// Fetches a NAR and its full transitive closure from `cache.nixos.org` using
/// [`NixFetcher::build_store_path`].
///
/// The manifest maps package names to their absolute Nix store paths
/// (`/nix/store/<32-char-hash>-<name>-<version>`).  The store hash is extracted
/// from the path and used to look up the `.narinfo` on `cache.nixos.org`; no
/// hash derivation formula is needed.
pub(crate) struct NetworkInstaller<'a> {
    pub(crate) http: &'a dyn swe_justpkg_pkg::HttpClient,
    pub(crate) manifest: NetworkManifest,
}

impl<'a> PackageInstaller for NetworkInstaller<'a> {
    fn install(&self, name: &str, dest_dir: &Path) -> Result<(), PackageError> {
        let store_path = self
            .manifest
            .entries
            .get(name)
            .ok_or_else(|| PackageError::NotInManifest { name: name.to_string() })?
            .clone();

        // NixFetcher::build_store_path extracts each NAR (and its closure) to
        // <dest_dir>/nix/store/<hash>-<name>/. Packages are visible inside the
        // guest at /nix/store/<hash>-<name>/ because:
        //   - without rootfs: dest_dir = "/", store paths appear at /nix/store/
        //   - with rootfs: dest_dir = <rootfs>, chroot makes them at /nix/store/
        NixFetcher { http: self.http }
            .build_store_path(&store_path, dest_dir)
            .map_err(|e| PackageError::NetworkFailed {
                name: name.to_string(),
                reason: e.to_string(),
            })
    }
}
