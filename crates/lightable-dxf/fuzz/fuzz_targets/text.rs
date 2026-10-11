//! Decoding text escapes must never panic, and must return the input
//! unchanged when there is nothing to decode.
#![no_main]

use lightable_dxf::{CodePage, decode_mtext, decode_text};
use libfuzzer_sys::fuzz_target;

const PAGES: [u16; 15] = [
    874, 932, 936, 949, 950, 1250, 1251, 1252, 1253, 1254, 1255, 1256, 1257, 1258, 1361,
];

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let page = CodePage(PAGES[usize::from(selector) % PAGES.len()]);
    let text = String::from_utf8_lossy(rest);
    let decoded = decode_text(&text, page);
    let mtext = decode_mtext(&text, page);
    if !text.contains('\\') && !text.contains("%%") {
        assert_eq!(decoded, text);
        assert_eq!(mtext, text);
    }
});
