#![no_main]

use libfuzzer_sys::fuzz_target;

// Bytes are an operator-supplied child manifest TOML: SDK parse, WIT
// namespace validation, and canonical staging rewrite.
fuzz_target!(|data: &[u8]| {
    mct_daemon::fuzz_child_package_manifest(data);
});
