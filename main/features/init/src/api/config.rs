/// Parsed volume mount specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpec {
    pub tag: String,
    pub guest_mount: String,
    pub read_only: bool,
}

/// Guest→host signaling strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalMode {
    /// Write XIKA_READY / XIKA_EXIT:N strings to /dev/ttyS0.
    Serial,
    /// Write a magic u32 to physical address 0x500 (shared memory page).
    SharedMemory,
}

/// Parsed /etc/xkvm.conf configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitConfig {
    pub entrypoint: Vec<String>,
    pub env: Vec<(String, String)>,
    pub volumes: Vec<VolumeSpec>,
    pub packages: Vec<String>,
    pub interactive: bool,
    pub start_agent: bool,
    pub kali_mode: bool,
    pub signal_mode: SignalMode,
    /// Path to the package manifest JSON inside the guest (e.g. `/etc/packages.json`).
    /// When set, enables network fallback for packages absent from the initrd.
    pub manifest_path: Option<String>,
    /// HTTPS URL of the package manifest to fetch at boot.
    /// Takes precedence over `manifest_path` when both are set.
    /// Only `https://` URLs are accepted; plain HTTP is rejected at parse time.
    pub manifest_url: Option<String>,
    /// Nix binary cache base URL for network package installation.
    pub cache_base: String,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            entrypoint: Vec::new(),
            env: Vec::new(),
            volumes: Vec::new(),
            packages: Vec::new(),
            interactive: false,
            start_agent: false,
            kali_mode: false,
            signal_mode: SignalMode::Serial,
            manifest_path: None,
            manifest_url: None,
            cache_base: "https://cache.nixos.org".to_string(),
        }
    }
}
