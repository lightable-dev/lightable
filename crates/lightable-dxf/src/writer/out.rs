//! Formatting group-code pairs.

use std::fmt::Write as _;

use crate::model::{Handle, Point3};
use crate::tokenizer::Pair;

/// Text output: group codes right-aligned in three columns and CRLF line
/// endings, as most DXF writers do.
#[derive(Default)]
pub(super) struct Out {
    pub(super) buf: String,
}

/// Formats a float so it reads back to exactly the same value.
pub(super) fn float(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".into();
    }
    let a = v.abs();
    let mut s = if a != 0.0 && !(1e-6..1e15).contains(&a) {
        format!("{v:E}")
    } else {
        format!("{v}")
    };
    if !s.contains(['.', 'E']) {
        s.push_str(".0");
    }
    s
}

impl Out {
    /// One pair. Line breaks inside a value would desynchronise the file,
    /// so they become spaces.
    pub(super) fn pair(&mut self, code: i16, value: &str) {
        let _ = write!(self.buf, "{code:>3}\r\n");
        if value.contains(['\r', '\n']) {
            self.buf.push_str(&value.replace(['\r', '\n'], " "));
        } else {
            self.buf.push_str(value);
        }
        self.buf.push_str("\r\n");
    }

    pub(super) fn int(&mut self, code: i16, value: impl std::fmt::Display) {
        self.pair(code, &value.to_string());
    }

    pub(super) fn float(&mut self, code: i16, value: f64) {
        self.pair(code, &float(value));
    }

    pub(super) fn handle(&mut self, code: i16, handle: Handle) {
        self.pair(code, &handle.to_string());
    }

    pub(super) fn point(&mut self, code: i16, p: Point3) {
        self.float(code, p.x);
        self.float(code + 10, p.y);
        self.float(code + 20, p.z);
    }

    pub(super) fn raw(&mut self, pairs: &[Pair]) {
        for p in pairs {
            self.pair(p.code, &p.raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_read_back_exactly() {
        for v in [
            0.0,
            1.0,
            -2.5,
            0.1,
            1.0 / 3.0,
            1e-7,
            1e20,
            -1e-300,
            123456.789,
        ] {
            let s = float(v);
            assert_eq!(s.parse::<f64>().unwrap(), v, "{s}");
            assert!(s.contains('.') || s.contains('E'), "{s}");
        }
        assert_eq!(float(1.0), "1.0");
        assert_eq!(float(f64::NAN), "0.0");
    }

    #[test]
    fn pairs_are_padded_with_crlf() {
        let mut out = Out::default();
        out.pair(0, "LINE");
        out.pair(1001, "APP");
        out.pair(1, "a\nb");
        assert_eq!(out.buf, "  0\r\nLINE\r\n1001\r\nAPP\r\n  1\r\na b\r\n");
    }
}
