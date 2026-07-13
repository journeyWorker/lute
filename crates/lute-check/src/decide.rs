//! §5.1: the decided-constant fragment (`dsl 0.4.0 §5.1`) — the ONE
//! reusable primitive of this release, consumed by reachability (T4/T5),
//! param-scoped `<match>` (T7), the `§6.4` compile-time fold (T8), `when=`
//! dead guards (T10), and `lute trace`'s ground-op evaluator (T17, D3).
//!
//! `decide()` implements EXACTLY R1–R5 — the spec's Closure clause forbids
//! anything stronger (no SAT, no interval/path-sensitive narrowing, no
//! cross-shot state flow). It is TOTAL (never panics) and returns `None`
//! ("undecided") for everything outside the fragment, including a
//! non-finite numeric result (overflow, `/0`). A decided constant is
//! provably the expression's runtime value on EVERY reachable run
//! (soundness, §5.1) — `decide()` never guesses.
//!
//! D1: this is a closed static constant-folder, not an evaluator — it reads
//! no runtime state and `lute-cel` stays parse-only.

use std::collections::BTreeMap;

use cel_parser::ast::{operators as op, CallExpr, EntryExpr, Expr};
use cel_parser::reference::Val;

use crate::cel_expand::{expand_cel, DefTable};
use crate::match_check::{infer_domain, Domain, DomainInfo, DomainValue};
use crate::meta::StateSchema;

/// A §5.1-decided constant — provably the expression's value in EVERY
/// reachable runtime state (soundness note, dsl 0.4.0 §5.1).
#[derive(Clone, Debug, PartialEq)]
pub enum Decided {
    Bool(bool),
    Num(f64),
    Str(String),
}

/// What `$` denotes while deciding (`parse_slot`/`parse_slot_marked_refs`
/// substitutes `$` -> `Ident("_")`).
pub enum DollarBinding<'a> {
    /// Checker contexts: `$` is a finite-domain subject, value unknown (R2).
    Domain(&'a DomainInfo),
    /// Compile-time §6.4 fold: the subject itself already decided — `$`
    /// participates like any other literal (R1/R3), not just in R2's
    /// restricted `==`/`!=`/`in` domain check.
    Value(Decided),
}

pub struct DecideCtx<'a> {
    pub schema: &'a StateSchema,
    pub dollar: Option<DollarBinding<'a>>,
    /// Component params (name -> domain) for §6 slots; empty elsewhere.
    pub params: &'a BTreeMap<String, DomainInfo>,
}

/// Wrap a `f64` arithmetic/negation result: non-finite (overflow, `/0`)
/// stays undecided rather than deciding to `NaN`/`inf` (totality note,
/// §5.1 R3).
fn finite(x: f64) -> Option<Decided> {
    x.is_finite().then_some(Decided::Num(x))
}

/// R3 ground-operation semantics, shared with `lute-trace`'s evaluator (D3).
/// `op` is the CEL synthetic operator name (`_&&_`, `_==_`, `_+_`, `_?_:_`,
/// `@in`, `!_`, … — the `is_profile_operator` vocabulary, cel_resolve.rs).
/// `@in`'s args are flattened: `args[0]` is the needle, `args[1..]` the
/// (already-decided) list members. Total: an unrecognized `op`/arity/operand
/// shape, or a non-finite numeric result, decides to `None`.
pub fn apply_op(name: &str, args: &[Decided]) -> Option<Decided> {
    match args {
        [Decided::Bool(b)] if name == op::LOGICAL_NOT => Some(Decided::Bool(!*b)),
        [Decided::Num(x)] if name == op::NEGATE => finite(-x),
        [Decided::Num(a), Decided::Num(b)] if name == op::ADD => finite(a + b),
        [Decided::Num(a), Decided::Num(b)] if name == op::SUBSTRACT => finite(a - b),
        [Decided::Num(a), Decided::Num(b)] if name == op::MULTIPLY => finite(a * b),
        [Decided::Num(a), Decided::Num(b)] if name == op::DIVIDE => finite(a / b),
        [Decided::Num(a), Decided::Num(b)] if name == op::GREATER => Some(Decided::Bool(a > b)),
        [Decided::Num(a), Decided::Num(b)] if name == op::GREATER_EQUALS => {
            Some(Decided::Bool(a >= b))
        }
        [Decided::Num(a), Decided::Num(b)] if name == op::LESS => Some(Decided::Bool(a < b)),
        [Decided::Num(a), Decided::Num(b)] if name == op::LESS_EQUALS => {
            Some(Decided::Bool(a <= b))
        }
        // Heterogeneous equality (string/bool/enum, dsl §5.1 R3): different
        // `Decided` variants are simply unequal, matching CEL semantics.
        [a, b] if name == op::EQUALS => Some(Decided::Bool(a == b)),
        [a, b] if name == op::NOT_EQUALS => Some(Decided::Bool(a != b)),
        [Decided::Bool(c), t, e] if name == op::CONDITIONAL => {
            Some(if *c { t.clone() } else { e.clone() })
        }
        [needle, rest @ ..] if name == op::IN => Some(Decided::Bool(rest.contains(needle))),
        _ => None,
    }
}

/// Either the CEL `null` literal — the DSL's `unset` value (`0.1 §11.2`) —
/// or an ordinary decided scalar. Only meaningful on the non-subject side of
/// an R2 domain-membership comparison (`const_side`).
enum Constant {
    Unset,
    Value(Decided),
}

fn literal_to_decided(v: &Val) -> Option<Decided> {
    match v {
        Val::Boolean(b) => Some(Decided::Bool(*b)),
        Val::Int(i) => finite(*i as f64),
        Val::UInt(u) => finite(*u as f64),
        Val::Double(d) => finite(*d),
        Val::String(s) => Some(Decided::Str(s.clone())),
        // `null` is only ever meaningful as R2's unset marker (`const_side`);
        // a bare `null` node elsewhere has no `Decided` counterpart. `Bytes`
        // never appears in the closed Lute-CEL profile.
        Val::Null | Val::Bytes(_) => None,
    }
}

/// Decide `expr` as a comparison-side constant (R2): the CEL `null` literal
/// decides to [`Constant::Unset`]; anything else is decided ordinarily
/// (R1/R3/R4) and wrapped as [`Constant::Value`]. `None` means this side
/// isn't itself decided — R2 cannot rule on an unknown value.
fn const_side(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<Constant> {
    if matches!(expr, Expr::Literal(Val::Null)) {
        return Some(Constant::Unset);
    }
    decide(expr, ctx).map(Constant::Value)
}

/// R2 domain membership: `unset` is a member only when `dom.maybe_unset` (a
/// defaulted path or a bound param is never unset, §5.1 R2); a scalar is a
/// member when it matches one of `dom`'s `Finite` values — a `Num` never
/// does (`infer_domain` only produces a finite domain for `bool`/`enum`
/// subjects; a numeric subject is always `Domain::Infinite`).
fn domain_contains(dom: &DomainInfo, value: &Constant) -> bool {
    match value {
        Constant::Unset => dom.maybe_unset,
        Constant::Value(Decided::Str(s)) => matches!(&dom.domain, Domain::Finite(vals) if vals.iter().any(|v| matches!(v, DomainValue::Str(x) if x == s))),
        Constant::Value(Decided::Bool(b)) => matches!(&dom.domain, Domain::Finite(vals) if vals.iter().any(|v| matches!(v, DomainValue::Bool(x) if x == b))),
        Constant::Value(Decided::Num(_)) => false,
    }
}

/// Resolve `expr` as a §5.1 R2 finite-domain SUBJECT: the substituted `$`
/// bound to a domain (not an already-decided value — that's the compile-time
/// §6.4 fold, handled by `decide`'s own `Ident("_")` base case), a marker
/// `@ref` ident naming a bound component param, or a plain dotted state path
/// (`infer_domain`). Returns an OWNED [`DomainInfo`] — cheap, at most a
/// handful of enum members — so the borrowed (dollar/param) and
/// freshly-inferred (path) cases share one return type.
fn resolve_domain(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<DomainInfo> {
    if let Expr::Ident(name) = expr {
        if name == "_" {
            return match &ctx.dollar {
                Some(DollarBinding::Domain(d)) => Some((*d).clone()),
                _ => None,
            };
        }
        if let Some(param) = name.strip_prefix(lute_cel::REF_MARKER) {
            return ctx.params.get(param).cloned();
        }
        // Any other bare ident (a state-tier root alone, e.g.) falls through
        // to the dotted-path attempt below via `select_path`.
    }
    let path = crate::cel_paths::select_path(expr)?;
    Some(infer_domain(Some(&path), ctx.schema))
}

/// R2's `==`/`!=`: try resolving EITHER side as a finite-domain subject
/// (`S == lit` / `lit == S`). Returns `None` — falling through to the R3
/// collect-and-`apply_op` path in the caller — when neither side resolves,
/// the resolved domain is `Infinite` (R2 requires FINITE), or the other side
/// isn't itself decided.
fn decide_domain_equality(
    op_name: &str,
    lhs: &Expr,
    rhs: &Expr,
    ctx: &DecideCtx<'_>,
) -> Option<Decided> {
    let (dom, other) = resolve_domain(lhs, ctx)
        .map(|d| (d, rhs))
        .or_else(|| resolve_domain(rhs, ctx).map(|d| (d, lhs)))?;
    if !matches!(dom.domain, Domain::Finite(_)) {
        return None;
    }
    let value = const_side(other, ctx)?;
    if domain_contains(&dom, &value) {
        None // a literal INSIDE the domain: the actual value is still unknown
    } else {
        Some(Decided::Bool(op_name == op::NOT_EQUALS))
    }
}

/// R2's `in`: `S in [lit, …]` decides **false** iff NONE of the list's
/// (fully decided) elements are members of `S`'s finite domain. Any domain
/// member present, or any undecidable element, falls through to the R3
/// fallback in the caller — an unknown element might equal `S`'s eventual
/// value, so non-membership can't be proven.
fn decide_domain_in(needle: &Expr, container: &Expr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let dom = resolve_domain(needle, ctx)?;
    if !matches!(dom.domain, Domain::Finite(_)) {
        return None;
    }
    let Expr::List(list) = container else {
        return None;
    };
    for el in &list.elements {
        let value = const_side(&el.expr, ctx)?;
        if domain_contains(&dom, &value) {
            return None; // a domain member is present: the subject might pick it
        }
    }
    Some(Decided::Bool(false))
}

fn decide_call(c: &CallExpr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let name = c.func_name.as_str();

    // R5: an unexpanded `@ref(args)` marker (a bodiless def — a component
    // param — or any other expansion failure `decide_slot` left intact,
    // D3), `isSet()`, and any fact-query/`now()` call (`is_profile_fact_query`,
    // cel_resolve.rs) are always undecided — `decide()` never reads runtime
    // state or resolves an unrecognized macro.
    if name.starts_with(lute_cel::REF_MARKER)
        || name.eq_ignore_ascii_case("isSet")
        || crate::cel_resolve::is_profile_fact_query(c)
    {
        return None;
    }

    match (name, c.args.as_slice()) {
        // R4 — connectives, Kleene-style short circuit. NEVER collect-then-
        // `apply_op`: a decided short-circuit must win even when the OTHER
        // side is undecided (`1 > 2 && run.flag` decides false).
        (n, [a]) if n == op::LOGICAL_NOT => decide(&a.expr, ctx)
            .and_then(|d| apply_op(op::LOGICAL_NOT, std::slice::from_ref(&d))),
        (op::LOGICAL_AND, [a, b]) => {
            match (decide(&a.expr, ctx), decide(&b.expr, ctx)) {
                (Some(Decided::Bool(false)), _) | (_, Some(Decided::Bool(false))) => {
                    Some(Decided::Bool(false))
                }
                (Some(Decided::Bool(true)), Some(Decided::Bool(true))) => {
                    Some(Decided::Bool(true))
                }
                _ => None,
            }
        }
        (op::LOGICAL_OR, [a, b]) => match (decide(&a.expr, ctx), decide(&b.expr, ctx)) {
            (Some(Decided::Bool(true)), _) | (_, Some(Decided::Bool(true))) => {
                Some(Decided::Bool(true))
            }
            (Some(Decided::Bool(false)), Some(Decided::Bool(false))) => Some(Decided::Bool(false)),
            _ => None,
        },
        (op::CONDITIONAL, [cnd, t, e]) => match decide(&cnd.expr, ctx)? {
            Decided::Bool(true) => decide(&t.expr, ctx),
            Decided::Bool(false) => decide(&e.expr, ctx),
            _ => None, // an ill-typed condition; never guess
        },
        // R2 first (can decide even when the subject side is itself
        // undecided), then the R3 fallback: both sides fully decided.
        (n, [a, b]) if n == op::EQUALS || n == op::NOT_EQUALS => {
            decide_domain_equality(n, &a.expr, &b.expr, ctx).or_else(|| {
                let da = decide(&a.expr, ctx)?;
                let db = decide(&b.expr, ctx)?;
                apply_op(n, &[da, db])
            })
        }
        (op::IN, [a, b]) => decide_domain_in(&a.expr, &b.expr, ctx).or_else(|| {
            let needle = decide(&a.expr, ctx)?;
            let Expr::List(list) = &b.expr else {
                return None;
            };
            let mut vals = Vec::with_capacity(list.elements.len() + 1);
            vals.push(needle);
            for el in &list.elements {
                vals.push(decide(&el.expr, ctx)?);
            }
            apply_op(op::IN, &vals)
        }),
        // R3 — ordinary ground operators: both operands must decide.
        (n, [a]) if n == op::NEGATE => {
            decide(&a.expr, ctx).and_then(|d| apply_op(n, std::slice::from_ref(&d)))
        }
        (op::ADD, [a, b])
        | (op::SUBSTRACT, [a, b])
        | (op::MULTIPLY, [a, b])
        | (op::DIVIDE, [a, b])
        | (op::GREATER, [a, b])
        | (op::GREATER_EQUALS, [a, b])
        | (op::LESS, [a, b])
        | (op::LESS_EQUALS, [a, b]) => {
            let da = decide(&a.expr, ctx)?;
            let db = decide(&b.expr, ctx)?;
            apply_op(name, &[da, db])
        }
        _ => None, // R5: unrecognized shape (index, unknown fn, wrong arity, …)
    }
}

/// Decide a MARKED CEL AST (`lute_cel::parse_slot_marked_refs`) under R1–R5
/// (dsl 0.4.0 §5.1). Implements EXACTLY the rule map — the spec's Closure
/// clause forbids stronger reasoning.
pub fn decide(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    match expr {
        // R1: a literal decides to itself.
        Expr::Literal(v) => literal_to_decided(v),
        // The substituted `$` (dsl §8.1): decided directly ONLY in the
        // compile-time §6.4 fold, where the subject is already a decided
        // value (R1-like). Everywhere else `$` is a domain with an unknown
        // value — R2 resolves it there, via `resolve_domain`, never here.
        Expr::Ident(name) if name == "_" => match &ctx.dollar {
            Some(DollarBinding::Value(v)) => Some(v.clone()),
            _ => None,
        },
        // R5: any other bare identifier — a marker `@ref` (a component
        // param has a domain, not a value), a bare state-tier root, or
        // anything else — is a read with no value here.
        Expr::Ident(_) => None,
        // R5: a field selection is always a path read, including the
        // test-only `Select` the `has()` macro expands to.
        Expr::Select(_) => None,
        Expr::Call(c) => decide_call(c, ctx),
        // R5: everything else — list/map/struct literals (a bare list only
        // ever reaches here outside `in`'s special handling), comprehension
        // macros, and the unspecified placeholder — is undecided.
        Expr::List(_) | Expr::Map(_) | Expr::Struct(_) | Expr::Comprehension(_) | Expr::Unspecified => {
            None
        }
    }
}

/// The §5.1 entry point: textually expand `@def`s (`cel_expand`; D2), then
/// re-parse MARKED into a scratch [`lute_cel::CelArena`] and `decide`.
///
/// `$` is threaded through `expand_cel` as the LITERAL text `"$"` — never
/// the real subject text — so a live `$` token survives expansion: a
/// non-bare subject is parenthesized (`subject_text`), so `"$"` -> `"($)"`,
/// still a `$` for `parse_slot_marked_refs` to mark as `Ident("_")` below.
/// This lets an `@def` body that itself reads `$` resolve through R2/the
/// dollar-value case at the AST level, rather than baking in a fixed text.
///
/// A bodiless ref (a component param) or ANY other expansion failure
/// (cycle, unresolved def, arity mismatch) leaves the ORIGINAL raw text
/// intact (D3): the marked re-parse then resolves a param ref via its
/// marker ident (R2) and anything else genuinely unresolved lands in R5.
pub fn decide_slot(raw: &str, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
    let ided = arena.get(handle)?;
    decide(&ided.expr, ctx)
}

/// dsl 0.5.2 §2.1: one detected unset-sentinel misspelling — `S ==/!= 'unset'`
/// with `S` a maybe-unset finite-domain subject and the string `'unset'`
/// FOREIGN to `S`'s domain (not a declared enum member literally named
/// `unset`). `subject` is a best-effort display name for the §2.2 message (a
/// dotted state path, `$`, or a bound component param's bare name);
/// `not_equals` distinguishes `S != 'unset'` (decides true, R2) from
/// `S == 'unset'` (decides false — owns `E-ARM-DEAD`, §2.3).
pub(crate) struct UnsetSentinelHit {
    pub subject: String,
    pub not_equals: bool,
}

/// Best-effort display name for a resolved-domain subject expr (§2.2's
/// message names the path): a dotted state path (`cel_paths::select_path`),
/// the substituted `$` marker, or a bound component param's bare name
/// (stripped of `lute_cel::REF_MARKER`) — the SAME three subject shapes
/// [`resolve_domain`] itself resolves.
fn subject_display(expr: &Expr) -> Option<String> {
    if let Expr::Ident(name) = expr {
        if name == "_" {
            return Some("$".to_string());
        }
        if let Some(param) = name.strip_prefix(lute_cel::REF_MARKER) {
            return Some(param.to_string());
        }
    }
    crate::cel_paths::select_path(expr)
}

/// One operand order of §2.1's trigger: `subject ==/!= 'unset'`. `None`
/// unless ALL THREE conditions hold: `other` is literally the STRING
/// `'unset'` (never CEL's `null` — the real *unset* sentinel, `Constant::Unset`
/// elsewhere in this file); `subject` resolves — via the SAME
/// [`resolve_domain`] R2 uses — to a `resolved`, maybe-unset, FINITE domain;
/// and `'unset'` is foreign to it (checked with the SAME [`domain_contains`]
/// R2 uses, so the lint and R2 can never disagree about domain membership).
fn unset_sentinel_operand(
    subject: &Expr,
    other: &Expr,
    ctx: &DecideCtx<'_>,
    not_equals: bool,
) -> Option<UnsetSentinelHit> {
    let Expr::Literal(Val::String(s)) = other else {
        return None;
    };
    if s != "unset" {
        return None;
    }
    let dom = resolve_domain(subject, ctx)?;
    if !dom.resolved || !dom.maybe_unset || !matches!(dom.domain, Domain::Finite(_)) {
        return None;
    }
    if domain_contains(&dom, &Constant::Value(Decided::Str(s.clone()))) {
        return None; // in-domain: a legit enum member literally named `unset`
    }
    Some(UnsetSentinelHit {
        subject: subject_display(subject).unwrap_or_else(|| "this subject".to_string()),
        not_equals,
    })
}

/// dsl 0.5.2 §2.1's independent AST lint: the FIRST unset-sentinel mistake
/// found by scanning `expr` and EVERY comparison sub-expression it contains —
/// nested inside `&&`/`||`/`!`/anything else, not only a top-level comparison
/// ("the lint scans every comparison sub-expression, not only a top-level
/// guard"). Recurses the whole closed CEL-profile shape (mirrors
/// `cel_paths::walk`), so a sentinel mistake buried in a list/map/struct/
/// comprehension sub-expression is still found. `pub(crate)`: shared by
/// `reachability.rs`'s independent lint walk AND its `E-ARM-DEAD`/
/// `W-OTHERWISE-DEAD` suppression (§2.3 ownership) — the two can never
/// disagree about what counts as the sentinel mistake.
pub(crate) fn find_unset_sentinel_cmp(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<UnsetSentinelHit> {
    match expr {
        Expr::Call(c) => {
            if (c.func_name == op::EQUALS || c.func_name == op::NOT_EQUALS) && c.args.len() == 2 {
                let not_equals = c.func_name == op::NOT_EQUALS;
                let (a, b) = (&c.args[0].expr, &c.args[1].expr);
                if let Some(hit) = unset_sentinel_operand(a, b, ctx, not_equals)
                    .or_else(|| unset_sentinel_operand(b, a, ctx, not_equals))
                {
                    return Some(hit);
                }
            }
            if let Some(target) = &c.target {
                if let Some(hit) = find_unset_sentinel_cmp(&target.expr, ctx) {
                    return Some(hit);
                }
            }
            c.args.iter().find_map(|a| find_unset_sentinel_cmp(&a.expr, ctx))
        }
        Expr::List(list) => list
            .elements
            .iter()
            .find_map(|el| find_unset_sentinel_cmp(&el.expr, ctx)),
        Expr::Map(map) => map
            .entries
            .iter()
            .find_map(|e| find_unset_sentinel_cmp_entry(&e.expr, ctx)),
        Expr::Struct(st) => st
            .entries
            .iter()
            .find_map(|e| find_unset_sentinel_cmp_entry(&e.expr, ctx)),
        Expr::Comprehension(c) => [&c.iter_range, &c.accu_init, &c.loop_cond, &c.loop_step, &c.result]
            .into_iter()
            .find_map(|e| find_unset_sentinel_cmp(&e.expr, ctx)),
        Expr::Select(sel) => find_unset_sentinel_cmp(&sel.operand.expr, ctx),
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => None,
    }
}

fn find_unset_sentinel_cmp_entry(entry: &EntryExpr, ctx: &DecideCtx<'_>) -> Option<UnsetSentinelHit> {
    match entry {
        EntryExpr::MapEntry(m) => find_unset_sentinel_cmp(&m.key.expr, ctx)
            .or_else(|| find_unset_sentinel_cmp(&m.value.expr, ctx)),
        EntryExpr::StructField(f) => find_unset_sentinel_cmp(&f.value.expr, ctx),
    }
}

/// The §2.1 entry point (mirrors [`decide_slot`]'s expand-then-parse
/// pipeline exactly, so the two can never see different trees for the same
/// raw text): expand `@def`s, re-parse MARKED, then scan with
/// [`find_unset_sentinel_cmp`] instead of deciding. `None` on a parse
/// failure (mirrors `decide_slot`'s `?` chain) — an unparseable guard makes
/// no claim.
pub(crate) fn unset_sentinel_in_slot(
    raw: &str,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Option<UnsetSentinelHit> {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
    let ided = arena.get(handle)?;
    find_unset_sentinel_cmp(&ided.expr, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truth_table_not() {
        assert_eq!(
            apply_op(op::LOGICAL_NOT, &[Decided::Bool(true)]),
            Some(Decided::Bool(false))
        );
        assert_eq!(
            apply_op(op::LOGICAL_NOT, &[Decided::Bool(false)]),
            Some(Decided::Bool(true))
        );
    }

    #[test]
    fn negate_flips_sign() {
        assert_eq!(
            apply_op(op::NEGATE, &[Decided::Num(3.0)]),
            Some(Decided::Num(-3.0))
        );
    }

    #[test]
    fn numeric_compare() {
        assert_eq!(
            apply_op(op::GREATER, &[Decided::Num(3.0), Decided::Num(2.0)]),
            Some(Decided::Bool(true))
        );
        assert_eq!(
            apply_op(op::LESS_EQUALS, &[Decided::Num(2.0), Decided::Num(2.0)]),
            Some(Decided::Bool(true))
        );
        assert_eq!(
            apply_op(op::GREATER_EQUALS, &[Decided::Num(1.0), Decided::Num(2.0)]),
            Some(Decided::Bool(false))
        );
    }

    #[test]
    fn arithmetic() {
        assert_eq!(
            apply_op(op::ADD, &[Decided::Num(2.0), Decided::Num(3.0)]),
            Some(Decided::Num(5.0))
        );
        assert_eq!(
            apply_op(op::MULTIPLY, &[Decided::Num(2.0), Decided::Num(3.0)]),
            Some(Decided::Num(6.0))
        );
    }

    #[test]
    fn string_equality() {
        assert_eq!(
            apply_op(
                op::EQUALS,
                &[
                    Decided::Str("a".to_string()),
                    Decided::Str("a".to_string())
                ]
            ),
            Some(Decided::Bool(true))
        );
        // Heterogeneous equality (different `Decided` variants) is false,
        // never a type error — matches CEL semantics.
        assert_eq!(
            apply_op(op::EQUALS, &[Decided::Str("a".to_string()), Decided::Bool(true)]),
            Some(Decided::Bool(false))
        );
    }

    #[test]
    fn in_list() {
        assert_eq!(
            apply_op(
                op::IN,
                &[
                    Decided::Str("b".to_string()),
                    Decided::Str("a".to_string()),
                    Decided::Str("b".to_string())
                ]
            ),
            Some(Decided::Bool(true))
        );
        assert_eq!(
            apply_op(
                op::IN,
                &[Decided::Str("z".to_string()), Decided::Str("a".to_string())]
            ),
            Some(Decided::Bool(false))
        );
    }

    #[test]
    fn conditional_selects_branch() {
        assert_eq!(
            apply_op(
                op::CONDITIONAL,
                &[Decided::Bool(true), Decided::Num(1.0), Decided::Num(2.0)]
            ),
            Some(Decided::Num(1.0))
        );
        assert_eq!(
            apply_op(
                op::CONDITIONAL,
                &[Decided::Bool(false), Decided::Num(1.0), Decided::Num(2.0)]
            ),
            Some(Decided::Num(2.0))
        );
    }

    #[test]
    fn non_finite_is_undecided() {
        // Division by zero.
        assert_eq!(
            apply_op(op::DIVIDE, &[Decided::Num(1.0), Decided::Num(0.0)]),
            None
        );
        // Overflow.
        assert_eq!(
            apply_op(op::MULTIPLY, &[Decided::Num(f64::MAX), Decided::Num(f64::MAX)]),
            None
        );
    }

    #[test]
    fn unrecognized_op_is_undecided() {
        assert_eq!(apply_op(op::INDEX, &[Decided::Num(1.0), Decided::Num(0.0)]), None);
        assert_eq!(apply_op("_%_", &[Decided::Num(5.0), Decided::Num(2.0)]), None);
    }
}
