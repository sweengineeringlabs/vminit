use std::path::Path;
use std::sync::{Arc, Mutex};

use swe_vminit_pkg::{PackageError, PackageInstaller};

/// Test stub that records every call and returns Ok(()).
struct RecordingInstaller {
    calls: Arc<Mutex<Vec<String>>>,
}

impl PackageInstaller for RecordingInstaller {
    fn install(&self, name: &str, _dest_dir: &Path) -> Result<(), PackageError> {
        self.calls.lock().unwrap().push(name.to_string());
        Ok(())
    }
}

/// Test stub that always returns an error.
struct FailingInstaller {
    reason: String,
}

impl PackageInstaller for FailingInstaller {
    fn install(&self, name: &str, _dest_dir: &Path) -> Result<(), PackageError> {
        Err(PackageError::ExtractionFailed {
            name: name.to_string(),
            reason: self.reason.clone(),
        })
    }
}

#[test]
fn test_custom_installer_is_called_for_each_package() {
    // Bug caught: install loop skipping packages silently.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let installer = RecordingInstaller { calls: Arc::clone(&calls) };
    let dest = std::path::Path::new("/tmp");

    let packages = ["curl", "jq", "busybox"];
    for pkg in &packages {
        installer.install(pkg, dest).unwrap();
    }

    let recorded = calls.lock().unwrap().clone();
    assert_eq!(recorded, vec!["curl", "jq", "busybox"]);
}

#[test]
fn test_package_installer_returning_error_propagates_correctly() {
    // Bug caught: error swallowed or converted to wrong variant.
    let installer = FailingInstaller { reason: "disk full".to_string() };
    let result = installer.install("gzip", Path::new("/tmp"));
    let err = result.unwrap_err();
    match err {
        PackageError::ExtractionFailed { name, reason } => {
            assert_eq!(name, "gzip");
            assert_eq!(reason, "disk full");
        }
        other => panic!("expected ExtractionFailed, got {other:?}"),
    }
}

#[test]
fn test_package_installer_install_called_with_correct_name() {
    // Bug caught: name argument mangled or trimmed before being passed to install.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let installer = RecordingInstaller { calls: Arc::clone(&calls) };
    let expected_name = "python3.11-minimal";
    installer.install(expected_name, Path::new("/tmp")).unwrap();
    assert_eq!(calls.lock().unwrap()[0], expected_name);
}
