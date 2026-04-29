//! vminit — PID 1 init process for Linux x86_64 microVMs.
//!
//! Boot sequence:
//!   1. Mount pseudo-filesystems (proc, sys, dev, devpts, shm)
//!   2. Parse /etc/xkvm.conf
//!   3. Mount 9P volume shares
//!   4. Mount rootfs from /dev/vda (if present)
//!   5. DHCP network configuration
//!   6. Install packages from initrd / Nix binary cache → /nix/store/<hash>-<name>/
//!   7. Apply [[files]] overlay from /overlay/.manifest
//!   8. Chroot setup (kali_mode)
//!   9. Apply spec.env to process environment
//!  10. Start guest agent (if configured)
//!  11. Signal readiness (XIKA_READY)
//!  12. Execute entrypoint or interactive shell
//!  13. Sync filesystems
//!  14. Report exit code and power off

mod exec;
mod ffi;
mod install;
mod manifest;
mod mount;
mod net;
mod serial;

fn main() {
    swe_vminit_shell::io::set_output(|data| serial::serial_write(data));
    swe_vminit_shell::io::set_input(|| {
        let mut buf = [0u8; 1];
        let n = unsafe { ffi::read(0, buf.as_mut_ptr(), 1) };
        if n == 1 { Some(buf[0]) } else { None }
    });

    serial::log("vminit starting");

    mount::mount_pseudofs();

    let text = std::fs::read_to_string("/etc/xkvm.conf").unwrap_or_default();
    let cfg = swe_vminit_init::parse_config(&text);

    mount::mount_volumes(&cfg.volumes);

    let rootfs = mount::mount_rootfs();

    net::dhcp_configure("eth0");

    let manifest_text = if let Some(ref url) = cfg.manifest_url {
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
        cfg.manifest_path.as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
    };

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

    install::install_packages(&cfg.packages, rootfs.as_deref(), manifest_text.as_deref(), &cfg.cache_base);

    if let Some(ref root) = rootfs {
        mount::apply_overlay(root);
    }

    if let Some(ref root) = rootfs {
        mount::setup_chroot(root, &cfg.volumes);
    }

    for (k, v) in &cfg.env {
        std::env::set_var(k, v);
    }

    if cfg.start_agent {
        exec::start_agent();
    }

    serial::signal_ready(cfg.signal_mode);

    let exit_code = if cfg.interactive {
        exec::run_interactive_shell(&cfg.env, rootfs.as_deref())
    } else {
        exec::run_entrypoint(&cfg.entrypoint, &cfg.env, rootfs.as_deref())
    };

    unsafe { ffi::sync(); }

    serial::report_exit(exit_code, cfg.signal_mode);
    serial::log("powering off");
    unsafe { ffi::power_off(); }

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
