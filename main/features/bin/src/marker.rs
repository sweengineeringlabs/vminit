//! The capability marker: an ELF note in the `.note.vminit` section of `vmfs`.
//!
//! The note is built at compile time from this package's version and
//! `swe_vminit_init::CAPABILITIES`, so it cannot differ from what the parser
//! accepts. It is `#[used]` so rustc keeps it although nothing references it, and
//! it is allocated (read-only data), so the linker puts it in a `PT_NOTE` segment,
//! which `strip` and the release profile's `strip = true` leave in place.

use swe_vminit_init::{encode_marker, marker_len, CAPABILITIES};

/// Version of this build: the package version in `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const NOTE_LEN: usize = marker_len(VERSION, CAPABILITIES);

/// An ELF note must start on a 4-byte boundary.
#[repr(C, align(4))]
struct AlignedNote([u8; NOTE_LEN]);

// The section name must equal swe_vminit_init::MARKER_SECTION; the attribute cannot
// take a constant, so the integration test checks the built file for that name.
#[link_section = ".note.vminit"]
#[used]
static NOTE: AlignedNote = AlignedNote(encode_marker::<NOTE_LEN>(VERSION, CAPABILITIES));
