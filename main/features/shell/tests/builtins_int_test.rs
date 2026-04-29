use std::sync::{Arc, Mutex};
use swe_vminit_shell::{builtins, io};

fn capture(f: impl FnOnce() -> i32) -> (String, i32) {
    let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let buf2 = buf.clone();
    io::set_output(move |data| buf2.lock().unwrap().extend_from_slice(data));
    let code = f();
    io::set_output(|_| {});
    let out = String::from_utf8_lossy(&buf.lock().unwrap()).into_owned();
    (out, code)
}

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_dispatch_echo_outputs_args_joined_by_space() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["echo", "hello", "world"])).unwrap());
    assert_eq!(code, 0);
    assert_eq!(out, "hello world\n");
}

#[test]
fn test_dispatch_echo_no_newline_flag_suppresses_trailing_newline() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["echo", "-n", "text"])).unwrap());
    assert_eq!(code, 0);
    assert_eq!(out, "text", "echo -n must not append a newline");
}

#[test]
fn test_dispatch_true_returns_zero() {
    let code = builtins::dispatch(&args(&["true"])).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_dispatch_false_returns_one() {
    let code = builtins::dispatch(&args(&["false"])).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_dispatch_empty_args_returns_zero() {
    let code = builtins::dispatch(&[]).unwrap();
    assert_eq!(code, 0, "dispatch([]) must return Some(0)");
}

#[test]
fn test_dispatch_unknown_command_returns_none() {
    let result = builtins::dispatch(&args(&["definitely_not_a_builtin_xyz"]));
    assert!(result.is_none(), "unknown command must not be dispatched as a builtin");
}

#[test]
fn test_dispatch_printf_substitutes_string_argument() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["printf", "%s\n", "world"])).unwrap());
    assert_eq!(code, 0);
    assert_eq!(out, "world\n");
}

#[test]
fn test_dispatch_exit_returns_given_code() {
    let code = builtins::dispatch(&args(&["exit", "7"])).unwrap();
    assert_eq!(code, 7, "exit N must return N");
}

#[test]
fn test_dispatch_exit_without_code_returns_zero() {
    let code = builtins::dispatch(&args(&["exit"])).unwrap();
    assert_eq!(code, 0, "bare exit must return 0");
}

#[test]
fn test_dispatch_uname_default_outputs_linux() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["uname"])).unwrap());
    assert_eq!(code, 0);
    assert!(out.contains("Linux"), "uname must output 'Linux', got: {out}");
}
