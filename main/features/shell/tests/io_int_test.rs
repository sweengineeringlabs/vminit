use std::sync::{Arc, Mutex};
use swe_vminit_shell::io;

#[test]
fn test_set_output_captures_write_bytes() {
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    io::set_output(move |data| {
        cap.lock().unwrap().extend_from_slice(data);
    });
    io::write_bytes(b"hello");
    assert_eq!(*captured.lock().unwrap(), b"hello");
}

#[test]
fn test_write_str_sends_utf8_bytes() {
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    io::set_output(move |data| { cap.lock().unwrap().extend_from_slice(data); });
    io::write_str("world");
    assert_eq!(*captured.lock().unwrap(), b"world");
}

#[test]
fn test_write_line_appends_newline() {
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    io::set_output(move |data| { cap.lock().unwrap().extend_from_slice(data); });
    io::write_line("hi");
    assert_eq!(*captured.lock().unwrap(), b"hi\n");
}
