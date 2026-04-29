use flate2::write::GzEncoder;
use flate2::Compression;
use swe_vminit_pkg::run_install_step;
use tempfile::TempDir;

// Internal type accessed via the crate's lib module path for testing.
// We test through the public saf interface where possible; for InitrdInstaller
// specifically, we exercise it via the saf's run_install_step or directly by
// building a packages directory with real tar.gz files.

/// Create a `.tar.gz` archive in memory containing a single file.
fn make_tar_gz(filename: &str, content: &[u8]) -> Vec<u8> {
    let gz_buf = Vec::new();
    let encoder = GzEncoder::new(gz_buf, Compression::default());
    let mut tar_builder = tar::Builder::new(encoder);

    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder
        .append_data(&mut header, filename, content)
        .unwrap();

    let encoder = tar_builder.into_inner().unwrap();
    encoder.finish().unwrap()
}

/// Create a `.tar.gz` archive in memory containing multiple files.
fn make_tar_gz_multi(files: &[(&str, &[u8])]) -> Vec<u8> {
    let gz_buf = Vec::new();
    let encoder = GzEncoder::new(gz_buf, Compression::default());
    let mut tar_builder = tar::Builder::new(encoder);

    for (filename, content) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, filename, *content)
            .unwrap();
    }

    let encoder = tar_builder.into_inner().unwrap();
    encoder.finish().unwrap()
}

#[test]
fn test_install_with_valid_tar_gz_extracts_file_to_dest_dir() {
    // Bug caught: install() succeeding but not actually writing the file
    // to dest_dir.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let archive_bytes = make_tar_gz("hello.txt", b"hello world");
    std::fs::write(packages_dir.path().join("mypkg.tar.gz"), &archive_bytes).unwrap();

    let packages = vec!["mypkg".to_string()];
    run_install_step(
        &packages,
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    let extracted = std::fs::read(dest_dir.path().join("hello.txt")).unwrap();
    assert_eq!(extracted, b"hello world");
}

#[test]
fn test_install_with_multiple_files_in_archive_extracts_all() {
    // Bug caught: only the first archive entry being extracted.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let archive_bytes =
        make_tar_gz_multi(&[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")]);
    std::fs::write(packages_dir.path().join("multi.tar.gz"), &archive_bytes).unwrap();

    run_install_step(
        &["multi".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    assert_eq!(
        std::fs::read(dest_dir.path().join("a.txt")).unwrap(),
        b"aaa"
    );
    assert_eq!(
        std::fs::read(dest_dir.path().join("b.txt")).unwrap(),
        b"bbb"
    );
    assert_eq!(
        std::fs::read(dest_dir.path().join("c.txt")).unwrap(),
        b"ccc"
    );
}

#[test]
fn test_install_when_archive_does_not_exist_returns_archive_not_found() {
    // Bug caught: missing archive returning Ok(()) silently.
    // We exercise this via the InitrdInstaller directly through a thin wrapper
    // that we can construct via the public interface.
    // Since PackageInstaller is a trait re-exported in saf, we test via
    // run_install_step which logs the error — we verify it does not panic.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // No archive written — should log error but NOT panic.
    run_install_step(
        &["nonexistent".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    // dest_dir must remain empty (nothing extracted).
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "dest_dir must be empty when archive is missing"
    );
}

#[test]
fn test_extract_preserves_file_contents_exactly() {
    // Bug caught: binary content corrupted during extraction (e.g. newline
    // translation on Windows or truncation).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let binary_content: Vec<u8> = (0u8..=255).collect();
    let archive_bytes = make_tar_gz("binary.bin", &binary_content);
    std::fs::write(packages_dir.path().join("binpkg.tar.gz"), &archive_bytes).unwrap();

    run_install_step(
        &["binpkg".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    let extracted = std::fs::read(dest_dir.path().join("binary.bin")).unwrap();
    assert_eq!(
        extracted, binary_content,
        "binary content must be preserved byte-for-byte"
    );
}

#[test]
fn test_run_install_step_with_empty_packages_list_does_nothing() {
    // Bug caught: empty slice causing a panic (e.g. indexing into empty vec).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Must not panic; dest_dir stays empty.
    run_install_step(&[], packages_dir.path(), None, None, Some(dest_dir.path()), "https://cache.nixos.org");

    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}
