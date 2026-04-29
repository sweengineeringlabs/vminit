//! Guest→host signaling via serial port or shared memory.

use crate::ffi;
use swe_vminit_init::{SignalMode, SHM_MAGIC_READY, format_exit, shm_exit_value, SHM_SIGNAL_ADDR};

const SERIAL_PATH: &[u8] = b"/dev/ttyS0\0";

pub fn serial_write(data: &[u8]) {
    unsafe {
        let fd = ffi::open(SERIAL_PATH.as_ptr(), ffi::O_WRONLY, 0);
        if fd >= 0 {
            let mut offset = 0;
            while offset < data.len() {
                let n = ffi::write(fd, data[offset..].as_ptr(), data.len() - offset);
                if n <= 0 { break; }
                offset += n as usize;
            }
            ffi::close(fd);
        }
    }
}

pub fn signal_ready(mode: SignalMode) {
    match mode {
        SignalMode::Serial => serial_write(b"XIKA_READY\n"),
        SignalMode::SharedMemory => unsafe {
            core::ptr::write_volatile(SHM_SIGNAL_ADDR as *mut u32, SHM_MAGIC_READY);
        },
    }
}

pub fn report_exit(code: i32, mode: SignalMode) {
    match mode {
        SignalMode::Serial => serial_write(format_exit(code).as_bytes()),
        SignalMode::SharedMemory => unsafe {
            core::ptr::write_volatile(SHM_SIGNAL_ADDR as *mut u32, shm_exit_value(code));
        },
    }
}

pub fn log(msg: &str) {
    serial_write(b"[vminit] ");
    serial_write(msg.as_bytes());
    serial_write(b"\n");
}
