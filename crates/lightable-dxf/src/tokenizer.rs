//! Splits ASCII DXF text into (group code, value) pairs.

use crate::error::DxfError;
use crate::value::Value;

/// The first bytes of a binary DXF file.
pub const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF";

/// One group code with its value exactly as it appeared in the file
/// (minus the line ending).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// Group code.
    pub code: i16,
    /// The value exactly as in the file, minus the line ending.
    pub raw: String,
}

impl Pair {
    /// A pair from a group code and its value.
    pub fn new(code: i16, raw: impl Into<String>) -> Self {
        Self {
            code,
            raw: raw.into(),
        }
    }

    /// The value interpreted according to the group code's type.
    pub fn value(&self) -> Value {
        Value::parse(self.code, &self.raw)
    }
}

/// A pair borrowed from the input, plus the 1-based line of its group code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    /// Group code.
    pub code: i16,
    /// The value exactly as in the input, minus the line ending.
    pub raw: &'a str,
    /// 1-based line of the group code in the input.
    pub line: usize,
}

impl Token<'_> {
    /// True if this is a `0` pair whose value is `name` (ignoring whitespace).
    pub fn is_marker(&self, name: &str) -> bool {
        self.code == 0 && self.raw.trim() == name
    }

    /// An owned copy of the pair.
    pub fn to_pair(&self) -> Pair {
        Pair::new(self.code, self.raw)
    }
}

/// Iterator over the pairs of an ASCII DXF document.
///
/// Accepts LF and CRLF line endings, whitespace around group codes and empty
/// values. Stops after the first error.
pub struct Tokenizer<'a> {
    lines: std::iter::Enumerate<std::str::Lines<'a>>,
    done: bool,
}

impl<'a> Tokenizer<'a> {
    /// A tokenizer over `text`.
    pub fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines().enumerate(),
            done: false,
        }
    }

    fn fail(&mut self, err: DxfError) -> Option<Result<Token<'a>, DxfError>> {
        self.done = true;
        Some(Err(err))
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Result<Token<'a>, DxfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let Some((index, code_line)) = self.lines.next() else {
            self.done = true;
            return None;
        };
        let line = index + 1;
        let code_text = code_line.trim();
        if code_text.is_empty() {
            // Trailing blank lines are common and harmless; blank lines
            // followed by more data mean the pairs are out of step.
            if self.lines.clone().all(|(_, rest)| rest.trim().is_empty()) {
                self.done = true;
                return None;
            }
            return self.fail(DxfError::Syntax {
                line,
                message: "empty group code".into(),
            });
        }
        let Ok(code) = code_text.parse::<i16>() else {
            return self.fail(DxfError::Syntax {
                line,
                message: format!("invalid group code {code_text:?}"),
            });
        };
        let Some((_, value)) = self.lines.next() else {
            return self.fail(DxfError::UnexpectedEof { line });
        };
        let value = value.strip_suffix('\r').unwrap_or(value);
        Some(Ok(Token {
            code,
            raw: value,
            line,
        }))
    }
}

impl std::iter::FusedIterator for Tokenizer<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(i16, String)> {
        Tokenizer::new(text)
            .map(|t| {
                let t = t.expect("valid input");
                (t.code, t.raw.to_owned())
            })
            .collect()
    }

    #[test]
    fn reads_lf_pairs() {
        assert_eq!(
            pairs("0\nSECTION\n2\nHEADER\n"),
            vec![(0, "SECTION".into()), (2, "HEADER".into())]
        );
    }

    #[test]
    fn reads_crlf_pairs() {
        assert_eq!(
            pairs("0\r\nSECTION\r\n2\r\nHEADER\r\n"),
            vec![(0, "SECTION".into()), (2, "HEADER".into())]
        );
    }

    #[test]
    fn trims_codes_but_keeps_value_text() {
        assert_eq!(
            pairs("  0\nLINE\n 10\n  1.5\n  1\n  text \n"),
            vec![
                (0, "LINE".into()),
                (10, "  1.5".into()),
                (1, "  text ".into())
            ]
        );
    }

    #[test]
    fn empty_values_are_kept() {
        assert_eq!(
            pairs("1\n\n0\nEOF\n"),
            vec![(1, String::new()), (0, "EOF".into())]
        );
    }

    #[test]
    fn missing_final_newline_is_fine() {
        assert_eq!(pairs("0\nEOF"), vec![(0, "EOF".into())]);
    }

    #[test]
    fn trailing_blank_lines_are_ignored() {
        assert_eq!(pairs("0\nEOF\n\n\n  \n"), vec![(0, "EOF".into())]);
    }

    #[test]
    fn records_line_numbers() {
        let lines: Vec<usize> = Tokenizer::new("0\nA\n0\nB\n")
            .map(|t| t.unwrap().line)
            .collect();
        assert_eq!(lines, vec![1, 3]);
    }

    #[test]
    fn invalid_group_code_is_an_error_and_stops() {
        let mut tok = Tokenizer::new("0\nA\nxx\nB\n0\nC\n");
        assert!(tok.next().unwrap().is_ok());
        let err = tok.next().unwrap().unwrap_err();
        assert_eq!(err.line(), Some(3));
        assert!(tok.next().is_none());
    }

    #[test]
    fn out_of_range_group_code_is_an_error() {
        assert!(Tokenizer::new("99999\nA\n").next().unwrap().is_err());
    }

    #[test]
    fn blank_code_in_the_middle_is_an_error() {
        let results: Vec<_> = Tokenizer::new("0\nA\n\n0\nB\n").collect();
        assert!(results[1].is_err());
    }

    #[test]
    fn code_without_value_is_unexpected_eof() {
        let results: Vec<_> = Tokenizer::new("0\nA\n0").collect();
        assert!(matches!(
            results[1],
            Err(DxfError::UnexpectedEof { line: 3 })
        ));
    }

    #[test]
    fn pair_value_is_typed() {
        assert_eq!(Pair::new(40, "2.5").value(), Value::Double(2.5));
    }
}
