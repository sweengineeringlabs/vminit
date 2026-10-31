#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};

// The control of `pkg_extract_model`: the same archives, with no check of the outcome. It is not a useful target; it shows what a
// "does not crash" version of the target misses (see `vminit_fuzz_bin::pkg_extract::run`).
fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let _ = vminit_fuzz_bin::pkg_extract::run(&mut u, false);
});
