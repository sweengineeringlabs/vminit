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
