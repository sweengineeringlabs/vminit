use swe_vminit_init::{format_exit, format_ready, parse_exit, EXIT_PREFIX, READY_SIGNAL};

#[test]
fn test_ready_signal_constant_matches_wire_value() {
    assert_eq!(READY_SIGNAL, "XIKA_READY");
}

#[test]
fn test_exit_prefix_constant_matches_wire_value() {
    assert_eq!(EXIT_PREFIX, "XIKA_EXIT:");
}

#[test]
fn test_format_ready_returns_ready_signal_with_newline() {
    assert_eq!(format_ready(), "XIKA_READY\n");
}

#[test]
fn test_format_exit_zero_produces_correct_wire_string() {
    assert_eq!(format_exit(0), "XIKA_EXIT:0\n");
}

#[test]
fn test_format_exit_nonzero_embeds_code() {
    assert_eq!(format_exit(42), "XIKA_EXIT:42\n");
    assert_eq!(format_exit(127), "XIKA_EXIT:127\n");
}

#[test]
fn test_parse_exit_round_trips_with_format_exit() {
    for code in [0, 1, 42, 127, 255] {
        let formatted = format_exit(code);
        let parsed = parse_exit(&formatted);
        assert_eq!(parsed, Some(code), "round-trip failed for code {}", code);
    }
}

#[test]
fn test_parse_exit_strips_trailing_newline() {
    assert_eq!(parse_exit("XIKA_EXIT:5\n"), Some(5));
    assert_eq!(parse_exit("XIKA_EXIT:5"), Some(5));
}

#[test]
fn test_parse_exit_returns_none_for_ready_signal() {
    assert_eq!(parse_exit("XIKA_READY"), None);
}
