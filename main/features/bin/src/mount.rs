//! Filesystem mounting for PID 1 init.

#[cfg(not(feature = "packages"))]
use alloc::string::{String, ToString};

use crate::ffi;
use crate::serial;
use swe_vminit_init::{BlockMount, OverlayEntry, OverlayManifest, VolumeSpec};

const OVERLAY_MANIFEST_PATH: &[u8] = b"/overlay/.manifest\0";
const OVERLAY_PREFIX: &str = "/overlay";
const ROOTFS_SENTINEL: &[u8] = b"/rootfs/.xkvm-rootfs-mounted\0";

fn mkdir_p(path: &[u8]) {
    unsafe { ffi::mkdir(path.as_ptr(), 0o755); }
}

fn do_mount(src: &[u8], target: &[u8], fstype: &[u8], flags: u64, data: &[u8]) -> bool {
    unsafe {
        let data_ptr = if data.is_empty() { core::ptr::null() } else { data.as_ptr() };
        ffi::mount(src.as_ptr(), target.as_ptr(), fstype.as_ptr(), flags, data_ptr) == 0
    }
}

pub fn mount_pseudofs() {
    mkdir_p(b"/proc\0"); mkdir_p(b"/sys\0"); mkdir_p(b"/dev\0");
    mkdir_p(b"/dev/pts\0"); mkdir_p(b"/dev/shm\0"); mkdir_p(b"/tmp\0");
    do_mount(b"proc\0", b"/proc\0", b"proc\0", 0, b"");
    do_mount(b"sysfs\0", b"/sys\0", b"sysfs\0", 0, b"");
    do_mount(b"devtmpfs\0", b"/dev\0", b"devtmpfs\0", 0, b"");
    do_mount(b"devpts\0", b"/dev/pts\0", b"devpts\0", 0, b"");
    do_mount(b"tmpfs\0", b"/dev/shm\0", b"tmpfs\0", 0, b"");
}

pub fn mount_volumes(volumes: &[VolumeSpec]) {
    for vol in volumes {
        let mount_point = format!("{}\0", vol.guest_mount);
        mkdir_p(mount_point.as_bytes());
        let tag = format!("{}\0", vol.tag);
        let opts = if vol.read_only {
            "trans=virtio,version=9p2000.L,msize=65536,ro\0".to_string()
        } else {
            "trans=virtio,version=9p2000.L,msize=65536,access=any\0".to_string()
        };
        if do_mount(tag.as_bytes(), mount_point.as_bytes(), b"9p\0", 0, opts.as_bytes()) {
            serial::log(&format!("mounted 9p {} at {}", vol.tag, vol.guest_mount));
        }
    }
}

/// Mount additional block devices (from `block_mount=` vminit.conf entries).
///
/// When `rootfs` is `Some("/rootfs")`, each mount target is prefixed with the
/// rootfs path so the device appears at the correct location after chroot.
/// When `rootfs` is `None`, devices are mounted at their absolute guest paths.
pub fn mount_block_devices(mounts: &[BlockMount], rootfs: Option<&str>) {
    for (i, mnt) in mounts.iter().enumerate() {
        // /dev/vdb for index 0, /dev/vdc for index 1, etc.
        let dev_char = (b'b' + i as u8) as char;
        let device = format!("/dev/vd{}\0", dev_char);
        let fstype = format!("{}\0", mnt.fstype);
        let target = match rootfs {
            Some(r) => format!("{}{}\0", r, mnt.guest_mount),
            None    => format!("{}\0", mnt.guest_mount),
        };
        mkdir_p(target.as_bytes());
        if do_mount(device.as_bytes(), target.as_bytes(), fstype.as_bytes(), 0, b"") {
            serial::log(&format!("mounted {} at {}", &device[..device.len()-1], mnt.guest_mount));
        } else {
            serial::log(&format!("block_mount failed: {} -> {}", &device[..device.len()-1], mnt.guest_mount));
        }
    }
}

pub fn mount_rootfs() -> Option<String> {
    if !unsafe { ffi::path_exists(b"/dev/vda\0".as_ptr()) } {
        serial::log("rootfs: /dev/vda absent — skipping rootfs mount");
        return None;
    }
    mkdir_p(b"/rootfs\0");
    let mounted = do_mount(b"/dev/vda\0", b"/rootfs\0", b"ext4\0", 0, b"")
        || do_mount(b"/dev/vda\0", b"/rootfs\0", b"ext2\0", 0, b"");
    if !mounted {
        serial::log("rootfs: mount(/dev/vda -> /rootfs) failed");
        return None;
    }
    let has_bin = unsafe { ffi::path_exists(b"/rootfs/bin\0".as_ptr()) };
    let has_nix = unsafe { ffi::path_exists(b"/rootfs/nix\0".as_ptr()) };
    let has_usr = unsafe { ffi::path_exists(b"/rootfs/usr\0".as_ptr()) };
    if !has_bin && !has_nix && !has_usr {
        serial::log("rootfs: no recognizable OS root (bin/nix/usr absent) — unmounting");
        unsafe { ffi::umount2(b"/rootfs\0".as_ptr(), 0); }
        return None;
    }
    mkdir_p(b"/rootfs/proc\0"); mkdir_p(b"/rootfs/sys\0"); mkdir_p(b"/rootfs/dev\0");
    do_mount(b"proc\0", b"/rootfs/proc\0", b"proc\0", 0, b"");
    do_mount(b"sysfs\0", b"/rootfs/sys\0", b"sysfs\0", 0, b"");
    do_mount(b"devtmpfs\0", b"/rootfs/dev\0", b"devtmpfs\0", 0, b"");
    if !unsafe { ffi::write_file(ROOTFS_SENTINEL.as_ptr(), b"ok\n") } {
        serial::log("rootfs: failed to write sentinel");
    }
    serial::log("mounted rootfs from /dev/vda");
    Some("/rootfs".to_string())
}

pub fn setup_chroot(rootfs: &str, volumes: &[VolumeSpec]) {
    let devpts = format!("{}/dev/pts\0", rootfs);
    let devshm = format!("{}/dev/shm\0", rootfs);
    mkdir_p(devpts.as_bytes()); mkdir_p(devshm.as_bytes());
    do_mount(b"devpts\0", devpts.as_bytes(), b"devpts\0", 0, b"");
    do_mount(b"tmpfs\0", devshm.as_bytes(), b"tmpfs\0", 0, b"");
    let etc_dir = format!("{}/etc\0", rootfs);
    mkdir_p(etc_dir.as_bytes());

    // Copy /etc/resolv.conf into the chroot if it exists
    let mut resolv_buf = [0u8; 512];
    let n = unsafe { ffi::read_file(b"/etc/resolv.conf\0".as_ptr(), &mut resolv_buf) };
    if n > 0 {
        let target = format!("{}/etc/resolv.conf\0", rootfs);
        unsafe { ffi::write_file(target.as_ptr(), &resolv_buf[..n as usize]); }
    }

    for vol in volumes {
        let inner = format!("{}{}\0", rootfs, vol.guest_mount);
        let outer = format!("{}\0", vol.guest_mount);
        mkdir_p(inner.as_bytes());
        do_mount(outer.as_bytes(), inner.as_bytes(), b"\0", ffi::MS_BIND, b"");
    }
    serial::log("chroot environment configured");
}

/// If the entrypoint begins with `su-exec UID:GID ...`, pre-chown every rw
/// volume mount inside the chroot to that uid:gid while still running as root.
/// This makes the 9P server see the correct ownership before the exec drops
/// privileges, so the child process can chmod/stat its own directories.
pub fn pre_chown_volumes_for_exec(rootfs: &str, volumes: &[VolumeSpec], entrypoint: &[String]) {
    let (uid, gid) = match parse_su_exec_uid_gid(entrypoint) {
        Some(pair) => pair,
        None => return,
    };
    for vol in volumes {
        if vol.read_only { continue; }
        let path = format!("{}{}\0", rootfs, vol.guest_mount);
        let rc = unsafe { ffi::chown(path.as_ptr(), uid, gid) };
        if rc == 0 {
            serial::log(&format!("pre-chown {} -> {}:{}", vol.guest_mount, uid, gid));
        } else {
            serial::log(&format!("pre-chown {} failed ({}:{})", vol.guest_mount, uid, gid));
        }
    }
}

/// Parse `su-exec UID:GID` from the start of an entrypoint argv.
/// Accepts both `/path/to/su-exec` and bare `su-exec` as the first arg.
fn parse_su_exec_uid_gid(entrypoint: &[String]) -> Option<(u32, u32)> {
    if entrypoint.len() < 2 { return None; }
    let cmd = &entrypoint[0];
    let is_su_exec = cmd == "su-exec"
        || cmd.ends_with("/su-exec")
        || cmd.ends_with("\\su-exec");
    if !is_su_exec { return None; }
    let spec = &entrypoint[1];
    let mut parts = spec.splitn(2, ':');
    let uid: u32 = parts.next()?.parse().ok()?;
    let gid: u32 = parts.next()?.parse().ok()?;
    Some((uid, gid))
}

pub fn apply_overlay(rootfs: &str) {
    if !unsafe { ffi::path_exists(OVERLAY_MANIFEST_PATH.as_ptr()) } { return; }

    let mut manifest_buf = vec![0u8; 65536];
    let n = unsafe { ffi::read_file(OVERLAY_MANIFEST_PATH.as_ptr(), &mut manifest_buf) };
    if n < 0 {
        serial::log("overlay: failed to read manifest — skipping");
        return;
    }
    let manifest_text = match String::from_utf8(manifest_buf[..n as usize].to_vec()) {
        Ok(s) => s,
        Err(_) => { serial::log("overlay: manifest is not valid UTF-8 — skipping"); return; }
    };

    let manifest = match OverlayManifest::parse(&manifest_text) {
        Ok(m) => m,
        Err(e) => {
            serial::log(&format!("overlay: manifest parse error: {e} — skipping"));
            return;
        }
    };
    serial::log(&format!("overlay: applying {} entries", manifest.entries.len()));
    let mut applied = 0usize;
    let mut failed = 0usize;
    for entry in &manifest.entries {
        match apply_one_overlay_entry(rootfs, entry) {
            Ok(()) => applied += 1,
            Err(e) => { failed += 1; serial::log(&format!("overlay: failed to apply {}: {e}", entry.dest)); }
        }
    }
    serial::log(&format!("overlay: applied {applied} entries, {failed} failed"));
}

fn apply_one_overlay_entry(rootfs: &str, entry: &OverlayEntry) -> Result<(), String> {
    let source_path = format!("{OVERLAY_PREFIX}{}\0", entry.dest);
    let target_path = format!("{rootfs}{}", entry.dest);
    let target_path_c = format!("{}\0", target_path);

    // Read source file
    let mut payload = vec![0u8; 16 * 1024 * 1024]; // 16 MB max overlay file
    let n = unsafe { ffi::read_file(source_path.as_ptr(), &mut payload) };
    if n < 0 { return Err(format!("read {source_path}: syscall failed")); }
    payload.truncate(n as usize);

    // Ensure parent directory exists
    if let Some(parent) = parent_dir_for(&target_path) { mkdir_p_chain(parent); }

    // Write target file
    if !unsafe { ffi::write_file(target_path_c.as_ptr(), &payload) } {
        return Err(format!("write {target_path}: syscall failed"));
    }

    let perm_bits = entry.mode & 0o7777;
    let chmod_rc = unsafe { ffi::chmod(target_path_c.as_ptr(), perm_bits) };
    if chmod_rc != 0 { return Err(format!("chmod {target_path} to {perm_bits:o} failed")); }
    if entry.uid != 0 || entry.gid != 0 {
        let chown_rc = unsafe { ffi::chown(target_path_c.as_ptr(), entry.uid, entry.gid) };
        if chown_rc != 0 { return Err(format!("chown {target_path} to {}:{} failed", entry.uid, entry.gid)); }
    }
    Ok(())
}

fn parent_dir_for(path: &str) -> Option<&str> {
    let last_slash = path.rfind('/')?;
    if last_slash == 0 { return None; }
    Some(&path[..last_slash])
}

fn mkdir_p_chain(path: &str) {
    let bytes = path.as_bytes();
    if bytes.is_empty() { return; }
    for (i, b) in bytes.iter().enumerate().skip(1) {
        if *b == b'/' {
            let mut anc = String::with_capacity(i + 1);
            anc.push_str(&path[..i]);
            anc.push('\0');
            unsafe { ffi::mkdir(anc.as_ptr(), 0o755); }
        }
    }
    let mut leaf = String::with_capacity(path.len() + 1);
    leaf.push_str(path);
    leaf.push('\0');
    unsafe { ffi::mkdir(leaf.as_ptr(), 0o755); }
}
