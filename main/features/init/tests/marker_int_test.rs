use swe_vminit_init::{
    encode_marker, format_banner, format_own_banner, marker_len, read_marker, Marker, MarkerError,
    CAPABILITIES, MARKER_FORMAT_VERSION,
};

const VERSION: &str = "1.2.3";
const CAPS: &[&str] = &["a", "bc"];
const LEN: usize = marker_len(VERSION, CAPS);

fn sample_note() -> [u8; LEN] {
    encode_marker::<LEN>(VERSION, CAPS)
}

/// A foreign note ("GNU", type 3, 4-byte description), to sit before the marker.
fn foreign_note() -> Vec<u8> {
    let mut note = Vec::new();
    note.extend_from_slice(&4u32.to_le_bytes());
    note.extend_from_slice(&4u32.to_le_bytes());
    note.extend_from_slice(&3u32.to_le_bytes());
    note.extend_from_slice(b"GNU\0");
    note.extend_from_slice(&[1, 2, 3, 4]);
    note
}

/// A minimal little-endian ELF64 file with one PT_NOTE program header over `segment`.
fn elf_with_note_segment(segment: &[u8]) -> Vec<u8> {
    let mut file = vec![0u8; 64 + 56];
    file[..4].copy_from_slice(b"\x7fELF");
    file[4] = 2; // ELFCLASS64
    file[5] = 1; // little endian
    file[0x20..0x28].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    file[0x36..0x38].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    file[0x38..0x3a].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
    file[64..68].copy_from_slice(&4u32.to_le_bytes()); // PT_NOTE
    file[64 + 8..64 + 16].copy_from_slice(&120u64.to_le_bytes()); // p_offset
    file[64 + 32..64 + 40].copy_from_slice(&(segment.len() as u64).to_le_bytes()); // p_filesz
    file[64 + 48..64 + 56].copy_from_slice(&4u64.to_le_bytes()); // p_align
    file.extend_from_slice(segment);
    file
}

#[test]
fn test_encode_marker_small_input_matches_documented_layout_byte_for_byte() {
    // Written out by hand from the layout in docs/xkvm_conf.md, so a change to the
    // encoder cannot move the expected bytes with it.
    let note = sample_note();
    let mut want: Vec<u8> = Vec::new();
    want.extend_from_slice(&[7, 0, 0, 0]); // namesz
    want.extend_from_slice(&[20, 0, 0, 0]); // descsz: 4 + (2+5) + 2 + (2+1) + (2+2)
    want.extend_from_slice(&[1, 0, 0, 0]); // type 1
    want.extend_from_slice(b"vminit\0\0"); // name, NUL, padded to 8
    want.extend_from_slice(&[1, 0, 0, 0]); // format version 1
    want.extend_from_slice(&[5, 0]);
    want.extend_from_slice(b"1.2.3");
    want.extend_from_slice(&[2, 0]); // two capabilities
    want.extend_from_slice(&[1, 0]);
    want.extend_from_slice(b"a");
    want.extend_from_slice(&[2, 0]);
    want.extend_from_slice(b"bc");
    assert_eq!(note.to_vec(), want);
}

#[test]
fn test_read_marker_encoded_note_returns_version_and_capabilities() {
    let file = elf_with_note_segment(&sample_note());
    let marker = read_marker(&file).expect("marker");
    assert_eq!(
        marker,
        Marker {
            format_version: MARKER_FORMAT_VERSION,
            version: "1.2.3".to_string(),
            capabilities: vec!["a".to_string(), "bc".to_string()],
        }
    );
    assert!(marker.has_capability("bc"));
    assert!(
        !marker.has_capability("b"),
        "a prefix of a capability is not the capability"
    );
}

#[test]
fn test_read_marker_note_after_a_foreign_note_is_found() {
    let mut segment = foreign_note();
    segment.extend_from_slice(&sample_note());
    let marker = read_marker(&elf_with_note_segment(&segment)).expect("marker");
    assert_eq!(marker.version, "1.2.3");
}

#[test]
fn test_read_marker_only_foreign_notes_returns_no_marker() {
    let file = elf_with_note_segment(&foreign_note());
    assert_eq!(read_marker(&file), Err(MarkerError::NoMarker));
}

#[test]
fn test_read_marker_elf_without_program_headers_returns_no_marker() {
    let mut file = elf_with_note_segment(&[]);
    file[0x38..0x3a].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(read_marker(&file), Err(MarkerError::NoMarker));
}

#[test]
fn test_read_marker_same_name_other_type_returns_no_marker() {
    let mut note = sample_note().to_vec();
    note[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::NoMarker)
    );
}

#[test]
fn test_read_marker_other_name_same_type_returns_no_marker() {
    let mut note = sample_note().to_vec();
    note[12..18].copy_from_slice(b"vminiT");
    assert_eq!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::NoMarker)
    );
}

#[test]
fn test_read_marker_not_elf_returns_not_elf64() {
    assert_eq!(
        read_marker(b"#!/bin/sh\necho hi\n"),
        Err(MarkerError::NotElf64)
    );
    assert_eq!(read_marker(&[]), Err(MarkerError::NotElf64));
}

#[test]
fn test_read_marker_elf32_or_big_endian_returns_not_elf64() {
    let mut elf32 = elf_with_note_segment(&sample_note());
    elf32[4] = 1;
    assert_eq!(read_marker(&elf32), Err(MarkerError::NotElf64));
    let mut big_endian = elf_with_note_segment(&sample_note());
    big_endian[5] = 2;
    assert_eq!(read_marker(&big_endian), Err(MarkerError::NotElf64));
}

#[test]
fn test_read_marker_segment_past_end_of_file_returns_truncated() {
    let file = elf_with_note_segment(&sample_note());
    for cut in 120..file.len() {
        assert_eq!(
            read_marker(&file[..cut]),
            Err(MarkerError::Truncated),
            "cut at {cut}"
        );
    }
}

#[test]
fn test_read_marker_program_header_table_past_end_returns_truncated() {
    let file = elf_with_note_segment(&sample_note());
    assert_eq!(read_marker(&file[..70]), Err(MarkerError::Truncated));
}

#[test]
fn test_read_marker_unknown_format_version_returns_unsupported_format() {
    let mut note = sample_note().to_vec();
    note[20..24].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::UnsupportedFormat(2))
    );
}

#[test]
fn test_read_marker_bytes_after_last_capability_returns_malformed() {
    let mut note = sample_note().to_vec();
    // descsz 20 -> 24: four payload bytes after the last capability.
    note[4..8].copy_from_slice(&24u32.to_le_bytes());
    note.extend_from_slice(&[0x41; 4]);
    assert!(matches!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Malformed(_))
    ));
}

#[test]
fn test_read_marker_string_longer_than_payload_returns_malformed() {
    let mut note = sample_note().to_vec();
    note[24..26].copy_from_slice(&200u16.to_le_bytes()); // version_len
    assert!(matches!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Malformed(_))
    ));
}

#[test]
fn test_read_marker_capability_count_larger_than_payload_returns_malformed() {
    let mut note = sample_note().to_vec();
    note[31..33].copy_from_slice(&60000u16.to_le_bytes()); // capability_count
    assert!(matches!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Malformed(_))
    ));
}

#[test]
fn test_read_marker_version_not_utf8_returns_malformed() {
    let mut note = sample_note().to_vec();
    note[26] = 0xff;
    assert!(matches!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Malformed(_))
    ));
}

#[test]
fn test_read_marker_hostile_segment_size_returns_error_without_panic() {
    let mut file = elf_with_note_segment(&sample_note());
    file[64 + 32..64 + 40].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(read_marker(&file), Err(MarkerError::Truncated));
    file[64 + 8..64 + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(read_marker(&file), Err(MarkerError::Truncated));
}

#[test]
fn test_read_marker_hostile_note_lengths_return_error_without_panic() {
    let mut note = sample_note().to_vec();
    note[..4].copy_from_slice(&u32::MAX.to_le_bytes()); // namesz
    assert_eq!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Truncated)
    );
    let mut note = sample_note().to_vec();
    note[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // descsz
    assert_eq!(
        read_marker(&elf_with_note_segment(&note)),
        Err(MarkerError::Truncated)
    );
}

#[test]
fn test_read_marker_program_header_entry_size_zero_returns_malformed() {
    let mut file = elf_with_note_segment(&sample_note());
    file[0x36..0x38].copy_from_slice(&0u16.to_le_bytes());
    assert!(matches!(read_marker(&file), Err(MarkerError::Malformed(_))));
}

#[test]
fn test_format_banner_lists_version_and_capabilities_in_order() {
    assert_eq!(
        format_banner("0.1.6", &["entrypoint", "entrypoint_hex"]),
        "vminit 0.1.6 capabilities=entrypoint,entrypoint_hex"
    );
}

#[test]
fn test_format_own_banner_names_every_capability_of_the_parser() {
    let banner = format_own_banner("9.9.9");
    assert_eq!(
        banner,
        format!("vminit 9.9.9 capabilities={}", CAPABILITIES.join(","))
    );
    assert!(banner.contains("entrypoint_hex"));
}
