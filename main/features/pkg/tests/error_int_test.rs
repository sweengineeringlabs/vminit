use swe_vminit_pkg::PackageError;

#[test]
fn test_archive_not_found_display_includes_path() {
    // Bug caught: display omitting the path field, making errors unactionable.
    let err = PackageError::ArchiveNotFound {
        path: "/packages/curl.tar.gz".to_string(),
    };
    let text = err.to_string();
    assert!(
        text.contains("/packages/curl.tar.gz"),
        "display must include the path, got: {text:?}"
    );
}

#[test]
fn test_network_failed_display_includes_name_and_reason() {
    // Bug caught: display missing either the name or reason, hiding which
    // package failed and why.
    let err = PackageError::NetworkFailed {
        name: "busybox".to_string(),
        reason: "connection refused".to_string(),
    };
    let text = err.to_string();
    assert!(
        text.contains("busybox"),
        "display must include the package name, got: {text:?}"
    );
    assert!(
        text.contains("connection refused"),
        "display must include the reason, got: {text:?}"
    );
}

#[test]
fn test_io_wraps_io_error_correctly() {
    // Bug caught: From<io::Error> impl broken — e.g. wrong variant used,
    // or message not forwarded.
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let pkg_err: PackageError = io_err.into();
    let text = pkg_err.to_string();
    assert!(
        text.contains("permission denied"),
        "Io variant must forward the io::Error message, got: {text:?}"
    );
}

#[test]
fn test_extraction_failed_display_includes_name_and_reason() {
    // Bug caught: extraction errors not reporting the package name, making
    // multi-package install logs ambiguous.
    let err = PackageError::ExtractionFailed {
        name: "python3".to_string(),
        reason: "unexpected EOF".to_string(),
    };
    let text = err.to_string();
    assert!(
        text.contains("python3"),
        "display must include name, got: {text:?}"
    );
    assert!(
        text.contains("unexpected EOF"),
        "display must include reason, got: {text:?}"
    );
}

#[test]
fn test_manifest_parse_display_includes_message() {
    let err = PackageError::ManifestParse("invalid JSON at line 3".to_string());
    let text = err.to_string();
    assert!(
        text.contains("invalid JSON at line 3"),
        "display must include the parse message, got: {text:?}"
    );
}
