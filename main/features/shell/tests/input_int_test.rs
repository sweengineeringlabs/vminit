use swe_vminit_shell::{io, wire_default_io};

#[test]
fn test_wire_default_io_input_source_can_be_replaced_after_wiring() {
    wire_default_io();
    // Override immediately so the test does not block on stdin.
    io::set_input(|| Some(b'z'));
    let b = io::read_byte();
    assert_eq!(b, Some(b'z'), "custom input installed after wire_default_io must be used");
    io::set_input(|| None);
}

#[test]
fn test_wire_default_io_input_source_replaced_with_eof_returns_none() {
    wire_default_io();
    io::set_input(|| None);
    let b = io::read_byte();
    assert_eq!(b, None, "EOF input source must return None");
}
