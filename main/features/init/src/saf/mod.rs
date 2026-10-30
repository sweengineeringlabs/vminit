pub use crate::api::config::{
    BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec, MAX_DNS_SERVERS,
};
pub use crate::api::error::VmInitError;
pub use crate::api::overlay::{OverlayEntry, OverlayManifest, ParseError};
pub use crate::spi::config::{format_resolv_conf, parse_config};
pub use crate::spi::signal::{
    format_exit, format_ready, parse_exit, shm_exit_value, EXIT_PREFIX, READY_SIGNAL,
    SHM_MAGIC_EXIT_MASK, SHM_MAGIC_READY, SHM_SIGNAL_ADDR,
};
pub use crate::api::marker::{
    Marker, MarkerError, CAPABILITIES, MARKER_FORMAT_VERSION, MARKER_NOTE_NAME, MARKER_NOTE_TYPE,
    MARKER_SECTION,
};
pub use crate::spi::marker::{encode_marker, format_banner, format_own_banner, marker_len, read_marker};
pub use crate::api::dhcp::{
    CarrierState, CarrierWait, DhcpFailure, DhcpLease, DhcpLink, DhcpPolicy, DhcpReply, DHCP_ACK,
    DHCP_BOOTREQUEST, DHCP_DISCOVER, DHCP_MAX_REPLY, DHCP_NAK, DHCP_OFFER, DHCP_REQUEST,
};
pub use crate::spi::dhcp::{
    acquire_lease, build_dhcp_packet, compute_retransmit_timeout_ms, parse_dhcp_response,
    wait_for_carrier,
};
