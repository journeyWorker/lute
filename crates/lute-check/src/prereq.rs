//! Restricted-CEL `after` prerequisite-profile grammar + validator (connectivity
//! layer, Task 1). The `after:` value on a quest/scene is a CEL string that MUST
//! reduce to a pure boolean formula over `visited("id")` / `completed("id")`
//! atoms combined with `&&` / `||` (parens are free — cel-parser bakes grouping
//! into tree shape, so there is no separate paren node to admit). This is a
//! DELIBERATELY narrower profile than [`crate::cel_resolve::check_cel_profile`]
//! (the general Lute-CEL admit-walk): no negation, no arithmetic/comparison
//! operators, no state-path reads, no other function calls. A NEW sibling walk
//! — never route through `check_cel_profile`, whose broad "any literal/ident
//! passes" leaves would silently reopen this grammar to `scene.x`, `1 + 1`,
//! `!visited(...)`, etc.
//!
//! Downstream connectivity tasks (graph assembly, reachability, envelope) all
//! consume [`PrereqFormula`]/[`atoms`] — the shapes here are load-bearing;
//! keep the enum/fn signatures stable.

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};

/// `E-CONN-PROFILE` (connectivity layer, Task 1): an `after` CEL formula used a
/// construct outside the restricted prerequisite profile — anything other than
/// `visited(StringLit)` / `completed(StringLit)` combined with `&&` / `||`
/// (parens are free; grouping is structural, not a separate node). Negation,
/// arithmetic, comparisons, state reads, and any other function call are all
/// out of profile. Emitted at the `span` passed to [`parse_prereq`], mirroring
/// `E_CEL_PROFILE`'s stop-and-report-then-skip-the-branch shape.
pub const E_CONN_PROFILE: &str = "E-CONN-PROFILE";

/// The parsed `after` prerequisite formula: a boolean expression over
/// `visited`/`completed` atoms, closed under `&&`/`||`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrereqFormula {
    Visited(String),
    Completed(String),
    And(Box<PrereqFormula>, Box<PrereqFormula>),
    Or(Box<PrereqFormula>, Box<PrereqFormula>),
}

/// A single leaf condition flattened out of a [`PrereqFormula`] by [`atoms`]
/// (edge-extraction helper for later connectivity tasks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    Visited(String),
    Completed(String),
}

/// Parse `raw` (the CEL text of an `after` value) under the restricted
/// prerequisite profile. Returns `(Some(formula), [])` when `raw` reduces
/// entirely to the admitted grammar; returns `(None, diags)` — with at least
/// one [`E_CONN_PROFILE`] diagnostic — otherwise. `span` is used verbatim for
/// every diagnostic (the caller owns mapping it to a real source location).
pub fn parse_prereq(raw: &str, span: Span) -> (Option<PrereqFormula>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let expr = match cel_parser::Parser::new().parse(raw) {
        Ok(ided) => ided.expr,
        Err(errs) => {
            let msg = errs
                .errors
                .first()
                .map(|e| e.msg.clone())
                .unwrap_or_else(|| "CEL parse error".to_string());
            diags.push(diag(
                format!("`after` value failed to parse as CEL: {msg}"),
                span,
            ));
            return (None, diags);
        }
    };
    let formula = walk(&expr, span, &mut diags);
    if !diags.is_empty() {
        (None, diags)
    } else {
        (formula, diags)
    }
}

/// The admit-walk: recurse into `&&`/`||` operator calls and well-shaped
/// `visited`/`completed` calls; anything else is out of profile and stops
/// descent into that branch (mirrors `E_CEL_PROFILE`'s stop-and-report).
fn walk(expr: &Expr, span: Span, diags: &mut Vec<Diagnostic>) -> Option<PrereqFormula> {
    use cel_parser::ast::operators as op;

    if let Expr::Call(c) = expr {
        if c.target.is_none() {
            match c.func_name.as_str() {
                name @ (op::LOGICAL_AND | op::LOGICAL_OR) if c.args.len() == 2 => {
                    let lhs = walk(&c.args[0].expr, span, diags);
                    let rhs = walk(&c.args[1].expr, span, diags);
                    return match (lhs, rhs) {
                        (Some(l), Some(r)) if name == op::LOGICAL_AND => {
                            Some(PrereqFormula::And(Box::new(l), Box::new(r)))
                        }
                        (Some(l), Some(r)) => Some(PrereqFormula::Or(Box::new(l), Box::new(r))),
                        _ => None,
                    };
                }
                "visited" | "completed" if c.args.len() == 1 => {
                    if let Expr::Literal(Val::String(s)) = &c.args[0].expr {
                        return Some(if c.func_name == "visited" {
                            PrereqFormula::Visited(s.clone())
                        } else {
                            PrereqFormula::Completed(s.clone())
                        });
                    }
                }
                _ => {}
            }
        }
    }

    diags.push(diag(out_of_profile_message(expr), span));
    None
}

fn out_of_profile_message(expr: &Expr) -> String {
    match expr {
        Expr::Call(c) => format!(
            "`{}(…)` is outside the `after` prerequisite profile — only \
             `visited(\"id\")`, `completed(\"id\")`, `&&`, and `||` are permitted \
             (no negation, arithmetic, comparisons, or other calls)",
            c.func_name
        ),
        _ => "this construct is outside the `after` prerequisite profile — only \
              `visited(\"id\")` / `completed(\"id\")` combined with `&&` / `||` \
              are permitted"
            .to_string(),
    }
}

/// Flatten a [`PrereqFormula`] into its leaf atoms (edge-extraction helper for
/// later connectivity tasks — graph assembly reads these to know which
/// `visited`/`completed` targets an `after` formula depends on).
pub fn atoms(f: &PrereqFormula) -> Vec<Atom> {
    let mut out = Vec::new();
    collect_atoms(f, &mut out);
    out
}

fn collect_atoms(f: &PrereqFormula, out: &mut Vec<Atom>) {
    match f {
        PrereqFormula::Visited(id) => out.push(Atom::Visited(id.clone())),
        PrereqFormula::Completed(id) => out.push(Atom::Completed(id.clone())),
        PrereqFormula::And(l, r) | PrereqFormula::Or(l, r) => {
            collect_atoms(l, out);
            collect_atoms(r, out);
        }
    }
}

/// Build a `Layer::Cel` `E_CONN_PROFILE` error diagnostic (mirrors
/// `cel_resolve::diag`'s shape).
fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_PROFILE.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn parse(s: &str) -> (Option<PrereqFormula>, Vec<String>) {
        let (f, diags) = parse_prereq(s, test_span());
        (f, diags.into_iter().map(|d| d.code).collect())
    }

    #[test]
    fn and_or_of_visited_completed_ok() {
        let (f, codes) = parse(r#"visited("sofia.ep02") && (completed("q1") || completed("q2"))"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert!(f.is_some());
    }

    #[test]
    fn negation_rejected() {
        let (_f, codes) = parse(r#"!visited("a")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn wrong_arity_rejected() {
        let (_f, codes) = parse(r#"visited("a", "b")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn non_string_arg_rejected() {
        let (_f, codes) = parse(r#"visited(42)"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn bare_string_rejected() {
        let (_f, codes) = parse(r#""x""#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn unknown_call_rejected() {
        let (_f, codes) = parse(r#"holds(a) && visited("x")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }
}
