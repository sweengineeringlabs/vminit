//! Interactive shell REPL with line editing, history, and tab completion.

use crate::spi::{io, shell};

const MAX_HISTORY: usize = 128;
const MAX_LINE: usize = 4096;

fn cursor_left(n: usize) { for _ in 0..n { io::write_bytes(b"\x1b[D"); } }
fn cursor_right(n: usize) { for _ in 0..n { io::write_bytes(b"\x1b[C"); } }
fn clear_to_eol() { io::write_bytes(b"\x1b[K"); }

fn redraw_from(line: &[u8], pos: usize) {
    clear_to_eol();
    io::write_bytes(&line[pos..]);
    let tail = line.len() - pos;
    if tail > 0 { cursor_left(tail); }
}

struct History {
    entries: Vec<String>,
    browse_idx: usize,
    saved_line: String,
}

impl History {
    fn new() -> Self { History { entries: Vec::new(), browse_idx: 0, saved_line: String::new() } }
    fn add(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() { return; }
        if self.entries.last().map(|s| s.as_str()) == Some(trimmed) { return; }
        self.entries.push(trimmed.to_string());
        if self.entries.len() > MAX_HISTORY { self.entries.remove(0); }
    }
    fn reset_browse(&mut self, current_line: &str) {
        self.browse_idx = self.entries.len();
        self.saved_line = current_line.to_string();
    }
    fn prev(&mut self) -> Option<&str> {
        if self.browse_idx > 0 { self.browse_idx -= 1; Some(&self.entries[self.browse_idx]) }
        else { None }
    }
    fn next(&mut self) -> Option<&str> {
        if self.browse_idx < self.entries.len() {
            self.browse_idx += 1;
            if self.browse_idx == self.entries.len() { Some(&self.saved_line) }
            else { Some(&self.entries[self.browse_idx]) }
        } else { None }
    }
}

const BUILTIN_NAMES: &[&str] = &[
    "cat", "cd", "chmod", "cp", "date", "echo", "env", "exit", "export",
    "false", "grep", "head", "hostname", "ls", "mkdir", "mv", "printf",
    "rm", "sh", "sleep", "tail", "touch", "true", "uname", "wc",
];

fn complete(partial: &str) -> Vec<String> {
    let mut matches = Vec::new();
    for name in BUILTIN_NAMES {
        if name.starts_with(partial) { matches.push(name.to_string()); }
    }
    if partial.contains('/') || partial.starts_with('.') {
        let (dir, prefix) = if let Some(pos) = partial.rfind('/') {
            let dir = if pos == 0 { "/" } else { &partial[..pos] };
            (dir.to_string(), partial[pos + 1..].to_string())
        } else { (".".to_string(), partial.to_string()) };
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) {
                    let full = if dir == "/" { format!("/{}", name) }
                        else if dir == "." { name }
                        else { format!("{}/{}", dir, name) };
                    if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                        matches.push(format!("{}/", full));
                    } else { matches.push(full); }
                }
            }
        }
    } else if !partial.is_empty() {
        if let Ok(entries) = std::fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(partial) { matches.push(name); }
            }
        }
    }
    matches.sort();
    matches.dedup();
    matches
}

fn prompt() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".to_string());
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let host = std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "vminit".to_string()).trim().to_string();
    format!("{}@{}:{}# ", user, host, cwd)
}

fn read_line(history: &mut History) -> Option<String> {
    let mut line: Vec<u8> = Vec::with_capacity(MAX_LINE);
    let mut pos: usize = 0;
    let prompt_str = prompt();
    io::write_str(&prompt_str);
    history.reset_browse("");
    loop {
        let byte = match io::read_byte() { Some(b) => b, None => continue };
        match byte {
            4 if line.is_empty() => { io::write_str("\n"); return None; }
            3 => { io::write_str("^C\n"); io::write_str(&prompt_str); line.clear(); pos = 0; history.reset_browse(""); }
            1 => { cursor_left(pos); pos = 0; }
            5 => { cursor_right(line.len() - pos); pos = line.len(); }
            21 => { cursor_left(pos); clear_to_eol(); line.clear(); pos = 0; }
            11 => { line.truncate(pos); clear_to_eol(); }
            23 if pos > 0 => {
                let old_pos = pos;
                while pos > 0 && line[pos - 1] == b' ' { pos -= 1; }
                while pos > 0 && line[pos - 1] != b' ' { pos -= 1; }
                line.drain(pos..old_pos);
                cursor_left(old_pos - pos);
                redraw_from(&line, pos);
            }
            12 => { io::write_bytes(b"\x1b[2J\x1b[H"); io::write_str(&prompt_str); io::write_bytes(&line); cursor_left(line.len() - pos); }
            9 => {
                let line_str = String::from_utf8_lossy(&line).to_string();
                let word_start = line_str[..pos].rfind(' ').map(|i| i + 1).unwrap_or(0);
                let partial = &line_str[word_start..pos];
                let matches = complete(partial);
                if matches.len() == 1 {
                    let completion = &matches[0][partial.len()..];
                    let suffix = if !matches[0].ends_with('/') { " " } else { "" };
                    let insert = format!("{}{}", completion, suffix);
                    for b in insert.bytes() { line.insert(pos, b); pos += 1; }
                    io::write_str(&insert);
                    if pos < line.len() { redraw_from(&line, pos); }
                } else if matches.len() > 1 {
                    io::write_str("\n"); io::write_str(&matches.join("  ")); io::write_str("\n");
                    io::write_str(&prompt_str); io::write_bytes(&line); cursor_left(line.len() - pos);
                }
            }
            b'\r' | b'\n' => {
                io::write_str("\n");
                let line_str = String::from_utf8_lossy(&line).to_string();
                history.add(&line_str);
                return Some(line_str);
            }
            127 | 8 if pos > 0 => { pos -= 1; line.remove(pos); io::write_bytes(b"\x08"); redraw_from(&line, pos); }
            0x1b => {
                if let Some(b'[') = io::read_byte() {
                    match io::read_byte() {
                        Some(b'A') => { if let Some(e) = history.prev() { let e = e.to_string(); cursor_left(pos); clear_to_eol(); io::write_str(&e); line = e.into_bytes(); pos = line.len(); } }
                        Some(b'B') => { if let Some(e) = history.next() { let e = e.to_string(); cursor_left(pos); clear_to_eol(); io::write_str(&e); line = e.into_bytes(); pos = line.len(); } }
                        Some(b'C') if pos < line.len() => { cursor_right(1); pos += 1; }
                        Some(b'D') if pos > 0 => { cursor_left(1); pos -= 1; }
                        Some(b'H') => { cursor_left(pos); pos = 0; }
                        Some(b'F') => { cursor_right(line.len() - pos); pos = line.len(); }
                        Some(b'3') => { let _ = io::read_byte(); if pos < line.len() { line.remove(pos); redraw_from(&line, pos); } }
                        _ => {}
                    }
                }
            }
            32..=126 if line.len() < MAX_LINE => {
                line.insert(pos, byte); pos += 1;
                io::write_bytes(&[byte]);
                if pos < line.len() { redraw_from(&line, pos); }
            }
            _ => {}
        }
    }
}

fn handle_cd(args: &[String]) -> i32 {
    let target = if args.is_empty() {
        std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
    } else { args[0].clone() };
    match std::env::set_current_dir(&target) {
        Ok(_) => 0,
        Err(e) => { io::write_line(&format!("cd: {}: {}", target, e)); 1 }
    }
}

/// Run the interactive REPL. Returns exit code.
pub fn run(env: &[(String, String)]) -> i32 {
    std::env::set_var("TERM", "linux");
    std::env::set_var("PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");
    std::env::set_var("HOME", "/root");
    std::env::set_var("USER", "root");
    std::env::set_var("SHELL", "/bin/vminit-shell");
    for (k, v) in env { std::env::set_var(k, v); }
    let _ = std::fs::create_dir_all("/root");
    let _ = std::env::set_current_dir("/");
    io::write_line("vminit shell");
    io::write_line("Type 'exit' or Ctrl+D to quit\n");
    let mut history = History::new();
    let mut last_exit = 0i32;
    while let Some(line) = read_line(&mut history) {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let words: Vec<String> = trimmed.split_whitespace().map(String::from).collect();
        match words[0].as_str() {
            "exit" => { last_exit = words.get(1).and_then(|s| s.parse::<i32>().ok()).unwrap_or(last_exit); break; }
            "cd" => { last_exit = handle_cd(&words[1..]); }
            "export" => { for arg in &words[1..] { if let Some(p) = arg.find('=') { std::env::set_var(&arg[..p], &arg[p+1..]); } } last_exit = 0; }
            _ => { last_exit = shell::execute(trimmed); }
        }
    }
    last_exit
}
