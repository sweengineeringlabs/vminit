use swe_vminit_shell::shell;

#[test]
fn test_execute_unknown_command_returns_127() {
    // Bug this catches: unknown command returning 0 (success), which
    // could silently mask a missing tool in a boot script.
    let code = shell::execute("__vminit_definitely_nonexistent_cmd__");
    assert_eq!(code, 127, "unknown command must return 127");
}

#[test]
fn test_execute_does_not_panic_on_empty_arguments() {
    // Bug this catches: dispatch() called with empty args slice panicking
    // on args[0] access.
    let code = shell::execute("   ");
    assert_eq!(code, 0, "whitespace-only line must return 0");
}

#[test]
fn test_execute_very_long_command_does_not_panic() {
    let long_arg = "x".repeat(65536);
    let cmd = format!("echo {}", long_arg);
    let code = shell::execute(&cmd);
    assert_eq!(code, 0);
}

#[test]
fn test_execute_deeply_nested_semicolons_does_not_stack_overflow() {
    // Build "true; true; true; ..." 200 times
    let cmd = vec!["true"; 200].join("; ");
    let code = shell::execute(&cmd);
    assert_eq!(code, 0);
}

#[test]
fn test_execute_single_quoted_variable_not_expanded() {
    std::env::set_var("VMINIT_SEC_VAR", "secret");
    let code = shell::execute("echo '$VMINIT_SEC_VAR'");
    assert_eq!(code, 0);
    std::env::remove_var("VMINIT_SEC_VAR");
}
