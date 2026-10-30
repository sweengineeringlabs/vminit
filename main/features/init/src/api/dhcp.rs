//! Types of the guest's DHCP exchange: the lease it yields, the timing it keeps, the ways it
//! fails, and the link it runs over.
//!
//! The exchange itself (`spi::dhcp`) touches nothing but a [`DhcpLink`], so it runs, and is
//! tested, natively. The `bin/` crate implements the link over a UDP socket and sysfs.

/// The most bytes of a reply the exchange reads (a DHCP message without a long options field).
pub const DHCP_MAX_REPLY: usize = 576;

/// `op` of a client message (BOOTREQUEST).
pub const DHCP_BOOTREQUEST: u8 = 1;
/// Option 53 value of a DISCOVER.
pub const DHCP_DISCOVER: u8 = 1;
/// Option 53 value of an OFFER.
pub const DHCP_OFFER: u8 = 2;
/// Option 53 value of a REQUEST.
pub const DHCP_REQUEST: u8 = 3;
/// Option 53 value of an ACK.
pub const DHCP_ACK: u8 = 5;
/// Option 53 value of a NAK.
pub const DHCP_NAK: u8 = 6;

/// The address and settings a server granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpLease {
    /// The address granted (`yiaddr`), as a big-endian-decoded `u32`.
    pub ip: u32,
    /// Subnet mask (option 1), 0 when the server sent none.
    pub mask: u32,
    /// Default router (option 3), 0 when the server sent none.
    pub gateway: u32,
    /// First DNS server (option 6), 0 when the server sent none.
    pub dns: u32,
    /// Server identifier (option 54), 0 when the server sent none.
    pub server_id: u32,
}

/// A parsed server reply: its message type, the transaction it answers, and what it grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpReply {
    /// Option 53 (0 when the reply carries none).
    pub message_type: u8,
    /// The `xid` field, which echoes the client's transaction id.
    pub transaction_id: u32,
    /// `yiaddr` and the options a lease is made of.
    pub lease: DhcpLease,
}

/// Whether the interface has a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierState {
    /// The link is up: a frame sent now can leave.
    Up,
    /// The link is down: the kernel drops what is sent, and `sendto` still succeeds.
    Down,
    /// The state cannot be read (no sysfs entry, or the interface is not up).
    Unknown,
}

/// How waiting for the link ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierWait {
    /// The link came up after `waited_ms` (0 when it was up at the first look).
    Up { waited_ms: u32 },
    /// The link was still down at the bound, after `waited_ms`.
    TimedOut { waited_ms: u32 },
    /// The state could not be read, so nothing was waited for.
    Unknown,
}

/// Why no lease was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpFailure {
    /// No OFFER arrived: `discovers` DISCOVERs were sent over `elapsed_ms`.
    NoOffer { discovers: u32, elapsed_ms: u32 },
    /// An OFFER arrived but no ACK followed: `requests` REQUESTs were sent over `elapsed_ms`.
    NoAck { requests: u32, elapsed_ms: u32 },
}

/// The timing the exchange keeps.
///
/// The defaults are chosen in `docs/xkvm_conf.md` ("DHCP timing"); they are not tuning knobs of
/// the host, because the guest reads no key for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpPolicy {
    /// The longest wait for the link before the first DISCOVER.
    pub carrier_bound_ms: u32,
    /// The pause between two looks at the link.
    pub carrier_poll_ms: u32,
    /// The wait for a reply after the first send.
    pub first_timeout_ms: u32,
    /// The longest wait for a reply after one send.
    pub max_timeout_ms: u32,
    /// The time after which the exchange gives up, counted from the first DISCOVER.
    pub overall_limit_ms: u32,
    /// Each wait is randomized by plus or minus `1 / jitter_divisor` of itself, so guests that
    /// start together do not retransmit together. 0 means no randomization.
    pub jitter_divisor: u32,
}

impl Default for DhcpPolicy {
    fn default() -> Self {
        Self {
            carrier_bound_ms: 2_000,
            carrier_poll_ms: 5,
            first_timeout_ms: 1_000,
            max_timeout_ms: 8_000,
            overall_limit_ms: 15_000,
            jitter_divisor: 8,
        }
    }
}

/// What the exchange needs from its surroundings: the link state, a clock, a pause, randomness,
/// a way to send and to receive, and a place to log. Implemented over a socket by the `bin/`
/// crate and by a scripted fake in the tests.
pub trait DhcpLink {
    /// Whether the interface has a link now.
    fn carrier(&mut self) -> CarrierState;
    /// Pause for `ms` milliseconds.
    fn sleep_ms(&mut self, ms: u32);
    /// A monotonic clock in milliseconds.
    fn now_ms(&mut self) -> u64;
    /// A random number, for the transaction id and the retransmit jitter.
    fn random_u32(&mut self) -> u32;
    /// Broadcast one DHCP message. `Err` carries the errno of a send that failed.
    fn send(&mut self, packet: &[u8]) -> Result<(), i32>;
    /// Wait up to `timeout_ms` for one datagram and return the number of bytes written to
    /// `buf`, or `None` when the wait ended with nothing received.
    fn receive(&mut self, buf: &mut [u8], timeout_ms: u32) -> Option<usize>;
    /// Write one line to the guest console.
    fn log(&mut self, message: &str);
}
