//! One test per capability: a config line for it is accepted and does what the key
//! says. Together with the list-equality test they keep `CAPABILITIES`, the parser
//! and the marker from drifting apart.

use swe_vminit_init::{parse_config, GuestNetworkMode, InitConfig, SignalMode, CAPABILITIES};

/// Every capability with a line that is valid for it and a check of its effect.
/// The first column must equal `CAPABILITIES` exactly (see the test below).
const CASES: &[(&str, &str, fn(&InitConfig) -> bool)] = &[
    ("entrypoint", "entrypoint=/bin/true", |c| c.entrypoint == ["/bin/true"]),
    ("entrypoint_hex", "entrypoint_hex=612062", |c| c.entrypoint == ["a b"]),
    ("env", "env=K=v=w", |c| c.env == [("K".to_string(), "v=w".to_string())]),
    ("volume", "volume=tag:/mnt:ro", |c| c.volumes.len() == 1 && c.volumes[0].tag == "tag" && c.volumes[0].read_only),
    ("block_mount", "block_mount=/dev/vdb:/data:ext4", |c| c.block_mounts.len() == 1 && c.block_mounts[0].fstype == "ext4"),
    ("install", "install=curl", |c| c.packages == ["curl"]),
    ("interactive", "interactive=1", |c| c.interactive),
    ("start_agent", "start_agent=1", |c| c.start_agent),
    ("mount_rootfs", "mount_rootfs=1", |c| c.mount_rootfs),
    ("signal_mode", "signal_mode=shared_memory", |c| c.signal_mode == SignalMode::SharedMemory),
    ("network", "network=none", |c| c.network_mode == GuestNetworkMode::None),
    ("dns", "dns=10.0.0.53", |c| c.dns_servers.len() == 1 && c.dns_servers[0].to_string() == "10.0.0.53"),
    ("manifest", "manifest=/etc/packages.json", |c| c.manifest_path.as_deref() == Some("/etc/packages.json")),
    ("manifest_url", "manifest_url=https://h/m.json", |c| c.manifest_url.as_deref() == Some("https://h/m.json")),
    ("manifest_hash", "manifest_hash=sha256:ab", |c| c.manifest_hash.as_deref() == Some("sha256:ab")),
    ("cache_base", "cache_base=https://cache.example", |c| c.cache_base == "https://cache.example"),
];

#[test]
fn test_capabilities_case_table_equals_the_capability_list() {
    let table: Vec<&str> = CASES.iter().map(|case| case.0).collect();
    assert_eq!(table, CAPABILITIES, "add the capability to CASES (and a parser arm) or remove it from CAPABILITIES");
}

#[test]
fn test_parse_config_every_capability_line_is_accepted_and_takes_effect() {
    for (capability, line, took_effect) in CASES {
        assert!(line.starts_with(&format!("{capability}=")), "case line {line:?} is not for {capability}");
        let cfg = parse_config(&format!("{line}\n"));
        assert!(cfg.errors.is_empty(), "{capability}: line refused: {:?}", cfg.errors);
        assert!(took_effect(&cfg), "{capability}: line {line:?} parsed but had no effect: {cfg:?}");
    }
}

#[test]
fn test_parse_config_every_capability_line_changes_the_default_config() {
    // An effect check that is also true of the default would pass with the arm removed.
    let default = parse_config("");
    for (capability, _, took_effect) in CASES {
        assert!(!took_effect(&default), "{capability}: effect check holds for an empty config");
    }
}

#[test]
fn test_parse_config_every_capability_name_is_a_key_not_a_prefix_of_another() {
    // `entrypoint` and `entrypoint_hex` share a prefix; neither line may be read as the other.
    let cfg = parse_config("entrypoint_hex=41\nentrypoint=B\n");
    assert_eq!(cfg.entrypoint, ["A", "B"]);
}
