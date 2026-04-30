//! On-disk schema for the [[files]] initramfs overlay.
//!
//! Wire format — one entry per line:
//!   <dest>:<mode-octal>:<uid>:<gid>
//!
//! dest must be absolute. mode may carry file-type bits or bare permission
//! bits. Comments (#) and blank lines are ignored.
//! emit() sorts by dest for byte-deterministic output.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

/// One file in the initramfs overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayEntry {
    pub dest: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

/// The manifest at /overlay/.manifest in the initramfs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayManifest {
    pub entries: Vec<OverlayEntry>,
}

/// Parse errors from OverlayManifest::parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MalformedLine { line_number: usize, content: String },
    NonAbsolutePath { line_number: usize, path: String },
    InvalidMode { line_number: usize, raw: String },
    InvalidUidGid { line_number: usize, field: &'static str, raw: String },
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::MalformedLine { line_number, content } =>
                write!(f, "manifest line {line_number}: malformed entry (expected `<dest>:<mode>:<uid>:<gid>`): {content:?}"),
            ParseError::NonAbsolutePath { line_number, path } =>
                write!(f, "manifest line {line_number}: dest path must be absolute (start with `/`): {path:?}"),
            ParseError::InvalidMode { line_number, raw } =>
                write!(f, "manifest line {line_number}: invalid octal mode: {raw:?}"),
            ParseError::InvalidUidGid { line_number, field, raw } =>
                write!(f, "manifest line {line_number}: invalid {field}: {raw:?}"),
        }
    }
}

impl OverlayManifest {
    /// Parse the manifest text. Strict: any malformed line aborts the parse.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut entries = Vec::new();
        for (idx, raw_line) in text.lines().enumerate() {
            let line_number = idx + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            entries.push(Self::parse_line(line_number, trimmed)?);
        }
        Ok(OverlayManifest { entries })
    }

    /// Parse one wire-format line. `line_number` is included in errors verbatim.
    pub fn parse_line(line_number: usize, content: &str) -> Result<OverlayEntry, ParseError> {
        let mut parts = content.splitn(4, ':');
        let dest = parts.next();
        let mode = parts.next();
        let uid = parts.next();
        let gid = parts.next();

        let (dest, mode, uid, gid) = match (dest, mode, uid, gid) {
            (Some(d), Some(m), Some(u), Some(g)) if !d.is_empty() && !m.is_empty() => (d, m, u, g),
            _ => {
                return Err(ParseError::MalformedLine {
                    line_number,
                    content: content.to_owned(),
                });
            }
        };

        if !dest.starts_with('/') {
            return Err(ParseError::NonAbsolutePath {
                line_number,
                path: dest.to_owned(),
            });
        }

        let mode_val = parse_octal_u32(mode).ok_or_else(|| ParseError::InvalidMode {
            line_number,
            raw: mode.to_owned(),
        })?;
        let uid_val = parse_decimal_u32(uid).ok_or_else(|| ParseError::InvalidUidGid {
            line_number,
            field: "uid",
            raw: uid.to_owned(),
        })?;
        let gid_val = parse_decimal_u32(gid).ok_or_else(|| ParseError::InvalidUidGid {
            line_number,
            field: "gid",
            raw: gid.to_owned(),
        })?;

        Ok(OverlayEntry {
            dest: dest.to_owned(),
            mode: mode_val,
            uid: uid_val,
            gid: gid_val,
        })
    }

    /// Serialize to canonical wire form. Entries sorted by dest.
    pub fn emit(&self) -> String {
        let mut sorted: Vec<&OverlayEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| a.dest.cmp(&b.dest));

        let mut estimated = 0usize;
        for entry in &sorted {
            estimated = estimated.saturating_add(entry.dest.len() + 32);
        }
        let mut out = String::with_capacity(estimated);

        for entry in &sorted {
            out.push_str(&entry.dest);
            out.push(':');
            push_octal_u32(&mut out, entry.mode);
            out.push(':');
            push_decimal_u32(&mut out, entry.uid);
            out.push(':');
            push_decimal_u32(&mut out, entry.gid);
            out.push('\n');
        }
        out
    }
}

fn parse_octal_u32(text: &str) -> Option<u32> {
    let stripped = text
        .strip_prefix("0o")
        .or_else(|| text.strip_prefix("0O"))
        .unwrap_or(text);
    if stripped.is_empty() {
        return None;
    }
    let mut acc: u32 = 0;
    for c in stripped.chars() {
        let digit = match c {
            '0'..='7' => (c as u32) - ('0' as u32),
            _ => return None,
        };
        acc = acc.checked_mul(8)?.checked_add(digit)?;
    }
    Some(acc)
}

fn parse_decimal_u32(text: &str) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    let mut acc: u32 = 0;
    for c in text.chars() {
        let digit = match c {
            '0'..='9' => (c as u32) - ('0' as u32),
            _ => return None,
        };
        acc = acc.checked_mul(10)?.checked_add(digit)?;
    }
    Some(acc)
}

fn push_octal_u32(out: &mut String, value: u32) {
    if value == 0 {
        out.push('0');
        return;
    }
    let mut buf = [0u8; 12];
    let mut n = value;
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + ((n & 0o7) as u8);
        n >>= 3;
    }
    out.push_str(core::str::from_utf8(&buf[i..]).expect("octal digits are ASCII"));
}

fn push_decimal_u32(out: &mut String, value: u32) {
    let mut buf = [0u8; 10];
    let mut n = value;
    let mut i = buf.len();
    if n == 0 {
        out.push('0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + ((n % 10) as u8);
        n /= 10;
    }
    out.push_str(core::str::from_utf8(&buf[i..]).expect("decimal digits are ASCII"));
}

impl core::fmt::Display for OverlayEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{:o}:{}:{}", self.dest, self.mode, self.uid, self.gid)
    }
}
