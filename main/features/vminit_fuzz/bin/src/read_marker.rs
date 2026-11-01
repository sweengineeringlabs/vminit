#![no_main]

use libfuzzer_sys::fuzz_target;
use swe_vminit_init::{read_marker, MARKER_FORMAT_VERSION};

// Contract (spi/marker.rs): every offset and length is bounds-checked, so a truncated or
// hostile file yields an error, never a panic.
fuzz_target!(|data: &[u8]| {
    if let Ok(marker) = read_marker(data) {
        // A payload whose layout version the reader does not know is `UnsupportedFormat`,
        // so a marker that was read has the version this build reads. If this ever fails
        // because the reader accepts several versions, the assertion is what to change.
        assert_eq!(marker.format_version, MARKER_FORMAT_VERSION);
    }
});
