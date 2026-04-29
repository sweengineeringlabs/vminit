use std::sync::{Arc, Mutex};
use swe_vminit_shell::{io, repl};

fn feed_bytes(bytes: Vec<u8>) {
    let pos = Arc::new(Mutex::new(0usize));
    let data = Arc::new(bytes);
    let pos2 = pos.clone();
    let data2 = data.clone();
    io::set_input(move || {
        let mut p = pos2.lock().unwrap();
        if *p < data2.len() {
            let b = data2[*p];
            *p += 1;
            Some(b)
        } else {
            None
        }
    });
    io::set_output(|_| {});
}

#[test]
fn test_run_exits_on_immediate_eof_with_code_zero() {
    feed_bytes(vec![]);
    let code = repl::run(&[]);
    assert_eq!(code, 0, "REPL must exit 0 when stdin is immediately EOF");
}

#[test]
fn test_run_exit_command_returns_specified_code() {
    feed_bytes(b"exit 5\r".to_vec());
    let code = repl::run(&[]);
    assert_eq!(code, 5, "exit N must return exit code N");
}

#[test]
fn test_run_exit_without_code_returns_zero() {
    feed_bytes(b"exit\r".to_vec());
    let code = repl::run(&[]);
    assert_eq!(code, 0, "bare exit must return 0");
}

#[test]
fn test_run_true_builtin_returns_zero() {
    feed_bytes(b"true\r".to_vec());
    let code = repl::run(&[]);
    assert_eq!(code, 0, "true followed by EOF must exit 0");
}

#[test]
fn test_run_false_builtin_sets_last_exit_to_one() {
    // false sets last_exit=1, then EOF exits the loop returning last_exit=1
    feed_bytes(b"false\r".to_vec());
    let code = repl::run(&[]);
    assert_eq!(code, 1, "false followed by EOF must exit 1");
}

#[test]
fn test_run_accepts_env_pairs_and_exposes_them_as_env_vars() {
    feed_bytes(vec![]);
    let env = [("VMINIT_REPL_TEST_42".to_string(), "expected_value".to_string())];
    repl::run(&env);
    assert_eq!(
        std::env::var("VMINIT_REPL_TEST_42").unwrap(),
        "expected_value",
        "env pairs passed to run() must be set as process env vars"
    );
    std::env::remove_var("VMINIT_REPL_TEST_42");
}
