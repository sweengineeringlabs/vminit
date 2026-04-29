use swe_vminit_shell::{io, wire_default_io};

#[test]
fn test_wire_default_io_output_sink_accepts_bytes_without_panic() {
    wire_default_io();
    io::write_bytes(b"output_test");
    io::set_output(|_| {});
}

#[test]
fn test_wire_default_io_output_sink_accepts_empty_write() {
    wire_default_io();
    io::write_bytes(b"");
    io::set_output(|_| {});
}

#[test]
fn test_wire_default_io_output_can_be_replaced_after_wiring() {
    wire_default_io();
    let wrote = std::sync::Arc::new(std::sync::Mutex::new(false));
    let wrote2 = wrote.clone();
    io::set_output(move |_| *wrote2.lock().unwrap() = true);
    io::write_bytes(b"x");
    assert!(*wrote.lock().unwrap(), "custom sink installed after wire_default_io must receive writes");
    io::set_output(|_| {});
}
