//! Reading arbitrary bytes must never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lightable_dxf::read_bytes(data);
});
