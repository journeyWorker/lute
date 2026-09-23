//! Classification of `<when is="…">` literal patterns (dsl §7.3.1; numeric
//! ranges dsl 0.18.0).
//!
//! `WhenPattern ::= Literal ("|" Literal)*` and
//! `Literal ::= EnumMember | "true" | "false" | Number | "unset" | Range` with
//! `Range ::= Number? ".." Number?` (at least one bound; both bounds
//! inclusive). The pattern is NOT CEL. This module is the single classifier
//! every consumer (checker, compiler, normalizer, trace runner) reads, so a
//! literal can never mean one thing to the checker and another at runtime.

/// An inclusive numeric interval. `None` is an open end (`2..` has no upper
/// bound, `..0` no lower bound). Never both `None`: `..` alone is malformed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumRange {
    pub lo: Option<f64>,
    pub hi: Option<f64>,
}

impl NumRange {
    /// Whether `n` lies inside the interval (both ends inclusive).
    pub fn contains(&self, n: f64) -> bool {
        self.lo.is_none_or(|lo| n >= lo) && self.hi.is_none_or(|hi| n <= hi)
    }
}

/// One classified `is=` alternative.
#[derive(Clone, Debug, PartialEq)]
pub enum IsLiteral {
    Bool(bool),
    /// The `unset` case (§9.4).
    Unset,
    /// A decimal `Number` literal.
    Num(f64),
    /// A numeric range literal (`N..M`, `N..`, `..M`).
    Range(NumRange),
    /// An enum-member ident (or branch/hub choice id), matched by string
    /// equality.
    Str(String),
}

/// Why an alternative containing `..` is not a valid range literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsLiteralError {
    /// Not `Number? ".." Number?` with at least one bound — e.g. `..`,
    /// `a..b`, `1...2`, `1..2..3`.
    MalformedRange,
    /// Both bounds present with `N > M`: the range matches nothing.
    EmptyRange,
}

/// True when `lit` is a decimal `Number` literal (dsl §7.3.1) rather than an
/// enum-member ident: a leading digit / sign / dot plus a successful `f64`
/// parse (which also rejects `inf`/`NaN`, since those lead with a letter).
pub fn is_number_literal(lit: &str) -> bool {
    parse_number(lit).is_some()
}

fn parse_number(lit: &str) -> Option<f64> {
    let head = lit.strip_prefix(['+', '-']).unwrap_or(lit);
    if !matches!(head.bytes().next(), Some(b'0'..=b'9' | b'.')) {
        return None;
    }
    lit.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Classify one trimmed `is=` alternative. Any alternative containing `..` is
/// a range literal — or an error; it never falls back to an enum member.
pub fn classify_is_literal(lit: &str) -> Result<IsLiteral, IsLiteralError> {
    match lit {
        "true" => return Ok(IsLiteral::Bool(true)),
        "false" => return Ok(IsLiteral::Bool(false)),
        "unset" => return Ok(IsLiteral::Unset),
        _ => {}
    }
    if let Some((lo_raw, hi_raw)) = lit.split_once("..") {
        // `1...2` would otherwise split into `1` and `.2` (a valid number).
        if hi_raw.starts_with('.') {
            return Err(IsLiteralError::MalformedRange);
        }
        let bound = |raw: &str| -> Result<Option<f64>, IsLiteralError> {
            if raw.is_empty() {
                Ok(None)
            } else {
                parse_number(raw)
                    .map(Some)
                    .ok_or(IsLiteralError::MalformedRange)
            }
        };
        let (lo, hi) = (bound(lo_raw)?, bound(hi_raw)?);
        return match (lo, hi) {
            (None, None) => Err(IsLiteralError::MalformedRange),
            (Some(l), Some(h)) if l > h => Err(IsLiteralError::EmptyRange),
            _ => Ok(IsLiteral::Range(NumRange { lo, hi })),
        };
    }
    Ok(match parse_number(lit) {
        Some(n) => IsLiteral::Num(n),
        None => IsLiteral::Str(lit.to_string()),
    })
}

/// The trimmed, non-empty alternatives of an `is=` pattern (a stray `|`
/// contributes nothing).
pub fn is_alternatives(raw: &str) -> impl Iterator<Item = &str> {
    raw.split('|').map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(lo: Option<f64>, hi: Option<f64>) -> Result<IsLiteral, IsLiteralError> {
        Ok(IsLiteral::Range(NumRange { lo, hi }))
    }

    #[test]
    fn scalar_literals_keep_their_meaning() {
        assert_eq!(classify_is_literal("true"), Ok(IsLiteral::Bool(true)));
        assert_eq!(classify_is_literal("unset"), Ok(IsLiteral::Unset));
        assert_eq!(classify_is_literal("-1.5"), Ok(IsLiteral::Num(-1.5)));
        assert_eq!(
            classify_is_literal("gold"),
            Ok(IsLiteral::Str("gold".into()))
        );
        assert_eq!(classify_is_literal("inf"), Ok(IsLiteral::Str("inf".into())));
    }

    #[test]
    fn range_shapes() {
        assert_eq!(classify_is_literal("2.."), range(Some(2.0), None));
        assert_eq!(classify_is_literal("..0"), range(None, Some(0.0)));
        assert_eq!(classify_is_literal("1..3"), range(Some(1.0), Some(3.0)));
        assert_eq!(classify_is_literal("-3..-1"), range(Some(-3.0), Some(-1.0)));
        assert_eq!(classify_is_literal("0.5..1.5"), range(Some(0.5), Some(1.5)));
        assert_eq!(classify_is_literal("2..2"), range(Some(2.0), Some(2.0)));
    }

    #[test]
    fn malformed_ranges_never_become_enum_members() {
        for lit in ["..", "a..b", "1...2", "1..2..3", "1..x", "inf..1"] {
            assert_eq!(
                classify_is_literal(lit),
                Err(IsLiteralError::MalformedRange),
                "{lit}"
            );
        }
        assert_eq!(classify_is_literal("3..1"), Err(IsLiteralError::EmptyRange));
    }

    #[test]
    fn range_bounds_are_inclusive() {
        let r = NumRange {
            lo: Some(1.0),
            hi: Some(3.0),
        };
        assert!(r.contains(1.0) && r.contains(3.0) && r.contains(2.5));
        assert!(!r.contains(0.999) && !r.contains(3.001));
        assert!(NumRange {
            lo: None,
            hi: Some(0.0)
        }
        .contains(-1e9));
    }
}
