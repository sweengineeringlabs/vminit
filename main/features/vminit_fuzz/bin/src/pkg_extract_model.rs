#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};

// vminit#18: the fuzzer's bytes choose tar entries (hostile names included), written as a gzipped archive that `run_install_step`
// extracts; everything beside the destination must be unchanged and the destination must hold exactly what the rule of
// `safe_path_join` says (see `vminit_fuzz_bin::pkg_extract::run`).
fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let problems = vminit_fuzz_bin::pkg_extract::run(&mut u, true);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
});
