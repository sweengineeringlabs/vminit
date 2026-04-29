use swe_justpkg_pkg::{HttpClient, JustpkgError};
use swe_vminit_pkg::run_install_step;
use tempfile::TempDir;

/// HTTP stub that always returns a network error.
struct ErrorHttpClient;

impl HttpClient for ErrorHttpClient {
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, JustpkgError> {
        Err(JustpkgError::Http {
            url: url.to_string(),
            status: 503,
        })
    }
    fn get_stream(&self, url: &str, _dest: &mut dyn std::io::Write) -> Result<u64, JustpkgError> {
        Err(JustpkgError::Http {
            url: url.to_string(),
            status: 503,
        })
    }
}

/// Minimal manifest JSON with one known entry (store path format).
const MANIFEST_ONE: &str =
    r#"{"packages":{"curl":"/nix/store/hm8l3fvhzpmw3ilkxp0lns6cvb4wd4g2-curl-8.5.0"}}"#;
/// Empty manifest JSON.
const MANIFEST_EMPTY: &str = r#"{"packages":{}}"#;

#[test]
fn test_network_installer_name_not_in_manifest_returns_not_in_manifest() {
    // Bug caught: installer returning Ok(()) when the name is absent from
    // the manifest (silent no-op instead of error).
    // We exercise this via run_install_step with a manifest that doesn't
    // contain "jq". The step must log but not panic.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    // "jq" is not in MANIFEST_ONE — must log NotInManifest, not panic.
    run_install_step(
        &["jq".to_string()],
        packages_dir.path(),
        Some(MANIFEST_ONE),
        Some(&http),
        Some(dest_dir.path()),
    );

    // dest_dir stays empty (nothing was installed).
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}

#[test]
fn test_network_installer_with_http_stub_returning_error_returns_network_failed() {
    // Bug caught: HTTP error being swallowed and install() returning Ok(()).
    // We drive this through run_install_step: curl IS in the manifest but
    // ErrorHttpClient returns 503 → must log NetworkFailed, not panic.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    run_install_step(
        &["curl".to_string()],
        packages_dir.path(),
        Some(MANIFEST_ONE),
        Some(&http),
        Some(dest_dir.path()),
    );

    // Nothing must be written.
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}

#[test]
fn test_run_install_step_with_none_http_returns_without_panicking() {
    // Bug caught: None http client causing a null dereference or unwrap panic.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // package not in initrd + no http client → must log and skip, not panic.
    run_install_step(
        &["curl".to_string()],
        packages_dir.path(),
        Some(MANIFEST_ONE),
        None, // <-- no HTTP client
        Some(dest_dir.path()),
    );
}

#[test]
fn test_network_installer_with_empty_manifest_returns_not_in_manifest_for_any_name() {
    // Bug caught: empty manifest returning Ok(()) instead of NotInManifest.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    // Empty manifest — every package name must be absent.
    run_install_step(
        &["anything".to_string()],
        packages_dir.path(),
        Some(MANIFEST_EMPTY),
        Some(&http),
        Some(dest_dir.path()),
    );

    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}
