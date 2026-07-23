#![no_main]

use libfuzzer_sys::fuzz_target;

// Bytes are a candidate gzip/tar release archive fed to the pre-extraction
// scan: entry walk, layout, manifest, internal checksums, display metadata.
fuzz_target!(|data: &[u8]| {
    mct_daemon::fuzz_release_archive(data);
});
