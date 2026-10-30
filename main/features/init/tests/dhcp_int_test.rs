//! The guest's DHCP exchange against a scripted link with a virtual clock.
//!
//! Each test names the failure it would catch. The one that matters most is the first group:
//! a DISCOVER sent while the link is down is dropped by the guest kernel with `sendto`
//! succeeding, which lost about 1 in 100 net guests (sweengineeringlabs/vmruntime#553).

use std::collections::VecDeque;

use swe_vminit_init::{
    acquire_lease, build_dhcp_packet, compute_retransmit_timeout_ms, parse_dhcp_response,
    wait_for_carrier, CarrierState, CarrierWait, DhcpFailure, DhcpLink, DhcpPolicy, DHCP_ACK,
    DHCP_DISCOVER, DHCP_NAK, DHCP_OFFER, DHCP_REQUEST,
};

const LEASE_IP: [u8; 4] = [10, 0, 2, 15];

/// What the scripted server does with one message the client sent: the replies it queues.
type Responder = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>>>;

struct FakeLink {
    now: u64,
    carrier_script: VecDeque<CarrierState>,
    last_carrier: CarrierState,
    carrier_read_ms: u64,
    sleeps: Vec<u32>,
    randoms: VecDeque<u32>,
    next_random: u32,
    /// (time of the send, packet)
    sent: Vec<(u64, Vec<u8>)>,
    send_errors: VecDeque<i32>,
    /// (time of arrival, datagram)
    inbox: Vec<(u64, Vec<u8>)>,
    responder: Responder,
    logs: Vec<String>,
}

impl FakeLink {
    fn new(responder: Responder) -> Self {
        Self {
            now: 1_000,
            carrier_script: VecDeque::new(),
            last_carrier: CarrierState::Up,
            carrier_read_ms: 0,
            sleeps: Vec::new(),
            randoms: VecDeque::new(),
            next_random: 0x1000_0000,
            sent: Vec::new(),
            send_errors: VecDeque::new(),
            inbox: Vec::new(),
            responder,
            logs: Vec::new(),
        }
    }

    fn with_carrier(mut self, script: &[CarrierState]) -> Self {
        self.carrier_script = script.iter().copied().collect();
        self
    }

    /// Every look at the link takes `ms` of the virtual clock, as a slow sysfs read does.
    fn with_slow_carrier_reads(mut self, ms: u64) -> Self {
        self.carrier_read_ms = ms;
        self
    }

    fn with_randoms(mut self, values: &[u32]) -> Self {
        self.randoms = values.iter().copied().collect();
        self
    }

    fn with_send_errors(mut self, errnos: &[i32]) -> Self {
        self.send_errors = errnos.iter().copied().collect();
        self
    }

    fn sends_of(&self, message_type: u8) -> Vec<&(u64, Vec<u8>)> {
        self.sent.iter().filter(|(_, p)| p[242] == message_type).collect()
    }
}

impl DhcpLink for FakeLink {
    fn carrier(&mut self) -> CarrierState {
        self.now += self.carrier_read_ms;
        if let Some(state) = self.carrier_script.pop_front() {
            self.last_carrier = state;
        }
        self.last_carrier
    }

    fn sleep_ms(&mut self, ms: u32) {
        self.sleeps.push(ms);
        self.now += ms as u64;
    }

    fn now_ms(&mut self) -> u64 {
        self.now
    }

    fn random_u32(&mut self) -> u32 {
        match self.randoms.pop_front() {
            Some(value) => value,
            None => {
                self.next_random = self.next_random.wrapping_add(0x0101_0101);
                self.next_random
            }
        }
    }

    fn send(&mut self, packet: &[u8]) -> Result<(), i32> {
        self.sent.push((self.now, packet.to_vec()));
        if let Some(errno) = self.send_errors.pop_front() {
            return Err(errno);
        }
        for reply in (self.responder)(packet) {
            self.inbox.push((self.now + 2, reply));
        }
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        let limit = self.now + timeout_ms as u64;
        let next = self
            .inbox
            .iter()
            .enumerate()
            .filter(|(_, (at, _))| *at <= limit)
            .min_by_key(|(_, (at, _))| *at)
            .map(|(i, _)| i);
        match next {
            Some(i) => {
                let (at, datagram) = self.inbox.remove(i);
                self.now = self.now.max(at);
                buf[..datagram.len()].copy_from_slice(&datagram);
                Some(datagram.len())
            }
            None => {
                self.now = limit;
                None
            }
        }
    }

    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }
}

/// A server reply of `message_type` to transaction `xid`, granting 10.0.2.15.
fn server_reply(message_type: u8, xid: u32) -> Vec<u8> {
    server_reply_granting(message_type, xid, LEASE_IP)
}

/// A server reply of `message_type` to transaction `xid`, granting `ip`.
fn server_reply_granting(message_type: u8, xid: u32, ip: [u8; 4]) -> Vec<u8> {
    let mut p = vec![0u8; 240];
    p[0] = 2; // BOOTREPLY
    p[4..8].copy_from_slice(&xid.to_be_bytes());
    p[16..20].copy_from_slice(&ip);
    p[236..240].copy_from_slice(&[99, 130, 83, 99]);
    p.extend_from_slice(&[53, 1, message_type]);
    p.extend_from_slice(&[1, 4, 255, 255, 255, 0]);
    p.extend_from_slice(&[3, 4, 10, 0, 2, 2]);
    p.extend_from_slice(&[6, 4, 10, 255, 255, 254]);
    p.extend_from_slice(&[54, 4, 10, 0, 2, 2]);
    p.push(255);
    p
}

fn xid_of(packet: &[u8]) -> u32 {
    u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]])
}

/// Answers a DISCOVER with an OFFER and a REQUEST with an ACK, at once.
fn well_behaved_server() -> Responder {
    Box::new(|packet| match packet[242] {
        DHCP_DISCOVER => vec![server_reply(DHCP_OFFER, xid_of(packet))],
        DHCP_REQUEST => vec![server_reply(DHCP_ACK, xid_of(packet))],
        _ => vec![],
    })
}

/// Ignores the first `lost` messages it is sent, then behaves.
fn server_that_loses_first(lost: usize) -> Responder {
    let mut seen = 0usize;
    let mut inner = well_behaved_server();
    Box::new(move |packet| {
        seen += 1;
        if seen <= lost {
            vec![]
        } else {
            inner(packet)
        }
    })
}

fn silent_server() -> Responder {
    Box::new(|_| vec![])
}

fn expected_lease() -> swe_vminit_init::DhcpLease {
    swe_vminit_init::DhcpLease {
        ip: u32::from_be_bytes(LEASE_IP),
        mask: 0xFFFF_FF00,
        gateway: 0x0A00_0202,
        dns: 0x0AFF_FFFE,
        server_id: 0x0A00_0202,
    }
}

// ── waiting for the link ──────────────────────────────────────────────────────────────────

#[test]
fn test_wait_for_carrier_up_at_first_look_returns_up_without_sleeping() {
    let mut link = FakeLink::new(silent_server());
    let result = wait_for_carrier(&mut link, &DhcpPolicy::default());
    assert_eq!(result, CarrierWait::Up { waited_ms: 0 });
    assert!(link.sleeps.is_empty(), "slept {:?} though the link was up", link.sleeps);
}

#[test]
fn test_wait_for_carrier_up_at_first_look_but_slow_to_read_is_not_a_wait() {
    // Reading sysfs took 3 ms and the link was up: nothing was waited for, and a normal boot
    // must not log a wait for it (real guests did, 67 of 220, before this was fixed).
    let mut link = FakeLink::new(silent_server()).with_slow_carrier_reads(3);
    assert_eq!(wait_for_carrier(&mut link, &DhcpPolicy::default()), CarrierWait::Up { waited_ms: 0 });
}

#[test]
fn test_acquire_lease_slow_sysfs_read_with_the_link_up_writes_nothing_to_the_console() {
    let mut link = FakeLink::new(well_behaved_server()).with_slow_carrier_reads(3);
    acquire_lease(&mut link, &DhcpPolicy::default()).expect("a lease");
    assert!(link.logs.is_empty(), "logged {:?} for a link that was up", link.logs);
}

#[test]
fn test_wait_for_carrier_up_after_three_polls_returns_the_time_waited() {
    let mut link = FakeLink::new(silent_server()).with_carrier(&[
        CarrierState::Down,
        CarrierState::Down,
        CarrierState::Down,
        CarrierState::Up,
    ]);
    let policy = DhcpPolicy { carrier_poll_ms: 5, ..DhcpPolicy::default() };
    assert_eq!(wait_for_carrier(&mut link, &policy), CarrierWait::Up { waited_ms: 15 });
    assert_eq!(link.sleeps, vec![5, 5, 5]);
}

#[test]
fn test_wait_for_carrier_never_up_returns_timed_out_at_the_bound() {
    let mut link = FakeLink::new(silent_server()).with_carrier(&[CarrierState::Down]);
    let policy = DhcpPolicy { carrier_bound_ms: 2_000, carrier_poll_ms: 5, ..DhcpPolicy::default() };
    assert_eq!(wait_for_carrier(&mut link, &policy), CarrierWait::TimedOut { waited_ms: 2_000 });
}

#[test]
fn test_wait_for_carrier_last_pause_is_cut_to_the_bound() {
    let mut link = FakeLink::new(silent_server()).with_carrier(&[CarrierState::Down]);
    let policy = DhcpPolicy { carrier_bound_ms: 12, carrier_poll_ms: 5, ..DhcpPolicy::default() };
    wait_for_carrier(&mut link, &policy);
    assert_eq!(link.sleeps, vec![5, 5, 2], "the pauses must add up to the bound, not past it");
}

#[test]
fn test_wait_for_carrier_unreadable_returns_unknown_without_sleeping() {
    let mut link = FakeLink::new(silent_server()).with_carrier(&[CarrierState::Unknown]);
    assert_eq!(wait_for_carrier(&mut link, &DhcpPolicy::default()), CarrierWait::Unknown);
    assert!(link.sleeps.is_empty());
}

// ── the retransmit schedule ───────────────────────────────────────────────────────────────

#[test]
fn test_compute_retransmit_timeout_without_jitter_doubles_then_stays_at_the_cap() {
    let policy = DhcpPolicy { first_timeout_ms: 1_000, max_timeout_ms: 8_000, jitter_divisor: 0, ..DhcpPolicy::default() };
    let waits: Vec<u32> = (0..6).map(|attempt| compute_retransmit_timeout_ms(&policy, attempt, 12345)).collect();
    assert_eq!(waits, vec![1_000, 2_000, 4_000, 8_000, 8_000, 8_000]);
}

#[test]
fn test_compute_retransmit_timeout_with_jitter_stays_within_one_eighth_of_the_base() {
    let policy = DhcpPolicy::default(); // first 1000 ms, jitter 1/8
    let mut seen_low = u32::MAX;
    let mut seen_high = 0;
    for random in 0..2_000u32 {
        let wait = compute_retransmit_timeout_ms(&policy, 0, random);
        assert!((875..=1_125).contains(&wait), "{} ms is outside 1000 ms plus or minus 1/8", wait);
        seen_low = seen_low.min(wait);
        seen_high = seen_high.max(wait);
    }
    assert!(seen_low < 900 && seen_high > 1_100, "the jitter never spread: {}..{}", seen_low, seen_high);
}

#[test]
fn test_compute_retransmit_timeout_for_a_huge_attempt_number_does_not_overflow() {
    let policy = DhcpPolicy { jitter_divisor: 0, ..DhcpPolicy::default() };
    assert_eq!(compute_retransmit_timeout_ms(&policy, u32::MAX, 0), policy.max_timeout_ms);
}

// ── the messages ──────────────────────────────────────────────────────────────────────────

#[test]
fn test_build_dhcp_packet_discover_carries_the_transaction_id_and_type() {
    let packet = build_dhcp_packet(DHCP_DISCOVER, 0xDEAD_BEEF, 0, 0);
    assert_eq!(packet[0], 1, "BOOTREQUEST");
    assert_eq!(xid_of(&packet), 0xDEAD_BEEF);
    assert_eq!(&packet[236..240], &[99, 130, 83, 99]);
    assert_eq!(&packet[240..243], &[53, 1, DHCP_DISCOVER]);
    assert_eq!(*packet.last().unwrap(), 255, "options end");
}

#[test]
fn test_build_dhcp_packet_request_names_the_address_and_the_server() {
    let packet = build_dhcp_packet(DHCP_REQUEST, 7, 0x0A00_020F, 0x0A00_0202);
    assert_eq!(&packet[243..249], &[50, 4, 10, 0, 2, 15], "requested address");
    assert_eq!(&packet[249..255], &[54, 4, 10, 0, 2, 2], "server identifier");
}

#[test]
fn test_parse_dhcp_response_offer_returns_the_lease_and_the_transaction() {
    let reply = parse_dhcp_response(&server_reply(DHCP_OFFER, 0xABCD_0001)).expect("a valid OFFER parses");
    assert_eq!(reply.message_type, DHCP_OFFER);
    assert_eq!(reply.transaction_id, 0xABCD_0001);
    assert_eq!(reply.lease, expected_lease());
}

#[test]
fn test_parse_dhcp_response_shorter_than_the_header_returns_none() {
    assert_eq!(parse_dhcp_response(&server_reply(DHCP_OFFER, 1)[..239]), None);
}

#[test]
fn test_parse_dhcp_response_without_the_magic_cookie_returns_none() {
    let mut datagram = server_reply(DHCP_OFFER, 1);
    datagram[236] = 0;
    assert_eq!(parse_dhcp_response(&datagram), None);
}

#[test]
fn test_parse_dhcp_response_option_running_past_the_end_does_not_panic() {
    let mut datagram = server_reply(DHCP_OFFER, 1);
    datagram.truncate(242); // the type option has its length byte but no value
    let reply = parse_dhcp_response(&datagram).expect("the fixed header is intact");
    assert_eq!(reply.message_type, 0, "the cut option must not be read");
}

// ── the exchange ──────────────────────────────────────────────────────────────────────────

#[test]
fn test_acquire_lease_server_answers_at_once_returns_the_lease_with_one_discover_and_one_request() {
    let mut link = FakeLink::new(well_behaved_server());
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("a lease");
    assert_eq!(lease, expected_lease());
    assert_eq!(link.sends_of(DHCP_DISCOVER).len(), 1);
    assert_eq!(link.sends_of(DHCP_REQUEST).len(), 1);
}

#[test]
fn test_acquire_lease_healthy_exchange_writes_nothing_to_the_console() {
    // Each console byte is a VM exit; a normal boot must not pay for log lines.
    let mut link = FakeLink::new(well_behaved_server());
    acquire_lease(&mut link, &DhcpPolicy::default()).expect("a lease");
    assert!(link.logs.is_empty(), "logged {:?} on the normal path", link.logs);
}

#[test]
fn test_acquire_lease_link_down_at_first_sends_nothing_until_the_link_is_up() {
    // The #553 failure: the link comes up a few milliseconds after the interface does.
    let mut link = FakeLink::new(well_behaved_server()).with_carrier(&[
        CarrierState::Down,
        CarrierState::Down,
        CarrierState::Down,
        CarrierState::Up,
    ]);
    let started = link.now;
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("a lease");
    assert_eq!(lease, expected_lease());
    let first_send = link.sent[0].0;
    assert!(first_send >= started + 15, "the DISCOVER left at +{} ms, before the link was up", first_send - started);
    assert_eq!(link.sends_of(DHCP_DISCOVER).len(), 1, "no retransmit was needed");
}

#[test]
fn test_acquire_lease_link_that_came_up_late_logs_the_wait_once() {
    let mut link = FakeLink::new(well_behaved_server()).with_carrier(&[CarrierState::Down, CarrierState::Up]);
    acquire_lease(&mut link, &DhcpPolicy::default()).expect("a lease");
    assert_eq!(link.logs, vec!["DHCP: the link came up after 5 ms".to_string()]);
}

#[test]
fn test_acquire_lease_link_still_down_at_the_bound_logs_it_and_tries_anyway() {
    let mut link = FakeLink::new(well_behaved_server()).with_carrier(&[CarrierState::Down]);
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("the exchange is tried regardless");
    assert_eq!(lease, expected_lease());
    assert!(link.logs[0].contains("still down after 2000 ms"), "{:?}", link.logs);
}

#[test]
fn test_acquire_lease_first_discover_lost_is_sent_again_and_binds() {
    let mut link = FakeLink::new(server_that_loses_first(1));
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("the second DISCOVER is answered");
    assert_eq!(lease, expected_lease());
    let discovers = link.sends_of(DHCP_DISCOVER);
    assert_eq!(discovers.len(), 2);
    let gap = discovers[1].0 - discovers[0].0;
    assert!((875..=1_125).contains(&gap), "retransmitted after {} ms, not about 1000", gap);
    assert_eq!(xid_of(&discovers[0].1), xid_of(&discovers[1].1), "a retransmit keeps the transaction id");
}

#[test]
fn test_acquire_lease_first_request_lost_is_sent_again_and_binds() {
    // Messages seen: DISCOVER (answered), REQUEST (lost), REQUEST again.
    let mut seen = 0;
    let mut inner = well_behaved_server();
    let responder: Responder = Box::new(move |packet| {
        seen += 1;
        if seen == 2 { vec![] } else { inner(packet) }
    });
    let mut link = FakeLink::new(responder);
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("the second REQUEST is answered");
    assert_eq!(lease, expected_lease());
    assert_eq!(link.sends_of(DHCP_REQUEST).len(), 2);
}

#[test]
fn test_acquire_lease_silent_server_gives_up_at_the_overall_limit_with_no_offer() {
    let mut link = FakeLink::new(silent_server());
    let policy = DhcpPolicy::default();
    let started = link.now;
    let failure = acquire_lease(&mut link, &policy).expect_err("nobody answers");
    match failure {
        DhcpFailure::NoOffer { discovers, elapsed_ms } => {
            assert!(discovers >= 4, "only {} DISCOVERs in {} ms", discovers, elapsed_ms);
            assert_eq!(elapsed_ms, policy.overall_limit_ms, "gave up at {} ms", elapsed_ms);
        }
        other => panic!("expected NoOffer, got {:?}", other),
    }
    assert_eq!(link.now - started, policy.overall_limit_ms as u64, "the exchange ran past its limit");
}

#[test]
fn test_acquire_lease_silent_server_spaces_the_discovers_with_a_growing_wait() {
    let mut link = FakeLink::new(silent_server());
    let policy = DhcpPolicy { jitter_divisor: 0, ..DhcpPolicy::default() };
    let _ = acquire_lease(&mut link, &policy);
    let times: Vec<u64> = link.sends_of(DHCP_DISCOVER).iter().map(|(t, _)| *t).collect();
    let gaps: Vec<u64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(gaps, vec![1_000, 2_000, 4_000], "the waits must double (the last one is cut by the limit)");
}

#[test]
fn test_acquire_lease_offer_without_ack_gives_up_with_no_ack() {
    let responder: Responder = Box::new(|packet| {
        if packet[242] == DHCP_DISCOVER { vec![server_reply(DHCP_OFFER, xid_of(packet))] } else { vec![] }
    });
    let mut link = FakeLink::new(responder);
    match acquire_lease(&mut link, &DhcpPolicy::default()).expect_err("no ACK ever comes") {
        DhcpFailure::NoAck { requests, .. } => assert!(requests >= 2, "only {} REQUEST(s)", requests),
        other => panic!("expected NoAck, got {:?}", other),
    }
}

#[test]
fn test_acquire_lease_reply_to_another_transaction_is_ignored() {
    // A stray OFFER for another transaction (xid 0x99, granting 10.0.9.9) arrives before the real one.
    let mut inner = well_behaved_server();
    let responder: Responder = Box::new(move |packet| {
        let mut replies = Vec::new();
        if packet[242] == DHCP_DISCOVER {
            replies.push(server_reply_granting(DHCP_OFFER, 0x99, [10, 0, 9, 9]));
        }
        replies.extend(inner(packet));
        replies
    });
    let mut link = FakeLink::new(responder);
    acquire_lease(&mut link, &DhcpPolicy::default()).expect("the real OFFER is taken");
    let request = &link.sends_of(DHCP_REQUEST)[0].1;
    assert_eq!(&request[243..249], &[50, 4, 10, 0, 2, 15], "the REQUEST named the address of the stray OFFER");
}

#[test]
fn test_acquire_lease_ack_in_answer_to_a_discover_is_ignored() {
    let responder: Responder = Box::new(|packet| match packet[242] {
        DHCP_DISCOVER => vec![server_reply(DHCP_ACK, xid_of(packet)), server_reply(DHCP_OFFER, xid_of(packet))],
        _ => vec![server_reply(DHCP_ACK, xid_of(packet))],
    });
    let mut link = FakeLink::new(responder);
    acquire_lease(&mut link, &DhcpPolicy::default()).expect("the OFFER is taken, the ACK is not");
    assert_eq!(link.sends_of(DHCP_REQUEST).len(), 1, "the early ACK must not end the exchange");
}

#[test]
fn test_acquire_lease_nak_starts_over_with_a_new_transaction_id() {
    let mut requests_seen = 0;
    let responder: Responder = Box::new(move |packet| match packet[242] {
        DHCP_DISCOVER => vec![server_reply(DHCP_OFFER, xid_of(packet))],
        _ => {
            requests_seen += 1;
            let kind = if requests_seen == 1 { DHCP_NAK } else { DHCP_ACK };
            vec![server_reply(kind, xid_of(packet))]
        }
    });
    let mut link = FakeLink::new(responder).with_randoms(&[0xAAAA_0001]);
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("the second round succeeds");
    assert_eq!(lease, expected_lease());
    let discovers = link.sends_of(DHCP_DISCOVER);
    assert_eq!(discovers.len(), 2);
    assert_ne!(xid_of(&discovers[0].1), xid_of(&discovers[1].1), "a new exchange needs a new transaction id");
    assert!(link.logs.iter().any(|l| l.contains("NAK")), "{:?}", link.logs);
}

#[test]
fn test_acquire_lease_server_that_always_refuses_stops_after_the_restart_limit() {
    let responder: Responder = Box::new(|packet| match packet[242] {
        DHCP_DISCOVER => vec![server_reply(DHCP_OFFER, xid_of(packet))],
        _ => vec![server_reply(DHCP_NAK, xid_of(packet))],
    });
    let mut link = FakeLink::new(responder);
    let failure = acquire_lease(&mut link, &DhcpPolicy::default()).expect_err("refused every time");
    assert!(matches!(failure, DhcpFailure::NoAck { .. }), "{:?}", failure);
    assert_eq!(link.sends_of(DHCP_DISCOVER).len(), 4, "one round plus three restarts, then stop");
}

#[test]
fn test_acquire_lease_send_that_fails_is_logged_with_the_errno_and_tried_again() {
    let mut link = FakeLink::new(well_behaved_server()).with_send_errors(&[101]);
    let lease = acquire_lease(&mut link, &DhcpPolicy::default()).expect("the retransmit goes through");
    assert_eq!(lease, expected_lease());
    assert!(link.logs.iter().any(|l| l.contains("errno 101")), "{:?}", link.logs);
    assert_eq!(link.sends_of(DHCP_DISCOVER).len(), 2);
}

#[test]
fn test_acquire_lease_each_exchange_draws_its_own_transaction_id() {
    let mut first = FakeLink::new(well_behaved_server()).with_randoms(&[0x1111_1111]);
    let mut second = FakeLink::new(well_behaved_server()).with_randoms(&[0x2222_2222]);
    acquire_lease(&mut first, &DhcpPolicy::default()).unwrap();
    acquire_lease(&mut second, &DhcpPolicy::default()).unwrap();
    assert_eq!(xid_of(&first.sent[0].1), 0x1111_1111);
    assert_eq!(xid_of(&second.sent[0].1), 0x2222_2222);
}

#[test]
fn test_acquire_lease_malformed_datagram_is_ignored_and_the_exchange_goes_on() {
    let mut inner = well_behaved_server();
    let responder: Responder = Box::new(move |packet| {
        let mut replies = vec![vec![1, 2, 3]];
        replies.extend(inner(packet));
        replies
    });
    let mut link = FakeLink::new(responder);
    assert_eq!(acquire_lease(&mut link, &DhcpPolicy::default()).unwrap(), expected_lease());
}
