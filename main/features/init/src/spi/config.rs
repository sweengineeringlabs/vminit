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

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::api::config::{BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec};

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

/// Parse /etc/vminit.conf text into an InitConfig.
/// Unknown keys are silently ignored. Missing file (empty string) returns default.
/// A line of a known key whose value cannot be honoured (`entrypoint_hex=` that is not
/// hex) is recorded in `InitConfig::errors`, never skipped silently.
pub fn parse_config(text: &str) -> InitConfig {
    let mut config = InitConfig::default();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(val) = line.strip_prefix("entrypoint=") {
            config.entrypoint.push(val.to_string());
        } else if let Some(val) = line.strip_prefix("entrypoint_hex=") {
            match decode_hex_argument(val) {
                Ok(argument) => config.entrypoint.push(argument),
                Err(reason) => config.errors.push(format!("line {}: {}", index + 1, reason)),
            }
        } else if let Some(val) = line.strip_prefix("env=") {
            if let Some(pos) = val.find('=') {
                config.env.push((val[..pos].to_string(), val[pos + 1..].to_string()));
            }
        } else if let Some(val) = line.strip_prefix("volume=") {
            let parts: Vec<&str> = val.splitn(3, ':').collect();
            if parts.len() == 3 {
                config.volumes.push(VolumeSpec {
                    tag: parts[0].to_string(),
                    guest_mount: parts[1].to_string(),
                    read_only: parts[2] == "ro",
                });
            }
        } else if let Some(val) = line.strip_prefix("block_mount=") {
            let parts: Vec<&str> = val.splitn(3, ':').collect();
            if parts.len() == 3 {
                config.block_mounts.push(BlockMount {
                    device: parts[0].to_string(),
                    guest_mount: parts[1].to_string(),
                    fstype: parts[2].to_string(),
                });
            }
        } else if let Some(val) = line.strip_prefix("install=") {
            config.packages.push(val.to_string());
        } else if let Some(val) = line.strip_prefix("interactive=") {
            config.interactive = val == "1";
        } else if let Some(val) = line.strip_prefix("start_agent=") {
            config.start_agent = val == "1";
        } else if let Some(val) = line.strip_prefix("mount_rootfs=") {
            config.mount_rootfs = val == "1";
        } else if let Some(val) = line.strip_prefix("signal_mode=") {
            config.signal_mode = match val {
                "shared_memory" => SignalMode::SharedMemory,
                _ => SignalMode::Serial,
            };
        } else if let Some(val) = line.strip_prefix("network=") {
            config.network_mode = match val {
                "none" => GuestNetworkMode::None,
                _ => GuestNetworkMode::Dhcp,
            };
        } else if let Some(val) = line.strip_prefix("manifest=") {
            config.manifest_path = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("manifest_url=") {
            if val.starts_with("https://") {
                config.manifest_url = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("manifest_hash=") {
            if !val.is_empty() {
                config.manifest_hash = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("cache_base=") {
            if !val.is_empty() {
                config.cache_base = val.to_string();
            }
        }
    }

    config
}
