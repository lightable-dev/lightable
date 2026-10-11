//! Typed access to the pairs of one record.

use crate::model::{Color, Handle, LineWeight, Point3, Z_AXIS};
use crate::tokenizer::{Pair, Token};
use crate::value::{parse_f64, parse_int};

pub(super) fn invalid(t: &Token<'_>) -> String {
    format!("invalid value {:?} for group code {}", t.raw, t.code)
}

pub(super) fn int_of(t: &Token<'_>) -> Result<i64, String> {
    parse_int(t.raw).ok_or_else(|| invalid(t))
}

pub(super) fn i16_of(t: &Token<'_>) -> Result<i16, String> {
    i16::try_from(int_of(t)?).map_err(|_| invalid(t))
}

pub(super) fn i32_of(t: &Token<'_>) -> Result<i32, String> {
    i32::try_from(int_of(t)?).map_err(|_| invalid(t))
}

pub(super) fn float_of(t: &Token<'_>) -> Result<f64, String> {
    parse_f64(t.raw).ok_or_else(|| invalid(t))
}

pub(super) fn handle_of(t: &Token<'_>) -> Result<Handle, String> {
    Handle::parse(t.raw).ok_or_else(|| invalid(t))
}

pub(super) fn flag_of(t: &Token<'_>) -> Result<bool, String> {
    match int_of(t)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid(t)),
    }
}

pub(super) fn aci_of(t: &Token<'_>) -> Result<Color, String> {
    Color::from_aci(int_of(t)?).ok_or_else(|| invalid(t))
}

pub(super) fn true_color_of(t: &Token<'_>) -> Result<Color, String> {
    Color::from_true_color(int_of(t)?).ok_or_else(|| invalid(t))
}

pub(super) fn lineweight_of(t: &Token<'_>) -> Result<LineWeight, String> {
    LineWeight::from_code(int_of(t)?).ok_or_else(|| invalid(t))
}

/// A name-like value: surrounding whitespace removed.
pub(super) fn name_of(t: &Token<'_>) -> String {
    t.raw.trim().to_owned()
}

/// Splits a record's pairs: pairs inside `102 {...}` groups, extended data
/// (codes 1000+) and embedded objects (from code 101 on) go to `extra`; the
/// rest are offered to `field`. Pairs it accepts (`Ok(true)`) are consumed,
/// the others are returned in order for type-specific parsing. Returns the
/// first error `field` reported, after visiting every pair.
pub(super) fn split<'r, 'a>(
    pairs: &'r [Token<'a>],
    extra: &mut Vec<&'r Token<'a>>,
    mut field: impl FnMut(&Token<'a>) -> Result<bool, String>,
) -> (Vec<&'r Token<'a>>, Option<String>) {
    let mut rest = Vec::with_capacity(pairs.len());
    let mut depth = 0usize;
    let mut tail = false;
    let mut first_error = None;
    for t in pairs {
        tail |= t.code >= 1000 || t.code == 101;
        if t.code == 102 {
            let value = t.raw.trim();
            if value.starts_with('{') {
                depth += 1;
            } else if value == "}" {
                depth = depth.saturating_sub(1);
            }
        }
        if tail || depth > 0 || t.code == 102 {
            extra.push(t);
            continue;
        }
        match field(t) {
            Ok(true) => {}
            Ok(false) => rest.push(t),
            Err(message) => {
                first_error.get_or_insert(message);
                extra.push(t);
            }
        }
    }
    (rest, first_error)
}

/// The pairs left for a record type that reads each of `codes` at most once.
///
/// Lookups return the first occurrence of a code; [`Fields::leftovers`]
/// returns everything else (other codes and repeats) so nothing is lost.
pub(super) struct Fields<'s, 'r, 'a> {
    pairs: &'s [&'r Token<'a>],
    codes: &'s [i16],
}

impl<'s, 'r, 'a> Fields<'s, 'r, 'a> {
    pub(super) fn new(pairs: &'s [&'r Token<'a>], codes: &'s [i16]) -> Self {
        Self { pairs, codes }
    }

    pub(super) fn token(&self, code: i16) -> Option<&'r Token<'a>> {
        debug_assert!(self.codes.contains(&code), "code {code} not declared");
        self.pairs.iter().copied().find(|t| t.code == code)
    }

    /// The raw value, untrimmed (for text content).
    pub(super) fn raw(&self, code: i16) -> Option<&'a str> {
        self.token(code).map(|t| t.raw)
    }

    /// A trimmed name-like value, or `default`.
    pub(super) fn name_or(&self, code: i16, default: &str) -> String {
        self.token(code).map_or_else(|| default.to_owned(), name_of)
    }

    pub(super) fn float(&self, code: i16) -> Result<Option<f64>, String> {
        self.token(code).map(float_of).transpose()
    }

    pub(super) fn float_or(&self, code: i16, default: f64) -> Result<f64, String> {
        Ok(self.float(code)?.unwrap_or(default))
    }

    pub(super) fn require(&self, code: i16, what: &str) -> Result<f64, String> {
        self.float(code)?
            .ok_or_else(|| format!("missing {what} (group code {code})"))
    }

    pub(super) fn int16(&self, code: i16) -> Result<Option<i16>, String> {
        self.token(code).map(i16_of).transpose()
    }

    pub(super) fn int16_or(&self, code: i16, default: i16) -> Result<i16, String> {
        Ok(self.int16(code)?.unwrap_or(default))
    }

    pub(super) fn int32(&self, code: i16) -> Result<Option<i32>, String> {
        self.token(code).map(i32_of).transpose()
    }

    /// A point from `x`, `x + 10` and (optional) `x + 20`.
    pub(super) fn point(&self, x: i16, what: &str) -> Result<Point3, String> {
        Ok(Point3::new(
            self.require(x, what)?,
            self.require(x + 10, what)?,
            self.float_or(x + 20, 0.0)?,
        ))
    }

    /// Like [`Fields::point`], but `None` when neither x nor y is present.
    pub(super) fn optional_point(&self, x: i16, what: &str) -> Result<Option<Point3>, String> {
        if self.token(x).is_none() && self.token(x + 10).is_none() {
            return Ok(None);
        }
        self.point(x, what).map(Some)
    }

    /// Groups 210/220/230, defaulting to the Z axis.
    pub(super) fn extrusion(&self) -> Result<Point3, String> {
        Ok(Point3::new(
            self.float_or(210, Z_AXIS.x)?,
            self.float_or(220, Z_AXIS.y)?,
            self.float_or(230, Z_AXIS.z)?,
        ))
    }

    /// Pairs not consumed: codes not declared, and repeats of declared codes.
    pub(super) fn leftovers(&self, extra: &mut Vec<&'r Token<'a>>) {
        // Index of the first pair of each declared code: those are consumed.
        // (Linear in the number of pairs, so huge records stay cheap.)
        let mut consumed: Vec<usize> = self
            .codes
            .iter()
            .filter_map(|&code| self.pairs.iter().position(|t| t.code == code))
            .collect();
        consumed.sort_unstable();
        consumed.dedup();
        let mut next = consumed.iter().peekable();
        for (i, &t) in self.pairs.iter().enumerate() {
            if next.peek() == Some(&&i) {
                next.next();
            } else {
                extra.push(t);
            }
        }
    }
}

/// Owned copies of `extra`, in file order.
pub(super) fn owned_in_order(mut extra: Vec<&Token<'_>>) -> Vec<Pair> {
    extra.sort_by_key(|t| t.line);
    extra.into_iter().map(Token::to_pair).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(pairs: &[(i16, &'static str)]) -> Vec<Token<'static>> {
        pairs
            .iter()
            .enumerate()
            .map(|(i, &(code, raw))| Token {
                code,
                raw,
                line: i * 2 + 1,
            })
            .collect()
    }

    #[test]
    fn split_routes_groups_xdata_and_embedded_objects_to_extra() {
        let t = toks(&[
            (8, "L"),
            (102, "{ACAD_REACTORS"),
            (330, "1"),
            (102, "}"),
            (10, "1"),
            (101, "Embedded Object"),
            (10, "2"),
            (1001, "APP"),
        ]);
        let mut extra = Vec::new();
        let (rest, err) = split(&t, &mut extra, |p| Ok(p.code == 8));
        assert!(err.is_none());
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].raw, "1");
        let codes: Vec<i16> = extra.iter().map(|p| p.code).collect();
        assert_eq!(codes, [102, 330, 102, 101, 10, 1001]);
        assert_eq!(owned_in_order(extra).len(), 6);
    }

    #[test]
    fn fields_take_first_occurrence_and_keep_the_rest() {
        let t = toks(&[(10, "1"), (20, "2"), (10, "9"), (99, "x")]);
        let refs: Vec<&Token> = t.iter().collect();
        let f = Fields::new(&refs, &[10, 20, 30]);
        assert_eq!(f.point(10, "p").unwrap(), Point3::new(1.0, 2.0, 0.0));
        assert_eq!(f.optional_point(10, "p").unwrap().map(|p| p.x), Some(1.0));
        let mut extra = Vec::new();
        f.leftovers(&mut extra);
        assert_eq!(
            owned_in_order(extra),
            vec![Pair::new(10, "9"), Pair::new(99, "x")]
        );
    }

    #[test]
    fn fields_report_bad_and_missing_values() {
        let t = toks(&[(10, "abc")]);
        let refs: Vec<&Token> = t.iter().collect();
        let f = Fields::new(&refs, &[10, 20, 30, 40]);
        assert!(f.point(10, "p").unwrap_err().contains("invalid value"));
        assert!(
            f.require(40, "radius")
                .unwrap_err()
                .contains("missing radius")
        );
        assert_eq!(f.float_or(30, 7.0), Ok(7.0));
    }
}
