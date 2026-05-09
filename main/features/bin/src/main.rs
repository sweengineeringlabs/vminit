//! vminit — PID 1 init process for Linux x86_64 microVMs.
//!
//! Boot sequence:
//!   1. Mount pseudo-filesystems (proc, sys, dev, devpts, shm)
//!   2. Parse /etc/vminit.conf
//!   3. Mount 9P volume shares
//!   4. Mount rootfs from /dev/vda (if present)
//!   5. DHCP network configuration
//!   6. Install packages from initrd / Nix binary cache → /nix/store/<hash>-<name>/
//!   7. Apply [[files]] overlay from /overlay/.manifest
//!   8. Chroot setup (mount_rootfs)
//!   9. Apply spec.env to process environment (passed via execve envp in exec.rs)
//!  10. Start guest agent (if configured)
//!  11. Signal readiness (XIKA_READY)
//!  12. Execute entrypoint or interactive shell
//!  13. Sync filesystems
//!  14. Report exit code and power off
#![cfg_attr(not(feature = "packages"), no_std)]
#![cfg_attr(not(feature = "packages"), no_main)]

// In no_std mode, pull in the alloc crate. The #[macro_use] attribute makes
// vec! and format! available crate-wide without explicit path qualification.
#[cfg(not(feature = "packages"))]
#[macro_use]
extern crate alloc;

#[cfg(not(feature = "packages"))]
use alloc::string::String;

mod exec;
mod ffi;
#[cfg(not(feature = "packages"))]
mod heap;
#[cfg(feature = "packages")]
mod install;
#[cfg(feature = "packages")]
mod manifest;
mod mount;
mod net;
mod serial;

// ── no_std entry point ────────────────────────────────────────────────────────

#[cfg(not(feature = "packages"))]
#[no_mangle]
pub unsafe extern "C" fn main(
    _argc: i32,
    _argv: *const *const u8,
    _envp: *const *const u8,
) -> i32 {
    heap::init();
    main_inner();
    0
}

#[cfg(not(feature = "packages"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { ffi::exit_group(101) }
}

// EH personality stub: referenced by DWARF .eh_frame sections even with panic=abort.
// The unwinding runtime never invokes this — it exists only to satisfy the linker.
#[cfg(not(feature = "packages"))]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

// ── std entry point (packages feature) ───────────────────────────────────────

#[cfg(feature = "packages")]
fn main() {
    main_inner();
}

// ── shared boot logic ─────────────────────────────────────────────────────────

fn main_inner() {
    swe_vminit_shell::io::set_output(|data| serial::serial_write(data));
    swe_vminit_shell::io::set_input(|| {
        let mut buf = [0u8; 1];
        let n = unsafe { ffi::read(0, buf.as_mut_ptr(), 1) };
        if n == 1 { Some(buf[0]) } else { None }
    });

    serial::log("vminit starting");

    mount::mount_pseudofs();

    let cfg = {
        let mut buf = vec![0u8; 65536];
        let n = unsafe { ffi::read_file(b"/etc/vminit.conf\0".as_ptr(), &mut buf) };
        let text = if n > 0 {
            String::from_utf8_lossy(&buf[..n as usize]).into_owned()
        } else {
            String::new()
        };
        swe_vminit_init::parse_config(&text)
    };

    mount::mount_volumes(&cfg.volumes);

    let rootfs = mount::mount_rootfs();

    if cfg.network_mode == swe_vminit_init::GuestNetworkMode::Dhcp {
        net::dhcp_configure("eth0");
    }
    net::configure_loopback();

    #[cfg(feature = "packages")]
    let manifest_text: Option<String> = if let Some(ref url) = cfg.manifest_url {
        let http = swe_justpkg_pkg::UreqClient;
        match manifest::fetch_manifest(&http, url) {
            Ok(text) => {
                serial::log("manifest_url: fetched successfully");
                Some(text)
            }
            Err(e) => {
                serial::log(&format!("FATAL: manifest_url fetch failed: {e}"));
                unsafe { ffi::power_off(); }
                unreachable!()
            }
        }
    } else {
        cfg.manifest_path.as_ref().and_then(|p| {
            let path_c = format!("{}\0", p);
            let mut buf = vec![0u8; 1024 * 1024];
            let n = unsafe { ffi::read_file(path_c.as_ptr(), &mut buf) };
            if n > 0 { String::from_utf8(buf[..n as usize].to_vec()).ok() } else { None }
        })
    };

    #[cfg(feature = "packages")]
    match (&manifest_text, &cfg.manifest_hash) {
        (Some(text), Some(hash_spec)) => {
            if let Err(e) = manifest::verify_manifest_hash(text.as_bytes(), hash_spec) {
                serial::log(&format!("FATAL: {e}"));
                unsafe { ffi::power_off(); }
                unreachable!()
            }
        }
        (Some(_), None) => {
            serial::log("warning: manifest loaded without integrity check (manifest_hash= not set)");
        }
        _ => {}
    }

    #[cfg(feature = "packages")]
    install::install_packages(&cfg.packages, rootfs.as_deref(), manifest_text.as_deref(), &cfg.cache_base);

    if let Some(ref root) = rootfs {
        mount::apply_overlay(root);
    }

    mount::mount_block_devices(&cfg.block_mounts, rootfs.as_deref());

    if let Some(ref root) = rootfs {
        mount::setup_chroot(root, &cfg.volumes);
        mount::pre_chown_volumes_for_exec(root, &cfg.volumes, &cfg.entrypoint);
    }

    if cfg.start_agent {
        exec::start_agent();
    }

    serial::signal_ready(cfg.signal_mode);

    let exit_code = if cfg.start_agent && cfg.entrypoint.is_empty() {
        // Agent is handling the exec command. Keep the VM alive so the daemon
        // can receive MSG_EXIT from the agent. The daemon will drop the VM
        // immediately after getting the result, killing this process.
        loop {
            unsafe { ffi::nanosleep_ms(5000); }
        }
    } else if cfg.interactive {
        exec::run_interactive_shell(&cfg.env, rootfs.as_deref())
    } else {
        exec::run_entrypoint(&cfg.entrypoint, &cfg.env, rootfs.as_deref())
    };

    unsafe { ffi::sync(); }

    serial::report_exit(exit_code, cfg.signal_mode);
    serial::log("powering off");
    unsafe { ffi::power_off(); }

    loop {
        unsafe { ffi::nanosleep_ms(1000); }
    }
}
