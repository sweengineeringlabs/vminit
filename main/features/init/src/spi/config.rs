//! /etc/vminit.conf parser.
//!
//! Pure function — takes text, returns InitConfig. No filesystem access.
//! The bin/ crate calls ffi::read_file and passes the text here.
//!
//! Format:
//!   entrypoint=<arg>        (may appear multiple times — argv[0], argv[1], ...)
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

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::api::config::{BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec};

/// Parse /etc/vminit.conf text into an InitConfig.
/// Unknown keys are silently ignored. Missing file (empty string) returns default.
pub fn parse_config(text: &str) -> InitConfig {
    let mut config = InitConfig::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(val) = line.strip_prefix("entrypoint=") {
            config.entrypoint.push(val.to_string());
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
