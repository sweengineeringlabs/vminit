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
//!   interactive=1
//!   start_agent=1
//!   mount_rootfs=1
//!   signal_mode=serial|shared_memory
//!   network=dhcp|none
//!   manifest=/etc/packages.json
//!   manifest_url=https://deploy.internal/manifest.json
//!   manifest_hash=sha256:<64-hex-chars>
//!   cache_base=https://cache.nixos.org
//!
//! The keys above are exactly `CAPABILITIES`; a line with any other key is an error.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::api::config::{BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec};
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

/// Parse /etc/vminit.conf text into an InitConfig.
/// Blank lines and `#` comments are skipped. Missing file (empty string) returns default.
///
/// Every other line must be `key=value` with a key in `CAPABILITIES`. A line that is
/// not, or whose key this build does not list, is recorded in `InitConfig::errors`
/// and never skipped silently: an unknown key may carry meaning the host depends on
/// (an argument, an environment variable, a mount), and running without it would be a
/// different workload than the host asked for. A line of a known key whose value
/// cannot be honoured (`entrypoint_hex=` that is not hex) is recorded the same way.
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

        match key {
            "entrypoint" => config.entrypoint.push(val.to_string()),
            "entrypoint_hex" => match decode_hex_argument(val) {
                Ok(argument) => config.entrypoint.push(argument),
                Err(reason) => config.errors.push(format!("line {}: {}", index + 1, reason)),
            },
            "env" => {
                if let Some(pos) = val.find('=') {
                    config.env.push((val[..pos].to_string(), val[pos + 1..].to_string()));
                }
            }
            "volume" => {
                let parts: Vec<&str> = val.splitn(3, ':').collect();
                if parts.len() == 3 {
                    config.volumes.push(VolumeSpec {
                        tag: parts[0].to_string(),
                        guest_mount: parts[1].to_string(),
                        read_only: parts[2] == "ro",
                    });
                }
            }
            "block_mount" => {
                let parts: Vec<&str> = val.splitn(3, ':').collect();
                if parts.len() == 3 {
                    config.block_mounts.push(BlockMount {
                        device: parts[0].to_string(),
                        guest_mount: parts[1].to_string(),
                        fstype: parts[2].to_string(),
                    });
                }
            }
            "install" => config.packages.push(val.to_string()),
            "interactive" => config.interactive = val == "1",
            "start_agent" => config.start_agent = val == "1",
            "mount_rootfs" => config.mount_rootfs = val == "1",
            "signal_mode" => {
                config.signal_mode = match val {
                    "shared_memory" => SignalMode::SharedMemory,
                    _ => SignalMode::Serial,
                };
            }
            "network" => {
                config.network_mode = match val {
                    "none" => GuestNetworkMode::None,
                    _ => GuestNetworkMode::Dhcp,
                };
            }
            "manifest" => config.manifest_path = Some(val.to_string()),
            "manifest_url" => {
                if val.starts_with("https://") {
                    config.manifest_url = Some(val.to_string());
                }
            }
            "manifest_hash" => {
                if !val.is_empty() {
                    config.manifest_hash = Some(val.to_string());
                }
            }
            "cache_base" => {
                if !val.is_empty() {
                    config.cache_base = val.to_string();
                }
            }
            // A key in CAPABILITIES with no arm above: the list promises a capability
            // the parser does not provide, so refuse the line like an unknown key.
            _ => config.errors.push(format!(
                "line {}: key \"{}\" is listed as a capability but has no parser",
                index + 1,
                quote_key(key)
            )),
        }
    }

    config
}
