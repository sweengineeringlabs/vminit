//! Guest→host signal protocol.
//!
//! Defines the wire strings written to /dev/ttyS0 (serial mode) or
//! the magic u32 values written to physical address 0x500 (shm mode).

/// Serial signal indicating the guest init completed boot and is ready.
pub const READY_SIGNAL: &str = "XIKA_READY";

/// Serial signal prefix for exit notification. Full message: "XIKA_EXIT:<code>".
pub const EXIT_PREFIX: &str = "XIKA_EXIT:";

/// Shared memory physical address for shm signal mode.
pub const SHM_SIGNAL_ADDR: u64 = 0x500;

/// Magic u32 written to SHM_SIGNAL_ADDR for "ready".
/// Upper 16 bits are 0xBEEF; the distinct lower 16 bits (0xCAFE) ensure
/// this value can never collide with any shm_exit_value() result, whose
/// upper 16 bits are always 0xDEAD.
pub const SHM_MAGIC_READY: u32 = 0xBEEF_CAFE;

/// Upper 16-bit mask for exit values. shm_exit_value(code) = SHM_MAGIC_EXIT_MASK | (code & 0xFFFF).
/// Upper 16 bits 0xDEAD are distinct from SHM_MAGIC_READY's 0xBEEF prefix,
/// so no exit code can collide with the ready signal.
pub const SHM_MAGIC_EXIT_MASK: u32 = 0xDEAD_0000;

/// Format the serial READY signal string (includes trailing newline).
pub fn format_ready() -> &'static str {
    "XIKA_READY\n"
}

/// Format the serial EXIT signal string for a given exit code.
pub fn format_exit(code: i32) -> String {
    format!("XIKA_EXIT:{code}\n")
}

/// Parse the exit code from a serial EXIT line.
/// Returns None if the line is not a valid XIKA_EXIT message.
pub fn parse_exit(line: &str) -> Option<i32> {
    let trimmed = line.trim();
    let tail = trimmed.strip_prefix(EXIT_PREFIX)?;
    tail.parse::<i32>().ok()
}

/// Compute the shm u32 value for a given exit code.
pub fn shm_exit_value(code: i32) -> u32 {
    SHM_MAGIC_EXIT_MASK | (code as u32 & 0xFFFF)
}
