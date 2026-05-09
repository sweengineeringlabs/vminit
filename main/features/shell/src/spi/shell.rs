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

/// Scan `cmd` for shell redirections, returning `(clean_command, output_file, append)`.
///
/// Handles:
/// - `> file` / `>> file` — stdout redirect; file may be quoted
/// - `N> file` / `N>> file` — fd-numbered redirect (treated as stdout)
/// - `N>&M` / `>&M` — fd duplication; stripped silently
/// - `< file` / `N< file` — stdin redirect; stripped (not executed)
///
/// All redirections are removed from the returned command string.
fn parse_redirect(cmd: &str) -> (String, Option<String>, bool) {
    let chars: Vec<char> = cmd.chars().collect();
    let len = chars.len();
    let mut clean = String::new();
    let mut output_file: Option<String> = None;
    let mut append = false;
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Quoted word — copy verbatim to clean.
        if ch == '\'' {
            clean.push(ch);
            i += 1;
            while i < len && chars[i] != '\'' { clean.push(chars[i]); i += 1; }
            if i < len { clean.push(chars[i]); i += 1; }
            continue;
        }
        if ch == '"' {
            clean.push(ch);
            i += 1;
            while i < len && chars[i] != '"' { clean.push(chars[i]); i += 1; }
            if i < len { clean.push(chars[i]); i += 1; }
            continue;
        }

        // Detect redirect: optional leading digit + `>` or `<`.
        let has_fd_digit = ch.is_ascii_digit()
            && i + 1 < len
            && (chars[i + 1] == '>' || chars[i + 1] == '<');
        let op_idx = if has_fd_digit { i + 1 } else { i };

        if op_idx < len && (chars[op_idx] == '>' || chars[op_idx] == '<') {
            let is_out = chars[op_idx] == '>';
            let is_append = is_out && op_idx + 1 < len && chars[op_idx + 1] == '>';
            let after_op = if is_append { op_idx + 2 } else { op_idx + 1 };

            // Skip whitespace before target.
            let mut j = after_op;
            while j < len && (chars[j] == ' ' || chars[j] == '\t') { j += 1; }

            // `>&N` or `N>&N` — fd duplication; skip the `&N` token.
            if is_out && j < len && chars[j] == '&' {
                j += 1;
                while j < len && chars[j].is_ascii_digit() { j += 1; }
                i = j;
                continue;
            }

            // Parse the target filename (may be single- or double-quoted).
            let (filename, new_j) = parse_word_unquote(&chars, j);
            if is_out {
                output_file = Some(filename);
                append = is_append;
            }
            // Input redirects (`<`) are stripped with no other effect.
            i = new_j;
            continue;
        }

        clean.push(ch);
        i += 1;
    }

    (clean.trim().to_string(), output_file, append)
}

/// Parse one shell word starting at `start`, unquoting it.
/// Returns `(unquoted_content, index_after_word)`.
fn parse_word_unquote(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut word = String::new();
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' { word.push(chars[i]); i += 1; }
                if i < chars.len() { i += 1; }
            }
            '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != '\'' { word.push(chars[i]); i += 1; }
                if i < chars.len() { i += 1; }
            }
            ' ' | '\t' | '>' | '<' | '|' | ';' | '&' => break,
            c => { word.push(c); i += 1; }
        }
    }
    (word, i)
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

    #[test]
    fn test_parse_redirect_fd_duplication_drops_redirect_and_strips_fd_digit() {
        // PostgreSQL initdb uses popen("\"<path>\" --version 2>&1", "r").
        // '2>&1' must not redirect stdout to a file named '&1' and must not
        // leave the stray '2' as an argument.
        let (cmd, file, _) = parse_redirect(
            r#""/nix/store/abc-postgresql-16.9/bin/postgres" --version 2>&1"#,
        );
        assert_eq!(cmd, r#""/nix/store/abc-postgresql-16.9/bin/postgres" --version"#);
        assert!(file.is_none(), "fd duplication must not produce a file redirect");
    }

    #[test]
    fn test_parse_redirect_plain_stdout_redirect_unaffected() {
        let (cmd, file, append) = parse_redirect("echo hi > /tmp/out");
        assert_eq!(cmd, "echo hi");
        assert_eq!(file.unwrap(), "/tmp/out");
        assert!(!append);
    }

    #[test]
    fn test_parse_redirect_quoted_filename_unquoted() {
        // PostgreSQL initdb: `"postgres" --check ... > "/dev/null" 2>&1`
        // Quoted /dev/null must be unquoted so File::create works.
        let (cmd, file, append) = parse_redirect(
            r#""/bin/echo" --check > "/dev/null" 2>&1"#,
        );
        assert_eq!(cmd, r#""/bin/echo" --check"#);
        assert_eq!(file.unwrap(), "/dev/null");
        assert!(!append);
    }

    #[test]
    fn test_parse_redirect_input_redirect_stripped_from_command() {
        // `< "/dev/null"` must be stripped; stdout redirect kept.
        let (cmd, file, _) = parse_redirect(
            r#""/bin/echo" --check < "/dev/null" > "/dev/null" 2>&1"#,
        );
        assert_eq!(cmd, r#""/bin/echo" --check"#);
        assert_eq!(file.unwrap(), "/dev/null");
    }
}
