//! Reads the marker of the `vmfs` binary cargo built for this package, the file the
//! host will load. Run with `--target x86_64-unknown-linux-musl`, and with `--release`
//! to cover the stripped, LTO build that is published.

use swe_vminit_init::{read_marker, CAPABILITIES, MARKER_SECTION};

fn read_built_vmfs() -> Vec<u8> {
    let path = env!("CARGO_BIN_EXE_vmfs");
    std::fs::read(path).unwrap_or_else(|error| panic!("cannot read the built vmfs at {path}: {error}"))
}

#[test]
fn test_read_marker_built_vmfs_returns_package_version() {
    let marker = read_marker(&read_built_vmfs()).expect("the built vmfs carries a marker");
    assert_eq!(marker.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_read_marker_built_vmfs_lists_every_capability_of_the_parser() {
    let marker = read_marker(&read_built_vmfs()).expect("the built vmfs carries a marker");
    assert_eq!(marker.capabilities, CAPABILITIES, "the file lists a different set than the parser accepts");
    assert!(marker.has_capability("entrypoint_hex"), "capabilities: {:?}", marker.capabilities);
}

#[test]
fn test_read_marker_built_vmfs_has_note_section_named_in_the_format() {
    // The section name is part of the documented format; `link_section` cannot take
    // the constant, so check the built file contains the name in its section names.
    let file = read_built_vmfs();
    let needle = MARKER_SECTION.as_bytes();
    assert!(file.windows(needle.len()).any(|window| window == needle), "no {MARKER_SECTION} section name in the file");
}

#[test]
fn test_read_marker_built_vmfs_shorter_than_elf_header_returns_error() {
    // A truncated download must not read as a valid or an unmarked file.
    let file = read_built_vmfs();
    assert!(read_marker(&file[..40]).is_err());
}
