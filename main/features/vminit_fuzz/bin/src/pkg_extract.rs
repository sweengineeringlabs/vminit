//! The initrd package extraction checks (vminit#18, fuzz epic sweengineeringlabs/vmruntime#574).
//!
//! `run_install_step` unpacks `<packages_dir>/<name>.tar.gz` into a destination directory. The fuzzer's bytes choose entries
//! (files, directories, symlinks, hard links) with names from a pool that includes `..`, absolute names and `C:`; the archive is
//! written here byte by byte (the `tar` builder refuses `..`) and gzipped, sometimes cut short or with one byte changed. The
//! outcome is checked on the file system against a model of the rule in the documentation of `safe_path_join`: everything
//! beside the destination is unchanged; every entry before the first refused one is extracted exactly; nothing after it is;
//! symlinks and hard links create nothing.

use flate2::{write::GzEncoder, Compression};
use libfuzzer_sys::arbitrary::Unstructured;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use swe_vminit_pkg::run_install_step;

/// Entry names, hostile ones included. None normalises to an empty path.
const NAMES: &[&str] = &[
    "a", "b", "dir/c", "dir/sub/d", "./e", "f//g", "a/./b2", "...", "....", "x.y", "dir2/", "..", "../escaped", "a/../../escaped",
    "dir/../../escaped", "dir/../x", "/abs", "/abs/file", "C:", "x/C:/y", "ab/..", "../../deep",
];

enum Kind {
    File(Vec<u8>),
    Dir,
    Symlink,
    Hardlink,
}

struct Entry {
    name: String,
    kind: Kind,
}

/// The rule of the documentation of `safe_path_join`, written here independently of the code: a leading `/`, a `..` segment and
/// a two-character segment ending in `:` are refused; empty and `.` segments are skipped.
fn is_safe(name: &str) -> bool {
    !name.contains('\0') && !name.starts_with('/') && name.split('/').all(|segment| segment != ".." && !(segment.len() == 2 && segment.ends_with(':')))
}

/// The segments of a safe name that name something.
fn segments(name: &str) -> Vec<&str> {
    name.split('/').filter(|segment| !segment.is_empty() && *segment != ".").collect()
}

fn ustar_header(name: &str, typeflag: u8, size: usize, linkname: &str) -> [u8; 512] {
    let mut header = [0u8; 512];
    let mut put = |at: usize, bytes: &[u8]| header[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, &name.as_bytes()[..name.len().min(100)]);
    put(100, b"0000644\0");
    put(108, b"0000000\0");
    put(116, b"0000000\0");
    put(124, format!("{size:011o}\0").as_bytes());
    put(136, b"00000000000\0");
    put(157, &linkname.as_bytes()[..linkname.len().min(100)]);
    put(257, b"ustar\0");
    put(263, b"00");
    header[148..156].fill(b' ');
    header[156] = typeflag;
    let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    header
}

fn write_archive(entries: &[Entry]) -> Vec<u8> {
    let mut tar = Vec::new();
    for entry in entries {
        match &entry.kind {
            Kind::File(content) => {
                tar.extend_from_slice(&ustar_header(&entry.name, b'0', content.len(), ""));
                tar.extend_from_slice(content);
                tar.resize(tar.len().div_ceil(512) * 512, 0);
            }
            Kind::Dir => tar.extend_from_slice(&ustar_header(&entry.name, b'5', 0, "")),
            Kind::Symlink => tar.extend_from_slice(&ustar_header(&entry.name, b'2', 0, "../outside_file")),
            Kind::Hardlink => tar.extend_from_slice(&ustar_header(&entry.name, b'1', 0, "../outside_file")),
        }
    }
    tar.extend_from_slice(&[0u8; 1024]);
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&tar).expect("gzip in memory");
    gz.finish().expect("gzip in memory")
}

/// `<tmp>/dest`, `<tmp>/packages`, and beside them `<tmp>/outside_file` and `<tmp>/outside_dir/inner`. Removed on drop.
struct Fixture {
    tmp: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let tmp = std::env::temp_dir().join(format!("vminit_fuzz_pkg_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("dest")).expect("create dest");
        fs::create_dir_all(tmp.join("packages")).expect("create packages");
        fs::create_dir_all(tmp.join("outside_dir")).expect("create the outside directory");
        fs::write(tmp.join("outside_file"), b"keep me").expect("write the outside file");
        fs::write(tmp.join("outside_dir").join("inner"), b"keep me too").expect("write the outside file");
        Fixture { tmp }
    }

    /// Everything under the temp directory that is not `dest` or `packages`: path to (is a directory, contents).
    fn outside_snapshot(&self) -> BTreeMap<String, (bool, Vec<u8>)> {
        let mut snapshot = BTreeMap::new();
        Self::walk(&self.tmp, &self.tmp, &[self.tmp.join("dest"), self.tmp.join("packages")], &mut snapshot);
        snapshot
    }

    /// The tree under `dest`: relative path to `None` for a directory or `Some(contents)` for a file.
    fn dest_tree(&self) -> BTreeMap<String, Option<Vec<u8>>> {
        let mut tree = BTreeMap::new();
        fn walk(dir: &Path, base: &Path, tree: &mut BTreeMap<String, Option<Vec<u8>>>) {
            let Ok(entries) = fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                let relative = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().into_owned();
                if path.is_dir() {
                    tree.insert(relative, None);
                    walk(&path, base, tree);
                } else {
                    tree.insert(relative, Some(fs::read(&path).unwrap_or_default()));
                }
            }
        }
        walk(&self.tmp.join("dest"), &self.tmp.join("dest"), &mut tree);
        tree
    }

    fn walk(dir: &Path, base: &Path, skip: &[PathBuf], snapshot: &mut BTreeMap<String, (bool, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip.contains(&path) {
                continue;
            }
            let relative = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().into_owned();
            if path.is_dir() {
                snapshot.insert(relative, (true, Vec::new()));
                Self::walk(&path, base, skip, snapshot);
            } else {
                snapshot.insert(relative, (false, fs::read(&path).unwrap_or_default()));
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

fn build_entries(u: &mut Unstructured) -> Vec<Entry> {
    (0..u.int_in_range(0..=8usize).unwrap_or(3))
        .map(|_| {
            let name = NAMES[u.int_in_range(0..=NAMES.len() - 1).unwrap_or(0)].to_string();
            let kind = match u.int_in_range(0..=5).unwrap_or(0) {
                0 | 1 | 2 => Kind::File((0..u.int_in_range(0..=300usize).unwrap_or(5)).map(|_| u.arbitrary().unwrap_or(0u8)).collect()),
                3 => Kind::Dir,
                4 => Kind::Symlink,
                _ => Kind::Hardlink,
            };
            Entry { name, kind }
        })
        .collect()
}

/// What the destination must hold after extracting `entries`, or `None` when the outcome is not fixed by the rule (one entry's
/// path is a directory prefix of another's, or a path is used as a file and as a directory).
fn expected_tree(entries: &[Entry]) -> Option<BTreeMap<String, Option<Vec<u8>>>> {
    let mut tree: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut paths: Vec<(Vec<&str>, bool)> = Vec::new();
    for entry in entries {
        if !is_safe(&entry.name) {
            break;
        }
        let parts = segments(&entry.name);
        match &entry.kind {
            Kind::File(content) => {
                for depth in 1..parts.len() {
                    tree.entry(parts[..depth].join("/")).or_insert(None);
                }
                tree.insert(parts.join("/"), Some(content.clone()));
                paths.push((parts, true));
            }
            Kind::Dir => {
                for depth in 1..=parts.len() {
                    tree.entry(parts[..depth].join("/")).or_insert(None);
                }
                paths.push((parts, false));
            }
            Kind::Symlink | Kind::Hardlink => {}
        }
    }
    for (i, (a, a_file)) in paths.iter().enumerate() {
        for (b, b_file) in paths.iter().skip(i + 1) {
            let a_prefix_of_b = a.len() < b.len() && b[..a.len()] == a[..];
            let b_prefix_of_a = b.len() < a.len() && a[..b.len()] == b[..];
            if a_prefix_of_b || b_prefix_of_a || (a == b && a_file != b_file) {
                return None;
            }
        }
    }
    Some(tree)
}

/// Run one archive through `run_install_step`. With `check`, return every way the outcome differs from the model.
pub fn run(u: &mut Unstructured, check: bool) -> Vec<String> {
    let entries = build_entries(u);
    let mut archive = write_archive(&entries);
    let damaged = u.ratio(1, 8).unwrap_or(false);
    if damaged && !archive.is_empty() {
        if u.arbitrary::<bool>().unwrap_or(false) {
            let cut = u.int_in_range(0..=archive.len() - 1).unwrap_or(0);
            archive.truncate(cut);
        } else {
            let at = u.int_in_range(0..=archive.len() - 1).unwrap_or(0);
            archive[at] ^= u.arbitrary::<u8>().unwrap_or(0xFF) | 1;
        }
    }
    let fixture = Fixture::new();
    fs::write(fixture.tmp.join("packages").join("pkg.tar.gz"), &archive).expect("write the archive");
    let outside_before = fixture.outside_snapshot();
    run_install_step(&["pkg".to_string()], &fixture.tmp.join("packages"), None, None, Some(&fixture.tmp.join("dest")), "");
    if !check {
        return Vec::new();
    }
    let mut problems = Vec::new();
    let outside_after = fixture.outside_snapshot();
    if outside_after != outside_before {
        let changed: Vec<&String> = outside_after.keys().filter(|k| outside_before.get(*k) != outside_after.get(*k)).chain(outside_before.keys().filter(|k| !outside_after.contains_key(*k))).collect();
        problems.push(format!("extraction changed what is outside the destination: {changed:?}; entries: {}", describe(&entries)));
    }
    if !damaged {
        if let Some(expected) = expected_tree(&entries) {
            let actual = fixture.dest_tree();
            if actual != expected {
                problems.push(format!("the destination holds {actual:?}, the model expects {expected:?}; entries: {}", describe(&entries)));
            }
        }
    }
    problems
}

fn describe(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| match &entry.kind {
            Kind::File(content) => format!("file {:?} ({} bytes)", entry.name, content.len()),
            Kind::Dir => format!("dir {:?}", entry.name),
            Kind::Symlink => format!("symlink {:?}", entry.name),
            Kind::Hardlink => format!("hardlink {:?}", entry.name),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
