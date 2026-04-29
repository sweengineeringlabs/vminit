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
        }
    }
}
