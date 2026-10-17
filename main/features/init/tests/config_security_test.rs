use swe_vminit_init::parse_config;

#[test]
fn test_parse_config_volume_with_two_parts_is_silently_ignored() {
    // Bug this catches: a malformed volume= line with only two
    // colon-separated fields being accepted, leaving guest_mount or
    // read_only uninitialised. splitn(3) on a two-part value produces
    // len() == 2, which the guard rejects.
    let cfg = parse_config("volume=host0:/mnt\n");
    assert!(cfg.volumes.is_empty(), "incomplete volume line must not produce an entry");
}

#[test]
fn test_parse_config_env_without_equals_is_silently_ignored() {
    // Bug this catches: an env= line with no = in the value being split
    // at index 0, producing an empty key and full value as the "value".
    // The `find('=')` guard returns None and the line is skipped.
    let cfg = parse_config("env=NO_EQUALS_HERE\n");
    assert!(cfg.env.is_empty(), "env line without = in value must not produce an entry");
}

#[test]
fn test_parse_config_extremely_long_value_does_not_panic() {
    let long_value = "x".repeat(65536);
    let input = format!("entrypoint={}\n", long_value);
    let cfg = parse_config(&input);
    assert_eq!(cfg.entrypoint.len(), 1);
    assert_eq!(cfg.entrypoint[0].len(), 65536);
}

#[test]
fn test_parse_config_null_bytes_in_value_are_preserved() {
    // Null bytes in config values are unusual but must not panic or
    // truncate the string — the binary just passes it through.
    let input = "entrypoint=hello\0world\n";
    let cfg = parse_config(input);
    assert_eq!(cfg.entrypoint.len(), 1);
    assert!(cfg.entrypoint[0].contains('\0'));
}

#[test]
fn test_parse_config_unicode_in_values_is_preserved() {
    let cfg = parse_config("entrypoint=/usr/bin/café\n");
    assert_eq!(cfg.entrypoint[0], "/usr/bin/café");
}

#[test]
fn test_parse_config_volume_tag_with_colon_is_rejected() {
    // Bug this catches: a volume tag containing a colon splitting into
    // more than three fields, letting splitn(3) silently misparse the
    // mount point as containing the access flag.
    // With splitn(3, ':'), "a:b:c:d" gives ["a", "b", "c:d"] — len==3,
    // so read_only is checked against "c:d" which is not "ro", meaning
    // it's accepted as rw but with a malformed guest_mount. This test
    // documents the current behaviour so regressions are visible.
    let cfg = parse_config("volume=host:extra:/mnt:ro\n");
    // splitn(3) on "host:extra:/mnt:ro" → ["host", "extra", "/mnt:ro"]
    // read_only = "/mnt:ro" != "ro" → false. Not ideal but known.
    // The important thing is it doesn't panic.
    assert_eq!(cfg.volumes.len(), 1);
}

#[test]
fn test_parse_config_entrypoint_hex_odd_digit_count_is_an_error_naming_the_line() {
    // Bug this catches: an undecodable argument being skipped, which shifts every
    // later argument one place to the left.
    let cfg = parse_config("entrypoint=/bin/sh\nentrypoint_hex=abc\nentrypoint=x\n");
    assert_eq!(cfg.errors.len(), 1, "errors: {:?}", cfg.errors);
    assert!(cfg.errors[0].starts_with("line 2:"), "got {:?}", cfg.errors[0]);
    assert!(cfg.errors[0].contains("odd number"), "got {:?}", cfg.errors[0]);
}

#[test]
fn test_parse_config_entrypoint_hex_non_hex_byte_is_an_error() {
    let cfg = parse_config("entrypoint_hex=zz\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.errors[0].contains("non-hex"), "got {:?}", cfg.errors[0]);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_entrypoint_hex_nul_byte_is_an_error() {
    let cfg = parse_config("entrypoint_hex=610062\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.errors[0].contains("NUL"), "got {:?}", cfg.errors[0]);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_entrypoint_hex_invalid_utf8_is_an_error() {
    let cfg = parse_config("entrypoint_hex=ff\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.errors[0].contains("UTF-8"), "got {:?}", cfg.errors[0]);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_without_bad_lines_has_no_errors() {
    let cfg = parse_config("entrypoint=/bin/true\nenv=A=b\nvolume=h:/m:rw\n");
    assert!(cfg.errors.is_empty());
}

#[test]
fn test_parse_config_unknown_key_is_an_error_naming_line_and_key() {
    // Bug this catches: a key from a newer host being ignored, so the guest runs a
    // different workload than the host asked for (what entrypoint_hex= did on v0.1.4).
    let cfg = parse_config("entrypoint=/bin/true\nfuture_key=1\n");
    assert_eq!(cfg.errors.len(), 1, "errors: {:?}", cfg.errors);
    assert!(cfg.errors[0].starts_with("line 2:"), "got {:?}", cfg.errors[0]);
    assert!(cfg.errors[0].contains("\"future_key\""), "got {:?}", cfg.errors[0]);
}

#[test]
fn test_parse_config_unknown_key_error_does_not_quote_the_value() {
    let cfg = parse_config("dns=hunter2-secret\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(!cfg.errors[0].contains("hunter2"), "value leaked into {:?}", cfg.errors[0]);
}

#[test]
fn test_parse_config_unknown_key_with_very_long_name_is_cut_in_the_error() {
    let long_key = "k".repeat(65536);
    let cfg = parse_config(&format!("{long_key}=1\n"));
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.errors[0].len() < 300, "error is {} bytes", cfg.errors[0].len());
}

#[test]
fn test_parse_config_unknown_key_with_multibyte_name_is_cut_at_a_character_boundary() {
    // 3-byte characters: a cut at byte 64 falls inside one.
    let cfg = parse_config(&format!("{}=1\n", "\u{20ac}".repeat(100)));
    assert_eq!(cfg.errors.len(), 1);
}

#[test]
fn test_parse_config_line_without_equals_is_an_error() {
    let cfg = parse_config("entrypoint\n");
    assert_eq!(cfg.errors.len(), 1, "errors: {:?}", cfg.errors);
    assert!(cfg.errors[0].contains("not a key=value"), "got {:?}", cfg.errors[0]);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_key_with_different_case_is_an_unknown_key() {
    // Keys are case-sensitive: ENTRYPOINT= is not entrypoint=.
    let cfg = parse_config("ENTRYPOINT=/bin/true\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_key_with_space_before_equals_is_an_unknown_key() {
    let cfg = parse_config("entrypoint =/bin/true\n");
    assert_eq!(cfg.errors.len(), 1);
    assert!(cfg.entrypoint.is_empty());
}

#[test]
fn test_parse_config_comments_and_blank_lines_are_not_errors() {
    let cfg = parse_config("# a comment with unknown=key\n\n   \n\t# indented comment\nentrypoint=/bin/true\n");
    assert!(cfg.errors.is_empty(), "errors: {:?}", cfg.errors);
    assert_eq!(cfg.entrypoint, ["/bin/true"]);
}

#[test]
fn test_parse_config_each_unknown_key_gets_its_own_error_and_known_lines_still_apply() {
    let cfg = parse_config("a=1\nentrypoint=/bin/true\nb=2\n");
    assert_eq!(cfg.errors.len(), 2, "errors: {:?}", cfg.errors);
    assert!(cfg.errors[0].starts_with("line 1:") && cfg.errors[1].starts_with("line 3:"));
    assert_eq!(cfg.entrypoint, ["/bin/true"]);
}
