use std::sync::Mutex;
use swe_vminit_shell::ShellOutput;

struct VecOutput(Mutex<Vec<u8>>);

impl ShellOutput for VecOutput {
    fn write_bytes(&self, data: &[u8]) {
        self.0.lock().unwrap().extend_from_slice(data);
    }
}

#[test]
fn test_write_bytes_with_empty_slice_does_not_panic() {
    let out = VecOutput(Mutex::new(Vec::new()));
    out.write_bytes(&[]);
    assert_eq!(out.0.lock().unwrap().as_slice(), b"", "empty write must produce empty buffer");
}

#[test]
fn test_write_bytes_with_large_payload_does_not_panic() {
    let out = VecOutput(Mutex::new(Vec::new()));
    let payload = vec![b'x'; 65536];
    out.write_bytes(&payload);
    assert_eq!(out.0.lock().unwrap().len(), 65536, "large write must store all bytes");
}

#[test]
fn test_write_line_with_empty_string_outputs_only_newline() {
    let out = VecOutput(Mutex::new(Vec::new()));
    out.write_line("");
    assert_eq!(out.0.lock().unwrap().as_slice(), b"\n", "write_line(\"\") must produce only a newline");
}

#[test]
fn test_write_bytes_with_non_utf8_data_does_not_panic() {
    let out = VecOutput(Mutex::new(Vec::new()));
    let binary: Vec<u8> = (0u8..=255u8).collect();
    out.write_bytes(&binary);
    assert_eq!(out.0.lock().unwrap().len(), 256, "all 256 byte values must be stored verbatim");
}
