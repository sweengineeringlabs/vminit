use swe_vminit_init::{ParseError, VmInitError};

#[test]
fn test_config_read_error_message_has_consistent_prefix_regardless_of_io_message() {
    for msg in &["secret internal detail", "ENOMEM", "kernel panic", "stack trace: 0x..."] {
        let err = VmInitError::ConfigRead(msg.to_string());
        let display = err.to_string();
        assert!(
            display.starts_with("config read error:"),
            "error must have consistent prefix regardless of internal message, got: {display}"
        );
    }
}

#[test]
fn test_overlay_parse_error_message_includes_line_number_for_forensics() {
    let parse_err = ParseError::MalformedLine {
        line_number: 42,
        content: "bad-content".to_string(),
    };
    let err = VmInitError::from(parse_err);
    let display = err.to_string();
    assert!(
        display.contains("42"),
        "error message must include line number for debugging, got: {display}"
    );
}

#[test]
fn test_overlay_parse_non_absolute_path_error_includes_path_in_message() {
    let parse_err = ParseError::NonAbsolutePath {
        line_number: 7,
        path: "relative/path".to_string(),
    };
    let err = VmInitError::from(parse_err);
    let display = err.to_string();
    assert!(
        display.contains("relative/path"),
        "error must echo the offending path for actionability, got: {display}"
    );
}
