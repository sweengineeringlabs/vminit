//! /etc/vminit.conf parser.
//!
//! Pure function — takes text, returns InitConfig. No filesystem access.
//! The bin/ crate calls ffi::read_file and passes the text here.
//!
//! Format:
//!   entrypoint=<arg>        (may appear multiple times — argv[0], argv[1], ...)
//!   entrypoint_hex=<hex>    (same, the argument written as hex of its bytes; for an
//!                            argument a line cannot carry: a newline, a carriage
//!                            return, trailing whitespace)
//!   env=KEY=VALUE
//!   volume=tag:mountpoint:rw|ro
//!   block_mount=device:mountpoint:fstype
//!   install=<package>
//!   interactive=0|1
//!   start_agent=0|1
//!   mount_rootfs=0|1
//!   signal_mode=serial|shared_memory
//!   network=dhcp|none
//!   dns=<IPv4 or IPv6 address>   (may appear up to MAX_DNS_SERVERS times)
//!   manifest=/etc/packages.json
//!   manifest_url=https://deploy.internal/manifest.json
//!   manifest_hash=sha256:<64-hex-chars>
//!   cache_base=https://cache.nixos.org
//!
//! The keys above are exactly `CAPABILITIES`; a line with any other key is an error,
//! and so is a line of a known key whose value cannot be honoured.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::net::IpAddr;

use crate::api::config::{
    BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec, MAX_DNS_SERVERS,
};
use crate::api::marker::CAPABILITIES;

/// Longest key quoted back in an error message; a longer one is cut (at a character
/// boundary) so a hostile line cannot grow the message without bound.
const MAX_KEY_IN_MESSAGE: usize = 64;

/// Decode the value of an `entrypoint_hex=` line: hex digits (either case) of the
/// argument's bytes, which must be UTF-8 without a NUL byte (execve cannot carry one).
fn decode_hex_argument(hex: &str) -> Result<String, String> {
    let digits = hex.as_bytes();
    if digits.len() % 2 != 0 {
        return Err(format!("entrypoint_hex has an odd number of digits ({})", digits.len()));
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push(high << 4 | low);
    }
    if bytes.contains(&0) {
        return Err("entrypoint_hex contains a NUL byte".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "entrypoint_hex is not valid UTF-8".to_string())
}

fn hex_digit(digit: u8) -> Result<u8, String> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        _ => Err(format!("entrypoint_hex has a non-hex byte 0x{:02x}", digit)),
    }
}

/// The key as it is quoted in an error message: cut to `MAX_KEY_IN_MESSAGE` bytes at
/// a character boundary. The value is never quoted, because it may hold a secret.
fn quote_key(key: &str) -> &str {
    let mut end = key.len().min(MAX_KEY_IN_MESSAGE);
    while !key.is_char_boundary(end) {
        end -= 1;
    }
    &key[..end]
}

/// Parse a `0`/`1` flag; anything else is an error naming the key.
fn parse_flag(key: &str, val: &str) -> Result<bool, String> {
    match val {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(format!("{} must be 0 or 1", key)),
    }
}

/// Split `a:b:c` into three non-empty fields; the third may itself contain colons.
fn split_three_fields<'a>(key: &str, val: &'a str, shape: &str) -> Result<[&'a str; 3], String> {
    let mut parts = val.splitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(c)) if !a.is_empty() && !b.is_empty() && !c.is_empty() => Ok([a, b, c]),
        _ => Err(format!("{} must be {} with three non-empty fields", key, shape)),
    }
}

/// Apply one `key=value` line whose key is in `CAPABILITIES`. `Err` is the reason the
/// value cannot be honoured.
fn apply_line(config: &mut InitConfig, key: &str, val: &str) -> Result<(), String> {
    match key {
        "entrypoint" => config.entrypoint.push(val.to_string()),
        "entrypoint_hex" => config.entrypoint.push(decode_hex_argument(val)?),
        "env" => match val.find('=') {
            Some(pos) if pos > 0 => config.env.push((val[..pos].to_string(), val[pos + 1..].to_string())),
            _ => return Err("env must be NAME=VALUE with a non-empty NAME".to_string()),
        },
        "volume" => {
            let [tag, guest_mount, mode] = split_three_fields(key, val, "tag:mountpoint:rw|ro")?;
            let read_only = match mode {
                "ro" => true,
                "rw" => false,
                _ => return Err("volume access must be rw or ro".to_string()),
            };
            config.volumes.push(VolumeSpec {
                tag: tag.to_string(),
                guest_mount: guest_mount.to_string(),
                read_only,
            });
        }
        "block_mount" => {
            let [device, guest_mount, fstype] = split_three_fields(key, val, "device:mountpoint:fstype")?;
            config.block_mounts.push(BlockMount {
                device: device.to_string(),
                guest_mount: guest_mount.to_string(),
                fstype: fstype.to_string(),
            });
        }
        "install" => {
            if val.is_empty() {
                return Err("install needs a package name".to_string());
            }
            config.packages.push(val.to_string());
        }
        "interactive" => config.interactive = parse_flag(key, val)?,
        "start_agent" => config.start_agent = parse_flag(key, val)?,
        "mount_rootfs" => config.mount_rootfs = parse_flag(key, val)?,
        "signal_mode" => {
            config.signal_mode = match val {
                "serial" => SignalMode::Serial,
                "shared_memory" => SignalMode::SharedMemory,
                _ => return Err("signal_mode must be serial or shared_memory".to_string()),
            };
        }
        "network" => {
            config.network_mode = match val {
                "dhcp" => GuestNetworkMode::Dhcp,
                "none" => GuestNetworkMode::None,
                _ => return Err("network must be dhcp or none".to_string()),
            };
        }
        "dns" => {
            let address: IpAddr = val.parse().map_err(|_| "dns must be an IPv4 or IPv6 address".to_string())?;
            if config.dns_servers.len() >= MAX_DNS_SERVERS {
                return Err(format!("dns may appear at most {} times (the resolver uses no more)", MAX_DNS_SERVERS));
            }
            config.dns_servers.push(address);
        }
        "manifest" => {
            if val.is_empty() {
                return Err("manifest needs a path".to_string());
            }
            config.manifest_path = Some(val.to_string());
        }
        "manifest_url" => {
            if !val.starts_with("https://") {
                return Err("manifest_url must start with https:// (plain http is refused)".to_string());
            }
            config.manifest_url = Some(val.to_string());
        }
        "manifest_hash" => {
            if val.is_empty() {
                return Err("manifest_hash needs a value such as sha256:<hex>".to_string());
            }
            config.manifest_hash = Some(val.to_string());
        }
        "cache_base" => {
            if val.is_empty() {
                return Err("cache_base needs a URL".to_string());
            }
            config.cache_base = val.to_string();
        }
        // A key in CAPABILITIES with no arm above: the list promises a capability
        // the parser does not provide, so refuse the line like an unknown key.
        _ => return Err(format!("\"{}\" is listed as a capability but has no parser", quote_key(key))),
    }
    Ok(())
}

/// Render `/etc/resolv.conf` for explicitly configured DNS servers: one `nameserver`
/// line each, in order.
pub fn format_resolv_conf(servers: &[IpAddr]) -> String {
    let mut text = String::new();
    for server in servers {
        text.push_str(&format!("nameserver {}\n", server));
    }
    text
}

/// Parse /etc/vminit.conf text into an InitConfig.
/// Blank lines and `#` comments are skipped. Missing file (empty string) returns default.
///
/// Every other line must be `key=value` with a key in `CAPABILITIES` and a value that
/// key can honour. A line that is not, or whose key this build does not list, or whose
/// value is malformed, is recorded in `InitConfig::errors` and never skipped silently:
/// the line may carry meaning the host depends on (an argument, an environment
/// variable, a mount, a resolver), and running without it would be a different
/// workload than the host asked for. The caller must refuse to boot when `errors` is
/// not empty.
pub fn parse_config(text: &str) -> InitConfig {
    let mut config = InitConfig::default();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, val)) = line.split_once('=') else {
            config.errors.push(format!("line {}: not a key=value line", index + 1));
            continue;
        };
        if !CAPABILITIES.contains(&key) {
            config.errors.push(format!(
                "line {}: unknown key \"{}\" (this vminit does not support it; a newer vminit may)",
                index + 1,
                quote_key(key)
            ));
            continue;
        }
        if let Err(reason) = apply_line(&mut config, key, val) {
            config.errors.push(format!("line {}: {}", index + 1, reason));
        }
    }

    config
}
