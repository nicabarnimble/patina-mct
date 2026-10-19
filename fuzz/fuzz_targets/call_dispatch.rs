#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    mct_daemon::call_dispatch::fuzz_call_dispatch(data);
});
