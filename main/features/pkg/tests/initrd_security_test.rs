use flate2::write::GzEncoder;
use flate2::Compression;
use swe_vminit_pkg::run_install_step;
use tempfile::TempDir;

/// Build a raw tar entry (512-byte POSIX header + padded data) with an
/// arbitrary path string, bypassing the `tar` crate's safety checks.
/// Used only to construct adversarial archives for security tests.
fn raw_tar_entry(path_bytes: &[u8], content: &[u8]) -> Vec<u8> {
    // POSIX ustar header — 512 bytes total.
    let mut header = [0u8; 512];

    // name field: bytes 0..100
    let name_len = path_bytes.len().min(100);
    header[..name_len].copy_from_slice(&path_bytes[..name_len]);

    // mode field: bytes 100..108 — "0000644\0"
    header[100..107].copy_from_slice(b"0000644");
    header[107] = 0;

    // uid / gid (108..116, 116..124) — leave as zero

    // size field: bytes 124..136 — octal, space-terminated
    let size_octal = format!("{:011o} ", content.len());
    header[124..136].copy_from_slice(size_octal.as_bytes());

    // mtime: bytes 136..148 — "00000000000\0"
    header[136..147].copy_from_slice(b"00000000000");
    header[147] = b'\0';

    // typeflag: byte 156 — '0' = regular file
    header[156] = b'0';

    // ustar magic: bytes 257..265 — "ustar  \0"
    header[257..262].copy_from_slice(b"ustar");
    header[262] = b' ';
    header[263] = b' ';
    header[264] = 0;

    // Checksum: bytes 148..156 — must be set last.
    // Initialise checksum field to spaces for the calculation.
    for b in &mut header[148..156] {
        *b = b' ';
    }
    let checksum: u32 = header.iter().map(|&b| b as u32).sum();
    let cksum_str = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(cksum_str.as_bytes());

    // Data: padded to 512-byte boundary.
    let data_padded_len = content.len().div_ceil(512) * 512;
    let mut entry = Vec::with_capacity(512 + data_padded_len + 1024);
    entry.extend_from_slice(&header);
    entry.extend_from_slice(content);
    entry.extend(std::iter::repeat_n(0u8, data_padded_len - content.len()));

    // End-of-archive: two 512-byte zero blocks.
    entry.extend(std::iter::repeat_n(0u8, 1024));
    entry
}

/// Wrap raw tar bytes in a gzip stream.
fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    std::io::copy(&mut std::io::Cursor::new(data), &mut enc).unwrap();
    enc.finish().unwrap()
}

#[test]
fn test_archive_entry_with_dotdot_path_is_rejected() {
    // Bug caught: path traversal allowing an attacker-controlled archive to
    // write files outside dest_dir (e.g. overwriting /etc/passwd on the host).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let raw = raw_tar_entry(b"../escape.txt", b"evil");
    let archive_bytes = gzip_bytes(&raw);
    std::fs::write(packages_dir.path().join("evil.tar.gz"), archive_bytes).unwrap();

    // Must not panic; dest_dir must remain empty.
    run_install_step(
        &["evil".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    // The escaped file must not exist anywhere above dest_dir.
    let escaped = dest_dir.path().parent().unwrap().join("escape.txt");
    assert!(
        !escaped.exists(),
        "path traversal must not write outside dest_dir"
    );

    // dest_dir itself must be empty — the entry was rejected.
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "dest_dir must be empty after rejected traversal entry"
    );
}

#[test]
fn test_archive_entry_with_absolute_path_is_rejected() {
    // Bug caught: absolute path in archive entry overwriting system files.
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let raw = raw_tar_entry(b"/etc/passwd", b"evil");
    let archive_bytes = gzip_bytes(&raw);
    std::fs::write(packages_dir.path().join("abspkg.tar.gz"), archive_bytes).unwrap();

    run_install_step(
        &["abspkg".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );

    // dest_dir must be empty — the absolute-path entry must be rejected.
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "absolute path entry must not be extracted to dest_dir"
    );
}

#[test]
fn test_archive_entry_with_null_byte_in_name_does_not_panic() {
    // Bug caught: null byte in entry name causing string formatting panic or
    // passing a poisoned path to the OS (CVE-class: null byte injection).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Embed the null directly in the raw header bytes.
    let raw = raw_tar_entry(b"evil\x00file.txt", b"evil");
    let archive_bytes = gzip_bytes(&raw);
    std::fs::write(packages_dir.path().join("nullbyte.tar.gz"), archive_bytes).unwrap();

    // Must not panic — safe_path_join rejects null bytes.
    run_install_step(
        &["nullbyte".to_string()],
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );
}

#[test]
fn test_very_large_number_of_packages_does_not_panic() {
    // Bug caught: stack overflow or OOM when calling install for 100 packages
    // that are all missing from the initrd (100 ArchiveNotFound errors handled
    // in a loop without recursion).
    let packages_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let packages: Vec<String> = (0..100).map(|i| format!("pkg{i}")).collect();

    // Must not panic — all 100 will log "not in initrd" and continue.
    run_install_step(
        &packages,
        packages_dir.path(),
        None,
        None,
        Some(dest_dir.path()),
        "https://cache.nixos.org",
    );
}
