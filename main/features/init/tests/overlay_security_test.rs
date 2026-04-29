use swe_vminit_init::{OverlayManifest, ParseError};

#[test]
fn test_overlay_parse_missing_field_returns_malformed_line() {
    let err = OverlayManifest::parse("/x:755:0\n").unwrap_err();
    match err {
        ParseError::MalformedLine { line_number, content } => {
            assert_eq!(line_number, 1);
            assert_eq!(content, "/x:755:0");
        }
        other => panic!("expected MalformedLine, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_empty_dest_returns_malformed_line() {
    match OverlayManifest::parse(":755:0:0\n").unwrap_err() {
        ParseError::MalformedLine { line_number, .. } => assert_eq!(line_number, 1),
        other => panic!("expected MalformedLine, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_relative_dest_returns_non_absolute_path() {
    match OverlayManifest::parse("usr/bin/llmd:755:0:0\n").unwrap_err() {
        ParseError::NonAbsolutePath { line_number, path } => {
            assert_eq!(line_number, 1);
            assert_eq!(path, "usr/bin/llmd");
        }
        other => panic!("expected NonAbsolutePath, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_invalid_mode_returns_invalid_mode() {
    match OverlayManifest::parse("/x:99x:0:0\n").unwrap_err() {
        ParseError::InvalidMode { line_number, raw } => {
            assert_eq!(line_number, 1);
            assert_eq!(raw, "99x");
        }
        other => panic!("expected InvalidMode, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_invalid_uid_names_the_uid_field() {
    match OverlayManifest::parse("/x:755:bad:0\n").unwrap_err() {
        ParseError::InvalidUidGid { field, raw, .. } => {
            assert_eq!(field, "uid");
            assert_eq!(raw, "bad");
        }
        other => panic!("expected InvalidUidGid, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_invalid_gid_names_the_gid_field() {
    match OverlayManifest::parse("/x:755:0:bad\n").unwrap_err() {
        ParseError::InvalidUidGid { field, raw, .. } => {
            assert_eq!(field, "gid");
            assert_eq!(raw, "bad");
        }
        other => panic!("expected InvalidUidGid, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_line_number_is_accurate_for_second_failure() {
    match OverlayManifest::parse("/x:755:0:0\n/y:WAT:0:0\n").unwrap_err() {
        ParseError::InvalidMode { line_number, .. } => assert_eq!(line_number, 2),
        other => panic!("expected InvalidMode on line 2, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_octal_overflow_returns_invalid_mode() {
    match OverlayManifest::parse("/x:777777777777:0:0\n").unwrap_err() {
        ParseError::InvalidMode { .. } => {}
        other => panic!("expected InvalidMode for overflow, got {other:?}"),
    }
}

#[test]
fn test_overlay_parse_path_traversal_attempt_is_rejected_as_relative() {
    // Bug this catches: an attacker embedding "../etc/passwd" in the
    // manifest. The schema requires dest to start with '/'; a relative
    // path with ".." doesn't, so it is rejected as NonAbsolutePath.
    match OverlayManifest::parse("../etc/passwd:755:0:0\n").unwrap_err() {
        ParseError::NonAbsolutePath { .. } => {}
        other => panic!("expected NonAbsolutePath, got {other:?}"),
    }
}
