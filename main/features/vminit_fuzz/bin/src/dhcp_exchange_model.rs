#![no_main]

use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};

// vminit#19: the fuzzer's bytes choose a DHCP policy and what the server answers to the n-th message of the client (an OFFER, an ACK, a NAK, a reply to
// another transaction, bytes that are no reply, nothing, each after a delay); `acquire_lease` runs against a scripted link with a virtual clock and
// what it sent and returned must agree with the model of RFC 2131 (see `vminit_fuzz_bin::dhcp_exchange::run`).
fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let problems = vminit_fuzz_bin::dhcp_exchange::run(&mut u, true);
    assert!(
        problems.is_empty(),
        "{}",
        problems.join(
            "
"
        )
    );
});
