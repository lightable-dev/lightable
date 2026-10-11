//! Inputs that once crashed a fuzz target must not panic again.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lightable_dxf::tokenizer::Tokenizer;
use lightable_dxf::{CodePage, decode_mtext, decode_text};

fn inputs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("regression folder exists")
        .map(|e| e.expect("readable entry").path())
        .filter(|p| p.extension().is_none_or(|e| e != "md"))
        .collect();
    files.sort();
    files
}

#[test]
fn fuzz_regressions_do_not_panic() {
    for path in inputs() {
        let bytes = std::fs::read(&path).unwrap();
        let start = Instant::now();
        let _ = lightable_dxf::read_bytes(&bytes);
        let text = String::from_utf8_lossy(&bytes);
        for token in Tokenizer::new(&text) {
            let _ = token;
        }
        for page in [932, 1250, 1252, 1361] {
            let _ = decode_text(&text, CodePage(page));
            let _ = decode_mtext(&text, CodePage(page));
        }
        // Slow inputs count as failures too (the fuzzers use a 10 s limit).
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{} took {:?}",
            path.display(),
            start.elapsed()
        );
    }
}
