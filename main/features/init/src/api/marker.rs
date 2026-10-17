//! The capability marker `vmfs` carries, and the one list of capabilities it names.
//!
//! The marker is an ELF note in the `vmfs` file. The host reads it without executing
//! the file and checks the capability a request needs before it boots a guest. The
//! byte layout is specified in `docs/xkvm_conf.md` ("Capability marker").

use alloc::string::String;
use alloc::vec::Vec;

/// The `/etc/vminit.conf` keys this build understands, which are its capabilities.
///
/// This is the single source of truth: the parser refuses every key that is not in
/// this list, the marker embeds this list, and the serial banner prints it. Adding a
/// parser arm without adding the key here makes the arm unreachable, which the
/// per-capability parser tests catch; adding a key here without a parser arm is
/// refused by the parser as well (see `parse_config`).
///
/// A capability is a key, not a version: a host that needs `entrypoint_hex` checks for
/// that name and does not compare version numbers.
pub const CAPABILITIES: &[&str] = &[
    "entrypoint",
    "entrypoint_hex",
    "env",
    "volume",
    "block_mount",
    "install",
    "interactive",
    "start_agent",
    "mount_rootfs",
    "signal_mode",
    "network",
    "manifest",
    "manifest_url",
    "manifest_hash",
    "cache_base",
];

/// Name of the ELF section that holds the marker.
pub const MARKER_SECTION: &str = ".note.vminit";

/// ELF note owner (the note's "name" field, without its NUL terminator).
pub const MARKER_NOTE_NAME: &str = "vminit";

/// ELF note type of the marker, scoped to `MARKER_NOTE_NAME`.
pub const MARKER_NOTE_TYPE: u32 = 1;

/// Version of the marker's payload layout. A reader that does not know the value it
/// finds must report "unsupported format" and not guess at the bytes.
pub const MARKER_FORMAT_VERSION: u32 = 1;

/// What a `vmfs` file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Layout version of the payload the values were read from.
    pub format_version: u32,
    /// Package version of the build, for example `0.1.6`.
    pub version: String,
    /// Config keys the build understands, in the order of `CAPABILITIES`.
    pub capabilities: Vec<String>,
}

impl Marker {
    /// Whether the build lists `capability`.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|name| name == capability)
    }
}

/// Why a `vmfs` file's marker could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerError {
    /// The bytes are not a little-endian ELF64 file.
    NotElf64,
    /// A header or note ends past the end of the file or segment.
    Truncated,
    /// The file is a valid ELF64 file with no `vminit` note: a `vmfs` that predates
    /// the marker (v0.1.5 and earlier), whose capabilities are unknown.
    NoMarker,
    /// The note's payload has a layout version this reader does not know.
    UnsupportedFormat(u32),
    /// The note is present but its payload does not follow the layout it declares.
    Malformed(&'static str),
}
