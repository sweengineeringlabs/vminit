use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Whether vminit runs DHCP inside the guest at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestNetworkMode {
    /// Run a DHCP DISCOVER/OFFER/REQUEST/ACK exchange on eth0 (default).
    Dhcp,
    /// Skip DHCP entirely — no network configuration is performed.
    None,
}

impl Default for GuestNetworkMode {
    fn default() -> Self { Self::Dhcp }
}

/// Parsed volume mount specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpec {
    pub tag: String,
    pub guest_mount: String,
    pub read_only: bool,
}

/// A block device to auto-mount before exec.
///
/// Written to vminit.conf as `block_mount=device:mountpoint:fstype`.
/// Example: `block_mount=/dev/vdb:/data:ext4`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockMount {
    /// Block device path, e.g. `/dev/vdb`.
    pub device: String,
    /// Guest mount point, e.g. `/data`.
    pub guest_mount: String,
    /// Filesystem type, e.g. `ext4`.
    pub fstype: String,
}

/// Guest→host signaling strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalMode {
    /// Write XIKA_READY / XIKA_EXIT:N strings to /dev/ttyS0.
    Serial,
    /// Write a magic u32 to physical address 0x500 (shared memory page).
    SharedMemory,
}

/// Parsed /etc/vminit.conf configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitConfig {
    pub entrypoint: Vec<String>,
    pub env: Vec<(String, String)>,
    pub volumes: Vec<VolumeSpec>,
    pub block_mounts: Vec<BlockMount>,
    pub packages: Vec<String>,
    pub interactive: bool,
    pub start_agent: bool,
    pub mount_rootfs: bool,
    pub signal_mode: SignalMode,
    /// Path to the package manifest JSON inside the guest (e.g. `/etc/packages.json`).
    /// When set, enables network fallback for packages absent from the initrd.
    pub manifest_path: Option<String>,
    /// HTTPS URL of the package manifest to fetch at boot.
    /// Takes precedence over `manifest_path` when both are set.
    /// Only `https://` URLs are accepted; plain HTTP is rejected at parse time.
    pub manifest_url: Option<String>,
    /// Expected SHA-256 hash of the manifest in `sha256:<hex>` format.
    /// When set, vminit verifies the manifest after loading and aborts on mismatch.
    /// When absent, manifest is accepted with a warning log.
    pub manifest_hash: Option<String>,
    /// Nix binary cache base URL for network package installation.
    pub cache_base: String,
    /// Whether vminit should run DHCP at boot. Default: Dhcp.
    pub network_mode: GuestNetworkMode,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            entrypoint: Vec::new(),
            env: Vec::new(),
            volumes: Vec::new(),
            block_mounts: Vec::new(),
            packages: Vec::new(),
            interactive: false,
            start_agent: false,
            mount_rootfs: false,
            signal_mode: SignalMode::Serial,
            manifest_path: None,
            manifest_url: None,
            manifest_hash: None,
            cache_base: "https://cache.nixos.org".to_string(),
            network_mode: GuestNetworkMode::Dhcp,
        }
    }
}
