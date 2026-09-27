//! Restricted-CEL `after` prerequisite-profile grammar + validator (connectivity
//! layer, Task 1). The `after:` value on a quest/scene is a CEL string that MUST
//! reduce to a pure boolean formula over `visited("id")` / `completed("id")` /
//! `active("id")` atoms combined with `&&` / `||` (parens are free — cel-parser
//! bakes grouping into tree shape, so there is no separate paren node to admit).
//! This is a DELIBERATELY narrower profile than
//! [`crate::cel_resolve::check_cel_profile`] (the general Lute-CEL admit-walk):
//! no negation, no arithmetic/comparison operators, no state-path reads, no
//! other function calls. A NEW sibling walk — never route through
//! `check_cel_profile`, whose broad "any literal/ident passes" leaves would
//! silently reopen this grammar to `scene.x`, `1 + 1`, `!visited(...)`, etc.
//!
//! ## `active` (lang 0.8.0)
//! The quest lifecycle is `unset → active → complete|failed`, but 0.7.0 admitted
//! only the terminal `completed(Q)` — gating on "this quest is currently under
//! way" was inexpressible, which is an asymmetry rather than a simplification
//! (the Stage adoption analysis measured 477 of 841 quest-referencing gate rows
//! needing exactly it). `active(Q)` is admitted ALONGSIDE `completed(Q)`, same
//! shape and same single-string-literal arity; the profile stays otherwise
//! closed. The two differ ONLY downstream:
//! - graph-wise they are IDENTICAL (both say "Q must be reached before me"), so
//!   [`crate::connectivity`]'s reachability and cycle detection treat them the
//!   same;
//! - envelope-wise `active` is STRICTLY WEAKER — after `completed(Q)` a consumer
//!   may assume `Q`'s completion writes landed, after `active(Q)` only that `Q`
//!   started, so [`crate::envelope`] contributes them to `possible` but never to
//!   `guaranteed`.
//!
//! Downstream connectivity tasks (graph assembly, reachability, envelope) all
//! consume [`PrereqFormula`]/[`atoms`] — the shapes here are load-bearing;
//! keep the enum/fn signatures stable.

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};

/// `E-CONN-PROFILE` (connectivity layer, Task 1): an `after` CEL formula used a
/// construct outside the restricted prerequisite profile — anything other than
/// `visited(StringLit)` / `completed(StringLit)` / `active(StringLit)` combined
/// with `&&` / `||` (parens are free; grouping is structural, not a separate
/// node). Negation, arithmetic, comparisons, state reads, and any other function
/// call are all out of profile. Emitted at the `span` passed to
/// [`parse_prereq`], mirroring `E_CEL_PROFILE`'s stop-and-report-then-skip-the-
/// branch shape.
pub const E_CONN_PROFILE: &str = "E-CONN-PROFILE";

/// The parsed `after` prerequisite formula: a boolean expression over
/// `visited`/`completed`/`active` atoms, closed under `&&`/`||`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrereqFormula {
    Visited(String),
    Completed(String),
    /// lang 0.8.0 `active("questId")`: the quest reached the `active`
    /// lifecycle state. A STRICTLY WEAKER claim than [`Self::Completed`] —
    /// see this module's header for the graph-identical / envelope-weaker
    /// split.
    Active(String),
    And(Box<PrereqFormula>, Box<PrereqFormula>),
    Or(Box<PrereqFormula>, Box<PrereqFormula>),
}

/// A single leaf condition flattened out of a [`PrereqFormula`] by [`atoms`]
/// (edge-extraction helper for later connectivity tasks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    Visited(String),
    Completed(String),
    /// lang 0.8.0: see [`PrereqFormula::Active`]. Targets a QUEST node,
    /// exactly like [`Self::Completed`].
    Active(String),
}

/// Parse `raw` (the CEL text of an `after` value) under the restricted
/// prerequisite profile. Returns `(Some(formula), [])` when `raw` reduces
/// entirely to the admitted grammar; returns `(None, diags)` — with at least
/// one [`E_CONN_PROFILE`] diagnostic — otherwise. `span` is used verbatim for
/// every diagnostic (the caller owns mapping it to a real source location).
///
/// A blank/whitespace-only `raw` is rejected BEFORE reaching
/// `cel_parser::Parser::parse` (connectivity T5 review-2 crash fix):
/// `cel-parser` 0.10.1 panics ("entered unreachable code") on empty or
/// whitespace-only input instead of returning a parse error — a real
/// dependency bug, not a profile violation this walk can otherwise catch.
/// Callers that need to treat an EXACT empty string as "no prerequisite"
/// (spec §4.1) MUST check that themselves before calling `parse_prereq`;
/// this guard only prevents the crash for whatever reaches here.
pub fn parse_prereq(raw: &str, span: Span) -> (Option<PrereqFormula>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    if raw.trim().is_empty() {
        diags.push(diag(
            "`after` value is empty or blank — not a valid prerequisite formula".to_string(),
            span,
        ));
        return (None, diags);
    }
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
/// `visited`/`completed`/`active` calls; anything else is out of profile and
/// stops descent into that branch (mirrors `E_CEL_PROFILE`'s stop-and-report).
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
                name @ ("visited" | "completed" | "active") if c.args.len() == 1 => {
                    if let Expr::Literal(Val::String(s)) = &c.args[0].expr {
                        return Some(match name {
                            "visited" => PrereqFormula::Visited(s.clone()),
                            "completed" => PrereqFormula::Completed(s.clone()),
                            _ => PrereqFormula::Active(s.clone()),
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

/// dsl 0.28.0 (T3-4): the offending part as written, and what to write
/// instead — a quoted id, a bare id's `visited("…")`, or `when:` for a
/// condition `after` cannot order by.
fn out_of_profile_message(expr: &Expr) -> String {
    const PROFILE: &str = "`after` lists what must come first: `visited(\"id\")`, \
                           `completed(\"id\")` and `active(\"id\")` joined with `&&` / `||`";
    let shown = crate::cel_types::show(expr);
    match expr {
        Expr::Call(c)
            if c.target.is_none()
                && matches!(c.func_name.as_str(), "visited" | "completed" | "active") =>
        {
            let id = match c.args.as_slice() {
                [a] => crate::cel_paths::select_path(&a.expr),
                _ => None,
            };
            match id {
                Some(id) => format!(
                    "`{shown}`: `{}` takes a quoted id — write `{}(\"{id}\")`",
                    c.func_name, c.func_name
                ),
                None => format!(
                    "`{shown}`: `{}` takes one quoted id; {PROFILE}",
                    c.func_name
                ),
            }
        }
        // A near miss (`complete('q1')`) is the prerequisite it misspells,
        // not a condition to move to `when:`.
        Expr::Call(c)
            if c.target.is_none()
                && lute_manifest::suggest::nearest(
                    &c.func_name,
                    ["visited", "completed", "active"],
                    2,
                )
                .is_some() =>
        {
            let meant = lute_manifest::suggest::nearest(
                &c.func_name,
                ["visited", "completed", "active"],
                2,
            )
            .unwrap_or_default();
            let id = match c.args.as_slice() {
                [a] => match &a.expr {
                    Expr::Literal(Val::String(s)) => Some(s.to_string()),
                    e => crate::cel_paths::select_path(e),
                },
                _ => None,
            };
            match id {
                Some(id) => format!("`{shown}`: did you mean `{meant}(\"{id}\")`? {PROFILE}"),
                None => format!("`{shown}`: did you mean `{meant}(…)`? {PROFILE}"),
            }
        }
        Expr::Ident(_) | Expr::Select(_) => match crate::cel_paths::select_path(expr) {
            Some(id) if !crate::cel_paths::is_state_path(&id) => format!(
                "`{shown}` is a bare id; {PROFILE} — did you mean `visited(\"{id}\")` (a scene) \
                 or `completed(\"{id}\")` (a quest)?"
            ),
            _ => format!(
                "`{shown}` is a state read, which `after` cannot order by; {PROFILE} — move the \
                 condition to `when:`"
            ),
        },
        _ => format!(
            "`{shown}` is a condition, which `after` cannot order by; {PROFILE} — move it to \
             `when:`"
        ),
    }
}

/// Flatten a [`PrereqFormula`] into its leaf atoms (edge-extraction helper for
/// later connectivity tasks — graph assembly reads these to know which
/// `visited`/`completed`/`active` targets an `after` formula depends on).
pub fn atoms(f: &PrereqFormula) -> Vec<Atom> {
    let mut out = Vec::new();
    collect_atoms(f, &mut out);
    out
}

fn collect_atoms(f: &PrereqFormula, out: &mut Vec<Atom>) {
    match f {
        PrereqFormula::Visited(id) => out.push(Atom::Visited(id.clone())),
        PrereqFormula::Completed(id) => out.push(Atom::Completed(id.clone())),
        PrereqFormula::Active(id) => out.push(Atom::Active(id.clone())),
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
        let (f, codes) = parse(r#"visited("elena.ep02") && (completed("q1") || completed("q2"))"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert!(f.is_some());
    }

    /// A misspelt prerequisite is named, not sent to `when:` (where it fails).
    #[test]
    fn a_near_miss_names_the_prerequisite() {
        let (_f, diags) = parse_prereq("complete('q1')", test_span());
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("did you mean `completed(\"q1\")`?"),
            "{}",
            diags[0].message
        );
        assert!(!diags[0].message.contains("when:"), "{}", diags[0].message);
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

    #[test]
    fn blank_input_rejected_without_panic() {
        // cel-parser 0.10.1 panics on empty/whitespace-only input instead of
        // returning a parse error (connectivity T5 review-2 crash fix); the
        // early guard in `parse_prereq` MUST reject it as E-CONN-PROFILE
        // before ever reaching `cel_parser::Parser::parse`.
        let (f, codes) = parse("   ");
        assert!(f.is_none());
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));

        let (f, codes) = parse("");
        assert!(f.is_none());
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    // --- lang 0.8.0: `active(StringLit)` ---

    #[test]
    fn active_string_literal_parses() {
        let (f, codes) = parse(r#"active("q1")"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert_eq!(f, Some(PrereqFormula::Active("q1".to_string())));
    }

    #[test]
    fn active_composes_with_completed_under_and() {
        let (f, codes) = parse(r#"active("a") && completed("b")"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert_eq!(
            f,
            Some(PrereqFormula::And(
                Box::new(PrereqFormula::Active("a".to_string())),
                Box::new(PrereqFormula::Completed("b".to_string())),
            ))
        );
    }

    #[test]
    fn active_flattens_to_its_own_atom() {
        let (f, codes) = parse(r#"active("a") || completed("a")"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert_eq!(
            atoms(&f.expect("in-profile formula")),
            vec![
                Atom::Active("a".to_string()),
                Atom::Completed("a".to_string())
            ],
            "`active` and `completed` on the SAME id must stay distinguishable atoms"
        );
    }

    #[test]
    fn negated_active_rejected() {
        // The profile stays closed: widening it with `active` admits the CALL,
        // never negation around it.
        let (f, codes) = parse(r#"!active("a")"#);
        assert!(f.is_none());
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn active_non_literal_arg_rejected() {
        // A state-path read is still out of profile as an `active` argument —
        // the arg shape check is the SAME single-string-literal one
        // `visited`/`completed` get.
        let (f, codes) = parse(r#"active(scene.x)"#);
        assert!(f.is_none());
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn active_wrong_arity_rejected() {
        let (f, codes) = parse(r#"active("a", "b")"#);
        assert!(f.is_none());
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }
}
