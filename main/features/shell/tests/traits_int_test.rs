use std::sync::Mutex;
use swe_vminit_shell::{ShellInput, ShellOutput};

struct VecOutput(Mutex<Vec<u8>>);

impl ShellOutput for VecOutput {
    fn write_bytes(&self, data: &[u8]) {
        self.0.lock().unwrap().extend_from_slice(data);
    }
}

#[test]
fn test_write_str_default_impl_delegates_to_write_bytes() {
    let out = VecOutput(Mutex::new(Vec::new()));
    out.write_str("hello");
    assert_eq!(out.0.lock().unwrap().as_slice(), b"hello");
}

#[test]
fn test_write_line_default_impl_appends_newline() {
    let out = VecOutput(Mutex::new(Vec::new()));
    out.write_line("hello");
    assert_eq!(out.0.lock().unwrap().as_slice(), b"hello\n");
}

#[test]
fn test_write_line_then_write_str_produce_correct_sequence() {
    let out = VecOutput(Mutex::new(Vec::new()));
    out.write_line("first");
    out.write_str("second");
    assert_eq!(out.0.lock().unwrap().as_slice(), b"first\nsecond");
}

struct SeqInput {
    data: Vec<u8>,
    pos: Mutex<usize>,
}

impl SeqInput {
    fn new(data: &[u8]) -> Self {
        SeqInput { data: data.to_vec(), pos: Mutex::new(0) }
    }
}

impl ShellInput for SeqInput {
    fn read_byte(&self) -> Option<u8> {
        let mut pos = self.pos.lock().unwrap();
        if *pos < self.data.len() {
            let b = self.data[*pos];
            *pos += 1;
            Some(b)
        } else {
            None
        }
    }
}

#[test]
fn test_shell_input_yields_bytes_in_order() {
    let input = SeqInput::new(b"abc");
    assert_eq!(input.read_byte(), Some(b'a'));
    assert_eq!(input.read_byte(), Some(b'b'));
    assert_eq!(input.read_byte(), Some(b'c'));
}

#[test]
fn test_shell_input_returns_none_when_exhausted() {
    let input = SeqInput::new(b"x");
    assert_eq!(input.read_byte(), Some(b'x'));
    assert_eq!(input.read_byte(), None);
    assert_eq!(input.read_byte(), None, "repeated reads past EOF must all return None");
}
