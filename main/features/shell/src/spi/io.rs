//! Thread-local (std) or static fn-ptr (no_std) I/O sinks for shell output and input.
//!
//! Builtins write via write_bytes/write_str/write_line without carrying
//! a reference through every function signature. Defaults to stdout/stdin
//! when no sink is configured (standalone mode, std only).

// ── std implementation ────────────────────────────────────────────────────────

#[cfg(feature = "std")]
mod imp {
    use std::cell::RefCell;
    use std::io::Write;

    type OutputFn = Box<dyn Fn(&[u8]) + Send>;

    thread_local! {
        static OUTPUT: RefCell<Option<OutputFn>> = RefCell::new(None);
    }

    pub fn set_output(f: impl Fn(&[u8]) + Send + 'static) {
        OUTPUT.with(|out| {
            *out.borrow_mut() = Some(Box::new(f));
        });
    }

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

    type InputFn = Box<dyn Fn() -> Option<u8> + Send>;

    thread_local! {
        static INPUT: RefCell<Option<InputFn>> = RefCell::new(None);
    }

    pub fn set_input(f: impl Fn() -> Option<u8> + Send + 'static) {
        INPUT.with(|inp| {
            *inp.borrow_mut() = Some(Box::new(f));
        });
    }

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
}

// ── no_std implementation ─────────────────────────────────────────────────────
// Uses bare function pointers — closures with captures cannot be stored without
// alloc. This is safe for vminit which configures I/O once on a single thread
// before any fork.

#[cfg(not(feature = "std"))]
mod imp {
    static mut OUTPUT_FN: fn(&[u8]) = |_| {};
    static mut INPUT_FN: fn() -> Option<u8> = || None;

    pub fn set_output(f: fn(&[u8])) {
        unsafe { OUTPUT_FN = f; }
    }

    pub fn set_input(f: fn() -> Option<u8>) {
        unsafe { INPUT_FN = f; }
    }

    pub fn write_bytes(data: &[u8]) {
        unsafe { (OUTPUT_FN)(data); }
    }

    pub fn read_byte() -> Option<u8> {
        unsafe { (INPUT_FN)() }
    }
}

// ── unified public API ────────────────────────────────────────────────────────

/// Set the output sink.
///
/// In `std` mode: accepts any closure `impl Fn(&[u8]) + Send + 'static`.
/// In `no_std` mode: accepts a bare function pointer `fn(&[u8])`.
#[cfg(feature = "std")]
pub fn set_output(f: impl Fn(&[u8]) + Send + 'static) { imp::set_output(f); }

#[cfg(not(feature = "std"))]
pub fn set_output(f: fn(&[u8])) { imp::set_output(f); }

/// Set the input source.
///
/// In `std` mode: accepts any closure `impl Fn() -> Option<u8> + Send + 'static`.
/// In `no_std` mode: accepts a bare function pointer `fn() -> Option<u8>`.
#[cfg(feature = "std")]
pub fn set_input(f: impl Fn() -> Option<u8> + Send + 'static) { imp::set_input(f); }

#[cfg(not(feature = "std"))]
pub fn set_input(f: fn() -> Option<u8>) { imp::set_input(f); }

/// Write bytes to the current output sink.
pub fn write_bytes(data: &[u8]) { imp::write_bytes(data); }

/// Write a UTF-8 string to the current output sink.
pub fn write_str(s: &str) { write_bytes(s.as_bytes()); }

/// Write a string followed by newline.
pub fn write_line(s: &str) { write_str(s); write_bytes(b"\n"); }

/// Read one byte from the current input source.
pub fn read_byte() -> Option<u8> { imp::read_byte() }
