//! `W-WHEN-TEST-LITERAL` (dsl 0.18.0 §3): a `<match>` arm written as a CEL
//! literal comparison — `<when test="$ == 'gold'">` — when the `is=` pattern
//! form (`<when is="gold">`, dsl §7.3.1) says the same thing. `is=` is the form
//! the checker reasons about (coverage, overlap, dead arms, literal domains);
//! a `test=` guard is opaque beyond a conservative sniff. The old form stays
//! VALID — this is a warning, never an error.
//!
//! Recognized `test` shapes (and nothing else — no `>`/`<`/`!=`/`null`, no
//! `@ref`, no subject path spelled out):
//!
//! | `test`                                   | `is`       |
//! |------------------------------------------|------------|
//! | `$ == L` / `L == $`                      | `L`        |
//! | `$ in [L, …]`                            | `a\|b\|…`  |
//! | `$ >= N` / `N <= $`                      | `N..`      |
//! | `$ <= N` / `N >= $`                      | `..N`      |
//! | `$ >= A && $ <= B` (either conjunct order, `A <= B`) | `A..B` |
//!
//! `L` is a string, bool, or number literal. SAFETY: a rewrite is produced
//! only when the `is=` pattern classifies
//! ([`lute_syntax::is_pattern::classify_is_literal`]) to exactly the meaning
//! the test had — a string must be an enum-member-shaped ident
//! (`^[A-Za-z_][A-Za-z0-9_-]*$`) and not `true`/`false`/`unset` (so `'1'`,
//! `'true'`, `'a b'`, `'x..y'` are left alone), and every produced pattern is
//! round-tripped through the classifier before it is returned. Numbers print
//! in shortest round-trip form (`2`, never `2.0`).
//!
//! The rewrite is meaning-preserving (the arm lowers to the identical guard),
//! so it rides a `"migrate"` [`Fixit`] and `lute fix` applies it unprompted
//! ([`crate::fix`] phase 2). Both read [`when_test_rewrites`], so the LSP fixit
//! and `lute fix` are byte-identical by construction.

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Fixit, Layer, Severity, Span, TextEdit};
use lute_syntax::ast::{Arm, Document, Node};
use lute_syntax::is_pattern::{classify_is_literal, is_alternatives, IsLiteral, NumRange};

pub const W_WHEN_TEST_LITERAL: &str = "W-WHEN-TEST-LITERAL";

/// A scalar literal a recognized `test` compares the subject against.
#[derive(Clone, Debug, PartialEq)]
enum Lit {
    Str(String),
    Bool(bool),
    Num(f64),
}

/// The `is=` pattern equivalent of a `<when test>` (dsl 0.18.0 §3), or `None`
/// when `test_raw` is not one of the recognized literal-comparison shapes (see
/// module docs) or its rewrite would not classify back to the same meaning.
/// `test_raw` is the RAW slot text, so the subject is the literal `$` token.
pub fn test_as_is_pattern(test_raw: &str) -> Option<String> {
    // Every DSL token must be the `$` subject (no `@ref`), and the recognized
    // shape must account for every one of them — `parse_slot` rewrites `$` to
    // `_`, so this is what tells a real `$` from an author-written `_`.
    let refs = lute_cel::scan_refs(test_raw);
    if refs.iter().any(|r| !r.is_dollar) {
        return None;
    }
    let expr = parse_expr(test_raw)?;
    let (pattern, subjects) = match recognize(&expr)? {
        Recognized::Points(lits) => (points_pattern(&lits)?, 1),
        Recognized::Range(range) => {
            let subjects = if range.lo.is_some() && range.hi.is_some() {
                2
            } else {
                1
            };
            (range_pattern(range)?, subjects)
        }
    };
    (refs.len() == subjects).then_some(pattern)
}

enum Recognized {
    Points(Vec<Lit>),
    Range(NumRange),
}

fn recognize(expr: &Expr) -> Option<Recognized> {
    let Expr::Call(c) = expr else {
        return None;
    };
    if c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    let (a, b) = (&c.args[0].expr, &c.args[1].expr);
    match c.func_name.as_str() {
        "_==_" => {
            let lit = if is_dollar(a) {
                literal(b)?
            } else if is_dollar(b) {
                literal(a)?
            } else {
                return None;
            };
            Some(Recognized::Points(vec![lit]))
        }
        "@in" if is_dollar(a) => {
            let Expr::List(list) = b else {
                return None;
            };
            if list.elements.is_empty() {
                return None;
            }
            let lits = list
                .elements
                .iter()
                .map(|e| literal(&e.expr))
                .collect::<Option<Vec<_>>>()?;
            Some(Recognized::Points(lits))
        }
        "_&&_" => {
            let (x, y) = (bound(a)?, bound(b)?);
            let (lo, hi) = match (x, y) {
                (Bound::Lo(lo), Bound::Hi(hi)) | (Bound::Hi(hi), Bound::Lo(lo)) => (lo, hi),
                _ => return None,
            };
            (lo <= hi).then_some(Recognized::Range(NumRange {
                lo: Some(lo),
                hi: Some(hi),
            }))
        }
        _ => Some(Recognized::Range(match bound(expr)? {
            Bound::Lo(lo) => NumRange {
                lo: Some(lo),
                hi: None,
            },
            Bound::Hi(hi) => NumRange {
                lo: None,
                hi: Some(hi),
            },
        })),
    }
}

/// One inclusive numeric bound on the subject.
enum Bound {
    /// `$ >= N` / `N <= $`.
    Lo(f64),
    /// `$ <= N` / `N >= $`.
    Hi(f64),
}

fn bound(expr: &Expr) -> Option<Bound> {
    let Expr::Call(c) = expr else {
        return None;
    };
    if c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    let (a, b) = (&c.args[0].expr, &c.args[1].expr);
    let (op, n) = if is_dollar(a) {
        (c.func_name.as_str(), number(b)?)
    } else if is_dollar(b) {
        // `N <= $` is `$ >= N`; `N >= $` is `$ <= N`.
        let flipped = match c.func_name.as_str() {
            "_<=_" => "_>=_",
            "_>=_" => "_<=_",
            _ => return None,
        };
        (flipped, number(a)?)
    } else {
        return None;
    };
    match op {
        "_>=_" => Some(Bound::Lo(n)),
        "_<=_" => Some(Bound::Hi(n)),
        _ => None,
    }
}

/// The substituted `$` subject (`parse_slot` rewrites `$` to `_`).
fn is_dollar(expr: &Expr) -> bool {
    matches!(expr, Expr::Ident(name) if name == "_")
}

fn literal(expr: &Expr) -> Option<Lit> {
    match expr {
        Expr::Literal(Val::String(s)) => Some(Lit::Str(s.clone())),
        Expr::Literal(Val::Boolean(b)) => Some(Lit::Bool(*b)),
        _ => number(expr).map(Lit::Num),
    }
}

/// A CEL `int`/`double` literal (optionally negated) as the real it denotes.
/// `uint` (`2u`) is not a Lute Number literal and is rejected, as is an `int`
/// too large to be exact in `f64`.
fn number(expr: &Expr) -> Option<f64> {
    const EXACT: i64 = 1 << 53;
    let n = match expr {
        Expr::Literal(Val::Int(i)) if (-EXACT..=EXACT).contains(i) => *i as f64,
        Expr::Literal(Val::Double(d)) if d.is_finite() => *d,
        Expr::Call(c) if c.target.is_none() && c.func_name == "-_" && c.args.len() == 1 => {
            return number(&c.args[0].expr).map(|n| -n);
        }
        _ => return None,
    };
    Some(n)
}

/// A string literal is only rewritable when, bare, it still classifies as the
/// same enum-member string: an ident (`^[A-Za-z_][A-Za-z0-9_-]*$`) that is not
/// one of the reserved literals.
fn safe_member(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !matches!(s, "true" | "false" | "unset")
}

fn points_pattern(lits: &[Lit]) -> Option<String> {
    let mut parts = Vec::with_capacity(lits.len());
    for lit in lits {
        parts.push(match lit {
            Lit::Str(s) if safe_member(s) => s.clone(),
            Lit::Str(_) => return None,
            Lit::Bool(b) => b.to_string(),
            Lit::Num(n) => format_num(*n),
        });
    }
    let pattern = parts.join("|");
    let round_trips = {
        let mut alts = is_alternatives(&pattern);
        lits.iter().all(|lit| {
            let Some(Ok(classified)) = alts.next().map(classify_is_literal) else {
                return false;
            };
            match (classified, lit) {
                (IsLiteral::Str(a), Lit::Str(b)) => a == *b,
                (IsLiteral::Bool(a), Lit::Bool(b)) => a == *b,
                (IsLiteral::Num(a), Lit::Num(b)) => a == *b,
                _ => false,
            }
        }) && alts.next().is_none()
    };
    round_trips.then_some(pattern)
}

fn range_pattern(range: NumRange) -> Option<String> {
    let fmt = |b: Option<f64>| b.map(format_num).unwrap_or_default();
    let pattern = format!("{}..{}", fmt(range.lo), fmt(range.hi));
    matches!(classify_is_literal(&pattern), Ok(IsLiteral::Range(r)) if r == range)
        .then_some(pattern)
}

/// Shortest round-trip decimal (`2`, `0.5`, `-3`); never exponent notation,
/// which the `is=` Number grammar would not read back.
fn format_num(n: f64) -> String {
    format!("{n}")
}

/// Throwaway re-parse of a raw CEL fragment into its root [`Expr`] (the same
/// `lute_cel::parse_slot` path `match_check.rs` uses).
fn parse_expr(raw: &str) -> Option<Expr> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, raw, 0).ok()?;
    arena.get(handle).map(|root| root.expr.clone())
}

/// One `<when test="…">` arm that rewrites to `<when is="…">`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WhenTestRewrite {
    /// Byte range of the whole `test="…"` attribute in the source.
    pub start: usize,
    pub end: usize,
    /// The authored `test` expression (the attribute's inner text).
    pub test: String,
    /// The equivalent `is=` pattern.
    pub pattern: String,
}

impl WhenTestRewrite {
    /// The replacement text for `[start, end)`.
    pub fn new_text(&self) -> String {
        format!("is=\"{}\"", self.pattern)
    }
}

/// Every rewritable `<when>` arm in `doc` (scene shots, quest bodies — and
/// every `<match>` nested in choices, hubs, arms, `<on>`, and objectives), in
/// document order. Only `<when>` arms WITHOUT `is=` and with a quoted
/// `test="…"` attribute qualify; a `<choice when>`/line `when` guard is never a
/// match arm and never visited. `src` is the text `doc` was parsed from.
pub(crate) fn when_test_rewrites(doc: &Document, src: &str) -> Vec<WhenTestRewrite> {
    let mut out = Vec::new();
    for shot in &doc.shots {
        collect(&shot.body, src.as_bytes(), &mut out);
    }
    for quest in &doc.quests {
        collect(&quest.body, src.as_bytes(), &mut out);
    }
    out
}

fn collect(nodes: &[Node], src: &[u8], out: &mut Vec<WhenTestRewrite>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect(&choice.body, src, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect(&choice.body, src, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { is, test, body, .. } => {
                            if is.is_none() {
                                out.extend(arm_rewrite(&test.raw, test.span, src));
                            }
                            collect(body, src, out);
                        }
                        Arm::Otherwise { body, .. } => collect(body, src, out),
                    }
                }
            }
            Node::On(o) => collect(&o.body, src, out),
            Node::Objective(o) => collect(&o.body, src, out),
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// The rewrite for one arm's `test` slot, when its attribute is the quoted
/// `test="…"` form (`scan_attrs` gives the slot the value's INNER span, so the
/// attribute is `test="` + value + `"` around it). A bare `test=x` or
/// `test=@ref` is never rewritten.
fn arm_rewrite(raw: &str, value: Span, src: &[u8]) -> Option<WhenTestRewrite> {
    const KEY: &[u8] = b"test=\"";
    let start = value.byte_start.checked_sub(KEY.len())?;
    if src.get(start..value.byte_start)? != KEY || src.get(value.byte_end) != Some(&b'"') {
        return None;
    }
    let pattern = test_as_is_pattern(raw)?;
    Some(WhenTestRewrite {
        start,
        end: value.byte_end + 1,
        test: raw.to_string(),
        pattern,
    })
}

/// `W-WHEN-TEST-LITERAL` diagnostics for `doc` (dsl 0.18.0 §3), each anchored
/// at the whole `test="…"` attribute and carrying ONE `"migrate"` fixit that
/// replaces it with `is="…"` — the same edit `lute fix` applies.
pub fn check_when_test_literals(doc: &Document, src: &str) -> Vec<Diagnostic> {
    when_test_rewrites(doc, src)
        .into_iter()
        .map(|rw| {
            let new_text = rw.new_text();
            Diagnostic {
                code: W_WHEN_TEST_LITERAL.to_string(),
                severity: Severity::Warning,
                message: format!(
                    "`test=\"{}\"` compares the match subject to a literal; write `{new_text}` \
                     — the pattern form the checker can reason about; `lute fix` rewrites it \
                     (dsl 0.18.0 §3)",
                    rw.test,
                ),
                span: zeroed_span(rw.start, rw.end),
                layer: Layer::Logic,
                fixits: vec![Fixit {
                    title: format!("rewrite as {new_text}"),
                    kind: "migrate".to_string(),
                    edit: vec![TextEdit {
                        span: zeroed_span(rw.start, rw.end),
                        new_text,
                    }],
                    confidence: 100,
                }],
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }
        })
        .collect()
}

/// A byte-only span; `check()`'s `normalize_spans` fills line/column/utf16
/// (diagnostic and fixit-edit spans alike).
fn zeroed_span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is(test: &str) -> Option<String> {
        test_as_is_pattern(test)
    }

    #[test]
    fn equality_either_side() {
        assert_eq!(is("$ == 'gold'").as_deref(), Some("gold"));
        assert_eq!(is("'gold' == $").as_deref(), Some("gold"));
        assert_eq!(is("$ == \"a-b_c\"").as_deref(), Some("a-b_c"));
        assert_eq!(is("$ == true").as_deref(), Some("true"));
        assert_eq!(is("false == $").as_deref(), Some("false"));
        assert_eq!(is("$ == 2").as_deref(), Some("2"));
        assert_eq!(is("$ == 2.0").as_deref(), Some("2"));
        assert_eq!(is("$ == -3").as_deref(), Some("-3"));
        assert_eq!(is("$ == 0.5").as_deref(), Some("0.5"));
    }

    #[test]
    fn membership_joins_alternatives() {
        assert_eq!(
            is("$ in ['silver', 'bronze']").as_deref(),
            Some("silver|bronze")
        );
        assert_eq!(is("$ in [1, 2.5, true]").as_deref(), Some("1|2.5|true"));
        assert_eq!(is("$ in []"), None);
        assert_eq!(
            is("$ in ['a', 'b c']"),
            None,
            "one unsafe member rejects all"
        );
    }

    #[test]
    fn single_bounds_both_spellings() {
        assert_eq!(is("$ >= 3").as_deref(), Some("3.."));
        assert_eq!(is("3 <= $").as_deref(), Some("3.."));
        assert_eq!(is("$ <= -1.5").as_deref(), Some("..-1.5"));
        assert_eq!(is("0 >= $").as_deref(), Some("..0"));
    }

    #[test]
    fn closed_range_either_conjunct_order() {
        assert_eq!(is("$ >= 1 && $ <= 5").as_deref(), Some("1..5"));
        assert_eq!(is("$ <= 5 && $ >= 1").as_deref(), Some("1..5"));
        assert_eq!(is("$ >= -3 && $ <= -1").as_deref(), Some("-3..-1"));
        assert_eq!(
            is("$ >= 2 && $ <= 2").as_deref(),
            Some("2..2"),
            "A == B is allowed"
        );
    }

    #[test]
    fn unsafe_strings_are_not_rewritten() {
        for t in [
            "$ == '1'",
            "$ == 'true'",
            "$ == 'false'",
            "$ == 'unset'",
            "$ == 'a b'",
            "$ == 'x..y'",
            "$ == ''",
            "$ == '-x'",
            "$ == 'a|b'",
            "$ == 'café'",
        ] {
            assert_eq!(is(t), None, "{t}");
        }
    }

    #[test]
    fn other_shapes_are_not_rewritten() {
        for t in [
            "$ > 2",
            "$ < 2",
            "$ != 'x'",
            "$ == null",
            "@ref",
            "@veteran == 'gold'",
            "@atLeast(3)",
            "$",
            "!$",
            "$ >= 3 && $ <= 1",
            "$ >= 1 && $ >= 2",
            "$ >= 1 || $ <= 0",
            "$ >= 1 && $ < 3",
            "$ == 2u",
            "$ == 'a' && $ == 'b'",
            "run.rank == 'gold'",
            "_ == 'gold'",
            "$ >= 1 && _ <= 3",
            "$ == $",
            "",
            "$ ==",
        ] {
            assert_eq!(is(t), None, "{t}");
        }
    }

    #[test]
    fn rewrite_needs_the_quoted_test_attribute() {
        let src = "<when test=\"$ == 'gold'\">";
        let value = zeroed_span(12, 23);
        let rw = arm_rewrite("$ == 'gold'", value, src.as_bytes()).expect("quoted form");
        assert_eq!(&src[rw.start..rw.end], "test=\"$ == 'gold'\"");
        assert_eq!(rw.new_text(), "is=\"gold\"");
        // A value span not preceded by `test="` (e.g. `test=@ref`) is skipped.
        assert_eq!(
            arm_rewrite("$ == 'gold'", zeroed_span(11, 22), src.as_bytes()),
            None
        );
    }
}
