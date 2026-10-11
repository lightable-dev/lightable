//! The tokenizer must never panic, and must stop after its first error.
#![no_main]

use lightable_dxf::tokenizer::Tokenizer;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut errors = 0;
    for token in Tokenizer::new(&text) {
        if token.is_err() {
            errors += 1;
        }
    }
    assert!(errors <= 1, "tokenizer continued after an error");
});
