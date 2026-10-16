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

// no_std subset of spi::repl (std mode). No line editing beyond backspace,
// no history, no tab completion, no `cd`/`export` (those need std::env).
// Dispatches through this module's own `builtins::dispatch` above — the
// same fs-independent subset the no-rootfs entrypoint path uses, because
// this REPL only ever runs when there is no mounted rootfs (interactive
// mode execs a real /bin/bash or /bin/sh directly when a rootfs provides
// one; this is purely the last-resort fallback when neither exists).
#[cfg(not(feature = "std"))]
pub mod repl {
    use crate::spi::io as shio;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    const MAX_LINE: usize = 4096;
    const PROMPT: &[u8] = b"# ";

    fn read_line() -> Option<Vec<u8>> {
        let mut line: Vec<u8> = Vec::new();
        loop {
            let byte = shio::read_byte()?;
            match byte {
                4 if line.is_empty() => return None,
                3 => {
                    shio::write_bytes(b"^C\n");
                    line.clear();
                    shio::write_bytes(PROMPT);
                }
                b'\r' | b'\n' => {
                    shio::write_bytes(b"\n");
                    return Some(line);
                }
                127 | 8 if !line.is_empty() => {
                    line.pop();
                    shio::write_bytes(b"\x08 \x08");
                }
                32..=126 if line.len() < MAX_LINE => {
                    line.push(byte);
                    shio::write_bytes(&[byte]);
                }
                _ => {}
            }
        }
    }

    /// Run the interactive REPL. Returns exit code.
    pub fn run(_env: &[(String, String)]) -> i32 {
        shio::write_bytes(b"vminit shell (minimal, no rootfs)\n");
        shio::write_bytes(b"Type 'exit' or Ctrl+D to quit\n\n");
        let mut last_exit = 0i32;
        shio::write_bytes(PROMPT);
        while let Some(line) = read_line() {
            let line_str = String::from_utf8_lossy(&line).to_string();
            let trimmed = line_str.trim();
            if trimmed.is_empty() {
                shio::write_bytes(PROMPT);
                continue;
            }
            let words: Vec<String> = trimmed.split_whitespace().map(String::from).collect();
            if words[0] == "exit" {
                last_exit = words
                    .get(1)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(last_exit);
                break;
            }
            match super::builtins::dispatch(&words) {
                Some(code) => last_exit = code,
                None => {
                    shio::write_bytes(words[0].as_bytes());
                    shio::write_bytes(b": not found (no rootfs mounted, no external commands available)\n");
                    last_exit = 127;
                }
            }
            shio::write_bytes(PROMPT);
        }
        last_exit
    }
}
