#![no_main]

use libfuzzer_sys::fuzz_target;
use swe_vminit_init::{parse_config, MAX_DNS_SERVERS};

// Contract (spi/config.rs): a pure function from text to `InitConfig`; a line that cannot be honoured
// is recorded in `errors`, never skipped silently, and the number of `dns=` servers is capped.
// The caller reads the file as bytes and passes text, so input that is not UTF-8 never reaches it.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let config = parse_config(text);
    assert!(config.dns_servers.len() <= MAX_DNS_SERVERS);
});
