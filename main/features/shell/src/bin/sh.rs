//! POSIX-compatible /bin/sh built on the swe_vminit_shell interpreter.
//!
//! Dispatch rules (in order):
//!   sh -c "script" [name [arg...]]  → execute script string
//!   sh script_file [arg...]         → execute script file
//!   sh                              → interactive REPL
//!
//! This binary is built as a musl static executable for x86_64-linux-musl
//! and packaged as `sh.tar.gz` (entry: bin/sh) for vminit's packages
//! feature. vminit installs it into the Nix closure rootfs at /bin/sh
//! before exec, allowing popen("/bin/sh", ...) callers (e.g. initdb)
//! to find a shell.
//!
//! Build for guest (in WSL2 / Linux):
//!   cargo build -p swe_vminit_shell --bin sh \
//!       --target x86_64-unknown-linux-musl --release
//!
//! Package (in WSL2):
//!   mkdir -p /tmp/sh-pkg/bin
//!   cp target/x86_64-unknown-linux-musl/release/sh /tmp/sh-pkg/bin/sh
//!   tar -czf /path/to/vmisolate/downloads/sh.tar.gz -C /tmp/sh-pkg bin/sh

fn main() {
    swe_vminit_shell::wire_default_io();
    let args: Vec<String> = std::env::args().collect();

    let code = if args.len() >= 3 && args[1] == "-c" {
        // `sh -c -- "cmd"` — POSIX allows `--` to end option parsing; the
        // command string follows. PostgreSQL initdb uses this form.
        let cmd_idx = if args[2] == "--" && args.len() >= 4 { 3 } else { 2 };
        swe_vminit_shell::shell::execute(&args[cmd_idx])
    } else if args.len() >= 2 && !args[1].starts_with('-') {
        match std::fs::read_to_string(&args[1]) {
            Ok(script) => swe_vminit_shell::shell::execute(&script),
            Err(e) => {
                eprintln!("sh: {}: {}", args[1], e);
                2
            }
        }
    } else {
        swe_vminit_shell::repl::run(&[])
    };

    std::process::exit(code);
}
