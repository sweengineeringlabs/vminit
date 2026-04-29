//! Built-in shell commands.
//!
//! Replaces common BusyBox applets so the shell can operate without a
//! rootfs. dispatch() returns Some(exit_code) when a builtin handled
//! the command, or None for external commands.

use crate::spi::io as shio;
use std::fs;
use std::io::BufRead;
use std::path::Path;

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
        "cat" => Some(builtin_cat(argv)),
        "head" => Some(builtin_head(argv)),
        "tail" => Some(builtin_tail(argv)),
        "ls" => Some(builtin_ls(argv)),
        "mkdir" => Some(builtin_mkdir(argv)),
        "rm" => Some(builtin_rm(argv)),
        "cp" => Some(builtin_cp(argv)),
        "mv" => Some(builtin_mv(argv)),
        "touch" => Some(builtin_touch(argv)),
        "wc" => Some(builtin_wc(argv)),
        "grep" => Some(builtin_grep(argv)),
        "uname" => Some(builtin_uname(argv)),
        "hostname" => Some(builtin_hostname(argv)),
        "env" => Some(builtin_env(argv)),
        "date" => Some(builtin_date(argv)),
        "sleep" => Some(builtin_sleep(argv)),
        "true" => Some(0),
        "false" => Some(1),
        "exit" => Some(argv.first().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)),
        "chmod" => Some(builtin_chmod(argv)),
        "sh" | "ash" => {
            if argv.first().map(|s| s.as_str()) == Some("-c") && argv.len() >= 2 {
                let script = argv[1..].join(" ");
                Some(crate::spi::shell::execute(&script))
            } else {
                None
            }
        }
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
    if !no_newline { shio::write_bytes(b"\n"); }
    0
}

fn builtin_printf(args: &[String]) -> i32 {
    if args.is_empty() { return 0; }
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
                Some(c) => { out.push('\\'); out.push(c); }
                None => out.push('\\'),
            },
            '%' => match chars.next() {
                Some('s') => {
                    if arg_idx < args.len() { out.push_str(&args[arg_idx]); arg_idx += 1; }
                }
                Some('%') => out.push('%'),
                Some(c) => { out.push('%'); out.push(c); }
                None => out.push('%'),
            },
            _ => out.push(ch),
        }
    }
    shio::write_bytes(out.as_bytes());
    0
}

fn builtin_cat(args: &[String]) -> i32 {
    if args.is_empty() { return 0; }
    let mut code = 0;
    for path in args {
        match fs::read(path) {
            Ok(data) => shio::write_bytes(&data),
            Err(e) => { shio::write_bytes(format!("cat: {}: {}\n", path, e).as_bytes()); code = 1; }
        }
    }
    code
}

fn builtin_head(args: &[String]) -> i32 {
    let mut lines = 10usize;
    let mut files: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() { lines = args[i + 1].parse().unwrap_or(10); i += 2; }
        else { files.push(&args[i]); i += 1; }
    }
    for path in &files {
        match fs::File::open(path) {
            Ok(f) => {
                let reader = std::io::BufReader::new(f);
                for (idx, line) in reader.lines().enumerate() {
                    if idx >= lines { break; }
                    if let Ok(l) = line { shio::write_bytes(l.as_bytes()); shio::write_bytes(b"\n"); }
                }
            }
            Err(e) => { shio::write_bytes(format!("head: {}: {}\n", path, e).as_bytes()); return 1; }
        }
    }
    0
}

fn builtin_tail(args: &[String]) -> i32 {
    let mut lines = 10usize;
    let mut files: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() { lines = args[i + 1].parse().unwrap_or(10); i += 2; }
        else { files.push(&args[i]); i += 1; }
    }
    for path in &files {
        match fs::read_to_string(path) {
            Ok(content) => {
                let all_lines: Vec<&str> = content.lines().collect();
                let start = all_lines.len().saturating_sub(lines);
                for line in &all_lines[start..] { shio::write_bytes(line.as_bytes()); shio::write_bytes(b"\n"); }
            }
            Err(e) => { shio::write_bytes(format!("tail: {}: {}\n", path, e).as_bytes()); return 1; }
        }
    }
    0
}

fn builtin_ls(args: &[String]) -> i32 {
    let mut show_all = false;
    let mut long_format = false;
    let mut paths: Vec<&str> = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-a" => show_all = true,
            "-l" => long_format = true,
            "-la" | "-al" => { show_all = true; long_format = true; }
            _ => paths.push(arg),
        }
    }
    if paths.is_empty() { paths.push("."); }
    for path in &paths {
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => { shio::write_bytes(format!("ls: {}: {}\n", path, e).as_bytes()); return 1; }
        };
        let mut names: Vec<String> = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !show_all && name.starts_with('.') { continue; }
            if long_format {
                let meta = e.metadata().ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                names.push(format!("{}rwxr-xr-x {:>8}  {}", if is_dir { "d" } else { "-" }, size, name));
            } else { names.push(name); }
        }
        names.sort();
        if long_format {
            for name in &names { shio::write_bytes(name.as_bytes()); shio::write_bytes(b"\n"); }
        } else {
            shio::write_bytes(names.join("  ").as_bytes()); shio::write_bytes(b"\n");
        }
    }
    0
}

fn builtin_mkdir(args: &[String]) -> i32 {
    let mut parents = false;
    let mut dirs: Vec<&str> = Vec::new();
    for arg in args {
        if arg == "-p" { parents = true; } else { dirs.push(arg); }
    }
    for dir in dirs {
        let result = if parents { fs::create_dir_all(dir) } else { fs::create_dir(dir) };
        if let Err(e) = result { shio::write_bytes(format!("mkdir: {}: {}\n", dir, e).as_bytes()); return 1; }
    }
    0
}

fn builtin_rm(args: &[String]) -> i32 {
    let mut recursive = false;
    let mut force = false;
    let mut paths: Vec<&str> = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-r" | "-R" => recursive = true,
            "-f" => force = true,
            "-rf" | "-fr" => { recursive = true; force = true; }
            _ => paths.push(arg),
        }
    }
    for path in paths {
        let result = if Path::new(path).is_dir() && recursive { fs::remove_dir_all(path) } else { fs::remove_file(path) };
        if let Err(e) = result {
            if !force { shio::write_bytes(format!("rm: {}: {}\n", path, e).as_bytes()); return 1; }
        }
    }
    0
}

fn builtin_cp(args: &[String]) -> i32 {
    if args.len() < 2 { shio::write_bytes(b"cp: missing operand\n"); return 1; }
    match fs::copy(&args[0], &args[1]) {
        Ok(_) => 0,
        Err(e) => { shio::write_bytes(format!("cp: {}\n", e).as_bytes()); 1 }
    }
}

fn builtin_mv(args: &[String]) -> i32 {
    if args.len() < 2 { shio::write_bytes(b"mv: missing operand\n"); return 1; }
    match fs::rename(&args[0], &args[1]) {
        Ok(_) => 0,
        Err(e) => { shio::write_bytes(format!("mv: {}\n", e).as_bytes()); 1 }
    }
}

fn builtin_chmod(args: &[String]) -> i32 {
    if args.len() < 2 { shio::write_bytes(b"chmod: missing operand\n"); return 1; }
    let mode_str = &args[0];
    let path = &args[1];
    let mode = match u32::from_str_radix(mode_str, 8) {
        Ok(m) => m,
        Err(_) => { shio::write_bytes(format!("chmod: invalid mode '{}'\n", mode_str).as_bytes()); return 1; }
    };
    set_permissions(path, mode)
}

#[cfg(unix)]
fn set_permissions(path: &str, mode: u32) -> i32 {
    use std::os::unix::fs::PermissionsExt;
    match fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
        Ok(_) => 0,
        Err(e) => { shio::write_bytes(format!("chmod: {}: {}\n", path, e).as_bytes()); 1 }
    }
}

#[cfg(not(unix))]
fn set_permissions(path: &str, _mode: u32) -> i32 {
    shio::write_bytes(format!("chmod: {}: not supported on this platform\n", path).as_bytes());
    1
}

fn builtin_touch(args: &[String]) -> i32 {
    for path in args {
        if let Err(e) = fs::OpenOptions::new().create(true).append(true).open(path) {
            shio::write_bytes(format!("touch: {}: {}\n", path, e).as_bytes()); return 1;
        }
    }
    0
}

fn builtin_wc(args: &[String]) -> i32 {
    let mut count_lines = false;
    let mut count_words = false;
    let mut count_bytes = false;
    let mut files: Vec<&str> = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-l" => count_lines = true,
            "-w" => count_words = true,
            "-c" => count_bytes = true,
            _ => files.push(arg),
        }
    }
    if !count_lines && !count_words && !count_bytes {
        count_lines = true; count_words = true; count_bytes = true;
    }
    for path in &files {
        match fs::read_to_string(path) {
            Ok(content) => {
                let mut parts = Vec::new();
                if count_lines { parts.push(format!("{}", content.lines().count())); }
                if count_words { parts.push(format!("{}", content.split_whitespace().count())); }
                if count_bytes { parts.push(format!("{}", content.len())); }
                parts.push(path.to_string());
                shio::write_bytes(parts.join(" ").as_bytes()); shio::write_bytes(b"\n");
            }
            Err(e) => { shio::write_bytes(format!("wc: {}: {}\n", path, e).as_bytes()); return 1; }
        }
    }
    0
}

fn builtin_grep(args: &[String]) -> i32 {
    let mut invert = false;
    let mut count_only = false;
    let mut ignore_case = false;
    let mut pattern_idx = 0;
    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "-v" => invert = true,
            "-c" => count_only = true,
            "-i" => ignore_case = true,
            _ => { pattern_idx = i; break; }
        }
    }
    if pattern_idx >= args.len() { shio::write_bytes(b"grep: missing pattern\n"); return 2; }
    let pattern = if ignore_case { args[pattern_idx].to_lowercase() } else { args[pattern_idx].clone() };
    let files = &args[pattern_idx + 1..];
    let mut found_any = false;
    let process = |content: &str, prefix: &str| -> (bool, usize) {
        let mut matched = false;
        let mut count = 0;
        for line in content.lines() {
            let haystack = if ignore_case { line.to_lowercase() } else { line.to_string() };
            let matches = haystack.contains(&pattern);
            let show = if invert { !matches } else { matches };
            if show {
                matched = true; count += 1;
                if !count_only {
                    if !prefix.is_empty() { shio::write_bytes(format!("{}:", prefix).as_bytes()); }
                    shio::write_bytes(line.as_bytes()); shio::write_bytes(b"\n");
                }
            }
        }
        (matched, count)
    };
    if files.is_empty() { return 2; }
    let multi = files.len() > 1;
    for path in files {
        match fs::read_to_string(path) {
            Ok(content) => {
                let prefix = if multi { path.as_str() } else { "" };
                let (matched, count) = process(&content, prefix);
                if count_only {
                    if multi { shio::write_bytes(format!("{}:{}\n", path, count).as_bytes()); }
                    else { shio::write_bytes(format!("{}\n", count).as_bytes()); }
                }
                if matched { found_any = true; }
            }
            Err(e) => { shio::write_bytes(format!("grep: {}: {}\n", path, e).as_bytes()); }
        }
    }
    if found_any { 0 } else { 1 }
}

fn builtin_uname(args: &[String]) -> i32 {
    let show_all = args.iter().any(|a| a == "-a");
    let show_s = show_all || args.is_empty() || args.iter().any(|a| a == "-s");
    let show_n = show_all || args.iter().any(|a| a == "-n");
    let show_r = show_all || args.iter().any(|a| a == "-r");
    let show_m = show_all || args.iter().any(|a| a == "-m");
    let mut parts = Vec::new();
    if show_s { parts.push("Linux".to_string()); }
    if show_n {
        let hostname = fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "vminit".to_string()).trim().to_string();
        parts.push(hostname);
    }
    if show_r {
        let release = fs::read_to_string("/proc/version")
            .ok()
            .and_then(|v| v.split_whitespace().nth(2).map(String::from))
            .unwrap_or_else(|| "unknown".to_string());
        parts.push(release);
    }
    if show_m { parts.push("x86_64".to_string()); }
    shio::write_bytes(parts.join(" ").as_bytes()); shio::write_bytes(b"\n");
    0
}

fn builtin_hostname(args: &[String]) -> i32 {
    if let Some(new_name) = args.first() {
        let _ = fs::write("/etc/hostname", format!("{}\n", new_name));
        0
    } else {
        let name = fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "vminit".to_string()).trim().to_string();
        shio::write_bytes(name.as_bytes()); shio::write_bytes(b"\n");
        0
    }
}

fn builtin_env(_args: &[String]) -> i32 {
    for (key, value) in std::env::vars() {
        shio::write_bytes(format!("{}={}\n", key, value).as_bytes());
    }
    0
}

fn builtin_date(_args: &[String]) -> i32 {
    match fs::read_to_string("/proc/uptime") {
        Ok(content) => {
            let uptime = content.split_whitespace().next().unwrap_or("0");
            shio::write_bytes(format!("uptime: {}s\n", uptime).as_bytes());
        }
        Err(_) => { shio::write_bytes(b"date: cannot read clock\n"); }
    }
    0
}

fn builtin_sleep(args: &[String]) -> i32 {
    let secs: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    std::thread::sleep(std::time::Duration::from_secs_f64(secs));
    0
}
