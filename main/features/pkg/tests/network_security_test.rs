use swe_justpkg_pkg::{HttpClient, JustpkgError};
use swe_vminit_pkg::run_install_step;
use tempfile::TempDir;

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

#[test]
fn test_manifest_with_path_traversal_in_package_name_does_not_panic() {
    // Bug caught: a crafted manifest whose key contains "../" being used as
    // a path component during install, causing path traversal or panic.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    // Package name contains path traversal sequence.
    let manifest = r#"{"packages":{"../../../etc/passwd":"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}"#;

    // Must not panic — NotInManifest (name not in initrd) or NetworkFailed.
    run_install_step(
        &["../../../etc/passwd".to_string()],
        packages_dir.path(),
        Some(manifest),
        Some(&http),
        Some(dest_dir.path()),
    );

    // /etc/passwd must not have been modified (we can't assert it was unchanged,
    // but dest_dir must be empty — no file landed there).
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}

#[test]
fn test_manifest_with_10000_entries_parses_without_panic() {
    // Bug caught: manifest parser blowing the stack or OOMing on a large
    // but syntactically valid JSON object.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    let entries: Vec<String> = (0..10_000)
        .map(|i| format!(r#""pkg{i}":"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=""#))
        .collect();
    let manifest = format!("{{\"packages\":{{{}}}}}", entries.join(","));

    // Must not panic, even with 10 000 entries.
    run_install_step(
        &["pkg0".to_string()],
        packages_dir.path(),
        Some(&manifest),
        Some(&http),
        Some(dest_dir.path()),
    );
}

#[test]
fn test_empty_manifest_json_does_not_panic_on_install() {
    // Bug caught: `{"packages":{}}` causing a panic (e.g. unwrap on empty map).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let http = ErrorHttpClient;

    run_install_step(
        &["anypkg".to_string()],
        packages_dir.path(),
        Some(r#"{"packages":{}}"#),
        Some(&http),
        Some(dest_dir.path()),
    );
    // Must not panic — test passes if we reach this line.
}
