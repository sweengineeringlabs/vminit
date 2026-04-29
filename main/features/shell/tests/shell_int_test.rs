use swe_vminit_shell::shell;

#[test]
fn test_execute_echo_returns_zero() {
    let code = shell::execute("echo hello");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_true_returns_zero() {
    assert_eq!(shell::execute("true"), 0);
}

#[test]
fn test_execute_false_returns_one() {
    assert_eq!(shell::execute("false"), 1);
}

#[test]
fn test_execute_and_chain_skips_second_on_failure() {
    let code = shell::execute("false && echo should_not_run");
    assert_ne!(code, 0, "false && cmd must not execute the second command");
}

#[test]
fn test_execute_and_chain_runs_second_on_success() {
    let code = shell::execute("true && true");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_or_chain_runs_second_on_failure() {
    let code = shell::execute("false || true");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_or_chain_skips_second_on_success() {
    let code = shell::execute("true || false");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_semicolon_runs_both_commands() {
    let code = shell::execute("true; true");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_comment_is_ignored() {
    let code = shell::execute("# this is a comment");
    assert_eq!(code, 0);
}

#[test]
fn test_execute_empty_string_returns_zero() {
    assert_eq!(shell::execute(""), 0);
}

#[test]
fn test_execute_variable_expansion() {
    std::env::set_var("VMINIT_TEST_VAR", "hello");
    let code = shell::execute("echo $VMINIT_TEST_VAR");
    assert_eq!(code, 0);
    std::env::remove_var("VMINIT_TEST_VAR");
}
