use swe_vminit_init::{format_exit, parse_exit, shm_exit_value, SHM_MAGIC_EXIT_MASK, SHM_MAGIC_READY};

#[test]
fn test_parse_exit_returns_none_for_empty_string() {
    assert_eq!(parse_exit(""), None);
}

#[test]
fn test_parse_exit_returns_none_for_malformed_prefix() {
    assert_eq!(parse_exit("XIKA_EXIT"), None);
    assert_eq!(parse_exit("xika_exit:0"), None);
}

#[test]
fn test_parse_exit_returns_none_for_non_numeric_code() {
    assert_eq!(parse_exit("XIKA_EXIT:abc"), None);
}

#[test]
fn test_parse_exit_does_not_panic_on_overflow_string() {
    // A code string that overflows i32 must return None, not panic.
    let overflow = format!("XIKA_EXIT:{}\n", u64::MAX);
    assert_eq!(parse_exit(&overflow), None);
}

#[test]
fn test_format_exit_negative_code_round_trips() {
    // Negative exit codes are unusual but the protocol carries them.
    // The format must not panic and must round-trip.
    let code = -1i32;
    let formatted = format_exit(code);
    assert_eq!(parse_exit(&formatted), Some(code));
}

#[test]
fn test_shm_exit_value_encodes_code_in_low_16_bits() {
    // Bug this catches: shm_exit_value using the wrong mask width,
    // clobbering the BEEF prefix or truncating negative codes.
    let v = shm_exit_value(42);
    assert_eq!(v & 0xFFFF, 42);
    assert_eq!(v & SHM_MAGIC_EXIT_MASK, SHM_MAGIC_EXIT_MASK);
}

#[test]
fn test_shm_ready_and_exit_values_do_not_collide() {
    // Bug this catches: the READY magic and any EXIT value sharing the
    // same bit pattern — the VMM would misinterpret exit(0) as ready.
    for code in [0i32, 1, 255] {
        assert_ne!(
            shm_exit_value(code),
            SHM_MAGIC_READY,
            "shm_exit_value({}) must not equal SHM_MAGIC_READY",
            code
        );
    }
}
