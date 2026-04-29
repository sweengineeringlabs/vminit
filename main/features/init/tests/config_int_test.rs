use swe_vminit_init::{parse_config, SignalMode};

#[test]
fn test_parse_config_entrypoint_accumulates_multiple_values() {
    let cfg = parse_config("entrypoint=echo\nentrypoint=hello\n");
    assert_eq!(cfg.entrypoint, vec!["echo", "hello"]);
}

#[test]
fn test_parse_config_env_splits_on_first_equals() {
    let cfg = parse_config("env=PATH=/usr/bin:/bin\nenv=HOME=/root\n");
    assert_eq!(cfg.env[0], ("PATH".into(), "/usr/bin:/bin".into()));
    assert_eq!(cfg.env[1], ("HOME".into(), "/root".into()));
}

#[test]
fn test_parse_config_volume_read_write_flag() {
    let cfg = parse_config("volume=host0:/mnt/data:rw\nvolume=host1:/mnt/ro:ro\n");
    assert_eq!(cfg.volumes.len(), 2);
    assert_eq!(cfg.volumes[0].tag, "host0");
    assert_eq!(cfg.volumes[0].guest_mount, "/mnt/data");
    assert!(!cfg.volumes[0].read_only);
    assert!(cfg.volumes[1].read_only);
}

#[test]
fn test_parse_config_boolean_flags() {
    let cfg = parse_config("interactive=1\nstart_agent=1\nkali_mode=1\n");
    assert!(cfg.interactive);
    assert!(cfg.start_agent);
    assert!(cfg.kali_mode);
}

#[test]
fn test_parse_config_flags_default_to_false() {
    let cfg = parse_config("");
    assert!(!cfg.interactive);
    assert!(!cfg.start_agent);
    assert!(!cfg.kali_mode);
}

#[test]
fn test_parse_config_signal_mode_shared_memory() {
    let cfg = parse_config("signal_mode=shared_memory\n");
    assert_eq!(cfg.signal_mode, SignalMode::SharedMemory);
}

#[test]
fn test_parse_config_signal_mode_defaults_to_serial() {
    let cfg = parse_config("");
    assert_eq!(cfg.signal_mode, SignalMode::Serial);
}

#[test]
fn test_parse_config_signal_mode_unknown_value_falls_back_to_serial() {
    let cfg = parse_config("signal_mode=unknown_mode\n");
    assert_eq!(cfg.signal_mode, SignalMode::Serial);
}

#[test]
fn test_parse_config_install_packages() {
    let cfg = parse_config("install=curl\ninstall=jq\n");
    assert_eq!(cfg.packages, vec!["curl", "jq"]);
}

#[test]
fn test_parse_config_comments_and_blank_lines_ignored() {
    let cfg = parse_config("# comment\n\nentrypoint=ls\n  \n");
    assert_eq!(cfg.entrypoint, vec!["ls"]);
}

#[test]
fn test_parse_config_empty_text_returns_defaults() {
    let cfg = parse_config("");
    assert!(cfg.entrypoint.is_empty());
    assert!(cfg.env.is_empty());
    assert!(cfg.volumes.is_empty());
    assert!(cfg.packages.is_empty());
    assert_eq!(cfg.signal_mode, SignalMode::Serial);
}
