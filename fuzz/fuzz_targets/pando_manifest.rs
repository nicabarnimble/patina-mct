#![no_main]

use libfuzzer_sys::fuzz_target;

// Bytes are an operator-supplied pando.toml: parse and structural validation
// through the public manifest surface.
fuzz_target!(|data: &[u8]| {
    let Ok(raw) = std::str::from_utf8(data) else {
        return;
    };
    let _ = mct_daemon::parse_pando_manifest_str(raw);
});
