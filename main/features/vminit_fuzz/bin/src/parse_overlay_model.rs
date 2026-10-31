#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};
use swe_vminit_init::OverlayManifest;

// Checks the VALUES `OverlayManifest::parse` returns. The fuzzer's bytes choose a list of entries (destination, mode,
// uid, gid); the manifest is written here in the documented `dest:mode:uid:gid` form (mode in octal, with or without
// the `0o` prefix, ids in decimal), with comment lines, blank lines and leading spaces between them. It must parse to
// exactly those entries in that order. With one malformed line added it must be rejected: the parse is strict.

const PATH_CHARS: &[char] = &['a', 'b', 'z', 'A', '0', '9', '.', '_', '-', '/', 'é'];

fn path(u: &mut Unstructured) -> String {
    let len = u.int_in_range(0..=24usize).unwrap_or(0);
    let tail: String = (0..len).map(|_| *u.choose(PATH_CHARS).unwrap_or(&'a')).collect();
    format!("/{tail}")
}

fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let mut text = String::new();
    let mut expected: Vec<(String, u32, u32, u32)> = Vec::new();

    for _ in 0..u.int_in_range(0..=8).unwrap_or(0) {
        match u.int_in_range(0..=3).unwrap_or(0) {
            0 => text.push_str("# a comment: with:colons\n"),
            1 => text.push_str("   \n"),
            _ => {
                let dest = path(&mut u);
                let mode: u32 = u.arbitrary().unwrap_or(0);
                let uid: u32 = u.arbitrary().unwrap_or(0);
                let gid: u32 = u.arbitrary().unwrap_or(0);
                let prefix = if u.arbitrary::<bool>().unwrap_or(false) { "0o" } else { "" };
                if u.arbitrary::<bool>().unwrap_or(false) {
                    text.push_str("  "); // leading spaces are trimmed
                }
                text.push_str(&format!("{dest}:{prefix}{mode:o}:{uid}:{gid}\n"));
                expected.push((dest, mode, uid, gid));
            }
        }
    }

    let manifest = OverlayManifest::parse(&text).unwrap_or_else(|error| panic!("a valid manifest was rejected: {error:?}\n{text}"));
    let got: Vec<(String, u32, u32, u32)> = manifest.entries.iter().map(|e| (e.dest.clone(), e.mode, e.uid, e.gid)).collect();
    assert_eq!(got, expected, "entries differ for manifest:\n{text}");

    // strict: one malformed line makes the whole manifest an error
    let bad = *u.choose(&["relative/path:0644:0:0", "/a:zz:0:0", "/b:0644:x:0", "/c:0644:0:y", ":0644:0:0", "/d:0644"]).unwrap_or(&"/d:0644");
    text.push_str(bad);
    text.push('\n');
    assert!(OverlayManifest::parse(&text).is_err(), "a manifest with the bad line {bad:?} was accepted:\n{text}");
});
