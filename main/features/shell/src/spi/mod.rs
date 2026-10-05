pub mod io;

#[cfg(feature = "std")]
pub mod builtins;
#[cfg(feature = "std")]
pub mod repl;
#[cfg(feature = "std")]
pub mod shell;

// no_std subset of spi::builtins (std mode). Only commands that need no
// filesystem access are implemented here, because this path runs with no
// rootfs mounted — the whole reason entrypoints get here at all. The
// filesystem-backed builtins (cat/ls/mkdir/...) stay std-only; running
// them would be meaningless anyway without a mounted root.
#[cfg(not(feature = "std"))]
pub mod builtins {
    use crate::spi::io as shio;
    use alloc::string::String;

    /// Dispatch args to a builtin. Returns Some(code) if handled.
    pub fn dispatch(args: &[String]) -> Option<i32> {
        if args.is_empty() {
            return Some(0);
        }
        let cmd = args[0].rsplit('/').next().unwrap_or(&args[0]);
        let argv = &args[1..];
        match cmd {
            "echo" => Some(builtin_echo(argv)),
            "printf" => Some(builtin_printf(argv)),
            "true" => Some(0),
            "false" => Some(1),
            "exit" => Some(argv.first().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)),
            _ => None,
        }
    }

    fn builtin_echo(args: &[String]) -> i32 {
        let mut no_newline = false;
        let mut start = 0;
        if args.first().map(|s| s.as_str()) == Some("-n") {
            no_newline = true;
            start = 1;
        }
        shio::write_bytes(args[start..].join(" ").as_bytes());
        if !no_newline {
            shio::write_bytes(b"\n");
        }
        0
    }

    fn builtin_printf(args: &[String]) -> i32 {
        if args.is_empty() {
            return 0;
        }
        let fmt = &args[0];
        let mut arg_idx = 1;
        let mut out = String::new();
        let mut chars = fmt.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('\\') => out.push('\\'),
                    Some(c) => {
                        out.push('\\');
                        out.push(c);
                    }
                    None => out.push('\\'),
                },
                '%' => match chars.next() {
                    Some('s') => {
                        if arg_idx < args.len() {
                            out.push_str(&args[arg_idx]);
                            arg_idx += 1;
                        }
                    }
                    Some('%') => out.push('%'),
                    Some(c) => {
                        out.push('%');
                        out.push(c);
                    }
                    None => out.push('%'),
                },
                _ => out.push(ch),
            }
        }
        shio::write_bytes(out.as_bytes());
        0
    }
}

#[cfg(not(feature = "std"))]
pub mod repl {
    use alloc::string::String;
    pub fn run(_env: &[(String, String)]) -> i32 { 0 }
}
