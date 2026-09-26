//! §5.1: the decided-constant fragment (`dsl 0.4.0 §5.1`) — the ONE
//! reusable primitive of this release, consumed by reachability (T4/T5),
//! param-scoped `<match>` (T7), the `§6.4` compile-time fold (T8), `when=`
//! dead guards (T10), and `lute trace`'s ground-op evaluator (T17, D3).
//!
//! `decide()` implements R1–R5 plus dsl 0.23.0 §9's per-path rule for the
//! connectives (see [`decide_chain`]); nothing stronger (no SAT, no
//! cross-path narrowing, no cross-shot state flow). It is TOTAL (never
//! panics) and returns `None` ("undecided") for everything outside the
//! fragment, including a non-finite numeric result (overflow, `/0`). A
//! decided constant is provably the expression's runtime value on EVERY
//! reachable run (soundness, §5.1) — `decide()` never guesses.
//!
//! D1: this is a closed static constant-folder, not an evaluator — it reads
//! no runtime state and `lute-cel` stays parse-only.
//!
//! dsl 0.20.0 §5: with a fact envelope in scope (`DecideCtx::facts`, set
//! only by `check-project`'s guard pass), R5's fact-query firewall opens for
//! `holds`/`count`: a relational call decides from the project's may/must
//! sets (`fact_env.rs`), and `count(P) ⋈ n` decides over its interval. It
//! is still a closed fold — the envelope is precomputed, never evaluated.

use std::collections::BTreeMap;

use cel_parser::ast::{operators as op, CallExpr, EntryExpr, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_manifest::types::Type;

use crate::cel_expand::{expand_cel, DefTable};
use crate::fact_env::{CountInterval, FactScope, GroundFact, HoldsVerdict, QueryPattern};
use crate::match_check::{infer_domain, Domain, DomainInfo, DomainValue};
use crate::meta::StateSchema;
use crate::rel_schema::RelVocab;
use crate::solution::{
    covers, domain_value, finite_set, holds_member, meet_spans, number_set, number_spans, Kind,
    PathDomain, SolutionSet, Truth, REALS,
};

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
    /// dsl 0.20.0 §5: the project's fact envelope bound to the slot being
    /// decided. `None` (single-file `check`, the LSP, the compiler) keeps
    /// every relational call undecided (R5); `Some` resolves `holds(P)` /
    /// `count(P)` from the may/must sets. `validAt` and `now()` stay
    /// undecided either way.
    pub facts: Option<FactScope<'a>>,
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
        // dsl 0.24.0 §1: integer `%`, CEL's truncated remainder (the sign of
        // the dividend, as Rust's `%` on integers). A fractional operand or a
        // zero divisor decides nothing — the runtime rule every evaluator
        // shares (docs/runtime/cel-and-facts.md).
        [Decided::Num(a), Decided::Num(b)] if name == op::MODULO => {
            if a.fract() != 0.0 || b.fract() != 0.0 || *b == 0.0 {
                return None;
            }
            // `+ 0.0` folds `-0` (`-4 % 2`) into `0`.
            finite(a % b + 0.0)
        }
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
/// member when it matches one of `dom`'s `Finite` values, and a number when
/// it is one of an `IntRange`'s whole numbers (`clock.weekday`, dsl 0.24.0
/// §1). A plain `Domain::Number` never reaches here (R2 leaves it undecided
/// exactly like `Domain::Infinite`); a string or bool against an `IntRange`
/// counts as a member — its type error is `E-CEL-TYPE`'s, never a dead arm.
fn domain_contains(dom: &DomainInfo, value: &Constant) -> bool {
    match (value, &dom.domain) {
        (Constant::Unset, _) => dom.maybe_unset,
        (Constant::Value(Decided::Num(n)), Domain::IntRange { lo, hi }) => {
            n.fract() == 0.0 && (*lo as f64) <= *n && *n <= (*hi as f64)
        }
        (Constant::Value(_), Domain::IntRange { .. }) => true,
        (Constant::Value(Decided::Str(s)), Domain::Finite(vals)) => vals
            .iter()
            .any(|v| matches!(v, DomainValue::Str(x) if x == s)),
        (Constant::Value(Decided::Bool(b)), Domain::Finite(vals)) => vals
            .iter()
            .any(|v| matches!(v, DomainValue::Bool(x) if x == b)),
        (Constant::Value(_), _) => false,
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
    if !matches!(dom.domain, Domain::Finite(_) | Domain::IntRange { .. }) {
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
    if !matches!(dom.domain, Domain::Finite(_) | Domain::IntRange { .. }) {
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

/// The connective an operand chain belongs to (dsl 0.23.0 §9).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    And,
    Or,
}

/// dsl 0.23.0 §9: an `&&` / `||` chain decided per path. Its operands are
/// flattened ([`chain_literals`]) and read as solution sets over one path
/// each ([`literal_truth`]); for some path with at least two of them —
///
/// - an `||` chain holds in every state when their TRUE sets cover every
///   value the path can take (`run.n < 5 || run.n >= 5`);
/// - an `&&` chain fails in every state when their FALSE sets do — the TRUE
///   sets of their negations (`run.n > 5 && run.n < 3`, `x && !x`,
///   `run.slot == 'a' && run.slot == 'b'`).
///
/// "Every value" includes `unset` for a maybe-unset path and every kind of
/// value for an undeclared one, and an operand that ERRS on a value (an
/// ordering on `unset`) is neither true nor false there — so a verdict is
/// the chain's actual value, never merely "not true" (`solution::covers`).
/// A single operand per path is left to R1–R5.
fn decide_chain(args: &[IdedExpr], chain: Chain, ctx: &DecideCtx<'_>) -> bool {
    let mut literals = Vec::new();
    for a in args {
        chain_literals(&a.expr, true, chain, &mut literals);
    }
    let negate = chain == Chain::And;
    let mut by_path: BTreeMap<String, (PathDomain, Vec<Truth>)> = BTreeMap::new();
    for (expr, positive) in literals {
        if let Some((key, dom, truth)) = literal_truth(expr, positive != negate, ctx) {
            by_path
                .entry(key)
                .or_insert_with(|| (dom, Vec::new()))
                .1
                .push(truth);
        }
    }
    by_path
        .values()
        .any(|(dom, truths)| truths.len() > 1 && covers(dom, truths))
        || ctx.facts.is_some_and(|scope| {
            !exclusive_pairs(&literals_needed(args, chain), scope.vocab).is_empty()
        })
}

/// dsl 0.25.0 §1: the ground `holds(F)` literals a chain needs TRUE — as
/// written in an `&&` chain, negated in an `||` chain (`!holds(A) ||
/// !holds(B)` needs `A` and `B` for its false outcome).
fn literals_needed(args: &[IdedExpr], chain: Chain) -> Vec<GroundFact> {
    let mut literals = Vec::new();
    for a in args {
        chain_literals(&a.expr, true, chain, &mut literals);
    }
    literals
        .into_iter()
        .filter(|(_, positive)| *positive == (chain == Chain::And))
        .filter_map(|(e, _)| held_ground(e))
        .collect()
}

/// The ground fact of a `holds(F)` call, `None` for anything else (a
/// pattern with `_`, including the `$` of a `<match>` arm).
pub(crate) fn held_ground(e: &Expr) -> Option<GroundFact> {
    let Expr::Call(c) = e else {
        return None;
    };
    if c.func_name != "holds" || !crate::cel_resolve::is_profile_fact_query(c) {
        return None;
    }
    let Expr::Call(p) = &c.args.first()?.expr else {
        return None;
    };
    QueryPattern::from_call(p)?.ground()
}

/// dsl 0.25.0 §1: every pair of `facts` that can never hold together — the
/// same arguments over relations one of which `excludes:` the other.
pub(crate) fn exclusive_pairs(
    facts: &[GroundFact],
    vocab: &RelVocab,
) -> Vec<(GroundFact, GroundFact)> {
    let mut out = Vec::new();
    for (i, a) in facts.iter().enumerate() {
        for b in &facts[i + 1..] {
            if a.args == b.args
                && vocab
                    .relations
                    .get(&a.relation)
                    .is_some_and(|d| d.args.len() == a.args.len())
                && vocab.excludes(&a.relation, &b.relation)
            {
                out.push((a.clone(), b.clone()));
            }
        }
    }
    out
}

/// dsl 0.25.0 §1: the ground `holds(F)` conjuncts of a guard's top-level
/// `&&` chain, as `(required, negated)` — the facts it needs and those it
/// needs absent (`!holds(F)`).
pub(crate) fn and_chain_holds(expr: &Expr) -> (Vec<GroundFact>, Vec<GroundFact>) {
    let mut literals = Vec::new();
    chain_literals(expr, true, Chain::And, &mut literals);
    let (mut pos, mut neg) = (Vec::new(), Vec::new());
    for (e, positive) in literals {
        if let Some(f) = held_ground(e) {
            if positive {
                pos.push(f)
            } else {
                neg.push(f)
            }
        }
    }
    (pos, neg)
}

/// Flatten one operand of a `chain` into its literals with their polarity
/// (`true` = as written), pushing `!` inward by De Morgan — which CEL's
/// commutative, error-absorbing `&&`/`||` preserve: inside an `&&` chain,
/// `!(a || b)` contributes `!a` and `!b`.
fn chain_literals<'e>(
    expr: &'e Expr,
    positive: bool,
    chain: Chain,
    out: &mut Vec<(&'e Expr, bool)>,
) {
    if let Expr::Call(c) = expr {
        if c.target.is_none() {
            match (c.func_name.as_str(), c.args.as_slice()) {
                (op::LOGICAL_NOT, [a]) => return chain_literals(&a.expr, !positive, chain, out),
                (n @ (op::LOGICAL_AND | op::LOGICAL_OR), [a, b])
                    if connective(n, positive) == chain =>
                {
                    chain_literals(&a.expr, positive, chain, out);
                    chain_literals(&b.expr, positive, chain, out);
                    return;
                }
                _ => {}
            }
        }
    }
    out.push((expr, positive));
}

/// The connective `&&`/`||` (`name`) acts as under `positive` polarity.
fn connective(name: &str, positive: bool) -> Chain {
    if (name == op::LOGICAL_AND) == positive {
        Chain::And
    } else {
        Chain::Or
    }
}

/// One literal read with `positive` polarity as a solution set over one
/// path: its key, the path's domain, and the values that make it TRUE.
/// `None` for any other shape — it then constrains nothing.
fn literal_truth(
    expr: &Expr,
    positive: bool,
    ctx: &DecideCtx<'_>,
) -> Option<(String, PathDomain, Truth)> {
    if let Expr::Call(c) = expr {
        if c.target.is_none() {
            match (c.func_name.as_str(), c.args.as_slice()) {
                (op::LOGICAL_NOT, [a]) => return literal_truth(&a.expr, !positive, ctx),
                (n @ (op::LOGICAL_AND | op::LOGICAL_OR), [_, _]) => {
                    return nested_truth(expr, connective(n, positive), positive, ctx)
                }
                (n, [a, b]) if flip_comparison(n).is_some() => {
                    return comparison_truth(n, &a.expr, &b.expr, positive, ctx)
                }
                (op::IN, [a, b]) => return in_truth(&a.expr, &b.expr, positive, ctx),
                (n, [a]) if n.eq_ignore_ascii_case("isSet") => {
                    let (key, dom) = subject(&a.expr, ctx)?;
                    let truth = Truth {
                        set: (!positive).then(|| SolutionSet::Values(Default::default())),
                        unset: !positive,
                    };
                    return Some((key, dom, truth));
                }
                _ => {}
            }
        }
    }
    // A bare read of a boolean (or undeclared) path.
    let (key, dom) = subject(expr, ctx)?;
    let boolean = match &dom.kind {
        Kind::Finite(members) => members.iter().all(|m| matches!(m, DomainValue::Bool(_))),
        Kind::Number => false,
        Kind::Open => true,
    };
    boolean.then(|| {
        let truth = Truth {
            set: Some(SolutionSet::Values(
                std::iter::once(DomainValue::Bool(positive)).collect(),
            )),
            unset: false,
        };
        (key, dom, truth)
    })
}

/// `S ⋈ c` (either operand order) with `S` a [`subject`] and `c` decided
/// (or `null`, the DSL's `unset`). Under negative polarity the operator is
/// complemented (`!(x > 5)` is `x <= 5` — still false on a non-number).
fn comparison_truth(
    op_name: &str,
    lhs: &Expr,
    rhs: &Expr,
    positive: bool,
    ctx: &DecideCtx<'_>,
) -> Option<(String, PathDomain, Truth)> {
    let (key, dom, other, op_name) = match subject(lhs, ctx) {
        Some((key, dom)) => (key, dom, rhs, op_name),
        None => {
            let (key, dom) = subject(rhs, ctx)?;
            (key, dom, lhs, flip_comparison(op_name)?)
        }
    };
    let op_name = if positive {
        op_name
    } else {
        match op_name {
            op::EQUALS => op::NOT_EQUALS,
            op::NOT_EQUALS => op::EQUALS,
            op::LESS => op::GREATER_EQUALS,
            op::LESS_EQUALS => op::GREATER,
            op::GREATER => op::LESS_EQUALS,
            op::GREATER_EQUALS => op::LESS,
            _ => return None,
        }
    };
    let truth = match const_side(other, ctx)? {
        Constant::Unset => match op_name {
            op::EQUALS => Truth {
                set: Some(SolutionSet::Values(Default::default())),
                unset: true,
            },
            op::NOT_EQUALS => Truth {
                set: None,
                unset: false,
            },
            _ => return None,
        },
        Constant::Value(v) => {
            let set = match (&dom.kind, &v) {
                (Kind::Finite(all), _) => finite_set(all, &domain_value(&v)?, op_name)?,
                (Kind::Number | Kind::Open, Decided::Num(n)) => number_set(op_name, *n)?,
                (Kind::Open, _) => match op_name {
                    op::EQUALS => SolutionSet::Values(std::iter::once(domain_value(&v)?).collect()),
                    op::NOT_EQUALS => SolutionSet::Except(v),
                    _ => return None,
                },
                (Kind::Number, _) => return None,
            };
            Truth {
                set: Some(set),
                unset: op_name == op::NOT_EQUALS,
            }
        }
    };
    Some((key, dom, truth))
}

/// `S in [c, …]` over a finite-domain subject, every element decided (or
/// `null`). `unset` is `in` the list exactly when `null` is listed.
fn in_truth(
    needle: &Expr,
    container: &Expr,
    positive: bool,
    ctx: &DecideCtx<'_>,
) -> Option<(String, PathDomain, Truth)> {
    let (key, dom) = subject(needle, ctx)?;
    let Kind::Finite(all) = &dom.kind else {
        return None;
    };
    let Expr::List(list) = container else {
        return None;
    };
    let mut listed = std::collections::BTreeSet::new();
    let mut null_listed = false;
    for el in &list.elements {
        match const_side(&el.expr, ctx)? {
            Constant::Unset => null_listed = true,
            Constant::Value(v) => {
                listed.extend(domain_value(&v));
            }
        }
    }
    let set = all
        .iter()
        .filter(|m| listed.contains(*m) == positive)
        .cloned()
        .collect();
    let truth = Truth {
        set: Some(SolutionSet::Values(set)),
        unset: null_listed == positive,
    };
    Some((key, dom, truth))
}

/// A nested chain of the OTHER connective (`(x == 'a' || x == 'b')` inside
/// an `&&` chain) whose literals all read ONE finite-domain or number path:
/// its TRUE set is the union (`||`) or intersection (`&&`) of theirs — over
/// a number path a union of intervals (`run.n > 3 || run.n < 3`).
fn nested_truth(
    expr: &Expr,
    chain: Chain,
    positive: bool,
    ctx: &DecideCtx<'_>,
) -> Option<(String, PathDomain, Truth)> {
    let mut literals = Vec::new();
    chain_literals(expr, positive, chain, &mut literals);
    let mut key_dom: Option<(String, PathDomain)> = None;
    let mut truths = Vec::with_capacity(literals.len());
    for (e, p) in literals {
        let (key, dom, truth) = literal_truth(e, p, ctx)?;
        match &key_dom {
            Some((k, _)) if *k != key => return None,
            Some(_) => {}
            None => key_dom = Some((key, dom)),
        }
        truths.push(truth);
    }
    let (key, dom) = key_dom?;
    let any = chain == Chain::Or;
    let set = match &dom.kind {
        Kind::Finite(all) => {
            let holds =
                |t: &Truth, m: &DomainValue| t.set.as_ref().is_none_or(|s| holds_member(s, m));
            SolutionSet::Values(
                all.iter()
                    .filter(|m| {
                        if any {
                            truths.iter().any(|t| holds(t, m))
                        } else {
                            truths.iter().all(|t| holds(t, m))
                        }
                    })
                    .cloned()
                    .collect(),
            )
        }
        Kind::Number => SolutionSet::Union(if any {
            truths
                .iter()
                .flat_map(|t| number_spans(t.set.as_ref()))
                .collect()
        } else {
            truths.iter().fold(vec![REALS], |acc, t| {
                meet_spans(&acc, &number_spans(t.set.as_ref()))
            })
        }),
        // A value of any kind may turn up: an ordering errs on a non-number.
        Kind::Open => return None,
    };
    let unset = if any {
        truths.iter().any(|t| t.unset)
    } else {
        truths.iter().all(|t| t.unset)
    };
    let truth = Truth {
        set: Some(set),
        unset,
    };
    Some((key, dom, truth))
}

/// A §9 subject: the `$` bound to a domain, a bound component param, a
/// relational call (`holds(P)` / `visited(id)` — a never-unset `bool`;
/// `count(P)` — a never-unset number), or a dotted state path. The key is
/// the subject's text; one guard evaluation reads each at one instant, so
/// equal text is an equal value.
fn subject(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<(String, PathDomain)> {
    match expr {
        Expr::Ident(name) if name == "_" => match &ctx.dollar {
            Some(DollarBinding::Domain(d)) => Some(("$".to_string(), PathDomain::of(d))),
            _ => None,
        },
        Expr::Ident(name) => {
            let param = name.strip_prefix(lute_cel::REF_MARKER)?;
            Some((name.clone(), PathDomain::of(ctx.params.get(param)?)))
        }
        Expr::Call(c) if c.target.is_none() => {
            let dom = match c.func_name.as_str() {
                "holds" if crate::cel_resolve::is_profile_fact_query(c) => PathDomain::boolean(),
                crate::cel_resolve::VISITED_FN if c.args.len() == 1 => PathDomain::boolean(),
                "count" | "countDistinct" if crate::cel_resolve::is_profile_fact_query(c) => {
                    PathDomain {
                        kind: Kind::Number,
                        maybe_unset: false,
                    }
                }
                _ => return None,
            };
            Some((ground_text(expr, ctx)?, dom))
        }
        _ => {
            let path = state_path(expr)?;
            let dom = path_domain(&path, ctx.schema);
            Some((path, dom))
        }
    }
}

/// A dotted `a.b.c` state path — never a bare identifier (a comprehension
/// variable, the §2.3 placeholder), a `has()` test, or a path rooted at `$`
/// or a param marker.
fn state_path(expr: &Expr) -> Option<String> {
    let Expr::Select(sel) = expr else {
        return None;
    };
    if sel.test {
        return None;
    }
    let base = match &sel.operand.expr {
        Expr::Ident(root) if root != "_" && !root.starts_with(lute_cel::REF_MARKER) => root.clone(),
        operand => state_path(operand)?,
    };
    Some(format!("{base}.{}", sel.field))
}

/// A state path's domain: [`infer_domain`]'s for a declared path, else the
/// declared type of a field under a declared record/map (which may be
/// absent — maybe unset), else open.
fn path_domain(path: &str, schema: &StateSchema) -> PathDomain {
    let info = infer_domain(Some(path), schema);
    if info.resolved {
        return PathDomain::of(&info);
    }
    let kind = match crate::set_op::resolve_type(path, schema) {
        Some(Type::Bool) => Kind::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)]),
        Some(Type::Enum(members)) => {
            Kind::Finite(members.iter().cloned().map(DomainValue::Str).collect())
        }
        Some(Type::Number) => Kind::Number,
        _ => Kind::Open,
    };
    PathDomain {
        kind,
        maybe_unset: true,
    }
}

/// The canonical text of a ground relational call. Inside a `<match>` arm
/// `$` and the `_` wildcard parse alike, so an `_` there declines.
fn ground_text(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<String> {
    match expr {
        Expr::Ident(name) if name == "_" && ctx.dollar.is_some() => None,
        Expr::Ident(name) => Some(name.clone()),
        Expr::Literal(Val::String(s)) => Some(format!("'{s}'")),
        Expr::Literal(Val::Int(i)) => Some(i.to_string()),
        Expr::Literal(Val::UInt(u)) => Some(u.to_string()),
        Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
        Expr::Select(_) => state_path(expr),
        Expr::Call(c) if c.target.is_none() => {
            let args: Option<Vec<String>> =
                c.args.iter().map(|a| ground_text(&a.expr, ctx)).collect();
            Some(format!("{}({})", c.func_name, args?.join(",")))
        }
        _ => None,
    }
}

fn decide_call(c: &CallExpr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let name = c.func_name.as_str();

    // R5: an unexpanded `@ref(args)` marker (a bodiless def — a component
    // param — or any other expansion failure `decide_slot` left intact,
    // D3), `isSet()`, and `visited()` (dsl 0.21.0 §7a.1 — presentation
    // history is never known per file) are always undecided — `decide()`
    // never reads runtime state or resolves an unrecognized macro.
    if name.starts_with(lute_cel::REF_MARKER)
        || name.eq_ignore_ascii_case("isSet")
        || name == crate::cel_resolve::VISITED_FN
    {
        return None;
    }
    // A fact-query/`now()` call (`is_profile_fact_query`, cel_resolve.rs) is
    // undecided unless a fact envelope is in scope (dsl 0.20.0 §5).
    if crate::cel_resolve::is_profile_fact_query(c) {
        return decide_fact_query(c, ctx);
    }

    match (name, c.args.as_slice()) {
        // R4 — connectives, Kleene-style short circuit. NEVER collect-then-
        // `apply_op`: a decided short-circuit must win even when the OTHER
        // side is undecided (`1 > 2 && run.flag` decides false).
        (n, [a]) if n == op::LOGICAL_NOT => {
            decide(&a.expr, ctx).and_then(|d| apply_op(op::LOGICAL_NOT, std::slice::from_ref(&d)))
        }
        (op::LOGICAL_AND, [a, b]) => match (decide(&a.expr, ctx), decide(&b.expr, ctx)) {
            (Some(Decided::Bool(false)), _) | (_, Some(Decided::Bool(false))) => {
                Some(Decided::Bool(false))
            }
            (Some(Decided::Bool(true)), Some(Decided::Bool(true))) => Some(Decided::Bool(true)),
            _ => decide_chain(&c.args, Chain::And, ctx).then_some(Decided::Bool(false)),
        },
        (op::LOGICAL_OR, [a, b]) => match (decide(&a.expr, ctx), decide(&b.expr, ctx)) {
            (Some(Decided::Bool(true)), _) | (_, Some(Decided::Bool(true))) => {
                Some(Decided::Bool(true))
            }
            (Some(Decided::Bool(false)), Some(Decided::Bool(false))) => Some(Decided::Bool(false)),
            _ => decide_chain(&c.args, Chain::Or, ctx).then_some(Decided::Bool(true)),
        },
        (op::CONDITIONAL, [cnd, t, e]) => match decide(&cnd.expr, ctx)? {
            Decided::Bool(true) => decide(&t.expr, ctx),
            Decided::Bool(false) => decide(&e.expr, ctx),
            _ => None, // an ill-typed condition; never guess
        },
        // R2 first (can decide even when the subject side is itself
        // undecided), then the R3 fallback: both sides fully decided.
        (n, [a, b]) if n == op::EQUALS || n == op::NOT_EQUALS => {
            decide_count_cmp(n, &a.expr, &b.expr, ctx)
                .or_else(|| decide_domain_equality(n, &a.expr, &b.expr, ctx))
                .or_else(|| {
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
        | (op::MODULO, [a, b]) => {
            let da = decide(&a.expr, ctx)?;
            let db = decide(&b.expr, ctx)?;
            apply_op(name, &[da, db])
        }
        (op::GREATER, [a, b])
        | (op::GREATER_EQUALS, [a, b])
        | (op::LESS, [a, b])
        | (op::LESS_EQUALS, [a, b]) => {
            decide_count_cmp(name, &a.expr, &b.expr, ctx).or_else(|| {
                let da = decide(&a.expr, ctx)?;
                let db = decide(&b.expr, ctx)?;
                apply_op(name, &[da, db])
            })
        }
        _ => None, // R5: unrecognized shape (index, unknown fn, wrong arity, …)
    }
}

/// dsl 0.20.0 §5: a well-shaped fact query under a fact envelope.
/// `holds(P)` decides `false` when no may-fact matches `P` and `true` when a
/// must-fact at the slot does; `count(P)` decides only when its interval is a
/// single point. `validAt` and `now()` never decide (narrative time is not
/// part of the envelope).
fn decide_fact_query(c: &CallExpr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let scope = ctx.facts?;
    let Expr::Call(pattern) = &c.args.first()?.expr else {
        return None;
    };
    match c.func_name.as_str() {
        "holds" => match scope.holds(&QueryPattern::from_call(pattern)?) {
            HoldsVerdict::Impossible | HoldsVerdict::Excluded(_) => Some(Decided::Bool(false)),
            HoldsVerdict::Guaranteed(_) => Some(Decided::Bool(true)),
            HoldsVerdict::Possible => None,
        },
        "count" | "countDistinct" => {
            let (q, column) = crate::fact_env::count_query(c)?;
            let iv = scope.count_in(&q, column)?;
            (iv.hi == Some(iv.lo)).then(|| Decided::Num(iv.lo as f64))
        }
        _ => None,
    }
}

/// The `count(P)` / `countDistinct(P, V)` interval of `expr` when it is
/// directly such a call and a fact envelope is in scope.
fn count_interval(expr: &Expr, ctx: &DecideCtx<'_>) -> Option<CountInterval> {
    let scope = ctx.facts?;
    let Expr::Call(c) = expr else {
        return None;
    };
    let (q, column) = crate::fact_env::count_query(c)?;
    scope.count_in(&q, column)
}

/// dsl 0.20.0 §5: `count(P) ⋈ n` (either operand order) decided over the
/// interval `[|Must ∩ P|, |May ∩ P|]` rather than a point — `count(P) >= 5`
/// is false when at most two facts can ever match, whatever the run.
fn decide_count_cmp(op_name: &str, lhs: &Expr, rhs: &Expr, ctx: &DecideCtx<'_>) -> Option<Decided> {
    let (iv, other, op_name) = match count_interval(lhs, ctx) {
        Some(iv) => (iv, rhs, op_name),
        None => (count_interval(rhs, ctx)?, lhs, flip_comparison(op_name)?),
    };
    let Decided::Num(n) = decide(other, ctx)? else {
        return None;
    };
    iv.compare(op_name, n).map(Decided::Bool)
}

/// A comparison operator with its operands exchanged (`n < x` is `x > n`).
fn flip_comparison(op_name: &str) -> Option<&'static str> {
    Some(match op_name {
        op::GREATER => op::LESS,
        op::GREATER_EQUALS => op::LESS_EQUALS,
        op::LESS => op::GREATER,
        op::LESS_EQUALS => op::GREATER_EQUALS,
        op::EQUALS => op::EQUALS,
        op::NOT_EQUALS => op::NOT_EQUALS,
        _ => return None,
    })
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
        Expr::List(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Comprehension(_)
        | Expr::Unspecified => None,
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

/// One literal comparison a guard slot gets wrong. `subject` is a
/// best-effort display name for the message (a dotted state path, `$`, or a
/// bound component param's bare name); `id` is the comparison `Call` node's
/// own arena id — used ONLY internally by [`analyze_literal_comparisons`]'s
/// causality substitution, never read outside this file.
pub(crate) struct LiteralCmpHit {
    pub subject: String,
    pub kind: LiteralCmpKind,
    id: u64,
}

/// What is wrong with one [`LiteralCmpHit`].
pub(crate) enum LiteralCmpKind {
    /// dsl 0.5.2 §2.1: `S ==/!= 'unset'` with `S` a maybe-unset
    /// finite-domain subject and the string `'unset'` FOREIGN to `S`'s
    /// domain (not a declared enum member literally named `unset`) — the
    /// DSL's unset sentinel misspelt. `not_equals` distinguishes
    /// `S != 'unset'` (decides true, R2) from `S == 'unset'` (decides false —
    /// a candidate `E-ARM-DEAD` root, §2.3).
    UnsetSentinel { not_equals: bool },
    /// dsl 0.26.0: a string literal compared (`==`, `!=`, either operand
    /// order, or an `in [...]` element) against a subject whose finite
    /// domain is a set of strings — an enum path, `occasion.target`, a
    /// quest's `state`/`failedBy`, a branch's `scene.choices.*`, an enum
    /// param — that has no such member: a typo, `E-WHEN-LITERAL-DOMAIN`.
    /// `members` is the subject's domain, in declaration order.
    ForeignMember {
        literal: String,
        members: Vec<String>,
    },
}

/// dsl 0.5.2 §2.1/§2.3: the full analysis of one CEL guard slot.
pub(crate) struct LiteralCmpAnalysis {
    /// EVERY distinct faulty comparison found (§2.1: "scans every
    /// comparison sub-expression, not only a top-level guard") — one
    /// diagnostic per hit.
    pub hits: Vec<LiteralCmpHit>,
    /// §2.3: "that comparison is its root" — `true` iff `hits` is
    /// non-empty AND substituting an UNDECIDED placeholder for every
    /// detected comparison (so the guard reasons about it exactly as
    /// little as an ordinary state-path read) no longer lets the guard
    /// decide `false`. `false` when the guard is ALSO independently dead
    /// for another reason (a literal `false`, `@never`, …) — that
    /// independent deadness must still surface as `E-ARM-DEAD`, so the
    /// diagnostics can pile on when genuinely warranted. Meaningful only
    /// when the ORIGINAL guard itself decides `Some(Decided::Bool(false))`;
    /// callers only ever consult this flag inside that branch.
    pub load_bearing_for_false: bool,
}

impl LiteralCmpAnalysis {
    /// The literal comparisons alone are why the guard decides false: the
    /// diagnostics they raise own the dead-guard root (§2.3, D4).
    pub(crate) fn owns_dead_guard(&self) -> bool {
        !self.hits.is_empty() && self.load_bearing_for_false
    }
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
/// Returns just the subject's display name — `not_equals`/`id` are attached
/// by the caller, which already knows the enclosing `Call` node.
fn unset_sentinel_operand(subject: &Expr, other: &Expr, ctx: &DecideCtx<'_>) -> Option<String> {
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
    Some(subject_display(subject).unwrap_or_else(|| "this subject".to_string()))
}

/// One operand order of the dsl 0.26.0 foreign-member trigger:
/// `subject ==/!= 'lit'` (or one `in` element), where `subject` resolves —
/// via the SAME [`resolve_domain`] R2 uses — to a `resolved` FINITE domain
/// of strings and `'lit'` is not one of them (the SAME [`domain_contains`]
/// R2 uses, so the diagnostic and R2 never disagree about membership).
/// Returns the subject's display name, the literal and the members. A
/// bool domain is left alone: a string against it is a type question.
fn foreign_member_operand(
    subject: &Expr,
    other: &Expr,
    ctx: &DecideCtx<'_>,
) -> Option<(String, String, Vec<String>)> {
    let Expr::Literal(Val::String(s)) = other else {
        return None;
    };
    let dom = resolve_domain(subject, ctx)?;
    let Domain::Finite(vals) = &dom.domain else {
        return None;
    };
    let members: Vec<String> = vals
        .iter()
        .filter_map(|v| match v {
            DomainValue::Str(m) => Some(m.clone()),
            DomainValue::Bool(_) => None,
        })
        .collect();
    if !dom.resolved
        || members.is_empty()
        || domain_contains(&dom, &Constant::Value(Decided::Str(s.clone())))
    {
        return None;
    }
    let name = subject_display(subject).unwrap_or_else(|| "this subject".to_string());
    Some((name, s.clone(), members))
}

/// The guard-slot literal lint: collect EVERY faulty literal comparison in
/// `ided`'s tree — nested inside `&&`/`||`/`!`/anything else, not only a
/// top-level comparison. Recurses the whole closed CEL-profile shape
/// (mirrors `cel_paths::walk`) AND into a matched comparison's own operands
/// (a pathological `S == (T == 'unset' ? a : b)` still finds the inner
/// mistake), so a mistake buried anywhere is found. A comparison that
/// misspells the unset sentinel (dsl 0.5.2 §2.1) is reported as that, never
/// also as a foreign member.
fn collect_literal_cmp(ided: &IdedExpr, ctx: &DecideCtx<'_>, out: &mut Vec<LiteralCmpHit>) {
    match &ided.expr {
        Expr::Call(c) => {
            if (c.func_name == op::EQUALS || c.func_name == op::NOT_EQUALS) && c.args.len() == 2 {
                let not_equals = c.func_name == op::NOT_EQUALS;
                let (a, b) = (&c.args[0].expr, &c.args[1].expr);
                if let Some(subject) =
                    unset_sentinel_operand(a, b, ctx).or_else(|| unset_sentinel_operand(b, a, ctx))
                {
                    out.push(LiteralCmpHit {
                        subject,
                        kind: LiteralCmpKind::UnsetSentinel { not_equals },
                        id: ided.id,
                    });
                } else if let Some((subject, literal, members)) =
                    foreign_member_operand(a, b, ctx).or_else(|| foreign_member_operand(b, a, ctx))
                {
                    out.push(LiteralCmpHit {
                        subject,
                        kind: LiteralCmpKind::ForeignMember { literal, members },
                        id: ided.id,
                    });
                }
            }
            if c.func_name == op::IN && c.args.len() == 2 {
                if let Expr::List(list) = &c.args[1].expr {
                    for el in &list.elements {
                        if let Some((subject, literal, members)) =
                            foreign_member_operand(&c.args[0].expr, &el.expr, ctx)
                        {
                            out.push(LiteralCmpHit {
                                subject,
                                kind: LiteralCmpKind::ForeignMember { literal, members },
                                id: ided.id,
                            });
                        }
                    }
                }
            }
            if let Some(target) = &c.target {
                collect_literal_cmp(target, ctx, out);
            }
            for a in &c.args {
                collect_literal_cmp(a, ctx, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                collect_literal_cmp(el, ctx, out);
            }
        }
        Expr::Map(map) => {
            for e in &map.entries {
                collect_literal_cmp_entry(&e.expr, ctx, out);
            }
        }
        Expr::Struct(st) => {
            for e in &st.entries {
                collect_literal_cmp_entry(&e.expr, ctx, out);
            }
        }
        Expr::Comprehension(c) => {
            for e in [
                &c.iter_range,
                &c.accu_init,
                &c.loop_cond,
                &c.loop_step,
                &c.result,
            ] {
                collect_literal_cmp(e, ctx, out);
            }
        }
        Expr::Select(sel) => collect_literal_cmp(&sel.operand, ctx, out),
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn collect_literal_cmp_entry(entry: &EntryExpr, ctx: &DecideCtx<'_>, out: &mut Vec<LiteralCmpHit>) {
    match entry {
        EntryExpr::MapEntry(m) => {
            collect_literal_cmp(&m.key, ctx, out);
            collect_literal_cmp(&m.value, ctx, out);
        }
        EntryExpr::StructField(f) => collect_literal_cmp(&f.value, ctx, out),
    }
}

/// §2.3's causality marker: an ordinary, otherwise-unused CEL identifier.
/// `decide()`'s `Expr::Ident(_) => None` arm treats it — like any other
/// bare ident — as UNDECIDED, exactly what the substitution needs: "reason
/// about this node as little as an ordinary state-path read".
const UNDECIDED_PLACEHOLDER: &str = "__lute_literal_cmp_undecided__";

/// Clone `ided`'s tree, replacing every node whose id is in `ids` with
/// [`UNDECIDED_PLACEHOLDER`] (a leaf — its own children are dropped, never
/// visited). `Expr`/`IdedExpr` and friends are cheap, structural `Clone`s
/// (no interior parse state), so cloning the whole slot-sized tree is fine.
fn undecide_ids(ided: &IdedExpr, ids: &[u64]) -> IdedExpr {
    let mut out = ided.clone();
    undecide_ids_mut(&mut out, ids);
    out
}

fn undecide_ids_mut(ided: &mut IdedExpr, ids: &[u64]) {
    if ids.contains(&ided.id) {
        ided.expr = Expr::Ident(UNDECIDED_PLACEHOLDER.to_string());
        return;
    }
    match &mut ided.expr {
        Expr::Call(c) => {
            if let Some(target) = &mut c.target {
                undecide_ids_mut(target, ids);
            }
            for a in &mut c.args {
                undecide_ids_mut(a, ids);
            }
        }
        Expr::List(list) => {
            for el in &mut list.elements {
                undecide_ids_mut(el, ids);
            }
        }
        Expr::Map(map) => {
            for e in &mut map.entries {
                undecide_ids_mut_entry(&mut e.expr, ids);
            }
        }
        Expr::Struct(st) => {
            for e in &mut st.entries {
                undecide_ids_mut_entry(&mut e.expr, ids);
            }
        }
        Expr::Comprehension(c) => {
            undecide_ids_mut(&mut c.iter_range, ids);
            undecide_ids_mut(&mut c.accu_init, ids);
            undecide_ids_mut(&mut c.loop_cond, ids);
            undecide_ids_mut(&mut c.loop_step, ids);
            undecide_ids_mut(&mut c.result, ids);
        }
        Expr::Select(sel) => undecide_ids_mut(&mut sel.operand, ids),
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn undecide_ids_mut_entry(entry: &mut EntryExpr, ids: &[u64]) {
    match entry {
        EntryExpr::MapEntry(m) => {
            undecide_ids_mut(&mut m.key, ids);
            undecide_ids_mut(&mut m.value, ids);
        }
        EntryExpr::StructField(f) => undecide_ids_mut(&mut f.value, ids),
    }
}

/// The §2.1/§2.3 entry point (mirrors [`decide_slot`]'s expand-then-parse
/// pipeline exactly, so the lint and R2 can never see different trees for
/// the same raw text): expand `@def`s, re-parse MARKED, collect every
/// faulty literal comparison, then (§2.3) re-decide a COPY of the tree with
/// every hit substituted to an undecided placeholder — `decide()`'s own
/// contract is untouched, this only ever runs on a cloned, throwaway tree.
/// An empty analysis (no hits, not load-bearing) on a parse failure —
/// mirrors `decide_slot`'s `?` chain: an unparseable guard makes no claim.
pub(crate) fn analyze_literal_comparisons(
    raw: &str,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> LiteralCmpAnalysis {
    let empty = || LiteralCmpAnalysis {
        hits: Vec::new(),
        load_bearing_for_false: false,
    };
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let Some(handle) = lute_cel::parse_slot_marked_refs(&mut arena, &expanded) else {
        return empty();
    };
    let Some(ided) = arena.get(handle) else {
        return empty();
    };
    let mut hits = Vec::new();
    collect_literal_cmp(ided, ctx, &mut hits);
    if hits.is_empty() {
        return empty();
    }
    let ids: Vec<u64> = hits.iter().map(|h| h.id).collect();
    let substituted = undecide_ids(ided, &ids);
    let load_bearing_for_false =
        !matches!(decide(&substituted.expr, ctx), Some(Decided::Bool(false)));
    LiteralCmpAnalysis {
        hits,
        load_bearing_for_false,
    }
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
                &[Decided::Str("a".to_string()), Decided::Str("a".to_string())]
            ),
            Some(Decided::Bool(true))
        );
        // Heterogeneous equality (different `Decided` variants) is false,
        // never a type error — matches CEL semantics.
        assert_eq!(
            apply_op(
                op::EQUALS,
                &[Decided::Str("a".to_string()), Decided::Bool(true)]
            ),
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
            apply_op(
                op::MULTIPLY,
                &[Decided::Num(f64::MAX), Decided::Num(f64::MAX)]
            ),
            None
        );
    }

    #[test]
    fn unrecognized_op_is_undecided() {
        assert_eq!(
            apply_op(op::INDEX, &[Decided::Num(1.0), Decided::Num(0.0)]),
            None
        );
    }

    /// dsl 0.24.0 §1: `%` is the integer truncated remainder; a fractional
    /// operand or a zero divisor is undecided, never a float remainder.
    #[test]
    fn modulo_is_integer_truncated_remainder() {
        let m = |a: f64, b: f64| apply_op(op::MODULO, &[Decided::Num(a), Decided::Num(b)]);
        assert_eq!(m(14.0, 7.0), Some(Decided::Num(0.0)));
        assert_eq!(m(15.0, 7.0), Some(Decided::Num(1.0)));
        assert_eq!(m(-7.0, 3.0), Some(Decided::Num(-1.0)));
        assert_eq!(m(7.0, -3.0), Some(Decided::Num(1.0)));
        assert_eq!(m(-4.0, 2.0), Some(Decided::Num(0.0)));
        assert_eq!(m(5.0, 0.0), None);
        assert_eq!(m(5.5, 2.0), None);
        assert_eq!(m(5.0, 2.5), None);
    }
}
