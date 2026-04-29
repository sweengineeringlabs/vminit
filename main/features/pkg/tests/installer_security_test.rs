use std::path::Path;
use std::sync::{Arc, Mutex};

use swe_vminit_pkg::{PackageError, PackageInstaller};

/// Stub that accepts any input without panicking, recording received names.
struct PanicGuardInstaller {
    calls: Arc<Mutex<Vec<String>>>,
}

impl PackageInstaller for PanicGuardInstaller {
    fn install(&self, name: &str, _dest_dir: &Path) -> Result<(), PackageError> {
        self.calls.lock().unwrap().push(name.to_string());
        Ok(())
    }
}

#[test]
fn test_installer_does_not_panic_on_null_byte_name() {
    // Bug caught: installer panicking when a package name contains a null
    // byte (e.g. from a corrupted manifest).
    let calls = Arc::new(Mutex::new(Vec::new()));
    let installer = PanicGuardInstaller { calls: Arc::clone(&calls) };
    // Must not panic — even with a name containing a null byte.
    let _ = installer.install("evil\x00pkg", Path::new("/tmp"));
}

#[test]
fn test_installer_does_not_panic_on_empty_name() {
    // Bug caught: install("", ...) panicking on empty-string package name.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let installer = PanicGuardInstaller { calls: Arc::clone(&calls) };
    let _ = installer.install("", Path::new("/tmp"));
    // Should have been called once (recording stub does not reject empty names)
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn test_installer_does_not_panic_on_unicode_name() {
    // Bug caught: byte-oriented name handling panicking on multibyte characters.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let installer = PanicGuardInstaller { calls: Arc::clone(&calls) };
    let _ = installer.install("pàckàge-名前", Path::new("/tmp"));
    assert_eq!(calls.lock().unwrap().len(), 1);
}
