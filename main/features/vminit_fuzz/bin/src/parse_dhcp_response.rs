#![no_main]

use libfuzzer_sys::fuzz_target;
use swe_vminit_init::parse_dhcp_response;

// The offset of the options and the magic cookie, as in spi/dhcp.rs (DHCP_OPTIONS_OFFSET and
// DHCP_MAGIC_COOKIE are private there).
const OPTIONS_OFFSET: usize = 240;
const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

// Contract (spi/dhcp.rs): a reply is `None` when it is shorter than the fixed header or lacks the
// magic cookie; nothing else may panic, whatever the options say.
fuzz_target!(|data: &[u8]| {
    if parse_dhcp_response(data).is_some() {
        assert!(data.len() >= OPTIONS_OFFSET);
        assert_eq!(data[236..OPTIONS_OFFSET], MAGIC_COOKIE);
    }
});
