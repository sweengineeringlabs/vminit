//! Thread-local I/O sinks for shell output and input.
//!
//! Builtins write via write_bytes/write_str/write_line without carrying
//! a reference through every function signature. Defaults to stdout/stdin
//! when no sink is configured (standalone mode).

use std::cell::RefCell;
use std::io::Write;

type OutputFn = Box<dyn Fn(&[u8]) + Send>;

thread_local! {
    static OUTPUT: RefCell<Option<OutputFn>> = RefCell::new(None);
}

/// Set the output sink for the current thread.
pub fn set_output(f: impl Fn(&[u8]) + Send + 'static) {
    OUTPUT.with(|out| {
        *out.borrow_mut() = Some(Box::new(f));
    });
}

/// Write bytes to the current output sink, falling back to stdout.
pub fn write_bytes(data: &[u8]) {
    OUTPUT.with(|out| {
        let borrowed = out.borrow();
        if let Some(ref f) = *borrowed {
            f(data);
        } else {
            let _ = std::io::stdout().write_all(data);
            let _ = std::io::stdout().flush();
        }
    });
}

/// Write a UTF-8 string to the current output sink.
pub fn write_str(s: &str) {
    write_bytes(s.as_bytes());
}

/// Write a string followed by newline.
pub fn write_line(s: &str) {
    write_str(s);
    write_bytes(b"\n");
}

type InputFn = Box<dyn Fn() -> Option<u8> + Send>;

thread_local! {
    static INPUT: RefCell<Option<InputFn>> = RefCell::new(None);
}

/// Set the input source for the current thread.
pub fn set_input(f: impl Fn() -> Option<u8> + Send + 'static) {
    INPUT.with(|inp| {
        *inp.borrow_mut() = Some(Box::new(f));
    });
}

/// Read one byte from the current input source, falling back to stdin.
pub fn read_byte() -> Option<u8> {
    INPUT.with(|inp| {
        let borrowed = inp.borrow();
        if let Some(ref f) = *borrowed {
            f()
        } else {
            let mut buf = [0u8; 1];
            use std::io::Read;
            match std::io::stdin().read(&mut buf) {
                Ok(1) => Some(buf[0]),
                _ => None,
            }
        }
    })
}
