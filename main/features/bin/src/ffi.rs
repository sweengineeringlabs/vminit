//! Raw Linux syscall wrappers via musl libc.

#![allow(dead_code)]

pub const MS_RDONLY: u64 = 1;
pub const MS_NOSUID: u64 = 2;
pub const MS_NODEV: u64 = 4;
pub const MS_NOEXEC: u64 = 8;
pub const MS_BIND: u64 = 4096;

pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0o100;
pub const O_TRUNC: i32 = 0o1000;
pub const O_NONBLOCK: i32 = 0o4000;

pub const TIOCSCTTY: u64 = 0x540E;
pub const SIOCSIFADDR: u64 = 0x8916;
pub const SIOCSIFNETMASK: u64 = 0x891C;
pub const SIOCSIFFLAGS: u64 = 0x8914;
pub const SIOCGIFFLAGS: u64 = 0x8913;

pub const IFF_UP: i16 = 0x1;
pub const IFF_RUNNING: i16 = 0x40;

pub const AF_INET: i32 = 2;
pub const AF_NETLINK: i32 = 16;
pub const SOCK_DGRAM: i32 = 2;
pub const SOCK_RAW: i32 = 3;
pub const IPPROTO_UDP: i32 = 17;
pub const SOL_SOCKET: i32 = 1;
pub const SO_BROADCAST: i32 = 6;
pub const SO_BINDTODEVICE: i32 = 25;
pub const SO_RCVTIMEO: i32 = 20;
pub const NETLINK_ROUTE: i32 = 0;
pub const INADDR_ANY: u32 = 0;
pub const INADDR_BROADCAST: u32 = 0xFFFFFFFF;

pub const RTM_NEWROUTE: u16 = 24;
pub const NLM_F_REQUEST: u16 = 1;
pub const NLM_F_CREATE: u16 = 0x400;
pub const NLM_F_ACK: u16 = 4;
pub const RT_TABLE_MAIN: u8 = 254;
pub const RT_SCOPE_UNIVERSE: u8 = 0;
pub const RTPROT_BOOT: u8 = 3;
pub const RTN_UNICAST: u8 = 1;
pub const RTA_GATEWAY: u16 = 5;

pub const LINUX_REBOOT_MAGIC1: i32 = 0xfee1dead_u32 as i32;
pub const LINUX_REBOOT_MAGIC2: i32 = 672274793;
pub const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321FEDC;

pub const WEXITSTATUS_SHIFT: i32 = 8;

#[repr(C)]
pub struct SockaddrIn {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: u32,
    pub sin_zero: [u8; 8],
}

#[repr(C)]
pub struct Ifreq {
    pub ifr_name: [u8; 16],
    pub ifr_data: [u8; 24],
}

impl Ifreq {
    pub fn new(name: &str) -> Self {
        let mut ifr = Ifreq { ifr_name: [0u8; 16], ifr_data: [0u8; 24] };
        let bytes = name.as_bytes();
        let len = bytes.len().min(15);
        ifr.ifr_name[..len].copy_from_slice(&bytes[..len]);
        ifr
    }
    pub fn set_sockaddr_in(&mut self, ip: u32) {
        self.ifr_data = [0u8; 24];
        self.ifr_data[0] = AF_INET as u8;
        self.ifr_data[4..8].copy_from_slice(&ip.to_be_bytes());
    }
    pub fn set_flags(&mut self, flags: i16) {
        self.ifr_data = [0u8; 24];
        self.ifr_data[0..2].copy_from_slice(&flags.to_ne_bytes());
    }
    pub fn get_flags(&self) -> i16 {
        i16::from_ne_bytes([self.ifr_data[0], self.ifr_data[1]])
    }
}

#[repr(C)]
pub struct Timeval { pub tv_sec: i64, pub tv_usec: i64 }

#[repr(C)]
pub struct NlMsgHdr {
    pub nlmsg_len: u32, pub nlmsg_type: u16, pub nlmsg_flags: u16,
    pub nlmsg_seq: u32, pub nlmsg_pid: u32,
}

#[repr(C)]
pub struct RtMsg {
    pub rtm_family: u8, pub rtm_dst_len: u8, pub rtm_src_len: u8, pub rtm_tos: u8,
    pub rtm_table: u8, pub rtm_protocol: u8, pub rtm_scope: u8, pub rtm_type: u8,
    pub rtm_flags: u32,
}

#[repr(C)]
pub struct RtAttr { pub rta_len: u16, pub rta_type: u16 }

#[repr(C)]
pub struct SockaddrNl { pub nl_family: u16, pub nl_pad: u16, pub nl_pid: u32, pub nl_groups: u32 }

extern "C" {
    pub fn mount(src: *const u8, target: *const u8, fstype: *const u8, flags: u64, data: *const u8) -> i32;
    pub fn umount2(target: *const u8, flags: i32) -> i32;
    pub fn mkdir(path: *const u8, mode: u32) -> i32;
    pub fn chmod(path: *const u8, mode: u32) -> i32;
    pub fn chown(path: *const u8, owner: u32, group: u32) -> i32;
    pub fn chroot(path: *const u8) -> i32;
    pub fn chdir(path: *const u8) -> i32;
    pub fn setsid() -> i32;
    pub fn fork() -> i32;
    pub fn execv(path: *const u8, argv: *const *const u8) -> i32;
    pub fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    pub fn reboot(cmd: i32) -> i32;
    pub fn getpid() -> i32;
    pub fn open(path: *const u8, flags: i32, mode: u32) -> i32;
    pub fn close(fd: i32) -> i32;
    pub fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    pub fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    pub fn dup2(oldfd: i32, newfd: i32) -> i32;
    pub fn ioctl(fd: i32, request: u64, ...) -> i32;
    pub fn stat(path: *const u8, buf: *mut u8) -> i32;
    pub fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    pub fn bind(fd: i32, addr: *const u8, addrlen: u32) -> i32;
    pub fn sendto(fd: i32, buf: *const u8, len: usize, flags: i32, addr: *const u8, addrlen: u32) -> isize;
    pub fn recvfrom(fd: i32, buf: *mut u8, len: usize, flags: i32, addr: *mut u8, addrlen: *mut u32) -> isize;
    pub fn setsockopt(fd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> i32;
}

pub unsafe fn sync() {
    extern "C" { fn sync(); }
    sync();
}

pub unsafe fn power_off() {
    sync();
    reboot(LINUX_REBOOT_CMD_POWER_OFF as i32);
}

pub unsafe fn path_exists(path: *const u8) -> bool {
    let mut statbuf = [0u8; 144];
    stat(path, statbuf.as_mut_ptr()) == 0
}
