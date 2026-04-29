use swe_vminit_pkg::run_install_step;

#[test]
fn test_run_install_step_empty_packages_does_not_panic() {
    let pkg_dir = tempfile::tempdir().unwrap();
    run_install_step(&[], pkg_dir.path(), None, None, None, "https://cache.nixos.org");
}

#[test]
fn test_run_install_step_package_absent_from_initrd_no_manifest_skips_without_panic() {
    let pkg_dir = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    run_install_step(
        &["nonexistent".to_string()],
        pkg_dir.path(),
        None,
        None,
        Some(dest.path()),
        "https://cache.nixos.org",
    );
    assert!(
        dest.path().read_dir().unwrap().next().is_none(),
        "dest must remain empty when package is missing and no manifest is provided"
    );
}

#[test]
fn test_run_install_step_initrd_package_extracted_to_dest() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Builder;

    let pkg_dir = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();

    // Create a minimal tar.gz for "myapp"
    let archive_path = pkg_dir.path().join("myapp.tar.gz");
    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::fast());
        let mut builder = Builder::new(enc);
        let data = b"content";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "hello.txt", data.as_ref())
            .unwrap();
        builder.finish().unwrap();
    }

    run_install_step(
        &["myapp".to_string()],
        pkg_dir.path(),
        None,
        None,
        Some(dest.path()),
        "https://cache.nixos.org",
    );

    assert!(
        dest.path().join("hello.txt").exists(),
        "extracted file must appear in dest dir"
    );
}
