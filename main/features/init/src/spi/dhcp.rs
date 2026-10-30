//! The guest's DHCP exchange: wait for the link, send a DISCOVER, take the OFFER, send a REQUEST,
//! take the ACK, and send again when no reply comes.
//!
//! RFC 2131 section 4.1: a client does not rely on one datagram. It retransmits with a
//! randomized exponential backoff, and it does not send before the link is up. A frame sent
//! while the interface has no carrier is dropped by the guest kernel, and `sendto` still
//! succeeds, so the first DISCOVER can be lost with nothing to show for it (vmruntime#553).
//!
//! Everything here runs against a [`DhcpLink`]; the `bin/` crate supplies the real one.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::api::dhcp::{
    CarrierState, CarrierWait, DhcpFailure, DhcpLease, DhcpLink, DhcpPolicy, DhcpReply, DHCP_ACK,
    DHCP_BOOTREQUEST, DHCP_DISCOVER, DHCP_MAX_REPLY, DHCP_NAK, DHCP_OFFER, DHCP_REQUEST,
};

const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const OPT_SUBNET_MASK: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_MSG_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_END: u8 = 255;

/// Offset of the options field: 236 bytes of fixed header and the 4-byte magic cookie.
const DHCP_OPTIONS_OFFSET: usize = 240;

/// How many times a NAK sends the exchange back to the DISCOVER before it gives up. The overall
/// limit bounds it in time; this bounds a server that refuses at once, every time.
const MAX_NAK_RESTARTS: u32 = 3;

/// Build a DHCP message of `msg_type` (DISCOVER or REQUEST) from a client with no address.
///
/// A REQUEST names the address wanted and the server it was offered by.
pub fn build_dhcp_packet(msg_type: u8, xid: u32, requested_ip: u32, server_id: u32) -> Vec<u8> {
    let mut pkt = vec![0u8; 300];
    pkt[0] = DHCP_BOOTREQUEST;
    pkt[1] = 1; // Ethernet
    pkt[2] = 6; // hardware address length
    pkt[4..8].copy_from_slice(&xid.to_be_bytes());
    pkt[236..DHCP_OPTIONS_OFFSET].copy_from_slice(&DHCP_MAGIC_COOKIE);
    let mut off = DHCP_OPTIONS_OFFSET;
    pkt[off] = OPT_MSG_TYPE;
    pkt[off + 1] = 1;
    pkt[off + 2] = msg_type;
    off += 3;
    if msg_type == DHCP_REQUEST {
        pkt[off] = OPT_REQUESTED_IP;
        pkt[off + 1] = 4;
        pkt[off + 2..off + 6].copy_from_slice(&requested_ip.to_be_bytes());
        off += 6;
        pkt[off] = OPT_SERVER_ID;
        pkt[off + 1] = 4;
        pkt[off + 2..off + 6].copy_from_slice(&server_id.to_be_bytes());
        off += 6;
    }
    pkt[off] = OPT_END;
    off += 1;
    pkt.truncate(off);
    pkt
}

/// Parse a server's reply, or `None` when it is shorter than the fixed header or lacks the
/// magic cookie.
pub fn parse_dhcp_response(data: &[u8]) -> Option<DhcpReply> {
    if data.len() < DHCP_OPTIONS_OFFSET || data[236..DHCP_OPTIONS_OFFSET] != DHCP_MAGIC_COOKIE {
        return None;
    }
    let transaction_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let offered_ip = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let mut message_type = 0u8;
    let (mut mask, mut gateway, mut dns, mut server_id) = (0u32, 0u32, 0u32, 0u32);
    let mut off = DHCP_OPTIONS_OFFSET;
    while off < data.len() {
        let opt = data[off];
        if opt == OPT_END {
            break;
        }
        if opt == 0 {
            off += 1;
            continue;
        }
        if off + 1 >= data.len() {
            break;
        }
        let len = data[off + 1] as usize;
        let val_start = off + 2;
        let val_end = val_start + len;
        if val_end > data.len() {
            break;
        }
        let four = |i: usize| u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        match opt {
            OPT_MSG_TYPE if len >= 1 => message_type = data[val_start],
            OPT_SUBNET_MASK if len >= 4 => mask = four(val_start),
            OPT_ROUTER if len >= 4 => gateway = four(val_start),
            OPT_DNS if len >= 4 => dns = four(val_start),
            OPT_SERVER_ID if len >= 4 => server_id = four(val_start),
            _ => {}
        }
        off = val_end;
    }
    Some(DhcpReply {
        message_type,
        transaction_id,
        lease: DhcpLease { ip: offered_ip, mask, gateway, dns, server_id },
    })
}

/// The wait after the `attempt`-th send (0 for the first): `first_timeout_ms` doubled each time,
/// capped at `max_timeout_ms`, then randomized by plus or minus `1 / jitter_divisor` of itself
/// with `random`.
pub fn compute_retransmit_timeout_ms(policy: &DhcpPolicy, attempt: u32, random: u32) -> u32 {
    let doubled = (policy.first_timeout_ms as u64) << attempt.min(16);
    let base = doubled.min(policy.max_timeout_ms as u64);
    if policy.jitter_divisor == 0 {
        return base as u32;
    }
    let span = base / policy.jitter_divisor as u64;
    (base - span + (random as u64) % (2 * span + 1)) as u32
}

fn elapsed_ms<L: DhcpLink>(link: &mut L, since: u64) -> u32 {
    link.now_ms().saturating_sub(since).min(u32::MAX as u64) as u32
}

/// Wait until the interface has a link, looking every `carrier_poll_ms`, for at most
/// `carrier_bound_ms`.
///
/// Returns at once when the state cannot be read, so a guest whose sysfs says nothing boots as
/// it did before this wait existed.
pub fn wait_for_carrier<L: DhcpLink>(link: &mut L, policy: &DhcpPolicy) -> CarrierWait {
    let start = link.now_ms();
    let mut found_down = false;
    loop {
        match link.carrier() {
            // Only a look that found the link down counts as waiting. Reading the state takes
            // time of its own (a millisecond or more in a loaded guest), and a boot that never
            // saw the link down must not be reported as one that waited.
            CarrierState::Up => {
                let waited_ms = if found_down { elapsed_ms(link, start) } else { 0 };
                return CarrierWait::Up { waited_ms };
            }
            CarrierState::Unknown => return CarrierWait::Unknown,
            CarrierState::Down => found_down = true,
        }
        let waited = elapsed_ms(link, start);
        if waited >= policy.carrier_bound_ms {
            return CarrierWait::TimedOut { waited_ms: waited };
        }
        link.sleep_ms(policy.carrier_poll_ms.min(policy.carrier_bound_ms - waited).max(1));
    }
}

fn log_carrier_wait<L: DhcpLink>(link: &mut L, policy: &DhcpPolicy) {
    match wait_for_carrier(link, policy) {
        CarrierWait::Up { waited_ms: 0 } | CarrierWait::Unknown => {}
        CarrierWait::Up { waited_ms } => {
            link.log(&format!("DHCP: the link came up after {} ms", waited_ms));
        }
        CarrierWait::TimedOut { waited_ms } => {
            link.log(&format!("DHCP: the link is still down after {} ms, trying anyway", waited_ms));
        }
    }
}

/// Send `packet` and wait for a reply to `xid` of one of the `accept`ed types, sending it again
/// with a growing wait until `overall_limit_ms` (counted from `start`) has passed. A reply that
/// is malformed, answers another transaction or has another type is ignored. `sent` counts
/// the sends.
fn await_reply<L: DhcpLink>(
    link: &mut L,
    policy: &DhcpPolicy,
    start: u64,
    packet: &[u8],
    xid: u32,
    accept: &[u8],
    name: &str,
    sent: &mut u32,
) -> Option<DhcpReply> {
    let mut buf = [0u8; DHCP_MAX_REPLY];
    let mut attempt = 0u32;
    loop {
        let elapsed = elapsed_ms(link, start);
        if elapsed >= policy.overall_limit_ms {
            return None;
        }
        if attempt > 0 {
            link.log(&format!("DHCP: no reply to the {} yet, sending it again", name));
        }
        *sent += 1;
        if let Err(errno) = link.send(packet) {
            link.log(&format!("DHCP: sending the {} failed (errno {})", name, errno));
        }
        let random = link.random_u32();
        let timeout = compute_retransmit_timeout_ms(policy, attempt, random)
            .min(policy.overall_limit_ms - elapsed)
            .max(1);
        let deadline = link.now_ms() + timeout as u64;
        loop {
            let now = link.now_ms();
            if now >= deadline {
                break;
            }
            let wait = (deadline - now).min(u32::MAX as u64) as u32;
            match link.receive(&mut buf, wait) {
                None => break,
                Some(n) => {
                    if let Some(reply) = parse_dhcp_response(&buf[..n.min(buf.len())]) {
                        if reply.transaction_id == xid && accept.contains(&reply.message_type) {
                            return Some(reply);
                        }
                    }
                }
            }
        }
        attempt += 1;
    }
}

/// Run the whole exchange and return the lease, or why there is none.
///
/// Waits for the link first (bounded), then DISCOVER and REQUEST, each sent again until
/// `overall_limit_ms` after the first DISCOVER. A NAK starts the exchange over with a new
/// transaction id, at most `MAX_NAK_RESTARTS` times.
pub fn acquire_lease<L: DhcpLink>(link: &mut L, policy: &DhcpPolicy) -> Result<DhcpLease, DhcpFailure> {
    log_carrier_wait(link, policy);
    let start = link.now_ms();
    let (mut discovers, mut requests, mut refusals) = (0u32, 0u32, 0u32);
    loop {
        let xid = link.random_u32();
        let discover = build_dhcp_packet(DHCP_DISCOVER, xid, 0, 0);
        let offer = match await_reply(link, policy, start, &discover, xid, &[DHCP_OFFER], "DISCOVER", &mut discovers) {
            Some(offer) => offer,
            None => return Err(DhcpFailure::NoOffer { discovers, elapsed_ms: elapsed_ms(link, start) }),
        };
        let request = build_dhcp_packet(DHCP_REQUEST, xid, offer.lease.ip, offer.lease.server_id);
        match await_reply(link, policy, start, &request, xid, &[DHCP_ACK, DHCP_NAK], "REQUEST", &mut requests) {
            Some(reply) if reply.message_type == DHCP_ACK => return Ok(reply.lease),
            Some(_) => {
                refusals += 1;
                if refusals > MAX_NAK_RESTARTS {
                    return Err(DhcpFailure::NoAck { requests, elapsed_ms: elapsed_ms(link, start) });
                }
                link.log("DHCP: the server refused the address (NAK), starting again");
            }
            None => return Err(DhcpFailure::NoAck { requests, elapsed_ms: elapsed_ms(link, start) }),
        }
    }
}
