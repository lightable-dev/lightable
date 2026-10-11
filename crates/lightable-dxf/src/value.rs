//! Typed views of raw group-code values.

/// A group-code value interpreted according to its code's type.
///
/// Values that do not parse as their code's type are kept as [`Value::Str`],
/// so converting never loses data.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Text (and values that did not parse as their code's type).
    Str(String),
    /// A floating-point value.
    Double(f64),
    /// An integer value.
    Int(i64),
    /// A boolean (0 or 1).
    Bool(bool),
}

/// The value type the DXF reference assigns to a group code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Text.
    Str,
    /// Floating point.
    Double,
    /// Integer.
    Int,
    /// Boolean.
    Bool,
}

impl ValueKind {
    /// The value type of group code `code`.
    pub fn of(code: i16) -> Self {
        match code {
            10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Self::Double,
            60..=79
            | 90..=99
            | 160..=179
            | 270..=289
            | 370..=389
            | 400..=409
            | 420..=429
            | 440..=459
            | 1060..=1071 => Self::Int,
            290..=299 => Self::Bool,
            _ => Self::Str,
        }
    }
}

impl Value {
    /// Interprets `raw` according to the type of `code`.
    pub fn parse(code: i16, raw: &str) -> Self {
        let parsed = match ValueKind::of(code) {
            ValueKind::Str => None,
            ValueKind::Double => parse_f64(raw).map(Self::Double),
            ValueKind::Int => parse_int(raw).map(Self::Int),
            ValueKind::Bool => match parse_int(raw) {
                Some(0) => Some(Self::Bool(false)),
                Some(1) => Some(Self::Bool(true)),
                _ => None,
            },
        };
        parsed.unwrap_or_else(|| Self::Str(raw.to_owned()))
    }
}

/// Parses a finite floating-point value, ignoring surrounding whitespace.
pub fn parse_f64(raw: &str) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// Parses an integer, ignoring surrounding whitespace. Some writers emit
/// integers as `1.0`, so integral floats are accepted too.
pub fn parse_int(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if let Ok(value) = text.parse::<i64>() {
        return Some(value);
    }
    let value: f64 = text.parse().ok()?;
    // 2^53: the largest range where every integer is exactly representable.
    (value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0)
        .then_some(value as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_kinds_follow_the_reference_ranges() {
        assert_eq!(ValueKind::of(0), ValueKind::Str);
        assert_eq!(ValueKind::of(8), ValueKind::Str);
        assert_eq!(ValueKind::of(10), ValueKind::Double);
        assert_eq!(ValueKind::of(40), ValueKind::Double);
        assert_eq!(ValueKind::of(62), ValueKind::Int);
        assert_eq!(ValueKind::of(100), ValueKind::Str);
        assert_eq!(ValueKind::of(210), ValueKind::Double);
        assert_eq!(ValueKind::of(290), ValueKind::Bool);
        assert_eq!(ValueKind::of(330), ValueKind::Str);
        assert_eq!(ValueKind::of(370), ValueKind::Int);
        assert_eq!(ValueKind::of(420), ValueKind::Int);
        assert_eq!(ValueKind::of(1000), ValueKind::Str);
        assert_eq!(ValueKind::of(1040), ValueKind::Double);
        assert_eq!(ValueKind::of(1071), ValueKind::Int);
    }

    #[test]
    fn parse_is_typed_and_lossless() {
        assert_eq!(Value::parse(10, " 1.5"), Value::Double(1.5));
        assert_eq!(Value::parse(70, "  4"), Value::Int(4));
        assert_eq!(Value::parse(70, "4.0"), Value::Int(4));
        assert_eq!(Value::parse(290, "1"), Value::Bool(true));
        assert_eq!(
            Value::parse(1, " keep spaces "),
            Value::Str(" keep spaces ".into())
        );
        assert_eq!(Value::parse(10, "abc"), Value::Str("abc".into()));
        assert_eq!(Value::parse(10, "NaN"), Value::Str("NaN".into()));
        assert_eq!(Value::parse(290, "2"), Value::Str("2".into()));
        assert_eq!(Value::parse(70, "1.5"), Value::Str("1.5".into()));
    }

    #[test]
    fn numbers_reject_non_finite_and_garbage() {
        assert_eq!(parse_f64("inf"), None);
        assert_eq!(parse_f64(""), None);
        assert_eq!(parse_f64("1e3"), Some(1000.0));
        assert_eq!(parse_int("1e300"), None);
        assert_eq!(parse_int("-3"), Some(-3));
    }
}
