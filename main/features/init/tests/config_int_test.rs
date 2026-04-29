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
    assert_eq!(cfg.manifest_path, None, "manifest_path must default to None");
    assert_eq!(cfg.manifest_url, None, "manifest_url must default to None");
    assert_eq!(cfg.manifest_hash, None, "manifest_hash must default to None");
    assert_eq!(
        cfg.cache_base, "https://cache.nixos.org",
        "cache_base must default to the public Nix cache"
    );
}

#[test]
fn test_parse_config_cache_base_is_set() {
    let cfg = parse_config("cache_base=https://my.company.cache.example\n");
    assert_eq!(
        cfg.cache_base, "https://my.company.cache.example",
        "cache_base= must override the default binary cache URL"
    );
}

#[test]
fn test_parse_config_cache_base_absent_keeps_default() {
    let cfg = parse_config("install=curl\ninteractive=1\n");
    assert_eq!(
        cfg.cache_base, "https://cache.nixos.org",
        "cache_base must remain default when cache_base= key is absent"
    );
}

#[test]
fn test_parse_config_manifest_path_is_set() {
    let cfg = parse_config("manifest=/etc/packages.json\n");
    assert_eq!(
        cfg.manifest_path,
        Some("/etc/packages.json".to_string()),
        "manifest= must set manifest_path"
    );
}

#[test]
fn test_parse_config_manifest_absent_leaves_path_none() {
    let cfg = parse_config("install=curl\ninteractive=1\n");
    assert_eq!(
        cfg.manifest_path, None,
        "manifest_path must remain None when manifest= key is absent"
    );
}

#[test]
fn test_parse_config_manifest_url_https_is_accepted() {
    // Bug caught: manifest_url= ignoring valid HTTPS URLs and leaving the field None.
    let cfg = parse_config("manifest_url=https://deploy.internal/manifest.json\n");
    assert_eq!(
        cfg.manifest_url,
        Some("https://deploy.internal/manifest.json".to_string()),
        "manifest_url= with https:// must be stored"
    );
}

#[test]
fn test_parse_config_manifest_url_http_is_rejected() {
    // Bug caught: plain HTTP manifest_url being accepted and used, bypassing TLS.
    let cfg = parse_config("manifest_url=http://deploy.internal/manifest.json\n");
    assert_eq!(
        cfg.manifest_url, None,
        "manifest_url= with plain http:// must be rejected (None)"
    );
}

#[test]
fn test_parse_config_manifest_url_absent_leaves_none() {
    // Bug caught: manifest_url defaulting to some non-None sentinel value.
    let cfg = parse_config("install=curl\n");
    assert_eq!(
        cfg.manifest_url, None,
        "manifest_url must remain None when manifest_url= key is absent"
    );
}

#[test]
fn test_parse_config_manifest_hash_sha256_is_stored() {
    // Bug caught: manifest_hash= being silently dropped instead of stored.
    const HASH: &str = "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576f4be72b4db32f73e";
    let cfg = parse_config(&format!("manifest_hash={HASH}\n"));
    assert_eq!(
        cfg.manifest_hash,
        Some(HASH.to_string()),
        "manifest_hash= must store the raw value including sha256: prefix"
    );
}

#[test]
fn test_parse_config_manifest_hash_absent_leaves_none() {
    // Bug caught: manifest_hash defaulting to a non-None value.
    let cfg = parse_config("install=curl\n");
    assert_eq!(
        cfg.manifest_hash, None,
        "manifest_hash must remain None when manifest_hash= key is absent"
    );
}

#[test]
fn test_parse_config_manifest_hash_empty_value_leaves_none() {
    // Bug caught: manifest_hash= with empty value being stored as Some(""),
    // causing a spurious hash mismatch at boot.
    let cfg = parse_config("manifest_hash=\n");
    assert_eq!(
        cfg.manifest_hash, None,
        "manifest_hash= with empty value must leave field as None"
    );
}
