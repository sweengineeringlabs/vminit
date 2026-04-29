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
fn test_dispatch_cat_nonexistent_file_returns_one_with_error_message() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["cat", "/nonexistent_vminit_test_xyz_abc"])).unwrap());
    assert_eq!(code, 1, "cat of nonexistent file must return exit code 1");
    assert!(out.contains("cat:"), "error output must identify the command as 'cat:'");
}

#[test]
fn test_dispatch_cp_with_single_arg_returns_one() {
    let (_, code) = capture(|| builtins::dispatch(&args(&["cp", "only_one_arg"])).unwrap());
    assert_eq!(code, 1, "cp with only one argument must return 1");
}

#[test]
fn test_dispatch_mv_with_single_arg_returns_one() {
    let (_, code) = capture(|| builtins::dispatch(&args(&["mv", "only_one_arg"])).unwrap());
    assert_eq!(code, 1, "mv with only one argument must return 1");
}

#[test]
fn test_dispatch_echo_with_shell_metacharacters_prints_literally() {
    let (out, code) = capture(|| builtins::dispatch(&args(&["echo", "$(id)", "`whoami`", "${USER}"])).unwrap());
    assert_eq!(code, 0);
    assert_eq!(
        out, "$(id) `whoami` ${USER}\n",
        "builtin echo must not evaluate shell metacharacters"
    );
}

#[test]
fn test_dispatch_chmod_invalid_mode_string_returns_one() {
    let (_, code) = capture(|| builtins::dispatch(&args(&["chmod", "not_octal", "/tmp"])).unwrap());
    assert_eq!(code, 1, "chmod with non-octal mode must return 1");
}

#[test]
fn test_dispatch_grep_missing_pattern_returns_two() {
    let (_, code) = capture(|| builtins::dispatch(&args(&["grep"])).unwrap());
    assert_eq!(code, 2, "grep with no pattern must return exit code 2");
}
