#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};

// The control of `dhcp_exchange_model`: the same exchanges, with no check of what was sent or returned (see `vminit_fuzz_bin::dhcp_exchange::run`).
fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let problems = vminit_fuzz_bin::dhcp_exchange::run(&mut u, false);
    let _ = problems;
});
