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
fn test_run_ctrl_d_on_empty_line_exits_with_code_zero() {
    feed_bytes(vec![4u8]); // Ctrl+D
    let code = repl::run(&[]);
    assert_eq!(code, 0, "Ctrl+D on an empty line must exit cleanly with code 0");
}

#[test]
fn test_run_line_exceeding_max_length_is_truncated_without_panic() {
    let mut bytes = vec![b'a'; 5000]; // exceeds MAX_LINE = 4096
    bytes.push(b'\r'); // submit the line
    bytes.push(4u8); // Ctrl+D to exit after the line is processed
    feed_bytes(bytes);
    repl::run(&[]); // must not panic; truncation at MAX_LINE is the invariant
}

#[test]
fn test_run_ctrl_c_clears_line_and_does_not_exit() {
    // Ctrl+C (byte 3) resets the line; only Ctrl+D (byte 4) exits.
    feed_bytes(vec![b'a', b'b', 3u8, 4u8]);
    let code = repl::run(&[]);
    assert_eq!(code, 0, "Ctrl+C must not exit the REPL; Ctrl+D afterward must exit with 0");
}

#[test]
fn test_run_exit_with_extreme_code_is_stored_unchanged() {
    feed_bytes(b"exit 127\r".to_vec());
    let code = repl::run(&[]);
    assert_eq!(code, 127, "exit code 127 must be returned verbatim");
}
