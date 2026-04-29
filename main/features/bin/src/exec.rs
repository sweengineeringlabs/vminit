//! Entrypoint execution and interactive shell.

use crate::ffi;
use crate::serial;

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
        for (k, v) in env { std::env::set_var(k, v); }
        let status = std::process::Command::new(&args[0]).args(&args[1..]).status();
        match status {
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => { serial::log(&format!("exec failed for {}: {}", args[0], e)); std::process::exit(127); }
        }
    }

    let mut status: i32 = 0;
    unsafe { ffi::waitpid(pid, &mut status, 0); }
    if status & 0x7F == 0 { (status >> 8) & 0xFF } else { 1 }
}

pub fn run_interactive_shell(env: &[(String, String)], rootfs: Option<&str>) -> i32 {
    if let Some(root) = rootfs {
        for (k, v) in env { std::env::set_var(k, v); }
        std::env::set_var("TERM", "linux");
        std::env::set_var("PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

        let tty_fd = unsafe { ffi::open(b"/dev/ttyS0\0".as_ptr(), ffi::O_RDWR, 0) };
        if tty_fd < 0 { serial::log("failed to open /dev/ttyS0"); return 1; }

        unsafe {
            ffi::setsid();
            ffi::ioctl(tty_fd, ffi::TIOCSCTTY, 0i32);
            ffi::dup2(tty_fd, 0); ffi::dup2(tty_fd, 1); ffi::dup2(tty_fd, 2);
            if tty_fd > 2 { ffi::close(tty_fd); }
        }

        let root_cstr = format!("{}\0", root);
        unsafe { ffi::chroot(root_cstr.as_ptr()); ffi::chdir(b"/\0".as_ptr()); }

        let bash = b"/bin/bash\0";
        let bash_args: [*const u8; 3] = [bash.as_ptr(), b"-l\0".as_ptr(), std::ptr::null()];
        unsafe { ffi::execv(bash.as_ptr(), bash_args.as_ptr()); }

        let sh = b"/bin/sh\0";
        let sh_args: [*const u8; 2] = [sh.as_ptr(), std::ptr::null()];
        unsafe { ffi::execv(sh.as_ptr(), sh_args.as_ptr()); }

        serial::log("no /bin/bash or /bin/sh in rootfs, using built-in shell");
    }

    swe_vminit_shell::repl::run(env)
}

pub fn start_agent() {
    if !unsafe { ffi::path_exists(b"/bin/vminit-agent\0".as_ptr()) } { return; }
    let pid = unsafe { ffi::fork() };
    if pid == 0 {
        let agent = b"/bin/vminit-agent\0";
        let args: [*const u8; 2] = [agent.as_ptr(), std::ptr::null()];
        unsafe { ffi::execv(agent.as_ptr(), args.as_ptr()); }
        std::process::exit(1);
    }
    if pid > 0 { serial::log("started guest agent"); }
}
