//! A known key with a value it cannot honour is an error, never a silent drop: the
//! workload would otherwise differ from the one the host asked for.

use swe_vminit_init::{format_resolv_conf, parse_config, MAX_DNS_SERVERS};

#[test]
fn test_parse_config_env_with_empty_name_is_an_error() {
    let cfg = parse_config("env==value\n");
    assert!(cfg.env.is_empty());
    assert_eq!(cfg.errors.len(), 1);
}

#[test]
fn test_parse_config_env_with_empty_value_is_accepted() {
    let cfg = parse_config("env=EMPTY=\n");
    assert_eq!(cfg.env, [("EMPTY".to_string(), String::new())]);
    assert!(cfg.errors.is_empty());
}

#[test]
fn test_parse_config_volume_access_other_than_rw_or_ro_is_an_error() {
    // Bug this catches: a "readonly" typo being mounted read-write.
    let cfg = parse_config("volume=tag:/mnt:readonly\n");
    assert!(cfg.volumes.is_empty());
    assert_eq!(cfg.errors.len(), 1);
}

#[test]
fn test_parse_config_volume_with_empty_field_is_an_error() {
    assert_eq!(parse_config("volume=:/mnt:rw\n").errors.len(), 1);
    assert_eq!(parse_config("volume=tag::rw\n").errors.len(), 1);
}

#[test]
fn test_parse_config_block_mount_with_two_parts_is_an_error() {
    let cfg = parse_config("block_mount=/dev/vdb:/data\n");
    assert!(cfg.block_mounts.is_empty());
    assert_eq!(cfg.errors.len(), 1);
}

#[test]
fn test_parse_config_flag_other_than_0_or_1_is_an_error() {
    for key in ["interactive", "start_agent", "mount_rootfs"] {
        let cfg = parse_config(&format!("{key}=yes\n"));
        assert_eq!(cfg.errors.len(), 1, "{key}: {:?}", cfg.errors);
        assert!(!cfg.interactive && !cfg.start_agent && !cfg.mount_rootfs, "{key} took effect");
    }
}

#[test]
fn test_parse_config_flag_zero_is_false_without_error() {
    let cfg = parse_config("interactive=0\nstart_agent=0\nmount_rootfs=0\n");
    assert!(cfg.errors.is_empty());
    assert!(!cfg.interactive && !cfg.start_agent && !cfg.mount_rootfs);
}

#[test]
fn test_parse_config_network_other_than_dhcp_or_none_is_an_error() {
    assert_eq!(parse_config("network=static\n").errors.len(), 1);
}

#[test]
fn test_parse_config_dns_not_an_address_is_an_error() {
    for value in ["", "example.com", "10.0.0.256", "10.0.0.1 10.0.0.2", "10.0.0.1\u{0}", "::g"] {
        let cfg = parse_config(&format!("dns={value}\n"));
        assert_eq!(cfg.errors.len(), 1, "value {value:?}: {:?}", cfg.errors);
        assert!(cfg.dns_servers.is_empty(), "value {value:?} was stored");
    }
}

#[test]
fn test_parse_config_dns_ipv6_and_repeats_keep_order() {
    let cfg = parse_config("dns=2001:4860:4860::8888\ndns=8.8.8.8\n");
    assert!(cfg.errors.is_empty(), "errors: {:?}", cfg.errors);
    let servers: Vec<String> = cfg.dns_servers.iter().map(|s| s.to_string()).collect();
    assert_eq!(servers, ["2001:4860:4860::8888", "8.8.8.8"]);
}

#[test]
fn test_parse_config_dns_more_than_the_resolver_uses_is_an_error() {
    let cfg = parse_config("dns=1.1.1.1\ndns=1.0.0.1\ndns=8.8.8.8\ndns=8.8.4.4\n");
    assert_eq!(cfg.dns_servers.len(), MAX_DNS_SERVERS);
    assert_eq!(cfg.errors.len(), 1);
}

#[test]
fn test_format_resolv_conf_lists_one_nameserver_line_per_server_in_order() {
    let cfg = parse_config("dns=10.0.0.53\ndns=fd00::53\n");
    assert_eq!(format_resolv_conf(&cfg.dns_servers), "nameserver 10.0.0.53\nnameserver fd00::53\n");
    assert_eq!(format_resolv_conf(&[]), "");
}

#[test]
fn test_parse_config_empty_value_for_path_keys_is_an_error() {
    for key in ["install", "manifest", "cache_base", "manifest_hash"] {
        assert_eq!(parse_config(&format!("{key}=\n")).errors.len(), 1, "{key}");
    }
}
