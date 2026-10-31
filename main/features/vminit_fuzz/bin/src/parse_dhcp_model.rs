#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};
use swe_vminit_init::parse_dhcp_response;

// Checks the VALUES `parse_dhcp_response` returns. The fuzzer's bytes choose a transaction id, an address and a list of
// options (known ones with values that are too short, long, or repeated; unknown ones; padding); the packet is written
// here from RFC 2131 (240-byte header, the magic cookie at 236, then options), and the expected reply is worked out
// by a small model: a later option of the same kind replaces an earlier one, and an option shorter than it needs is ignored.

const COOKIE: [u8; 4] = [99, 130, 83, 99];

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let xid: u32 = u.arbitrary().unwrap_or(0);
    let yiaddr: u32 = u.arbitrary().unwrap_or(0);

    let mut packet = vec![0u8; 240];
    packet[0] = 2; // BOOTREPLY
    packet[1] = 1;
    packet[2] = 6;
    packet[4..8].copy_from_slice(&xid.to_be_bytes());
    packet[16..20].copy_from_slice(&yiaddr.to_be_bytes());
    packet[236..240].copy_from_slice(&COOKIE);

    let (mut message_type, mut mask, mut gateway, mut dns, mut server) = (0u8, 0u32, 0u32, 0u32, 0u32);
    for _ in 0..u.int_in_range(0..=12).unwrap_or(0) {
        if u.arbitrary::<bool>().unwrap_or(false) {
            packet.push(0); // a pad byte between options
            continue;
        }
        let code = *u.choose(&[53u8, 1, 3, 6, 54, 51, 12, 77, 120]).unwrap_or(&120);
        let len = u.int_in_range(0..=9usize).unwrap_or(0);
        let value: Vec<u8> = (0..len).map(|_| u.arbitrary().unwrap_or(0)).collect();
        packet.push(code);
        packet.push(len as u8);
        packet.extend_from_slice(&value);
        match code {
            53 if len >= 1 => message_type = value[0],
            1 if len >= 4 => mask = be32(&value),
            3 if len >= 4 => gateway = be32(&value),
            6 if len >= 4 => dns = be32(&value),
            54 if len >= 4 => server = be32(&value),
            _ => {}
        }
    }
    packet.push(255); // end

    let reply = parse_dhcp_response(&packet).expect("a packet with the header and the magic cookie must parse");
    assert_eq!(reply.transaction_id, xid, "transaction id");
    assert_eq!(reply.lease.ip, yiaddr, "yiaddr");
    assert_eq!(reply.message_type, message_type, "message type");
    assert_eq!(reply.lease.mask, mask, "subnet mask");
    assert_eq!(reply.lease.gateway, gateway, "router");
    assert_eq!(reply.lease.dns, dns, "dns");
    assert_eq!(reply.lease.server_id, server, "server identifier");
});
