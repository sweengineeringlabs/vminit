use std::sync::{Arc, Mutex};
use swe_vminit_shell::io;

#[test]
fn test_set_output_replaces_previous_sink() {
    // Bug this catches: set_output not replacing the old sink, causing
    // output to go to a stale/dropped closure.
    let first: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let second: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let c1 = Arc::clone(&first);
    io::set_output(move |data| { c1.lock().unwrap().extend_from_slice(data); });
    let c2 = Arc::clone(&second);
    io::set_output(move |data| { c2.lock().unwrap().extend_from_slice(data); });
    io::write_bytes(b"after");
    assert!(first.lock().unwrap().is_empty(), "first sink must not receive data after replacement");
    assert_eq!(*second.lock().unwrap(), b"after");
}

#[test]
fn test_write_bytes_empty_slice_does_not_panic() {
    io::set_output(|_| {});
    io::write_bytes(b"");
}

#[test]
fn test_write_bytes_large_payload_does_not_panic() {
    io::set_output(|_| {});
    io::write_bytes(&vec![0u8; 1_000_000]);
}
