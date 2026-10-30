//! DHCP client and network configuration.
//!
//! The DHCP exchange itself (waiting for the link, DISCOVER/OFFER/REQUEST/ACK, sending again when
//! no reply comes) lives in `swe_vminit_init::acquire_lease` and is tested there. This file
//! supplies what it runs over, a broadcast UDP socket bound to the interface plus sysfs, and
//! applies the lease it returns: the address, the default route by netlink, and the resolver.

use swe_vminit_init::{acquire_lease, CarrierState, DhcpFailure, DhcpLink, DhcpPolicy};

use crate::ffi;
use crate::serial;

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;

/// `recvfrom` result when the receive timeout ran out with nothing to read (-EAGAIN).
const RECV_TIMED_OUT: isize = -11;

/// What the DHCP exchange runs over: a broadcast UDP socket bound to the interface.
struct SocketLink<'a> {
    sock: i32,
    ifname: &'a str,
    broadcast_addr: ffi::SockaddrIn,
    /// Time kept by our own pauses and waits, used only when the monotonic clock cannot be read.
    fallback_ms: u64,
    /// Mixed into random numbers drawn without the kernel's pool.
    counter: u32,
}

impl DhcpLink for SocketLink<'_> {
    fn carrier(&mut self) -> CarrierState {
        // The kernel refuses to read `carrier` of an interface that is not up (EINVAL); that and a
        // missing file both read as "cannot tell".
        let path = format!("/sys/class/net/{}/carrier\0", self.ifname);
        let mut buf = [0u8; 4];
        let n = unsafe { ffi::read_file(path.as_ptr(), &mut buf) };
        match (n, buf[0]) {
            (n, b'1') if n >= 1 => CarrierState::Up,
            (n, b'0') if n >= 1 => CarrierState::Down,
            _ => CarrierState::Unknown,
        }
    }

    fn sleep_ms(&mut self, ms: u32) {
        unsafe { ffi::nanosleep_ms(ms as u64) };
        self.fallback_ms += ms as u64;
    }

    fn now_ms(&mut self) -> u64 {
        let now = unsafe { ffi::monotonic_ms() };
        if now != 0 { now } else { self.fallback_ms }
    }

    fn random_u32(&mut self) -> u32 {
        let mut bytes = [0u8; 4];
        if unsafe { ffi::getrandom_nonblock(&mut bytes) } == 4 {
            return u32::from_le_bytes(bytes);
        }
        // The kernel's pool is not ready this early in boot. The value is a transaction id and a
        // retransmit jitter, which only have to differ between guests that boot together, so the
        // clock mixed with a counter is enough.
        self.counter = self.counter.wrapping_add(0x9E37_79B9);
        (self.now_ms() as u32) ^ self.counter
    }

    fn send(&mut self, packet: &[u8]) -> Result<(), i32> {
        let sent = unsafe {
            ffi::sendto(
                self.sock,
                packet.as_ptr(),
                packet.len(),
                0,
                &self.broadcast_addr as *const ffi::SockaddrIn as *const u8,
                core::mem::size_of::<ffi::SockaddrIn>() as u32,
            )
        };
        if sent < 0 { Err((-sent) as i32) } else { Ok(()) }
    }

    fn receive(&mut self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        let ms = timeout_ms.max(1);
        let timeout = ffi::Timeval { tv_sec: (ms / 1000) as i64, tv_usec: ((ms % 1000) * 1000) as i64 };
        unsafe {
            ffi::setsockopt(
                self.sock,
                ffi::SOL_SOCKET,
                ffi::SO_RCVTIMEO,
                &timeout as *const ffi::Timeval as *const u8,
                core::mem::size_of::<ffi::Timeval>() as u32,
            );
        }
        let n = unsafe {
            ffi::recvfrom(self.sock, buf.as_mut_ptr(), buf.len(), 0, core::ptr::null_mut(), core::ptr::null_mut())
        };
        if n > 0 {
            return Some(n as usize);
        }
        if n != RECV_TIMED_OUT {
            // The call failed without waiting. Pause for the time it was meant to wait, so a
            // broken socket cannot turn the exchange into a loop that floods the network.
            unsafe { ffi::nanosleep_ms(ms as u64) };
        }
        self.fallback_ms += ms as u64;
        None
    }

    fn log(&mut self, message: &str) {
        serial::log(message);
    }
}

pub fn dhcp_configure(ifname: &str) {
    let sysfs_path = format!("/sys/class/net/{}\0", ifname);
    if !unsafe { ffi::path_exists(sysfs_path.as_ptr()) } { return; }
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
    let bind_addr = ffi::SockaddrIn { sin_family: ffi::AF_INET as u16, sin_port: DHCP_CLIENT_PORT.to_be(), sin_addr: ffi::INADDR_ANY.to_be(), sin_zero: [0; 8] };
    unsafe {
        if ffi::bind(sock, &bind_addr as *const ffi::SockaddrIn as *const u8, core::mem::size_of::<ffi::SockaddrIn>() as u32) < 0 {
            serial::log("DHCP: bind failed"); ffi::close(sock); return;
        }
    }
    let broadcast_addr = ffi::SockaddrIn { sin_family: ffi::AF_INET as u16, sin_port: DHCP_SERVER_PORT.to_be(), sin_addr: ffi::INADDR_BROADCAST.to_be(), sin_zero: [0; 8] };
    let mut link = SocketLink { sock, ifname, broadcast_addr, fallback_ms: 0, counter: 0 };
    let outcome = acquire_lease(&mut link, &DhcpPolicy::default());
    unsafe { ffi::close(sock); }
    let lease = match outcome {
        Ok(lease) => lease,
        Err(DhcpFailure::NoOffer { discovers, elapsed_ms }) => {
            serial::log(&format!("DHCP: no OFFER received after {} DISCOVER(s) in {} ms", discovers, elapsed_ms));
            return;
        }
        Err(DhcpFailure::NoAck { requests, elapsed_ms }) => {
            serial::log(&format!("DHCP: no ACK received after {} REQUEST(s) in {} ms", requests, elapsed_ms));
            return;
        }
    };
    configure_interface(ifname, lease.ip, lease.mask);
    add_default_route(lease.gateway);
    if lease.dns != 0 {
        let d = lease.dns.to_be_bytes();
        let resolv = format!("nameserver {}.{}.{}.{}\n", d[0], d[1], d[2], d[3]);
        unsafe {
            ffi::mkdir(b"/etc\0".as_ptr(), 0o755);
            ffi::write_file(b"/etc/resolv.conf\0".as_ptr(), resolv.as_bytes());
        }
    }
    let ip = lease.ip.to_be_bytes();
    serial::log(&format!("DHCP: configured {}.{}.{}.{} on {}", ip[0], ip[1], ip[2], ip[3], ifname));
}

pub fn configure_loopback() {
    bring_interface_up("lo");
    configure_interface("lo", 0x7f000001u32, 0xff000000u32);
    serial::log("loopback: configured 127.0.0.1/8 on lo");
}

/// Write the DNS servers the host configured (`dns=`) to `/etc/resolv.conf`, replacing
/// any server a DHCP lease wrote. Returns false when the file cannot be written.
pub fn write_dns_servers(servers: &[core::net::IpAddr]) -> bool {
    let resolv = swe_vminit_init::format_resolv_conf(servers);
    let written = unsafe {
        ffi::mkdir(b"/etc\0".as_ptr(), 0o755);
        ffi::write_file(b"/etc/resolv.conf\0".as_ptr(), resolv.as_bytes())
    };
    if written {
        serial::log(&format!(
            "dns: wrote {} nameserver line(s) to /etc/resolv.conf",
            servers.len()
        ));
    }
    written
}

fn bring_interface_up(ifname: &str) {
    let sock = unsafe { ffi::socket(ffi::AF_INET, ffi::SOCK_DGRAM, 0) };
    if sock < 0 { return; }
    let mut ifr = ffi::Ifreq::new(ifname);
    unsafe { ffi::ioctl(sock, ffi::SIOCGIFFLAGS, &mut ifr as *mut ffi::Ifreq as usize); }
    let flags = ifr.get_flags();
    ifr.set_flags(flags | ffi::IFF_UP | ffi::IFF_RUNNING);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFFLAGS, &ifr as *const ffi::Ifreq as usize); ffi::close(sock); }
}

fn configure_interface(ifname: &str, ip: u32, mask: u32) {
    let sock = unsafe { ffi::socket(ffi::AF_INET, ffi::SOCK_DGRAM, 0) };
    if sock < 0 { return; }
    let mut ifr = ffi::Ifreq::new(ifname);
    ifr.set_sockaddr_in(ip);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFADDR, &ifr as *const ffi::Ifreq as usize); }
    ifr.set_sockaddr_in(mask);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFNETMASK, &ifr as *const ffi::Ifreq as usize); }
    ifr.set_flags(ffi::IFF_UP | ffi::IFF_RUNNING);
    unsafe { ffi::ioctl(sock, ffi::SIOCSIFFLAGS, &ifr as *const ffi::Ifreq as usize); ffi::close(sock); }
}

fn add_default_route(gateway: u32) {
    if gateway == 0 { return; }
    let sock = unsafe { ffi::socket(ffi::AF_NETLINK, ffi::SOCK_RAW, ffi::NETLINK_ROUTE) };
    if sock < 0 { return; }
    let nl_addr = ffi::SockaddrNl { nl_family: ffi::AF_NETLINK as u16, nl_pad: 0, nl_pid: 0, nl_groups: 0 };
    unsafe { ffi::bind(sock, &nl_addr as *const ffi::SockaddrNl as *const u8, core::mem::size_of::<ffi::SockaddrNl>() as u32); }
    let rt_attr_len = 4 + 4;
    let rt_msg_len = core::mem::size_of::<ffi::RtMsg>();
    let nlmsg_len = core::mem::size_of::<ffi::NlMsgHdr>() + rt_msg_len + rt_attr_len;
    let mut buf = vec![0u8; nlmsg_len];
    let hdr = ffi::NlMsgHdr { nlmsg_len: nlmsg_len as u32, nlmsg_type: ffi::RTM_NEWROUTE, nlmsg_flags: ffi::NLM_F_REQUEST | ffi::NLM_F_CREATE | ffi::NLM_F_ACK, nlmsg_seq: 1, nlmsg_pid: 0 };
    let hdr_bytes = unsafe { core::slice::from_raw_parts(&hdr as *const ffi::NlMsgHdr as *const u8, core::mem::size_of::<ffi::NlMsgHdr>()) };
    buf[..hdr_bytes.len()].copy_from_slice(hdr_bytes);
    let rt_off = core::mem::size_of::<ffi::NlMsgHdr>();
    let rtm = ffi::RtMsg { rtm_family: ffi::AF_INET as u8, rtm_dst_len: 0, rtm_src_len: 0, rtm_tos: 0, rtm_table: ffi::RT_TABLE_MAIN, rtm_protocol: ffi::RTPROT_BOOT, rtm_scope: ffi::RT_SCOPE_UNIVERSE, rtm_type: ffi::RTN_UNICAST, rtm_flags: 0 };
    let rtm_bytes = unsafe { core::slice::from_raw_parts(&rtm as *const ffi::RtMsg as *const u8, core::mem::size_of::<ffi::RtMsg>()) };
    buf[rt_off..rt_off + rtm_bytes.len()].copy_from_slice(rtm_bytes);
    let attr_off = rt_off + rt_msg_len;
    let rta = ffi::RtAttr { rta_len: rt_attr_len as u16, rta_type: ffi::RTA_GATEWAY };
    let rta_bytes = unsafe { core::slice::from_raw_parts(&rta as *const ffi::RtAttr as *const u8, core::mem::size_of::<ffi::RtAttr>()) };
    buf[attr_off..attr_off + rta_bytes.len()].copy_from_slice(rta_bytes);
    buf[attr_off + 4..attr_off + 8].copy_from_slice(&gateway.to_be_bytes());
    let dest = ffi::SockaddrNl { nl_family: ffi::AF_NETLINK as u16, nl_pad: 0, nl_pid: 0, nl_groups: 0 };
    unsafe {
        ffi::sendto(sock, buf.as_ptr(), buf.len(), 0, &dest as *const ffi::SockaddrNl as *const u8, core::mem::size_of::<ffi::SockaddrNl>() as u32);
        ffi::close(sock);
    }
}
