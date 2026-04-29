//! DHCP client and network configuration.
//!
//! Minimal DHCP DISCOVER/OFFER/REQUEST/ACK exchange via raw UDP socket.
//! Configures the network interface and adds a default route via netlink.

use crate::ffi;
use crate::serial;

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER: u8 = 2;
const DHCP_REQUEST: u8 = 3;
const DHCP_ACK: u8 = 5;
const BOOTP_REQUEST: u8 = 1;
const OPT_SUBNET_MASK: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_MSG_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_END: u8 = 255;

struct DhcpLease { ip: u32, mask: u32, gateway: u32, dns: u32, server_id: u32 }

fn build_dhcp_packet(msg_type: u8, xid: u32, requested_ip: u32, server_id: u32) -> Vec<u8> {
    let mut pkt = vec![0u8; 300];
    pkt[0] = BOOTP_REQUEST; pkt[1] = 1; pkt[2] = 6;
    pkt[4..8].copy_from_slice(&xid.to_be_bytes());
    pkt[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);
    let mut off = 240;
    pkt[off] = OPT_MSG_TYPE; pkt[off + 1] = 1; pkt[off + 2] = msg_type; off += 3;
    if msg_type == DHCP_REQUEST {
        pkt[off] = 50; pkt[off + 1] = 4;
        pkt[off + 2..off + 6].copy_from_slice(&requested_ip.to_be_bytes()); off += 6;
        pkt[off] = OPT_SERVER_ID; pkt[off + 1] = 4;
        pkt[off + 2..off + 6].copy_from_slice(&server_id.to_be_bytes()); off += 6;
    }
    pkt[off] = OPT_END; off += 1;
    pkt.truncate(off);
    pkt
}

fn parse_dhcp_response(data: &[u8]) -> Option<(u8, DhcpLease)> {
    if data.len() < 240 || data[236..240] != DHCP_MAGIC_COOKIE { return None; }
    let offered_ip = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let mut msg_type = 0u8;
    let mut mask = 0u32; let mut gateway = 0u32; let mut dns = 0u32; let mut server_id = 0u32;
    let mut off = 240;
    while off < data.len() {
        let opt = data[off];
        if opt == OPT_END { break; }
        if opt == 0 { off += 1; continue; }
        if off + 1 >= data.len() { break; }
        let len = data[off + 1] as usize;
        let val_start = off + 2;
        let val_end = val_start + len;
        if val_end > data.len() { break; }
        match opt {
            OPT_MSG_TYPE if len >= 1 => msg_type = data[val_start],
            OPT_SUBNET_MASK if len >= 4 => mask = u32::from_be_bytes([data[val_start], data[val_start+1], data[val_start+2], data[val_start+3]]),
            OPT_ROUTER if len >= 4 => gateway = u32::from_be_bytes([data[val_start], data[val_start+1], data[val_start+2], data[val_start+3]]),
            OPT_DNS if len >= 4 => dns = u32::from_be_bytes([data[val_start], data[val_start+1], data[val_start+2], data[val_start+3]]),
            OPT_SERVER_ID if len >= 4 => server_id = u32::from_be_bytes([data[val_start], data[val_start+1], data[val_start+2], data[val_start+3]]),
            _ => {}
        }
        off = val_end;
    }
    Some((msg_type, DhcpLease { ip: offered_ip, mask, gateway, dns, server_id }))
}

pub fn dhcp_configure(ifname: &str) {
    let sysfs_path = format!("/sys/class/net/{}", ifname);
    if !std::path::Path::new(&sysfs_path).exists() { return; }
    bring_interface_up(ifname);
    let sock = unsafe { ffi::socket(ffi::AF_INET, ffi::SOCK_DGRAM, ffi::IPPROTO_UDP) };
    if sock < 0 { serial::log("DHCP: failed to create socket"); return; }
    let one: i32 = 1;
    unsafe {
        ffi::setsockopt(sock, ffi::SOL_SOCKET, ffi::SO_BROADCAST, &one as *const i32 as *const u8, 4);
    }
    let ifname_bytes = format!("{}\0", ifname);
    unsafe {
        ffi::setsockopt(sock, ffi::SOL_SOCKET, ffi::SO_BINDTODEVICE, ifname_bytes.as_ptr(), ifname_bytes.len() as u32);
    }
    let timeout = ffi::Timeval { tv_sec: 3, tv_usec: 0 };
    unsafe {
        ffi::setsockopt(sock, ffi::SOL_SOCKET, ffi::SO_RCVTIMEO, &timeout as *const ffi::Timeval as *const u8, std::mem::size_of::<ffi::Timeval>() as u32);
    }
    let bind_addr = ffi::SockaddrIn { sin_family: ffi::AF_INET as u16, sin_port: DHCP_CLIENT_PORT.to_be(), sin_addr: ffi::INADDR_ANY.to_be(), sin_zero: [0; 8] };
    unsafe {
        if ffi::bind(sock, &bind_addr as *const ffi::SockaddrIn as *const u8, std::mem::size_of::<ffi::SockaddrIn>() as u32) < 0 {
            serial::log("DHCP: bind failed"); ffi::close(sock); return;
        }
    }
    let broadcast_addr = ffi::SockaddrIn { sin_family: ffi::AF_INET as u16, sin_port: DHCP_SERVER_PORT.to_be(), sin_addr: ffi::INADDR_BROADCAST.to_be(), sin_zero: [0; 8] };
    let xid: u32 = 0x12345678;
    let discover = build_dhcp_packet(DHCP_DISCOVER, xid, 0, 0);
    unsafe {
        ffi::sendto(sock, discover.as_ptr(), discover.len(), 0, &broadcast_addr as *const ffi::SockaddrIn as *const u8, std::mem::size_of::<ffi::SockaddrIn>() as u32);
    }
    let mut buf = vec![0u8; 576];
    let n = unsafe { ffi::recvfrom(sock, buf.as_mut_ptr(), buf.len(), 0, std::ptr::null_mut(), std::ptr::null_mut()) };
    if n <= 0 { serial::log("DHCP: no OFFER received"); unsafe { ffi::close(sock); } return; }
    let (msg_type, lease) = match parse_dhcp_response(&buf[..n as usize]) {
        Some(r) => r,
        None => { serial::log("DHCP: failed to parse OFFER"); unsafe { ffi::close(sock); } return; }
    };
    if msg_type != DHCP_OFFER { serial::log("DHCP: expected OFFER"); unsafe { ffi::close(sock); } return; }
    let request = build_dhcp_packet(DHCP_REQUEST, xid, lease.ip, lease.server_id);
    unsafe {
        ffi::sendto(sock, request.as_ptr(), request.len(), 0, &broadcast_addr as *const ffi::SockaddrIn as *const u8, std::mem::size_of::<ffi::SockaddrIn>() as u32);
    }
    let n = unsafe { ffi::recvfrom(sock, buf.as_mut_ptr(), buf.len(), 0, std::ptr::null_mut(), std::ptr::null_mut()) };
    if n <= 0 { serial::log("DHCP: no ACK received"); unsafe { ffi::close(sock); } return; }
    let (msg_type, _) = match parse_dhcp_response(&buf[..n as usize]) {
        Some(r) => r,
        None => { serial::log("DHCP: failed to parse ACK"); unsafe { ffi::close(sock); } return; }
    };
    if msg_type != DHCP_ACK { serial::log("DHCP: expected ACK"); unsafe { ffi::close(sock); } return; }
    unsafe { ffi::close(sock); }
    configure_interface(ifname, lease.ip, lease.mask);
    add_default_route(lease.gateway);
    if lease.dns != 0 {
        let d = lease.dns.to_be_bytes();
        let resolv = format!("nameserver {}.{}.{}.{}\n", d[0], d[1], d[2], d[3]);
        let _ = std::fs::create_dir_all("/etc");
        let _ = std::fs::write("/etc/resolv.conf", &resolv);
    }
    let ip = lease.ip.to_be_bytes();
    serial::log(&format!("DHCP: configured {}.{}.{}.{} on {}", ip[0], ip[1], ip[2], ip[3], ifname));
}

fn bring_interface_up(ifname: &str) {
    let sock = unsafe { ffi::socket(ffi::AF_INET, ffi::SOCK_DGRAM, 0) };
    if sock < 0 { return; }
    let mut ifr = ffi::Ifreq::new(ifname);
    unsafe { ffi::ioctl(sock, ffi::SIOCGIFFLAGS, &mut ifr as *mut ffi::Ifreq); }
    let flags = ifr.get_flags();
    ifr.set_flags(flags | ffi::IFF_UP | ffi::IFF_RUNNING);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFFLAGS, &ifr as *const ffi::Ifreq); ffi::close(sock); }
}

fn configure_interface(ifname: &str, ip: u32, mask: u32) {
    let sock = unsafe { ffi::socket(ffi::AF_INET, ffi::SOCK_DGRAM, 0) };
    if sock < 0 { return; }
    let mut ifr = ffi::Ifreq::new(ifname);
    ifr.set_sockaddr_in(ip);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFADDR, &ifr as *const ffi::Ifreq); }
    ifr.set_sockaddr_in(mask);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFNETMASK, &ifr as *const ffi::Ifreq); }
    ifr.set_flags(ffi::IFF_UP | ffi::IFF_RUNNING);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFFLAGS, &ifr as *const ffi::Ifreq); ffi::close(sock); }
}

fn add_default_route(gateway: u32) {
    if gateway == 0 { return; }
    let sock = unsafe { ffi::socket(ffi::AF_NETLINK, ffi::SOCK_RAW, ffi::NETLINK_ROUTE) };
    if sock < 0 { return; }
    let nl_addr = ffi::SockaddrNl { nl_family: ffi::AF_NETLINK as u16, nl_pad: 0, nl_pid: 0, nl_groups: 0 };
    unsafe { ffi::bind(sock, &nl_addr as *const ffi::SockaddrNl as *const u8, std::mem::size_of::<ffi::SockaddrNl>() as u32); }
    let rt_attr_len = 4 + 4;
    let rt_msg_len = std::mem::size_of::<ffi::RtMsg>();
    let nlmsg_len = std::mem::size_of::<ffi::NlMsgHdr>() + rt_msg_len + rt_attr_len;
    let mut buf = vec![0u8; nlmsg_len];
    let hdr = ffi::NlMsgHdr { nlmsg_len: nlmsg_len as u32, nlmsg_type: ffi::RTM_NEWROUTE, nlmsg_flags: ffi::NLM_F_REQUEST | ffi::NLM_F_CREATE | ffi::NLM_F_ACK, nlmsg_seq: 1, nlmsg_pid: 0 };
    let hdr_bytes = unsafe { std::slice::from_raw_parts(&hdr as *const ffi::NlMsgHdr as *const u8, std::mem::size_of::<ffi::NlMsgHdr>()) };
    buf[..hdr_bytes.len()].copy_from_slice(hdr_bytes);
    let rt_off = std::mem::size_of::<ffi::NlMsgHdr>();
    let rtm = ffi::RtMsg { rtm_family: ffi::AF_INET as u8, rtm_dst_len: 0, rtm_src_len: 0, rtm_tos: 0, rtm_table: ffi::RT_TABLE_MAIN, rtm_protocol: ffi::RTPROT_BOOT, rtm_scope: ffi::RT_SCOPE_UNIVERSE, rtm_type: ffi::RTN_UNICAST, rtm_flags: 0 };
    let rtm_bytes = unsafe { std::slice::from_raw_parts(&rtm as *const ffi::RtMsg as *const u8, std::mem::size_of::<ffi::RtMsg>()) };
    buf[rt_off..rt_off + rtm_bytes.len()].copy_from_slice(rtm_bytes);
    let attr_off = rt_off + rt_msg_len;
    let rta = ffi::RtAttr { rta_len: rt_attr_len as u16, rta_type: ffi::RTA_GATEWAY };
    let rta_bytes = unsafe { std::slice::from_raw_parts(&rta as *const ffi::RtAttr as *const u8, std::mem::size_of::<ffi::RtAttr>()) };
    buf[attr_off..attr_off + rta_bytes.len()].copy_from_slice(rta_bytes);
    buf[attr_off + 4..attr_off + 8].copy_from_slice(&gateway.to_be_bytes());
    let dest = ffi::SockaddrNl { nl_family: ffi::AF_NETLINK as u16, nl_pad: 0, nl_pid: 0, nl_groups: 0 };
    unsafe {
        ffi::sendto(sock, buf.as_ptr(), buf.len(), 0, &dest as *const ffi::SockaddrNl as *const u8, std::mem::size_of::<ffi::SockaddrNl>() as u32);
        ffi::close(sock);
    }
}
