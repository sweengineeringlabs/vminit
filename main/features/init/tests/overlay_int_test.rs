use swe_vminit_init::{OverlayEntry, OverlayManifest};

fn entry(dest: &str, mode: u32, uid: u32, gid: u32) -> OverlayEntry {
    OverlayEntry { dest: dest.to_string(), mode, uid, gid }
}

#[test]
fn test_overlay_parse_then_emit_round_trips() {
    let m = OverlayManifest {
        entries: vec![
            entry("/usr/bin/llmd", 0o100755, 0, 0),
            entry("/etc/llmd.toml", 0o100644, 1000, 1000),
        ],
    };
    let text = m.emit();
    let back = OverlayManifest::parse(&text).unwrap();
    let mut expected = m.entries.clone();
    expected.sort_by(|a, b| a.dest.cmp(&b.dest));
    assert_eq!(back.entries, expected);
}

#[test]
fn test_overlay_emit_is_deterministic_regardless_of_input_order() {
    let a = OverlayManifest {
        entries: vec![entry("/a", 0o755, 0, 0), entry("/b", 0o644, 0, 0)],
    };
    let b = OverlayManifest {
        entries: vec![entry("/b", 0o644, 0, 0), entry("/a", 0o755, 0, 0)],
    };
    assert_eq!(a.emit(), b.emit());
}

#[test]
fn test_overlay_parse_tolerates_comments_and_blank_lines() {
    let text = "# comment\n\n/x:755:0:0\n  \n# trailing\n";
    let m = OverlayManifest::parse(text).unwrap();
    assert_eq!(m.entries.len(), 1);
    assert_eq!(m.entries[0].dest, "/x");
}

#[test]
fn test_overlay_parse_accepts_full_mode_with_type_bits() {
    let text = "/etc/conf:100644:0:0\n";
    let m = OverlayManifest::parse(text).unwrap();
    assert_eq!(m.entries[0].mode, 0o100644);
}

#[test]
fn test_overlay_emit_format_is_dest_colon_octal_colon_uid_colon_gid_newline() {
    let m = OverlayManifest { entries: vec![entry("/x", 0o100755, 0, 0)] };
    assert_eq!(m.emit(), "/x:100755:0:0\n");
}

#[test]
fn test_overlay_emit_zero_mode_emits_single_zero() {
    let m = OverlayManifest { entries: vec![entry("/x", 0, 0, 0)] };
    assert_eq!(m.emit(), "/x:0:0:0\n");
}

#[test]
fn test_overlay_emit_empty_manifest_is_empty_string() {
    assert_eq!(OverlayManifest::default().emit(), "");
}

#[test]
fn test_overlay_display_entry_uses_wire_format() {
    let e = entry("/x", 0o100755, 0, 0);
    assert_eq!(e.to_string(), "/x:100755:0:0");
}
