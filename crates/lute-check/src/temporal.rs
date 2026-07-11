//! Narrative-time typing: `E-TEMPORAL-ARG` (dsl 0.3.0 §6, Task 12).
//!
//! `Type::NarrativeTime` (`lute-manifest/src/types.rs`) is an OPAQUE,
//! ordering-only value: `now()` (admitted into the Lute-CEL profile by Task
//! 11) and any engine-declared anchor path typed `narrativeTime` both produce
//! it. This module is a THIRD, independent CEL-AST pass (`cel_resolve.rs`'s
//! own module doc: "more than one may fire") that enforces §6's algebra: a
//! narrative-time value may appear ONLY as one side of an admitted ordering
//! comparison against ANOTHER narrative-time value, or as `validAt`'s second
//! argument. Every other use — arithmetic, indexing, field access, list
//! construction, a bare root position, or a comparison against a
//! non-narrative-time value — is [`E_TEMPORAL_ARG`].
//!
//! **D8 (controller-tightened):** the admitted comparison set is the FIVE
//! ordering/identity operators `<`, `<=`, `==`, `>`, `>=` — NOT six. `!=` is
//! deliberately REJECTED even between two narrative-time values: it is
//! identity-NEGATION, a broader predicate outside §6's admitted surface; an
//! author needing it writes `!(a == b)`. This diverges from an earlier
//! plan-body reading that treated `!=` as legal ("all six ops") — the
//! Decisions section (D8) is authoritative and wins.
//!
//! **D11:** `narrativeTime` is never author-declarable state; an author
//! `state:` decl (inline or schema doc) of it is rejected at the decl site
//! ([`crate::meta`]'s `state:` loop), not here — this module only ever sees
//! it via an already-folded, engine-declared anchor.
//!
//! Runs on the SAME marker re-parse `check_cel_slot` already built for
//! `check_cel_profile`/`check_fact_queries` (called right after them), so a
//! malformed CEL parse never reaches here (gated by the caller's `slot.ast`
//! check).

use cel_parser::ast::{operators, Expr};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::CelSlot;

use crate::cel_resolve::is_profile_fact_query;
use crate::Ctx;

/// dsl 0.3.0 §6 + D8/D11: a narrative-time value used anywhere other than an
/// admitted ordering comparison against another narrative-time value, or
/// `validAt`'s second argument.
pub const E_TEMPORAL_ARG: &str = "E-TEMPORAL-ARG";

/// D8 (controller-tightened): the FIVE comparison operators admitted between
/// two narrative-time values. `operators::NOT_EQUALS` is deliberately absent
/// — see the module doc.
const ADMITTED_COMPARISONS: &[&str] = &[
    operators::EQUALS,
    operators::LESS,
    operators::LESS_EQUALS,
    operators::GREATER,
    operators::GREATER_EQUALS,
];

/// Ordering-only typing pass for narrative-time expressions (dsl 0.3.0 §6).
/// Bottom-up classification ([`is_nt`]): a narrative-time value is well-typed
/// ONLY as one side of an admitted ordering comparison (both sides NT) or
/// `validAt`'s second argument; every other use is [`E_TEMPORAL_ARG`].
pub fn check_temporal(expr: &Expr, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // The slot ROOT is always a value/bool position, never a comparison
    // operand or a validAt argument — a bare narrative-time expression there
    // (`now()` alone, a bare anchor path) is always illegal.
    if is_nt(expr, ctx) {
        diags.push(diag(
            E_TEMPORAL_ARG,
            "a narrative-time value cannot stand alone; use it only in an ordering \
             comparison against another narrative-time value, or as `validAt`'s \
             second argument (dsl 0.3.0 §6)"
                .to_string(),
            slot.span,
        ));
        return;
    }
    walk(expr, slot, ctx, diags);
}

/// `true` when `expr` produces a narrative-time value (dsl 0.3.0 §6): a bare
/// `now()` call, or a pure `Ident`/`Select` state-path chain
/// ([`crate::cel_paths::select_path`]) whose declared type
/// ([`crate::set_op::resolve_type`]) is [`Type::NarrativeTime`]. Purely
/// syntactic/type-driven — an unresolvable (undeclared) path is never NT, so
/// it stays whatever else the checker independently flags it as (plain
/// `E-UNDECLARED`, dsl 0.3.0 D11's reuse note).
fn is_nt(expr: &Expr, ctx: &Ctx<'_>) -> bool {
    if let Expr::Call(c) = expr {
        if c.func_name == "now" && c.target.is_none() && c.args.is_empty() {
            return true;
        }
    }
    match crate::cel_paths::select_path(expr) {
        Some(path) => matches!(
            crate::set_op::resolve_type(&path, &ctx.env.state),
            Some(Type::NarrativeTime)
        ),
        None => false,
    }
}

/// Structural walk enforcing §6's algebra on every non-root position. Every
/// call site only ever recurses into a child ALREADY known not to be
/// narrative-time itself (checked via [`is_nt`] first) — a narrative-time
/// leaf (`now()`, a bare anchor path) has no further substructure to walk, so
/// `walk` never needs to witness one directly; each arm below re-establishes
/// that invariant for its own children before descending.
fn walk(expr: &Expr, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            let name = c.func_name.as_str();
            let binary = c.target.is_none() && c.args.len() == 2;

            // D8: the five admitted ordering comparisons. Both sides must
            // agree on narrative-time-ness; if they do, the pair is a leaf —
            // legal, nothing further to check on either side.
            if binary && ADMITTED_COMPARISONS.contains(&name) {
                let lhs_nt = is_nt(&c.args[0].expr, ctx);
                let rhs_nt = is_nt(&c.args[1].expr, ctx);
                if lhs_nt || rhs_nt {
                    if lhs_nt != rhs_nt {
                        diags.push(diag(
                            E_TEMPORAL_ARG,
                            "narrative-time values compare only against another \
                             narrative-time value (dsl 0.3.0 §6)"
                                .to_string(),
                            slot.span,
                        ));
                    }
                    return;
                }
                walk(&c.args[0].expr, slot, ctx, diags);
                walk(&c.args[1].expr, slot, ctx, diags);
                return;
            }

            // D8 (tightened): `!=` is rejected outright between narrative-time
            // values — identity-negation, not an ordering comparison, and
            // stays outside §6's admitted surface even though `==` is
            // admitted. Given its own message rather than folding into the
            // generic "no arithmetic" bucket below.
            if binary && name == operators::NOT_EQUALS {
                let lhs_nt = is_nt(&c.args[0].expr, ctx);
                let rhs_nt = is_nt(&c.args[1].expr, ctx);
                if lhs_nt || rhs_nt {
                    diags.push(diag(
                        E_TEMPORAL_ARG,
                        "`!=` is not admitted between narrative-time values (D8 \
                         admits only <, <=, ==, >, >=); negate an `==` comparison \
                         instead, e.g. `!(a == b)` (dsl 0.3.0 §6)"
                            .to_string(),
                        slot.span,
                    ));
                    return;
                }
                walk(&c.args[0].expr, slot, ctx, diags);
                walk(&c.args[1].expr, slot, ctx, diags);
                return;
            }

            // `holds`/`count`/`validAt`/`now` (dsl 0.3.0 §6/§8, T11): the
            // pattern argument (`holds`/`count`'s sole arg, `validAt`'s first
            // arg) is a relation Call, never a CEL sub-expression — mirrors
            // `cel_resolve::check_fact_queries`'s own exemption.
            if is_profile_fact_query(c) {
                if name == "validAt" {
                    let t = &c.args[1].expr;
                    if is_nt(t, ctx) {
                        return;
                    }
                    diags.push(diag(
                        E_TEMPORAL_ARG,
                        "`validAt`'s second argument must be a narrative-time \
                         expression (dsl 0.3.0 §6)"
                            .to_string(),
                        slot.span,
                    ));
                    // T may itself independently misuse a narrative-time value
                    // (e.g. `validAt(rel, now() + 1)`) — keep walking it.
                    walk(t, slot, ctx, diags);
                }
                return;
            }

            // Any other operator/function call: arithmetic, indexing,
            // logical/ternary operators, or an ordinary function — none of
            // these admit a narrative-time operand (dsl 0.3.0 §6: "no
            // arithmetic, no literal construction — ordering comparison and
            // validAt only").
            let nt_operand = c.target.as_ref().is_some_and(|t| is_nt(&t.expr, ctx))
                || c.args.iter().any(|a| is_nt(&a.expr, ctx));
            if nt_operand {
                diags.push(diag(
                    E_TEMPORAL_ARG,
                    "a narrative-time value admits no arithmetic, indexing, or \
                     other operator/function use — only an ordering comparison \
                     against another narrative-time value, or `validAt`'s second \
                     argument (dsl 0.3.0 §6)"
                        .to_string(),
                    slot.span,
                ));
                return;
            }
            if let Some(t) = &c.target {
                walk(&t.expr, slot, ctx, diags);
            }
            for a in &c.args {
                walk(&a.expr, slot, ctx, diags);
            }
        }
        Expr::List(list) => {
            if list.elements.iter().any(|el| is_nt(&el.expr, ctx)) {
                diags.push(diag(
                    E_TEMPORAL_ARG,
                    "a narrative-time value cannot be placed in a list literal — \
                     no literal construction (dsl 0.3.0 §6)"
                        .to_string(),
                    slot.span,
                ));
                return;
            }
            for el in &list.elements {
                walk(&el.expr, slot, ctx, diags);
            }
        }
        // A field selection: `now().foo` / `<anchor>.foo` — narrative-time is
        // opaque, so field access on it is illegal regardless of what `foo`
        // is (never reaches `check_state_path`'s Record/Map descent, since
        // narrativeTime has no fields).
        Expr::Select(sel) => {
            if is_nt(&sel.operand.expr, ctx) {
                diags.push(diag(
                    E_TEMPORAL_ARG,
                    "field access on a narrative-time value is not permitted \
                     (dsl 0.3.0 §6)"
                        .to_string(),
                    slot.span,
                ));
                return;
            }
            walk(&sel.operand.expr, slot, ctx, diags);
        }
        Expr::Comprehension(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Ident(_)
        | Expr::Literal(_)
        | Expr::Unspecified => {}
    }
}

/// Build a `Layer::Cel` error diagnostic (mirrors `cel_resolve.rs`'s own
/// helper — this module's diagnostics are CEL-surface, same layer).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
    }
}
