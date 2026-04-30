use swe_vminit_init::{ParseError, VmInitError};

#[test]
fn test_config_read_error_display_includes_prefix() {
    let err = VmInitError::ConfigRead("no such file".to_string());
    let msg = err.to_string();
    assert!(
        msg.starts_with("config read error:"),
        "display must start with 'config read error:', got: {msg}"
    );
}

#[test]
fn test_config_read_error_is_config_read_variant() {
    let err = VmInitError::ConfigRead("access denied".to_string());
    assert!(matches!(err, VmInitError::ConfigRead(_)));
}

#[test]
fn test_overlay_parse_error_from_conversion_is_overlay_parse_variant() {
    let parse_err = ParseError::MalformedLine {
        line_number: 1,
        content: "bad".to_string(),
    };
    let err = VmInitError::from(parse_err);
    assert!(matches!(err, VmInitError::OverlayParse(_)));
}

#[test]
fn test_overlay_parse_error_display_includes_overlay_parse_prefix() {
    let parse_err = ParseError::MalformedLine {
        line_number: 3,
        content: "broken-line".to_string(),
    };
    let err = VmInitError::from(parse_err);
    let msg = err.to_string();
    assert!(
        msg.starts_with("overlay manifest parse error:"),
        "display must start with 'overlay manifest parse error:', got: {msg}"
    );
}
