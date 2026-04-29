pub use crate::api::config::{InitConfig, SignalMode, VolumeSpec};
pub use crate::api::error::VmInitError;
pub use crate::api::overlay::{OverlayEntry, OverlayManifest, ParseError};
pub use crate::spi::config::parse_config;
pub use crate::spi::signal::{
    format_exit, format_ready, parse_exit, shm_exit_value, EXIT_PREFIX, READY_SIGNAL,
    SHM_MAGIC_EXIT_MASK, SHM_MAGIC_READY, SHM_SIGNAL_ADDR,
};
