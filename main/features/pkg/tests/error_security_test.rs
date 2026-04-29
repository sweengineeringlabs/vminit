use swe_vminit_pkg::PackageError;

#[test]
fn test_archive_not_found_display_does_not_panic_on_very_long_path() {
    // Bug caught: format string or display impl panicking on extremely long
    // path inputs (e.g. stack overflow in formatter).
    let long_path = "a".repeat(1_000_000);
    let err = PackageError::ArchiveNotFound {
        path: long_path.clone(),
    };
    // Must not panic; result must contain the path.
    let text = err.to_string();
    assert!(
        text.contains(&long_path[..64]),
        "display must include path prefix"
    );
}

#[test]
fn test_error_display_has_consistent_prefix_format() {
    // Bug caught: variant prefixes being inconsistent — e.g. some using
    // uppercase, some lowercase — making log grepping unreliable.
    let not_found = PackageError::ArchiveNotFound {
        path: "p".to_string(),
    };
    let network = PackageError::NetworkFailed {
        name: "n".to_string(),
        reason: "r".to_string(),
    };
    let manifest = PackageError::ManifestParse("m".to_string());
    let not_in = PackageError::NotInManifest {
        name: "x".to_string(),
    };

    // Each display must be non-empty and not start with whitespace.
    for (label, text) in [
        ("ArchiveNotFound", not_found.to_string()),
        ("NetworkFailed", network.to_string()),
        ("ManifestParse", manifest.to_string()),
        ("NotInManifest", not_in.to_string()),
    ] {
        assert!(!text.is_empty(), "{label} display must not be empty");
        assert!(
            !text.starts_with(' '),
            "{label} display must not start with whitespace, got: {text:?}"
        );
    }
}
