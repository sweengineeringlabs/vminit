//! The DHCP exchange checks (vminit#19, vmruntime fuzz epic #574).
//!
//! `acquire_lease` runs the guest's whole DHCP exchange against a `DhcpLink`: wait for the link, send a DISCOVER, take the OFFER,
//! send a REQUEST, take the ACK, send again when nothing comes, and start over after a NAK. The parsers it calls have their own
//! targets (`parse_dhcp_response`, `parse_dhcp_model`); this one checks the exchange. The link is a scripted fake with a virtual
//! clock: a pause or a wait advances the clock, a datagram the server sends is delivered at the time the script says, and the
//! server's answer to the n-th message the client sends is chosen by the fuzzer's bytes (an OFFER, an ACK, a NAK, a reply to
//! another transaction, bytes that are not a reply, nothing, each after a delay from 0 to 3 seconds).
//!
//! The model is written from RFC 2131 (4.1 retransmission with a randomized exponential backoff, 4.3.1 and 4.4.1 the messages
//! and their options, 4.4.2 and 4.4.5 the REQUEST in the SELECTING state) and from the policy the code states. After the run it
//! checks, from what the client sent and what the fake delivered:
//! - the first message is a DISCOVER; each REQUEST answers an OFFER of its own transaction, and carries that OFFER's address
//!   (option 50) and server identifier (option 54); a DISCOVER carries neither;
//! - the wait before each retransmission lies within the randomized backoff of the policy;
//! - an `Ok` lease is the lease of an ACK to the REQUEST, and is a usable address (not 0.0.0.0);
//! - `NoOffer` and `NoAck` report the sends that were made, only after the overall limit or (for `NoAck`) after more NAKs than
//!   the restart limit allows, and never while an acceptable reply had been delivered;
//! - the number of sends stays bounded.
//!
//! Not modelled: the wait for the link (`wait_for_carrier` has its own tests), the text of the log lines, the value of the
//! transaction id (the fake's random numbers may repeat), and options other than 50, 53 and 54 of the client's messages.

use libfuzzer_sys::arbitrary::Unstructured;
use swe_vminit_init::{
    acquire_lease, CarrierState, DhcpFailure, DhcpLease, DhcpLink, DhcpPolicy, DHCP_ACK,
    DHCP_DISCOVER, DHCP_NAK, DHCP_OFFER, DHCP_REQUEST,
};

const COOKIE: [u8; 4] = [99, 130, 83, 99];
const HEADER_LEN: usize = 240;
const OPT_MASK: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_MESSAGE_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_END: u8 = 255;
/// The most NAKs a client puts up with: a refusal beyond this many gives up (the init's `MAX_NAK_RESTARTS`, 3, plus the one that ends it).
const NAKS_THAT_END_THE_EXCHANGE: usize = 4;
/// More sends than any exchange within the policies below can make; reaching it means the exchange does not stop.
const RUNAWAY_SENDS: usize = 400;
/// Where the fake's clock starts (a monotonic clock does not start at 0).
const CLOCK_START_MS: u64 = 1_000_000;
const SCRIPT_LEN: usize = 10;

/// Where the fuzzer's bytes run out, a seeded generator takes over: the choices of a run must not turn to zeros once a short input is
/// used up (the first version of the x86 target never wrote junk because of that).
struct Choices<'a, 'b> {
    input: &'a mut Unstructured<'b>,
    state: u64,
}

impl<'a, 'b> Choices<'a, 'b> {
    fn new(input: &'a mut Unstructured<'b>) -> Self {
        let state = input.arbitrary().unwrap_or(0x5EED);
        Self { input, state }
    }

    fn byte(&mut self) -> u8 {
        if let Ok(byte) = self.input.arbitrary::<u8>() {
            return byte;
        }
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u8
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.byte(), self.byte(), self.byte(), self.byte()])
    }

    fn below(&mut self, limit: u32) -> u32 {
        u32::from(self.byte()) % limit.max(1)
    }
}

/// What the server does about one message of the client.
#[derive(Debug, Clone)]
enum Reply {
    Offer {
        lease: DhcpLease,
        other_transaction: bool,
    },
    Ack {
        lease: DhcpLease,
        other_transaction: bool,
    },
    Nak {
        other_transaction: bool,
    },
    NotAReply {
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone)]
struct Scripted {
    reply: Reply,
    delay_ms: u32,
}

/// A lease that grants a usable address.
fn choose_lease_with_address(c: &mut Choices) -> DhcpLease {
    DhcpLease {
        ip: c.u32() | 0x0100_0000,
        ..choose_lease(c)
    }
}

fn choose_lease(c: &mut Choices) -> DhcpLease {
    // an address of 0 is rare but present: a server (or an attacker) can send it
    let ip = if c.below(10) == 0 {
        0
    } else {
        c.u32() | 0x0100_0000
    };
    DhcpLease {
        ip,
        mask: if c.below(4) == 0 { 0 } else { 0xFFFF_FF00 },
        gateway: if c.below(4) == 0 { 0 } else { c.u32() },
        dns: if c.below(4) == 0 { 0 } else { c.u32() },
        server_id: if c.below(6) == 0 { 0 } else { c.u32() },
    }
}

fn choose_reply(c: &mut Choices, answering_a_discover: bool) -> Option<Scripted> {
    let delay_ms = match c.below(6) {
        0 => 3_000, // later than the first wait: the client has sent again by then
        1 => 900 + c.below(300),
        _ => c.below(200),
    };
    let other_transaction = c.below(8) == 0;
    // the reply that fits the message is likely, so that exchanges get far; anything else is likely enough to be met
    let kind = c.below(20);
    let reply = match (answering_a_discover, kind) {
        (_, 0..=2) => return None, // nothing: the message or the reply is lost
        (true, 3..=12) => Reply::Offer {
            lease: choose_lease(c),
            other_transaction,
        },
        (false, 3..=10) => Reply::Ack {
            lease: choose_lease(c),
            other_transaction,
        },
        (false, 11..=13) => Reply::Nak { other_transaction },
        (_, 14) => Reply::Offer {
            lease: choose_lease(c),
            other_transaction,
        },
        (_, 15) => Reply::Ack {
            lease: choose_lease(c),
            other_transaction,
        },
        (_, 16) => Reply::Nak { other_transaction },
        (_, _) => {
            let mut bytes: Vec<u8> = (0..c.below(300)).map(|_| c.byte()).collect();
            if bytes.len() > 239 {
                bytes[236] = 0; // not the magic cookie: these bytes are not a DHCP reply
            }
            Reply::NotAReply { bytes }
        }
    };
    Some(Scripted { reply, delay_ms })
}

/// A server that answers every DISCOVER with an OFFER and every REQUEST with a NAK, at once: the exchange has to give up after the restart
/// limit, not go on until the clock runs out (the generic script makes four NAKs in a row about once in 2,000 runs).
fn refusing_script(c: &mut Choices) -> Vec<Option<Scripted>> {
    (0..SCRIPT_LEN)
        .map(|i| {
            let delay_ms = c.below(50);
            let reply = if i % 2 == 0 {
                Reply::Offer {
                    lease: choose_lease_with_address(c),
                    other_transaction: false,
                }
            } else {
                Reply::Nak {
                    other_transaction: false,
                }
            };
            Some(Scripted { reply, delay_ms })
        })
        .collect()
}

fn choose_policy(c: &mut Choices) -> DhcpPolicy {
    match c.below(4) {
        0 => DhcpPolicy::default(),
        1 => DhcpPolicy {
            carrier_bound_ms: 100,
            carrier_poll_ms: 5,
            first_timeout_ms: 100,
            max_timeout_ms: 800,
            overall_limit_ms: 3_000,
            jitter_divisor: 8,
        },
        2 => DhcpPolicy {
            carrier_bound_ms: 100,
            carrier_poll_ms: 5,
            first_timeout_ms: 500,
            max_timeout_ms: 2_000,
            overall_limit_ms: 7_000,
            jitter_divisor: 0,
        },
        _ => DhcpPolicy {
            carrier_bound_ms: 0,
            carrier_poll_ms: 1,
            first_timeout_ms: 250,
            max_timeout_ms: 1_000,
            overall_limit_ms: 5_000,
            jitter_divisor: 4,
        },
    }
}

/// A message the client sent, read back from its bytes by a reader of RFC 2131's layout that is not the init's.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sent {
    at: u64,
    /// The place of the event among all sends and deliveries: virtual time does not move between a send and the reply, so `at` cannot
    /// say which came first.
    seq: usize,
    bytes: Vec<u8>,
    message_type: u8,
    xid: u32,
    requested_ip: Option<u32>,
    server_id: Option<u32>,
}

fn read_sent(at: u64, seq: usize, bytes: &[u8]) -> Result<Sent, String> {
    if bytes.len() < HEADER_LEN || bytes[0] != 1 || bytes[236..240] != COOKIE {
        return Err(format!(
            "the client sent {} bytes that are not a BOOTREQUEST with the cookie",
            bytes.len()
        ));
    }
    let xid = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let (mut message_type, mut requested_ip, mut server_id) = (0u8, None, None);
    let mut at_option = HEADER_LEN;
    while at_option < bytes.len() && bytes[at_option] != OPT_END {
        let code = bytes[at_option];
        let len = usize::from(
            *bytes
                .get(at_option + 1)
                .ok_or("an option without its length")?,
        );
        let value = bytes
            .get(at_option + 2..at_option + 2 + len)
            .ok_or("an option longer than the message")?;
        match (code, len) {
            (OPT_MESSAGE_TYPE, 1) => message_type = value[0],
            (OPT_REQUESTED_IP, 4) => {
                requested_ip = Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
            }
            (OPT_SERVER_ID, 4) => {
                server_id = Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
            }
            _ => {}
        }
        at_option += 2 + len;
    }
    Ok(Sent {
        at,
        seq,
        bytes: bytes.to_vec(),
        message_type,
        xid,
        requested_ip,
        server_id,
    })
}

/// A datagram of the server, in the form the client reads.
fn encode_reply(message_type: u8, xid: u32, lease: &DhcpLease) -> Vec<u8> {
    let mut bytes = vec![0u8; HEADER_LEN];
    bytes[0] = 2; // BOOTREPLY
    bytes[1] = 1;
    bytes[2] = 6;
    bytes[4..8].copy_from_slice(&xid.to_be_bytes());
    bytes[16..20].copy_from_slice(&lease.ip.to_be_bytes());
    bytes[236..240].copy_from_slice(&COOKIE);
    bytes.extend_from_slice(&[OPT_MESSAGE_TYPE, 1, message_type]);
    for (code, value) in [
        (OPT_MASK, lease.mask),
        (OPT_ROUTER, lease.gateway),
        (OPT_DNS, lease.dns),
        (OPT_SERVER_ID, lease.server_id),
    ] {
        if value != 0 {
            bytes.extend_from_slice(&[code, 4]);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    bytes.push(OPT_END);
    bytes
}

/// A reply the fake handed to the client.
#[derive(Debug, Clone)]
struct Delivered {
    at: u64,
    seq: usize,
    xid: u32,
    message_type: u8,
    lease: DhcpLease,
}

struct PendingDatagram {
    arrives_at: u64,
    bytes: Vec<u8>,
    meta: Option<(u32, u8, DhcpLease)>,
}

struct ScriptedLink {
    now: u64,
    script: Vec<Option<Scripted>>,
    pending: Vec<PendingDatagram>,
    sent: Vec<Sent>,
    delivered: Vec<Delivered>,
    random_state: u64,
    carrier_down_looks: u32,
    format_problems: Vec<String>,
}

impl ScriptedLink {
    fn next_random(&mut self) -> u32 {
        self.random_state = self.random_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.random_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
}

impl DhcpLink for ScriptedLink {
    fn carrier(&mut self) -> CarrierState {
        if self.carrier_down_looks > 0 {
            self.carrier_down_looks -= 1;
            CarrierState::Down
        } else {
            CarrierState::Up
        }
    }

    fn sleep_ms(&mut self, ms: u32) {
        self.now += u64::from(ms);
    }

    fn now_ms(&mut self) -> u64 {
        self.now
    }

    fn random_u32(&mut self) -> u32 {
        self.next_random()
    }

    fn send(&mut self, packet: &[u8]) -> Result<(), i32> {
        assert!(
            self.sent.len() < RUNAWAY_SENDS,
            "the exchange sent {RUNAWAY_SENDS} messages and has not stopped"
        );
        let index = self.sent.len();
        match read_sent(self.now, self.sent.len() + self.delivered.len(), packet) {
            Ok(sent) => {
                let answer = self.script.get(index).cloned().flatten();
                if let Some(Scripted { reply, delay_ms }) = answer {
                    let xid_of = |other: bool| {
                        if other {
                            sent.xid ^ 0x5555_5555
                        } else {
                            sent.xid
                        }
                    };
                    let (bytes, meta) = match reply {
                        Reply::Offer {
                            lease,
                            other_transaction,
                        } => {
                            let xid = xid_of(other_transaction);
                            (
                                encode_reply(DHCP_OFFER, xid, &lease),
                                Some((xid, DHCP_OFFER, lease)),
                            )
                        }
                        Reply::Ack {
                            lease,
                            other_transaction,
                        } => {
                            let xid = xid_of(other_transaction);
                            (
                                encode_reply(DHCP_ACK, xid, &lease),
                                Some((xid, DHCP_ACK, lease)),
                            )
                        }
                        Reply::Nak { other_transaction } => {
                            let xid = xid_of(other_transaction);
                            let lease = DhcpLease {
                                ip: 0,
                                mask: 0,
                                gateway: 0,
                                dns: 0,
                                server_id: 0,
                            };
                            (
                                encode_reply(DHCP_NAK, xid, &lease),
                                Some((xid, DHCP_NAK, lease)),
                            )
                        }
                        Reply::NotAReply { bytes } => (bytes, None),
                    };
                    self.pending.push(PendingDatagram {
                        arrives_at: self.now + u64::from(delay_ms),
                        bytes,
                        meta,
                    });
                }
                self.sent.push(sent);
            }
            Err(problem) => {
                self.format_problems.push(problem);
                self.sent.push(Sent {
                    at: self.now,
                    seq: self.sent.len() + self.delivered.len(),
                    bytes: packet.to_vec(),
                    message_type: 0,
                    xid: 0,
                    requested_ip: None,
                    server_id: None,
                });
            }
        }
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        let until = self.now + u64::from(timeout_ms);
        let next = (0..self.pending.len())
            .filter(|i| self.pending[*i].arrives_at <= until)
            .min_by_key(|i| self.pending[*i].arrives_at);
        match next {
            Some(i) => {
                let datagram = self.pending.remove(i);
                self.now = self.now.max(datagram.arrives_at);
                let n = datagram.bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&datagram.bytes[..n]);
                if let Some((xid, message_type, lease)) = datagram.meta {
                    self.delivered.push(Delivered {
                        at: self.now,
                        seq: self.sent.len() + self.delivered.len(),
                        xid,
                        message_type,
                        lease,
                    });
                }
                Some(n)
            }
            None => {
                self.now = until;
                None
            }
        }
    }

    fn log(&mut self, _message: &str) {}
}

/// Whether the client may take `reply` as a `message_type`: it is one, and it grants an address (an OFFER or an ACK with `yiaddr` 0 is
/// malformed and is ignored, vminit#22; RFC 2131 4.3.1).
fn acceptable(reply: &Delivered, message_type: u8) -> bool {
    reply.message_type == message_type && reply.lease.ip != 0
}

/// Every way the run differs from the model.
fn check(
    link: &ScriptedLink,
    policy: &DhcpPolicy,
    start: u64,
    outcome: &Result<DhcpLease, DhcpFailure>,
) -> Vec<String> {
    let mut problems: Vec<String> = link.format_problems.clone();
    let sent = &link.sent;
    // the first message is a DISCOVER
    match sent.first() {
        Some(first) if first.message_type != DHCP_DISCOVER => problems.push(format!(
            "the first message is of type {}, not a DISCOVER",
            first.message_type
        )),
        None => problems.push("no message was sent".into()),
        _ => {}
    }
    for (index, message) in sent.iter().enumerate() {
        match message.message_type {
            DHCP_DISCOVER => {
                if message.requested_ip.is_some() || message.server_id.is_some() {
                    problems.push(format!("send {index}: a DISCOVER carries option 50 or 54"));
                }
            }
            DHCP_REQUEST => {
                // it answers the first OFFER of its transaction that had arrived before it was sent
                let offer = link.delivered.iter().find(|d| {
                    acceptable(d, DHCP_OFFER) && d.xid == message.xid && d.seq < message.seq
                });
                match offer {
                    None => problems.push(format!("send {index}: a REQUEST of transaction {:#x} with no OFFER of that transaction delivered before it", message.xid)),
                    Some(offer) => {
                        if message.requested_ip != Some(offer.lease.ip) {
                            problems.push(format!("send {index}: the REQUEST asks for {:x?}, the OFFER gave {:#x}", message.requested_ip, offer.lease.ip));
                        }
                        if message.server_id != Some(offer.lease.server_id) {
                            problems.push(format!("send {index}: the REQUEST names server {:x?}, the OFFER came from {:#x}", message.server_id, offer.lease.server_id));
                        }
                    }
                }
            }
            other => problems.push(format!(
                "send {index}: a message of type {other} (only DISCOVER and REQUEST are sent)"
            )),
        }
    }
    // the wait before each retransmission is within the randomized backoff (RFC 2131 4.1)
    let mut attempt = 0u32;
    for pair in sent.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.bytes == b.bytes {
            let doubled = u64::from(policy.first_timeout_ms) << attempt.min(16);
            let base = doubled.min(u64::from(policy.max_timeout_ms));
            let span = if policy.jitter_divisor == 0 {
                0
            } else {
                base / u64::from(policy.jitter_divisor)
            };
            let gap = b.at - a.at;
            if gap < base - span || gap > base + span {
                problems.push(format!(
                    "retransmission {attempt}: waited {gap} ms, the backoff allows {} to {} ms",
                    base - span,
                    base + span
                ));
            }
            attempt += 1;
        } else {
            attempt = 0;
        }
    }
    // a NAK to the REQUEST in progress sends the exchange back to a DISCOVER (RFC 2131 4.4.1) or ends it; it is not waited out
    for nak in link.delivered.iter().filter(|d| d.message_type == DHCP_NAK) {
        // it answers the REQUEST only if that is the last thing the client sent: a NAK of a transaction the client has already left (it
        // started again with a new DISCOVER) is ignored, and the next message after it is not about it
        let request = sent
            .iter()
            .rev()
            .find(|s| s.seq < nak.seq)
            .filter(|s| s.message_type == DHCP_REQUEST);
        let answers_it = request.is_some_and(|r| {
            r.xid == nak.xid
                && !link.delivered.iter().any(|d| {
                    acceptable(d, DHCP_ACK) && d.xid == r.xid && d.seq > r.seq && d.seq < nak.seq
                })
        });
        if answers_it {
            if let Some(next) = sent.iter().find(|s| s.seq > nak.seq) {
                if next.message_type != DHCP_DISCOVER {
                    problems.push(format!("a NAK of transaction {:#x} was delivered at {} and the next message is of type {}, not a DISCOVER", nak.xid, nak.at, next.message_type));
                }
            }
        }
    }
    // it gives up at the fourth NAK that answers the REQUEST in progress (the init's restart limit, 3, and the refusal that ends it)
    let accepted_naks = link
        .delivered
        .iter()
        .filter(|nak| {
            nak.message_type == DHCP_NAK
                && sent
                    .iter()
                    .rev()
                    .find(|s| s.seq < nak.seq)
                    .is_some_and(|last| last.message_type == DHCP_REQUEST && last.xid == nak.xid)
        })
        .count();
    if accepted_naks > NAKS_THAT_END_THE_EXCHANGE {
        problems.push(format!(
            "{accepted_naks} NAKs answered a REQUEST in progress; the exchange must give up at the {NAKS_THAT_END_THE_EXCHANGE}th"
        ));
    }
    // the outcome
    let elapsed = link.now - start;
    let discovers = sent
        .iter()
        .filter(|s| s.message_type == DHCP_DISCOVER)
        .count() as u32;
    let requests = sent
        .iter()
        .filter(|s| s.message_type == DHCP_REQUEST)
        .count() as u32;
    match outcome {
        Ok(lease) => {
            let last_request = sent.iter().rposition(|s| s.message_type == DHCP_REQUEST);
            let first_answer = last_request.and_then(|r| {
                link.delivered.iter().find(|d| {
                    (acceptable(d, DHCP_ACK) || d.message_type == DHCP_NAK)
                        && d.xid == sent[r].xid
                        && d.seq > sent[r].seq
                })
            });
            match first_answer {
                Some(answer) if answer.message_type == DHCP_ACK => {
                    if answer.lease != *lease {
                        problems.push(format!("the lease is {lease:x?}, the ACK granted {:x?}", answer.lease));
                    }
                }
                other => problems.push(format!("the exchange returned a lease, but the first reply to its last REQUEST is {other:x?}")),
            }
            if lease.ip == 0 {
                problems.push(format!("the lease is the address 0.0.0.0 (RFC 2131 4.3.1: yiaddr is the address granted): {lease:x?}"));
            }
        }
        Err(DhcpFailure::NoOffer {
            discovers: reported,
            elapsed_ms,
        }) => {
            if *reported != discovers {
                problems.push(format!(
                    "NoOffer says {reported} DISCOVERs were sent, {discovers} were"
                ));
            }
            if u64::from(*elapsed_ms) < u64::from(policy.overall_limit_ms)
                || u64::from(*elapsed_ms) != elapsed
            {
                problems.push(format!(
                    "NoOffer after {elapsed_ms} ms (the clock says {elapsed}), the limit is {} ms",
                    policy.overall_limit_ms
                ));
            }
            if let Some(last) = sent.iter().rev().find(|s| s.message_type == DHCP_DISCOVER) {
                if let Some(offer) = link
                    .delivered
                    .iter()
                    .find(|d| acceptable(d, DHCP_OFFER) && d.xid == last.xid && d.seq > last.seq)
                {
                    problems.push(format!("NoOffer, but an OFFER of the last DISCOVER's transaction was delivered: {offer:x?}"));
                }
            }
        }
        Err(DhcpFailure::NoAck {
            requests: reported,
            elapsed_ms,
        }) => {
            if *reported != requests {
                problems.push(format!(
                    "NoAck says {reported} REQUESTs were sent, {requests} were"
                ));
            }
            if u64::from(*elapsed_ms) != elapsed {
                problems.push(format!(
                    "NoAck after {elapsed_ms} ms, the clock says {elapsed}"
                ));
            }
            let naks = link
                .delivered
                .iter()
                .filter(|d| {
                    d.message_type == DHCP_NAK
                        && sent.iter().any(|s| {
                            s.message_type == DHCP_REQUEST && s.xid == d.xid && s.seq < d.seq
                        })
                })
                .count();
            if u64::from(*elapsed_ms) < u64::from(policy.overall_limit_ms)
                && naks < NAKS_THAT_END_THE_EXCHANGE
            {
                problems.push(format!(
                    "NoAck after {elapsed_ms} ms, before the {} ms limit, with only {naks} NAKs",
                    policy.overall_limit_ms
                ));
            }
        }
    }
    problems
}

/// The policy and the scripted link a run uses, chosen from the fuzzer's bytes.
fn build_link(c: &mut Choices) -> (ScriptedLink, DhcpPolicy) {
    let policy = choose_policy(c);
    let script: Vec<Option<Scripted>> = if c.below(8) == 0 {
        refusing_script(c)
    } else {
        (0..SCRIPT_LEN)
            .map(|i| choose_reply(c, i % 2 == 0))
            .collect()
    };
    let link = ScriptedLink {
        now: CLOCK_START_MS,
        script,
        pending: Vec::new(),
        sent: Vec::new(),
        delivered: Vec::new(),
        random_state: u64::from(c.u32()) << 32 | u64::from(c.u32()),
        carrier_down_looks: c.below(4),
        format_problems: Vec::new(),
    };
    (link, policy)
}

/// Run one exchange. With `check_outcome`, return every way it differs from the model; without it, only drive it (the control).
pub fn run(u: &mut Unstructured, check_outcome: bool) -> Vec<String> {
    let mut c = Choices::new(u);
    let (mut link, policy) = build_link(&mut c);
    let outcome = acquire_lease(&mut link, &policy);
    if !check_outcome {
        return Vec::new();
    }
    let start = link.sent.first().map(|s| s.at).unwrap_or(link.now);
    let mut problems = check(&link, &policy, start, &outcome);
    if !problems.is_empty() {
        problems.push(format!(
            "outcome {outcome:x?}, policy {policy:?}
{}",
            trace(&link, start)
        ));
    }
    problems
}

/// What was sent and delivered, in the order it happened and with times relative to the first message, for the report of a problem.
fn trace(link: &ScriptedLink, start: u64) -> String {
    let mut lines: Vec<(usize, String)> = Vec::new();
    for s in &link.sent {
        lines.push((
            s.seq,
            format!(
                "{:>6} ms  client sends type {} xid {:#x} ip {:x?} server {:x?}",
                s.at - start,
                s.message_type,
                s.xid,
                s.requested_ip,
                s.server_id
            ),
        ));
    }
    for d in &link.delivered {
        lines.push((
            d.seq,
            format!(
                "{:>6} ms  client receives type {} xid {:#x} {:x?}",
                d.at - start,
                d.message_type,
                d.xid,
                d.lease
            ),
        ));
    }
    lines.sort_by_key(|(seq, _)| *seq);
    lines
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(ip: u32) -> DhcpLease {
        DhcpLease {
            ip,
            mask: 0xFFFF_FF00,
            gateway: 0x0A00_0201,
            dns: 0x0A00_0202,
            server_id: 0x0A00_0201,
        }
    }

    fn link_with(script: Vec<Option<Scripted>>) -> ScriptedLink {
        ScriptedLink {
            now: CLOCK_START_MS,
            script,
            pending: Vec::new(),
            sent: Vec::new(),
            delivered: Vec::new(),
            random_state: 7,
            carrier_down_looks: 0,
            format_problems: Vec::new(),
        }
    }

    fn scripted(reply: Reply, delay_ms: u32) -> Option<Scripted> {
        Some(Scripted { reply, delay_ms })
    }

    #[test]
    fn test_read_sent_reads_what_build_dhcp_packet_wrote_by_the_rfc_layout() {
        let discover = swe_vminit_init::build_dhcp_packet(DHCP_DISCOVER, 0xAABBCCDD, 0, 0);
        let sent = read_sent(5, 0, &discover).unwrap();
        assert_eq!(
            (
                sent.message_type,
                sent.xid,
                sent.requested_ip,
                sent.server_id
            ),
            (DHCP_DISCOVER, 0xAABBCCDD, None, None)
        );
        let request = swe_vminit_init::build_dhcp_packet(DHCP_REQUEST, 1, 0x0A00_020F, 0x0A00_0201);
        let sent = read_sent(6, 1, &request).unwrap();
        assert_eq!(
            (sent.message_type, sent.requested_ip, sent.server_id),
            (DHCP_REQUEST, Some(0x0A00_020F), Some(0x0A00_0201))
        );
        assert!(
            read_sent(0, 0, &discover[..100]).is_err(),
            "a message shorter than the header is not a message"
        );
    }

    #[test]
    fn test_a_clean_exchange_gives_the_lease_of_the_ack_and_the_model_finds_nothing() {
        let granted = lease(0x0A00_020F);
        let mut link = link_with(vec![
            scripted(
                Reply::Offer {
                    lease: lease(0x0A00_020F),
                    other_transaction: false,
                },
                5,
            ),
            scripted(
                Reply::Ack {
                    lease: granted,
                    other_transaction: false,
                },
                5,
            ),
        ]);
        let policy = DhcpPolicy::default();
        let outcome = acquire_lease(&mut link, &policy);
        assert_eq!(outcome, Ok(granted));
        let start = link.sent[0].at;
        assert_eq!(check(&link, &policy, start, &outcome), Vec::<String>::new());
    }

    #[test]
    fn test_a_reply_to_another_transaction_is_not_taken_and_the_exchange_ends_without_an_offer() {
        let mut link = link_with(vec![scripted(
            Reply::Offer {
                lease: lease(0x0A00_020F),
                other_transaction: true,
            },
            5,
        )]);
        let policy = DhcpPolicy {
            overall_limit_ms: 3_000,
            first_timeout_ms: 500,
            max_timeout_ms: 1_000,
            jitter_divisor: 0,
            ..DhcpPolicy::default()
        };
        let outcome = acquire_lease(&mut link, &policy);
        assert!(
            matches!(outcome, Err(DhcpFailure::NoOffer { .. })),
            "{outcome:?}"
        );
        assert_eq!(
            check(&link, &policy, link.sent[0].at, &outcome),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_an_offer_and_an_ack_of_the_address_zero_end_the_exchange_without_a_lease() {
        let mut link = link_with(vec![
            scripted(
                Reply::Offer {
                    lease: lease(0),
                    other_transaction: false,
                },
                5,
            ),
            scripted(
                Reply::Ack {
                    lease: lease(0),
                    other_transaction: false,
                },
                5,
            ),
        ]);
        let policy = DhcpPolicy {
            overall_limit_ms: 3_000,
            first_timeout_ms: 500,
            max_timeout_ms: 1_000,
            jitter_divisor: 0,
            ..DhcpPolicy::default()
        };
        let outcome = acquire_lease(&mut link, &policy);
        assert!(
            matches!(outcome, Err(DhcpFailure::NoOffer { .. })),
            "{outcome:?}"
        );
        assert_eq!(
            check(&link, &policy, link.sent[0].at, &outcome),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_every_discover_answered_with_an_offer_of_the_address_zero_is_a_clean_no_offer_for_the_model(
    ) {
        // the model must not count an OFFER the init rightly ignores as one it should have taken (it did: the first baseline of the fixed init)
        let script = (0..8)
            .map(|_| {
                scripted(
                    Reply::Offer {
                        lease: lease(0),
                        other_transaction: false,
                    },
                    5,
                )
            })
            .collect();
        let mut link = link_with(script);
        let policy = DhcpPolicy {
            overall_limit_ms: 3_000,
            first_timeout_ms: 500,
            max_timeout_ms: 1_000,
            jitter_divisor: 0,
            ..DhcpPolicy::default()
        };
        let outcome = acquire_lease(&mut link, &policy);
        assert!(
            matches!(outcome, Err(DhcpFailure::NoOffer { .. })),
            "{outcome:?}"
        );
        assert!(
            link.delivered
                .iter()
                .filter(|d| d.message_type == DHCP_OFFER)
                .count()
                >= 2,
            "zero-address OFFERs were delivered to more than one DISCOVER"
        );
        assert_eq!(
            check(&link, &policy, link.sent[0].at, &outcome),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_the_model_reports_a_lease_of_the_address_zero() {
        // a clean exchange, checked against an outcome the init must not return
        let mut link = link_with(vec![
            scripted(
                Reply::Offer {
                    lease: lease(0x0A00_020F),
                    other_transaction: false,
                },
                5,
            ),
            scripted(
                Reply::Ack {
                    lease: lease(0x0A00_020F),
                    other_transaction: false,
                },
                5,
            ),
        ]);
        let policy = DhcpPolicy::default();
        assert!(acquire_lease(&mut link, &policy).is_ok());
        let problems = check(&link, &policy, link.sent[0].at, &Ok(lease(0)));
        assert!(
            problems.iter().any(|p| p.contains("0.0.0.0")),
            "{problems:?}"
        );
    }

    /// A link holding a made-up history: the messages sent and the replies delivered, in the order they are given by `seq`.
    fn link_with_history(sent: Vec<Sent>, delivered: Vec<Delivered>, now: u64) -> ScriptedLink {
        let mut link = link_with(Vec::new());
        link.sent = sent;
        link.delivered = delivered;
        link.now = now;
        link
    }

    fn sent_at(at: u64, seq: usize, message_type: u8, xid: u32, granted: &DhcpLease) -> Sent {
        let packet =
            swe_vminit_init::build_dhcp_packet(message_type, xid, granted.ip, granted.server_id);
        read_sent(at, seq, &packet).unwrap()
    }

    fn delivered_at(
        at: u64,
        seq: usize,
        message_type: u8,
        xid: u32,
        granted: DhcpLease,
    ) -> Delivered {
        Delivered {
            at,
            seq,
            xid,
            message_type,
            lease: granted,
        }
    }

    #[test]
    fn test_the_model_reports_a_nak_that_was_waited_out_instead_of_restarting_the_exchange() {
        let granted = lease(0x0A00_020F);
        let link = link_with_history(
            vec![
                sent_at(0, 0, DHCP_DISCOVER, 5, &granted),
                sent_at(10, 2, DHCP_REQUEST, 5, &granted),
                sent_at(1_010, 4, DHCP_REQUEST, 5, &granted), // the NAK (seq 3) was waited out: the REQUEST goes out again
            ],
            vec![
                delivered_at(5, 1, DHCP_OFFER, 5, granted),
                delivered_at(20, 3, DHCP_NAK, 5, lease(0)),
            ],
            3_000,
        );
        let policy = DhcpPolicy::default();
        let problems = check(
            &link,
            &policy,
            0,
            &Err(DhcpFailure::NoAck {
                requests: 2,
                elapsed_ms: 3_000,
            }),
        );
        assert!(
            problems.iter().any(|p| p.contains("not a DISCOVER")),
            "{problems:?}"
        );
    }

    #[test]
    fn test_the_model_does_not_take_a_request_sent_in_the_same_millisecond_before_a_nak_for_the_message_after_it(
    ) {
        // virtual time stands still between a send and the reply: everything here is at 5 ms, and only `seq` says the order
        let granted = lease(0x0A00_020F);
        let link = link_with_history(
            vec![
                sent_at(5, 0, DHCP_DISCOVER, 5, &granted),
                sent_at(5, 2, DHCP_REQUEST, 5, &granted),
                sent_at(5, 4, DHCP_DISCOVER, 6, &granted), // the restart after the NAK (seq 3)
            ],
            vec![
                delivered_at(5, 1, DHCP_OFFER, 5, granted),
                delivered_at(5, 3, DHCP_NAK, 5, lease(0)),
            ],
            5,
        );
        let policy = DhcpPolicy::default();
        let problems = check(
            &link,
            &policy,
            5,
            &Err(DhcpFailure::NoOffer {
                discovers: 2,
                elapsed_ms: 0,
            }),
        );
        assert!(
            !problems.iter().any(|p| p.contains("not a DISCOVER")),
            "{problems:?}"
        );
    }

    #[test]
    fn test_the_model_does_not_expect_a_reaction_to_a_nak_of_a_transaction_the_client_has_left() {
        // the trace of a fuzzer's input: a NAK of transaction 5 arrives after the client restarted with transaction 6 and is rightly ignored
        let granted = lease(0x0A00_020F);
        let link = link_with_history(
            vec![
                sent_at(0, 0, DHCP_DISCOVER, 5, &granted),
                sent_at(10, 2, DHCP_REQUEST, 5, &granted),
                sent_at(20, 4, DHCP_DISCOVER, 6, &granted), // the restart after the first NAK (seq 3)
                sent_at(30, 7, DHCP_DISCOVER, 6, &granted),
            ],
            vec![
                delivered_at(5, 1, DHCP_OFFER, 5, granted),
                delivered_at(15, 3, DHCP_NAK, 5, lease(0)),
                delivered_at(25, 5, DHCP_NAK, 5, lease(0)), // stale: the client is on transaction 6
                delivered_at(26, 6, DHCP_NAK, 5, lease(0)),
            ],
            3_000,
        );
        let policy = DhcpPolicy::default();
        let problems = check(
            &link,
            &policy,
            0,
            &Err(DhcpFailure::NoOffer {
                discovers: 3,
                elapsed_ms: 3_000,
            }),
        );
        assert!(
            !problems.iter().any(|p| p.contains("not a DISCOVER")),
            "{problems:?}"
        );
    }

    #[test]
    fn test_the_model_reports_an_exchange_that_goes_on_after_the_fourth_refusal() {
        let granted = lease(0x0A00_020F);
        let mut sent = Vec::new();
        let mut delivered = Vec::new();
        // five rounds: DISCOVER, OFFER, REQUEST, NAK, each of them answering the REQUEST in progress
        for round in 0..5usize {
            let xid = 10 + round as u32;
            sent.push(sent_at(
                round as u64 * 10,
                round * 4,
                DHCP_DISCOVER,
                xid,
                &granted,
            ));
            delivered.push(delivered_at(
                round as u64 * 10,
                round * 4 + 1,
                DHCP_OFFER,
                xid,
                granted,
            ));
            sent.push(sent_at(
                round as u64 * 10,
                round * 4 + 2,
                DHCP_REQUEST,
                xid,
                &granted,
            ));
            delivered.push(delivered_at(
                round as u64 * 10,
                round * 4 + 3,
                DHCP_NAK,
                xid,
                lease(0),
            ));
        }
        let link = link_with_history(sent, delivered, 100);
        let policy = DhcpPolicy::default();
        let problems = check(
            &link,
            &policy,
            0,
            &Err(DhcpFailure::NoAck {
                requests: 5,
                elapsed_ms: 100,
            }),
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("must give up at the 4th")),
            "{problems:?}"
        );
    }

    #[test]
    fn test_a_server_that_refuses_every_request_ends_the_exchange_at_the_fourth_refusal() {
        let script = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    scripted(
                        Reply::Offer {
                            lease: lease(0x0A00_020F),
                            other_transaction: false,
                        },
                        5,
                    )
                } else {
                    scripted(
                        Reply::Nak {
                            other_transaction: false,
                        },
                        5,
                    )
                }
            })
            .collect();
        let mut link = link_with(script);
        let policy = DhcpPolicy::default();
        let outcome = acquire_lease(&mut link, &policy);
        assert!(
            matches!(outcome, Err(DhcpFailure::NoAck { requests: 4, .. })),
            "{outcome:?}"
        );
        assert_eq!(
            check(&link, &policy, link.sent[0].at, &outcome),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_run_reaches_every_outcome_and_every_kind_of_reply_with_inputs_as_short_as_a_fuzzers() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        };
        let rounds = 1_500;
        let (
            mut ok,
            mut no_offer,
            mut no_ack,
            mut retransmitted,
            mut other_transaction_delivered,
            mut zero_address_granted,
            mut nak_delivered,
        ) = (0, 0, 0, 0, 0, 0, 0);
        for _ in 0..rounds {
            let length = 16 + usize::from(next_byte());
            let seed: Vec<u8> = (0..length).map(|_| next_byte()).collect();
            let mut u = Unstructured::new(&seed);
            let mut c = Choices::new(&mut u);
            let (mut link, policy) = build_link(&mut c);
            let outcome = acquire_lease(&mut link, &policy);
            match outcome {
                Ok(_) => ok += 1,
                Err(DhcpFailure::NoOffer { .. }) => no_offer += 1,
                Err(DhcpFailure::NoAck { .. }) => no_ack += 1,
            }
            zero_address_granted += usize::from(link.delivered.iter().any(|d| {
                (d.message_type == DHCP_OFFER || d.message_type == DHCP_ACK) && d.lease.ip == 0
            }));
            retransmitted += usize::from(link.sent.windows(2).any(|w| w[0].bytes == w[1].bytes));
            other_transaction_delivered += usize::from(
                link.delivered
                    .iter()
                    .any(|d| link.sent.iter().all(|s| s.xid != d.xid)),
            );
            nak_delivered += usize::from(link.delivered.iter().any(|d| d.message_type == DHCP_NAK));
        }
        for (name, count) in [
            ("a lease obtained", ok),
            ("no OFFER", no_offer),
            ("no ACK", no_ack),
            ("a message sent again", retransmitted),
            (
                "a reply to another transaction delivered",
                other_transaction_delivered,
            ),
            (
                "an OFFER or ACK that grants the address 0 delivered",
                zero_address_granted,
            ),
            ("a NAK delivered", nak_delivered),
        ] {
            assert!(
                count * 50 >= rounds,
                "{name}: {count} of {rounds} inputs (below 2%)"
            );
        }
    }

    #[test]
    fn test_run_without_checking_never_panics_on_an_empty_input() {
        assert!(run(&mut Unstructured::new(&[]), false).is_empty());
    }
}
