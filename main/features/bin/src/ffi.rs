//! Raw Linux syscall wrappers — direct asm, no libc.
//!
//! All functions use the x86_64 Linux syscall ABI:
//!   rax = syscall number
//!   rdi, rsi, rdx, r10, r8, r9 = arguments (in order)
//!   return value in rax; negative values are errors (-errno)
//!   rcx and r11 are clobbered by the CPU

// ── constants ────────────────────────────────────────────────────────────────

pub const MS_BIND: u64 = 4096;

pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0o100;
pub const O_TRUNC: i32 = 0o1000;
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


// ── C-compatible structs ──────────────────────────────────────────────────────

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

#[repr(C)]
struct Timespec { tv_sec: i64, tv_nsec: i64 }

// ── syscall primitives ────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn sc0(nr: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc1(nr: usize, a0: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc2(nr: usize, a0: usize, a1: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0, in("rsi") a1,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0, in("rsi") a1, in("rdx") a2,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sc6(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    let ret: isize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => ret,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4, in("r9") a5,
        out("rcx") _, out("r11") _,
        options(nostack)
    );
    ret
}

// ── syscall wrappers (replaces extern "C") ────────────────────────────────────

#[cfg(not(feature = "packages"))]
pub const PROT_READ: i32 = 1;
#[cfg(not(feature = "packages"))]
pub const PROT_WRITE: i32 = 2;
#[cfg(not(feature = "packages"))]
pub const MAP_PRIVATE: i32 = 2;
#[cfg(not(feature = "packages"))]
pub const MAP_ANONYMOUS: i32 = 0x20;

const SYS_READ: usize = 0;
const SYS_WRITE: usize = 1;
const SYS_OPEN: usize = 2;
const SYS_CLOSE: usize = 3;
const SYS_STAT: usize = 4;
const SYS_DUP2: usize = 33;
const SYS_NANOSLEEP: usize = 35;
const SYS_FORK: usize = 57;
const SYS_EXECVE: usize = 59;
const SYS_WAIT4: usize = 61;
const SYS_CHDIR: usize = 80;
const SYS_MKDIR: usize = 83;
const SYS_CHMOD: usize = 90;
const SYS_CHOWN: usize = 92;
const SYS_SOCKET: usize = 41;
const SYS_BIND: usize = 49;
const SYS_SENDTO: usize = 44;
const SYS_RECVFROM: usize = 45;
const SYS_SETSOCKOPT: usize = 54;
const SYS_IOCTL: usize = 16;
const SYS_SETSID: usize = 112;
const SYS_CHROOT: usize = 161;
const SYS_MOUNT: usize = 165;
const SYS_UMOUNT2: usize = 166;
const SYS_REBOOT: usize = 169;
const SYS_SYNC: usize = 162;
const SYS_EXIT_GROUP: usize = 231;
#[cfg(not(feature = "packages"))]
const SYS_MMAP: usize = 9;

pub unsafe fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    sc3(SYS_READ, fd as usize, buf as usize, count)
}

pub unsafe fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    sc3(SYS_WRITE, fd as usize, buf as usize, count)
}

pub unsafe fn open(path: *const u8, flags: i32, mode: u32) -> i32 {
    sc3(SYS_OPEN, path as usize, flags as usize, mode as usize) as i32
}

pub unsafe fn close(fd: i32) -> i32 {
    sc1(SYS_CLOSE, fd as usize) as i32
}

pub unsafe fn stat(path: *const u8, buf: *mut u8) -> i32 {
    sc2(SYS_STAT, path as usize, buf as usize) as i32
}

pub unsafe fn mount(
    src: *const u8, target: *const u8, fstype: *const u8, flags: u64, data: *const u8,
) -> i32 {
    sc5(SYS_MOUNT, src as usize, target as usize, fstype as usize, flags as usize, data as usize) as i32
}

pub unsafe fn umount2(target: *const u8, flags: i32) -> i32 {
    sc2(SYS_UMOUNT2, target as usize, flags as usize) as i32
}

pub unsafe fn mkdir(path: *const u8, mode: u32) -> i32 {
    sc2(SYS_MKDIR, path as usize, mode as usize) as i32
}

pub unsafe fn chmod(path: *const u8, mode: u32) -> i32 {
    sc2(SYS_CHMOD, path as usize, mode as usize) as i32
}

pub unsafe fn chown(path: *const u8, owner: u32, group: u32) -> i32 {
    sc3(SYS_CHOWN, path as usize, owner as usize, group as usize) as i32
}

pub unsafe fn chroot(path: *const u8) -> i32 {
    sc1(SYS_CHROOT, path as usize) as i32
}

pub unsafe fn chdir(path: *const u8) -> i32 {
    sc1(SYS_CHDIR, path as usize) as i32
}

pub unsafe fn setsid() -> i32 {
    sc0(SYS_SETSID) as i32
}

pub unsafe fn fork() -> i32 {
    sc0(SYS_FORK) as i32
}

/// execve: path, null-terminated argv array, null-terminated envp array.
pub unsafe fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i32 {
    sc3(SYS_EXECVE, path as usize, argv as usize, envp as usize) as i32
}

/// execv: execve with inherited environment (envp = null → kernel uses existing environ).
/// Kept for call sites that don't need a custom env.
pub unsafe fn execv(path: *const u8, argv: *const *const u8) -> i32 {
    // Pass null envp — Linux inherits parent environ when envp is NULL via execve
    sc3(SYS_EXECVE, path as usize, argv as usize, core::ptr::null::<*const u8>() as usize) as i32
}

pub unsafe fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32 {
    sc4(SYS_WAIT4, pid as usize, status as usize, options as usize, 0) as i32
}

pub unsafe fn reboot(cmd: i32) -> i32 {
    sc3(SYS_REBOOT, LINUX_REBOOT_MAGIC1 as usize, LINUX_REBOOT_MAGIC2 as usize, cmd as usize) as i32
}

pub unsafe fn dup2(oldfd: i32, newfd: i32) -> i32 {
    sc2(SYS_DUP2, oldfd as usize, newfd as usize) as i32
}

pub unsafe fn ioctl(fd: i32, request: u64, arg: usize) -> i32 {
    sc3(SYS_IOCTL, fd as usize, request as usize, arg) as i32
}

pub unsafe fn socket(domain: i32, ty: i32, protocol: i32) -> i32 {
    sc3(SYS_SOCKET, domain as usize, ty as usize, protocol as usize) as i32
}

pub unsafe fn bind(fd: i32, addr: *const u8, addrlen: u32) -> i32 {
    sc3(SYS_BIND, fd as usize, addr as usize, addrlen as usize) as i32
}

pub unsafe fn sendto(
    fd: i32, buf: *const u8, len: usize, flags: i32,
    addr: *const u8, addrlen: u32,
) -> isize {
    sc6(SYS_SENDTO, fd as usize, buf as usize, len, flags as usize, addr as usize, addrlen as usize)
}

pub unsafe fn recvfrom(
    fd: i32, buf: *mut u8, len: usize, flags: i32,
    addr: *mut u8, addrlen: *mut u32,
) -> isize {
    sc6(SYS_RECVFROM, fd as usize, buf as usize, len, flags as usize, addr as usize, addrlen as usize)
}

pub unsafe fn setsockopt(fd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> i32 {
    sc5(SYS_SETSOCKOPT, fd as usize, level as usize, optname as usize, optval as usize, optlen as usize) as i32
}

// ── higher-level helpers ──────────────────────────────────────────────────────

pub unsafe fn exit_group(code: i32) -> ! {
    sc1(SYS_EXIT_GROUP, code as usize);
    // unreachable but satisfies the type checker
    loop { sc0(SYS_EXIT_GROUP); }
}

pub unsafe fn nanosleep_ms(ms: u64) {
    let ts = Timespec { tv_sec: (ms / 1000) as i64, tv_nsec: ((ms % 1000) * 1_000_000) as i64 };
    sc2(SYS_NANOSLEEP, &ts as *const Timespec as usize, 0);
}

/// Write all of `data` to `path` (create/truncate). Returns true on success.
pub unsafe fn write_file(path: *const u8, data: &[u8]) -> bool {
    let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 { return false; }
    let mut written = 0usize;
    while written < data.len() {
        let n = write(fd, data.as_ptr().add(written), data.len() - written);
        if n <= 0 { close(fd); return false; }
        written += n as usize;
    }
    close(fd);
    true
}

/// Read up to `buf.len()` bytes from `path` into `buf`. Returns bytes read, or -1 on error.
pub unsafe fn read_file(path: *const u8, buf: &mut [u8]) -> isize {
    let fd = open(path, O_RDONLY, 0);
    if fd < 0 { return -1; }
    let mut total = 0isize;
    loop {
        if total as usize >= buf.len() { break; }
        let n = read(fd, buf.as_mut_ptr().add(total as usize), buf.len() - total as usize);
        if n < 0 { close(fd); return -1; }
        if n == 0 { break; }
        total += n;
    }
    close(fd);
    total
}

pub unsafe fn sync() {
    sc0(SYS_SYNC);
}

pub unsafe fn power_off() {
    sync();
    reboot(LINUX_REBOOT_CMD_POWER_OFF as i32);
}

/// Allocate an anonymous private mapping of `len` bytes. Returns the mapped
/// address. The caller must verify that the return is not null / MAP_FAILED.
#[cfg(not(feature = "packages"))]
pub unsafe fn mmap_anon(len: usize) -> *mut u8 {
    sc6(
        SYS_MMAP,
        0,                                                       // addr = NULL (kernel chooses)
        len,
        (PROT_READ | PROT_WRITE) as usize,
        (MAP_PRIVATE | MAP_ANONYMOUS) as usize,
        usize::MAX,                                              // fd = -1
        0,                                                       // offset = 0
    ) as *mut u8
}

pub unsafe fn path_exists(path: *const u8) -> bool {
    let mut statbuf = [0u8; 144];
    stat(path, statbuf.as_mut_ptr()) == 0
}
