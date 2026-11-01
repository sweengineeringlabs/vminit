#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};
use swe_vminit_init::{read_marker, MARKER_FORMAT_VERSION};

// Checks the VALUES `read_marker` returns, not only that it survives. The fuzzer's bytes choose a version,
// a list of capabilities and the shape of an ELF64 file (program headers that are not notes, foreign notes
// before the marker, note alignment 4 or 8); the file is written here from the layout documented in
// docs/xkvm_conf.md, independently of the init's own encoder. A file that is valid by construction must
// read back as exactly that marker, and the same file cut short must never yield a marker.

const CHARS: &[char] = &['a', 'z', '0', '9', '.', '-', '_', '=', ':', '/', ' ', 'é', '漢', '😀'];

fn text(u: &mut Unstructured, max: usize) -> String {
    let len = u.int_in_range(0..=max).unwrap_or(0);
    (0..len).map(|_| *u.choose(CHARS).unwrap_or(&'a')).collect()
}

/// An ELF note: namesz, descsz, type, the name padded to `align`, the description padded to `align`.
fn note(name: &[u8], kind: u32, desc: &[u8], align: usize) -> Vec<u8> {
    let pad = |n: usize| (n + align - 1) / align * align;
    let mut out = Vec::new();
    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
    out.extend_from_slice(&(desc.len() as u32).to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(name);
    out.resize(12 + pad(name.len()), 0);
    out.extend_from_slice(desc);
    out.resize(12 + pad(name.len()) + pad(desc.len()), 0);
    out
}

/// The payload of the `vminit` note: format version, the version string, the capability count and each capability,
/// strings as a 16-bit length and the bytes, all little-endian.
fn payload(version: &str, capabilities: &[String]) -> Vec<u8> {
    let mut out = MARKER_FORMAT_VERSION.to_le_bytes().to_vec();
    out.extend_from_slice(&(version.len() as u16).to_le_bytes());
    out.extend_from_slice(version.as_bytes());
    out.extend_from_slice(&(capabilities.len() as u16).to_le_bytes());
    for capability in capabilities {
        out.extend_from_slice(&(capability.len() as u16).to_le_bytes());
        out.extend_from_slice(capability.as_bytes());
    }
    out
}

/// A little-endian ELF64 file: `headers` program headers (a PT_LOAD with no data, or a PT_NOTE over the next segment).
fn elf(segments: &[Option<Vec<u8>>], align: u64) -> Vec<u8> {
    let table = 64 + 56 * segments.len();
    let mut file = vec![0u8; table];
    file[..4].copy_from_slice(b"\x7fELF");
    file[4] = 2;
    file[5] = 1;
    file[0x20..0x28].copy_from_slice(&64u64.to_le_bytes());
    file[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
    file[0x38..0x3a].copy_from_slice(&(segments.len() as u16).to_le_bytes());
    let mut data = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let header = 64 + 56 * index;
        match segment {
            Some(bytes) => {
                file[header..header + 4].copy_from_slice(&4u32.to_le_bytes()); // PT_NOTE
                file[header + 8..header + 16].copy_from_slice(&((table + data.len()) as u64).to_le_bytes());
                file[header + 32..header + 40].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
                file[header + 48..header + 56].copy_from_slice(&align.to_le_bytes());
                data.extend_from_slice(bytes);
            }
            None => file[header..header + 4].copy_from_slice(&1u32.to_le_bytes()), // PT_LOAD, nothing in it
        }
    }
    file.extend_from_slice(&data);
    file
}

fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let version = text(&mut u, 24);
    let capabilities: Vec<String> = (0..u.int_in_range(0..=20).unwrap_or(0)).map(|_| text(&mut u, 24)).collect();
    let align: usize = if u.arbitrary::<bool>().unwrap_or(false) { 8 } else { 4 };

    let marker = note(b"vminit\0", 1, &payload(&version, &capabilities), align);
    let foreign = note(b"GNU\0", 3, &[1, 2, 3, 4], align);
    // the marker is alone, after a foreign note in the same segment, or in a later segment after one that holds only a foreign note
    let mut segments: Vec<Option<Vec<u8>>> = Vec::new();
    for _ in 0..u.int_in_range(0..=2).unwrap_or(0) {
        segments.push(None);
    }
    match u.int_in_range(0..=2).unwrap_or(0) {
        0 => segments.push(Some(marker.clone())),
        1 => segments.push(Some([foreign.clone(), marker.clone()].concat())),
        _ => {
            segments.push(Some(foreign.clone()));
            segments.push(Some(marker.clone()));
        }
    }
    let file = elf(&segments, align as u64);

    match read_marker(&file) {
        Ok(found) => {
            assert_eq!(found.format_version, MARKER_FORMAT_VERSION);
            assert_eq!(found.version, version);
            assert_eq!(found.capabilities, capabilities);
        }
        Err(error) => panic!("a valid marker was rejected: {error:?} (version {version:?}, {} capabilities, align {align})", capabilities.len()),
    }

    // the same file cut short must not produce a marker
    let cut = u.int_in_range(0..=file.len() - 1).unwrap_or(0);
    assert!(read_marker(&file[..cut]).is_err(), "a file cut at {cut} of {} still produced a marker", file.len());
});
