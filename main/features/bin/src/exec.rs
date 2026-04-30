//! Entrypoint execution and interactive shell.

#[cfg(not(feature = "packages"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "packages"))]
use alloc::vec::Vec;

use crate::ffi;
use crate::serial;

/// Build a null-terminated argv array from a slice of strings.
/// The returned Vec owns the null-terminated byte strings; the pointer array
/// borrows from them. Caller must keep the Vec alive for the duration of execve.
fn build_cstr_vec(strings: &[String]) -> (Vec<Vec<u8>>, Vec<*const u8>) {
    let owned: Vec<Vec<u8>> = strings.iter()
        .map(|s| { let mut v = s.as_bytes().to_vec(); v.push(0); v })
        .collect();
    let mut ptrs: Vec<*const u8> = owned.iter().map(|v| v.as_ptr()).collect();
    ptrs.push(core::ptr::null());
    (owned, ptrs)
}

/// Build a null-terminated envp array from (key, value) pairs.
fn build_envp(env: &[(String, String)]) -> (Vec<Vec<u8>>, Vec<*const u8>) {
    let owned: Vec<Vec<u8>> = env.iter()
        .map(|(k, v)| { let mut s = format!("{}={}", k, v).into_bytes(); s.push(0); s })
        .collect();
    let mut ptrs: Vec<*const u8> = owned.iter().map(|v| v.as_ptr()).collect();
    ptrs.push(core::ptr::null());
    (owned, ptrs)
}

pub fn run_entrypoint(args: &[String], env: &[(String, String)], rootfs: Option<&str>) -> i32 {
    if args.is_empty() { return 0; }

    if rootfs.is_none() {
        if let Some(code) = swe_vminit_shell::builtins::dispatch(args) {
            return code;
        }
    }

    let pid = unsafe { ffi::fork() };
    if pid < 0 { serial::log("fork() failed"); return 1; }

    if pid == 0 {
        if let Some(root) = rootfs {
            let root_cstr = format!("{}\0", root);
            unsafe { ffi::chroot(root_cstr.as_ptr()); ffi::chdir(b"/\0".as_ptr()); }
        }

        let (argv_bufs, argv_ptrs) = build_cstr_vec(args);
        let (env_bufs, env_ptrs) = build_envp(env);

        unsafe {
            ffi::execve(argv_ptrs[0], argv_ptrs.as_ptr(), env_ptrs.as_ptr());
        }

        // execve only returns on error
        serial::log(&format!("exec failed for {}", args[0]));
        drop(argv_bufs); drop(env_bufs);
        unsafe { ffi::exit_group(127) }
    }

    let mut status: i32 = 0;
    unsafe { ffi::waitpid(pid, &mut status, 0); }
    if status & 0x7F == 0 { (status >> 8) & 0xFF } else { 1 }
}

pub fn run_interactive_shell(env: &[(String, String)], rootfs: Option<&str>) -> i32 {
    let tty_fd = unsafe { ffi::open(b"/dev/ttyS0\0".as_ptr(), ffi::O_RDWR, 0) };
    if tty_fd >= 0 {
        unsafe {
            ffi::setsid();
            ffi::ioctl(tty_fd, ffi::TIOCSCTTY, 0);
            ffi::dup2(tty_fd, 0); ffi::dup2(tty_fd, 1); ffi::dup2(tty_fd, 2);
            if tty_fd > 2 { ffi::close(tty_fd); }
        }
    }

    if let Some(root) = rootfs {
        let root_cstr = format!("{}\0", root);
        unsafe { ffi::chroot(root_cstr.as_ptr()); ffi::chdir(b"/\0".as_ptr()); }
    }

    let mut term_env = env.to_vec();
    term_env.push(("TERM".to_string(), "linux".to_string()));
    term_env.push(("PATH".to_string(), "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()));

    let (env_bufs, env_ptrs) = build_envp(&term_env);

    let bash = b"/bin/bash\0";
    let bash_arg0 = b"/bin/bash\0";
    let bash_flag = b"-l\0";
    let bash_argv: [*const u8; 3] = [bash_arg0.as_ptr(), bash_flag.as_ptr(), core::ptr::null()];
    unsafe { ffi::execve(bash.as_ptr(), bash_argv.as_ptr(), env_ptrs.as_ptr()); }

    let sh = b"/bin/sh\0";
    let sh_arg0 = b"/bin/sh\0";
    let sh_argv: [*const u8; 2] = [sh_arg0.as_ptr(), core::ptr::null()];
    unsafe { ffi::execve(sh.as_ptr(), sh_argv.as_ptr(), env_ptrs.as_ptr()); }

    serial::log("no /bin/bash or /bin/sh in rootfs, using built-in shell");
    drop(env_bufs);
    swe_vminit_shell::repl::run(env)
}

pub fn start_agent() {
    if !unsafe { ffi::path_exists(b"/bin/xkvm-agent\0".as_ptr()) } { return; }
    let pid = unsafe { ffi::fork() };
    if pid == 0 {
        let agent = b"/bin/xkvm-agent\0";
        let args: [*const u8; 2] = [agent.as_ptr(), core::ptr::null()];
        unsafe { ffi::execv(agent.as_ptr(), args.as_ptr()); }
        unsafe { ffi::exit_group(1) }
    }
    if pid > 0 { serial::log("started guest agent"); }
}
