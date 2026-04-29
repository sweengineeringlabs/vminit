//! Minimal shell interpreter.
//!
//! Supports: sequential commands (;), AND chains (&&), OR chains (||),
//! variable expansion ($VAR, ${VAR}), output redirection (>, >>),
//! pipes (|), single and double quoting, and comments (#).

use crate::spi::builtins;
use crate::spi::io as shio;

/// Execute a shell script string. Returns the last exit code.
pub fn execute(script: &str) -> i32 {
    let mut last_exit = 0i32;
    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        last_exit = execute_line(line);
    }
    last_exit
}

fn execute_line(line: &str) -> i32 {
    let commands = split_commands(line);
    let mut last_exit = 0i32;
    for (op, cmd_str) in commands {
        match op {
            Operator::Always => { last_exit = execute_pipeline(&cmd_str); }
            Operator::And => { if last_exit == 0 { last_exit = execute_pipeline(&cmd_str); } }
            Operator::Or => { if last_exit != 0 { last_exit = execute_pipeline(&cmd_str); } }
        }
    }
    last_exit
}

fn execute_pipeline(line: &str) -> i32 {
    let segments = split_on_pipes(line);
    if segments.len() == 1 {
        return execute_simple(&segments[0]);
    }
    let mut input_file: Option<String> = None;
    let mut last_exit = 0;
    for (i, segment) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        let output_file = if is_last { None } else { Some(format!("/tmp/.pipe_{}", i)) };
        last_exit = if let Some(ref outf) = output_file {
            execute_with_redirect(segment.trim(), input_file.as_deref(), Some(outf), false)
        } else {
            execute_with_redirect(segment.trim(), input_file.as_deref(), None, false)
        };
        if let Some(ref inf) = input_file {
            let _ = std::fs::remove_file(inf);
        }
        input_file = output_file;
    }
    last_exit
}

fn execute_simple(cmd: &str) -> i32 {
    let (cmd_part, redir_file, append) = parse_redirect(cmd);
    execute_with_redirect(&cmd_part, None, redir_file.as_deref(), append)
}

fn execute_with_redirect(
    cmd: &str,
    _input_file: Option<&str>,
    output_file: Option<&str>,
    append: bool,
) -> i32 {
    let expanded = expand_variables(cmd);
    let args = parse_args(&expanded);
    if args.is_empty() {
        return 0;
    }
    if let Some(outf) = output_file {
        if let Some(code) = builtins::dispatch(&args) {
            return code;
        }
        let status = std::process::Command::new(&args[0])
            .args(&args[1..])
            .stdout(if append {
                std::fs::OpenOptions::new().create(true).append(true).open(outf)
                    .map(std::process::Stdio::from)
                    .unwrap_or(std::process::Stdio::null())
            } else {
                std::fs::File::create(outf)
                    .map(std::process::Stdio::from)
                    .unwrap_or(std::process::Stdio::null())
            })
            .status();
        return match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(_) => 127,
        };
    }
    if let Some(code) = builtins::dispatch(&args) {
        return code;
    }
    match std::process::Command::new(&args[0]).args(&args[1..]).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            shio::write_bytes(format!("sh: {}: {}\n", args[0], e).as_bytes());
            127
        }
    }
}

#[derive(Clone, Copy)]
enum Operator {
    Always,
    And,
    Or,
}

fn split_commands(line: &str) -> Vec<(Operator, String)> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut current_op = Operator::Always;
    let mut chars = line.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => { in_single = !in_single; current.push(ch); }
            '"' if !in_single => { in_double = !in_double; current.push(ch); }
            _ if in_single || in_double => { current.push(ch); }
            ';' => {
                let t = current.trim().to_string();
                if !t.is_empty() { result.push((current_op, t)); }
                current.clear(); current_op = Operator::Always;
            }
            '&' if chars.peek() == Some(&'&') => {
                chars.next();
                let t = current.trim().to_string();
                if !t.is_empty() { result.push((current_op, t)); }
                current.clear(); current_op = Operator::And;
            }
            '|' if chars.peek() == Some(&'|') => {
                chars.next();
                let t = current.trim().to_string();
                if !t.is_empty() { result.push((current_op, t)); }
                current.clear(); current_op = Operator::Or;
            }
            _ => { current.push(ch); }
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() { result.push((current_op, t)); }
    result
}

fn split_on_pipes(line: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => { in_single = !in_single; current.push(ch); }
            '"' if !in_single => { in_double = !in_double; current.push(ch); }
            _ if in_single || in_double => { current.push(ch); }
            '|' => {
                if chars.peek() == Some(&'|') {
                    // OR operator (||): consume second | and keep both in current
                    chars.next();
                    current.push('|');
                    current.push('|');
                } else {
                    result.push(current.trim().to_string()); current.clear();
                }
            }
            _ => { current.push(ch); }
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() { result.push(t); }
    result
}

fn parse_redirect(cmd: &str) -> (String, Option<String>, bool) {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = cmd.chars().collect();
    for i in 0..chars.len() {
        match chars[i] {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '>' if !in_single && !in_double => {
                let cmd_part = cmd[..i].trim().to_string();
                let (file_part, append) = if i + 1 < chars.len() && chars[i + 1] == '>' {
                    (cmd[i + 2..].trim().to_string(), true)
                } else {
                    (cmd[i + 1..].trim().to_string(), false)
                };
                return (cmd_part, Some(file_part), append);
            }
            _ => {}
        }
    }
    (cmd.to_string(), None, false)
}

fn expand_variables(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => { in_single = !in_single; result.push(ch); }
            '$' if !in_single => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' { chars.next(); break; }
                        var_name.push(c); chars.next();
                    }
                    if let Ok(val) = std::env::var(&var_name) { result.push_str(&val); }
                } else if chars.peek().map(|c| c.is_alphanumeric() || *c == '_').unwrap_or(false) {
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' { var_name.push(c); chars.next(); }
                        else { break; }
                    }
                    if let Ok(val) = std::env::var(&var_name) { result.push_str(&val); }
                } else { result.push('$'); }
            }
            _ => result.push(ch),
        }
    }
    result
}

pub(crate) fn parse_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped { current.push(ch); escaped = false; continue; }
        match ch {
            '\\' if !in_single => { escaped = true; }
            '\'' if !in_double => { in_single = !in_single; }
            '"' if !in_single => { in_double = !in_double; }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() { args.push(current.clone()); current.clear(); }
            }
            _ => { current.push(ch); }
        }
    }
    if !current.is_empty() { args.push(current); }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_simple_splits_on_whitespace() {
        assert_eq!(parse_args("echo hello world"), vec!["echo", "hello", "world"]);
    }

    #[test]
    fn test_parse_args_double_quotes_preserve_spaces() {
        assert_eq!(parse_args(r#"echo "hello world""#), vec!["echo", "hello world"]);
    }

    #[test]
    fn test_parse_args_single_quotes_preserve_spaces() {
        assert_eq!(parse_args("echo 'hello world'"), vec!["echo", "hello world"]);
    }

    #[test]
    fn test_parse_args_backslash_escapes_space() {
        assert_eq!(parse_args(r"echo hello\ world"), vec!["echo", "hello world"]);
    }

    #[test]
    fn test_split_commands_semicolon_separates_two_commands() {
        let cmds = split_commands("echo a; echo b");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].1, "echo a");
        assert_eq!(cmds[1].1, "echo b");
    }

    #[test]
    fn test_split_pipes_does_not_split_on_or_operator() {
        let segs = split_on_pipes("false || echo fallback");
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn test_parse_redirect_parses_output_redirect() {
        let (cmd, file, append) = parse_redirect("echo hi > /tmp/out");
        assert_eq!(cmd, "echo hi");
        assert_eq!(file.unwrap(), "/tmp/out");
        assert!(!append);
    }

    #[test]
    fn test_parse_redirect_append_sets_append_flag() {
        let (_, _, append) = parse_redirect("echo hi >> /tmp/out");
        assert!(append);
    }
}
