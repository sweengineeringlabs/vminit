//! Encoding, reading and printing of the capability marker.
//!
//! `encode_marker` is a `const fn`, so the `bin` crate builds the whole ELF note at
//! compile time from `CARGO_PKG_VERSION` and `CAPABILITIES` and places it in the
//! `.note.vminit` section. `read_marker` is the reader a host uses on a `vmfs` file.
//!
//! Note layout (ELF note, all integers little-endian, as the file is x86_64):
//!
//! ```text
//! u32 namesz   = 7                 "vminit" plus its NUL
//! u32 descsz   = payload length    not counting padding
//! u32 type     = 1
//! name         "vminit\0", padded with zeros to a multiple of 4
//! payload      descsz bytes, padded with zeros to a multiple of 4
//! ```
//!
//! Payload, format version 1:
//!
//! ```text
//! u32 format_version = 1
//! u16 version_len,  version_len bytes of UTF-8 (the package version)
//! u16 capability_count
//! capability_count times: u16 length, length bytes of UTF-8
//! ```

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::api::marker::{
    Marker, MarkerError, CAPABILITIES, MARKER_FORMAT_VERSION, MARKER_NOTE_NAME, MARKER_NOTE_TYPE,
};

/// Bytes of the note header: namesz, descsz, type.
const NOTE_HEADER_LEN: usize = 12;

/// ELF note name field length: "vminit" and its NUL, which is 7 and pads to 8.
const NOTE_NAME_FIELD_LEN: usize = 8;

/// Note fields are padded to a multiple of this many bytes.
const NOTE_ALIGN: usize = 4;

const PT_NOTE: u32 = 4;
const ELF_HEADER_LEN: usize = 64;
const PROGRAM_HEADER_LEN: usize = 56;

const fn pad_to_align(len: usize) -> usize {
    (len + NOTE_ALIGN - 1) / NOTE_ALIGN * NOTE_ALIGN
}

const fn payload_len(version: &str, capabilities: &[&str]) -> usize {
    let mut len = 4 + 2 + version.len() + 2;
    let mut index = 0;
    while index < capabilities.len() {
        len += 2 + capabilities[index].len();
        index += 1;
    }
    len
}

/// Length in bytes of the whole note `encode_marker` produces for these inputs. The
/// caller passes it as the array length: `encode_marker::<{ marker_len(V, C) }>(V, C)`.
pub const fn marker_len(version: &str, capabilities: &[&str]) -> usize {
    NOTE_HEADER_LEN + NOTE_NAME_FIELD_LEN + pad_to_align(payload_len(version, capabilities))
}

const fn put_u16(out: &mut [u8], at: usize, value: u16) -> usize {
    out[at] = value as u8;
    out[at + 1] = (value >> 8) as u8;
    at + 2
}

const fn put_u32(out: &mut [u8], at: usize, value: u32) -> usize {
    out[at] = value as u8;
    out[at + 1] = (value >> 8) as u8;
    out[at + 2] = (value >> 16) as u8;
    out[at + 3] = (value >> 24) as u8;
    at + 4
}

const fn put_bytes(out: &mut [u8], at: usize, bytes: &[u8]) -> usize {
    let mut index = 0;
    while index < bytes.len() {
        out[at + index] = bytes[index];
        index += 1;
    }
    at + bytes.len()
}

const fn put_string(out: &mut [u8], at: usize, text: &str) -> usize {
    assert!(
        text.len() <= u16::MAX as usize,
        "marker string longer than 65535 bytes"
    );
    let at = put_u16(out, at, text.len() as u16);
    put_bytes(out, at, text.as_bytes())
}

/// Encode the marker note. `N` must equal `marker_len(version, capabilities)`; a
/// mismatch is a compile error when called in a `const` context.
pub const fn encode_marker<const N: usize>(version: &str, capabilities: &[&str]) -> [u8; N] {
    assert!(
        N == marker_len(version, capabilities),
        "N must be marker_len(version, capabilities)"
    );
    assert!(
        capabilities.len() <= u16::MAX as usize,
        "more than 65535 capabilities"
    );
    let mut out = [0u8; N];
    let name = MARKER_NOTE_NAME.as_bytes();
    let mut at = put_u32(&mut out, 0, (name.len() + 1) as u32);
    at = put_u32(&mut out, at, payload_len(version, capabilities) as u32);
    at = put_u32(&mut out, at, MARKER_NOTE_TYPE);
    at = put_bytes(&mut out, at, name);
    // The NUL and the padding after the name are already zero.
    at += NOTE_NAME_FIELD_LEN - name.len();
    at = put_u32(&mut out, at, MARKER_FORMAT_VERSION);
    at = put_string(&mut out, at, version);
    at = put_u16(&mut out, at, capabilities.len() as u16);
    let mut index = 0;
    while index < capabilities.len() {
        at = put_string(&mut out, at, capabilities[index]);
        index += 1;
    }
    assert!(
        pad_to_align(at) == N,
        "encoded length differs from marker_len"
    );
    out
}

/// The serial line printed once at start: `vminit <version> capabilities=<comma list>`.
/// The text carries no newline.
pub fn format_banner(version: &str, capabilities: &[&str]) -> String {
    format!("vminit {} capabilities={}", version, capabilities.join(","))
}

/// Banner for this build's own capability list.
pub fn format_own_banner(version: &str) -> String {
    format_banner(version, CAPABILITIES)
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, MarkerError> {
    let end = at.checked_add(2).ok_or(MarkerError::Truncated)?;
    let field = bytes.get(at..end).ok_or(MarkerError::Truncated)?;
    Ok(u16::from_le_bytes([field[0], field[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, MarkerError> {
    let end = at.checked_add(4).ok_or(MarkerError::Truncated)?;
    let field = bytes.get(at..end).ok_or(MarkerError::Truncated)?;
    Ok(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, MarkerError> {
    let end = at.checked_add(8).ok_or(MarkerError::Truncated)?;
    let field = bytes.get(at..end).ok_or(MarkerError::Truncated)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(field);
    Ok(u64::from_le_bytes(raw))
}

/// Read a length-prefixed UTF-8 string of the payload, advancing `at`.
fn read_string(payload: &[u8], at: &mut usize) -> Result<String, MarkerError> {
    let len = read_u16(payload, *at)
        .map_err(|_| MarkerError::Malformed("string length past the payload"))?
        as usize;
    let start = *at + 2;
    let end = start + len;
    let raw = payload
        .get(start..end)
        .ok_or(MarkerError::Malformed("string past the payload"))?;
    *at = end;
    core::str::from_utf8(raw)
        .map(|text| text.to_string())
        .map_err(|_| MarkerError::Malformed("string is not UTF-8"))
}

fn decode_payload(payload: &[u8]) -> Result<Marker, MarkerError> {
    let format_version = read_u32(payload, 0)
        .map_err(|_| MarkerError::Malformed("payload shorter than its format version"))?;
    if format_version != MARKER_FORMAT_VERSION {
        return Err(MarkerError::UnsupportedFormat(format_version));
    }
    let mut at = 4;
    let version = read_string(payload, &mut at)?;
    let count = read_u16(payload, at)
        .map_err(|_| MarkerError::Malformed("capability count past the payload"))?
        as usize;
    at += 2;
    let mut capabilities = Vec::with_capacity(count.min(payload.len()));
    for _ in 0..count {
        capabilities.push(read_string(payload, &mut at)?);
    }
    if at != payload.len() {
        return Err(MarkerError::Malformed("bytes after the last capability"));
    }
    Ok(Marker {
        format_version,
        version,
        capabilities,
    })
}

/// Walk the notes of one `PT_NOTE` segment and decode the first `vminit` note.
fn find_note(segment: &[u8], align: usize) -> Result<Option<Marker>, MarkerError> {
    let pad = |len: usize| (len + align - 1) / align * align;
    let mut at = 0usize;
    while at < segment.len() {
        let name_len = read_u32(segment, at)? as usize;
        let desc_len = read_u32(segment, at + 4)? as usize;
        let note_type = read_u32(segment, at + 8)?;
        let name_start = at + NOTE_HEADER_LEN;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(MarkerError::Truncated)?;
        let desc_start = name_start
            .checked_add(pad(name_len))
            .ok_or(MarkerError::Truncated)?;
        let desc_end = desc_start
            .checked_add(desc_len)
            .ok_or(MarkerError::Truncated)?;
        let name = segment
            .get(name_start..name_end)
            .ok_or(MarkerError::Truncated)?;
        let desc = segment
            .get(desc_start..desc_end)
            .ok_or(MarkerError::Truncated)?;
        if note_type == MARKER_NOTE_TYPE && name == [MARKER_NOTE_NAME.as_bytes(), &[0]].concat() {
            return decode_payload(desc).map(Some);
        }
        at = desc_start
            .checked_add(pad(desc_len))
            .ok_or(MarkerError::Truncated)?;
    }
    Ok(None)
}

/// Read the capability marker from the bytes of a `vmfs` file without executing it.
///
/// Walks the program headers of a little-endian ELF64 file, scans each `PT_NOTE`
/// segment for the note owned by `vminit` with type 1, and decodes its payload. Every
/// offset and length is bounds-checked, so a truncated or hostile file yields an
/// error, never a panic.
pub fn read_marker(file: &[u8]) -> Result<Marker, MarkerError> {
    if file.len() < ELF_HEADER_LEN || file[..4] != *b"\x7fELF" || file[4] != 2 || file[5] != 1 {
        return Err(MarkerError::NotElf64);
    }
    let table_offset = read_u64(file, 0x20)? as usize;
    let entry_size = read_u16(file, 0x36)? as usize;
    let entry_count = read_u16(file, 0x38)? as usize;
    if entry_count > 0 && entry_size < PROGRAM_HEADER_LEN {
        return Err(MarkerError::Malformed(
            "program header entries smaller than 56 bytes",
        ));
    }
    for index in 0..entry_count {
        let header = index
            .checked_mul(entry_size)
            .and_then(|offset| offset.checked_add(table_offset))
            .ok_or(MarkerError::Truncated)?;
        if read_u32(file, header)? != PT_NOTE {
            continue;
        }
        let offset = read_u64(file, header + 8)? as usize;
        let size = read_u64(file, header + 32)? as usize;
        let align = if read_u64(file, header + 48)? == 8 {
            8
        } else {
            NOTE_ALIGN
        };
        let end = offset.checked_add(size).ok_or(MarkerError::Truncated)?;
        let segment = file.get(offset..end).ok_or(MarkerError::Truncated)?;
        if let Some(marker) = find_note(segment, align)? {
            return Ok(marker);
        }
    }
    Err(MarkerError::NoMarker)
}
