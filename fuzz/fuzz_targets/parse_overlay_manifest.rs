#![no_main]

use libfuzzer_sys::fuzz_target;
use swe_vminit_init::OverlayManifest;

// Contract (api/overlay.rs): strict, any malformed line aborts the parse; a destination that is
// empty or relative is an error, so every entry of a manifest that parsed has an absolute path.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    if let Ok(manifest) = OverlayManifest::parse(text) {
        for entry in &manifest.entries {
            assert!(entry.dest.starts_with('/'), "destination is not absolute: {:?}", entry.dest);
        }
    }
});
