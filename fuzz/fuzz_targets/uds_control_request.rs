#![no_main]

use libfuzzer_sys::fuzz_target;

// First byte selects whether an owner-mutation handler is available; the
// remainder is the raw UDS control request head up to the header terminator.
fuzz_target!(|data: &[u8]| {
    mct_daemon::fuzz_uds_control_request(data);
});
