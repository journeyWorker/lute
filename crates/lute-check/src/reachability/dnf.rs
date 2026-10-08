use cel_parser::ast::{operators as op, Expr};
use cel_parser::reference::Val;
use lute_manifest::types::Type;

use crate::cel_expand::DefTable;
use crate::match_check::DomainValue;
use crate::solution::{disjoint, solution_set, SolutionSet};

use super::{comparison_set, comparison_set_polar};

/// dsl 0.27.0 (T3-3): `path in [lit, …]` as solution sets over the path's
/// declared type — the union of the members' `==` sets; `negated`
/// (`!(path in […])`), one `!=` set per member. `None` for any other shape,
/// a non-literal or ill-typed member, or an empty list (which holds nowhere
/// and so constrains nothing we can use).
fn membership_sets_polar(
    expr: &Expr,
    schema: &crate::meta::StateSchema,
    negated: bool,
) -> Option<Vec<(String, SolutionSet)>> {
    let Expr::Call(c) = expr else {
        return None;
    };
    if c.target.is_some() || c.func_name != op::IN || c.args.len() != 2 {
        return None;
    }
    let path = crate::cel_paths::select_path(&c.args[0].expr)?;
    let Expr::List(list) = &c.args[1].expr else {
        return None;
    };
    let declared = crate::set_op::resolve_type(&path, schema)?;
    let opname = if negated { op::NOT_EQUALS } else { op::EQUALS };
    let sets = list
        .elements
        .iter()
        .map(|el| match &el.expr {
            Expr::Literal(v) => solution_set(declared, opname, v),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if negated {
        return Some(sets.into_iter().map(|s| (path.clone(), s)).collect());
    }
    let (first, rest) = sets.split_first()?;
    let union = rest
        .iter()
        .try_fold(first.clone(), |acc, s| join(&acc, s))?;
    Some(vec![(path, union)])
}

/// The top-level `&&` conjuncts of a condition that are in-domain comparisons
/// (dsl 0.22.0 §13, `W-BEAT-PRIORITY-TIE`): each `path op literal`, a bare
/// `bool` path / its `!` as `== true` / `== false` — the reserved
/// `entry.<id>.read` / `entry.<id>.everRead` flags and a ground `holds(…)` /
/// `visited('…')` query included, as pseudo-paths — and (dsl 0.24.0, T3-3)
/// what a positive `holds(A)` of a pure-schedule derived atom implies about
/// state ([`schedule_conjuncts`]). dsl 0.27.0 (T3-3): a `path in [lit, …]`
/// (or its `!`) is read as its value set, and an `a || b` constrains a path
/// BOTH sides constrain, to the union of their sets. Every other conjunct
/// constrains nothing here — which only makes exclusivity harder to prove.
///
/// Pairwise disjoint TRUE sets mean "never both true" — weaker than the
/// conjunction deciding `false` (dsl 0.23.0 §9), which an erring read of an
/// unset path (`run.flag && !run.flag`) does not.
#[derive(Clone, Debug, Default)]
pub(crate) struct Conjuncts(pub(crate) Vec<(String, SolutionSet)>);

/// [`Conjuncts`] of `raw` after `@def` expansion, typed against `schema`;
/// `vocab` (the document's relational vocabulary) lets a `holds(A)` conjunct
/// contribute its schedule's state constraints.
pub(crate) fn when_conjuncts(
    raw: &str,
    defs: &DefTable<'_>,
    schema: &crate::meta::StateSchema,
    vocab: Option<&crate::rel_schema::RelVocab>,
) -> Conjuncts {
    let mut out = Vec::new();
    if let Some(expr) = parse_expanded(raw, defs) {
        let ctx = ConjunctCtx {
            defs,
            schema,
            vocab,
        };
        collect_conjuncts(&expr, &ctx, &mut out);
    }
    Conjuncts(out)
}

fn parse_expanded(raw: &str, defs: &DefTable<'_>) -> Option<Expr> {
    let mut stack = Vec::new();
    let expanded = crate::cel_expand::expand_cel(raw, defs, None, &mut stack)
        .unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
    arena.get(handle).map(|ided| ided.expr.clone())
}

struct ConjunctCtx<'a> {
    defs: &'a DefTable<'a>,
    schema: &'a crate::meta::StateSchema,
    vocab: Option<&'a crate::rel_schema::RelVocab>,
}

fn collect_conjuncts(expr: &Expr, ctx: &ConjunctCtx<'_>, out: &mut Vec<(String, SolutionSet)>) {
    if let Expr::Call(c) = expr {
        if c.target.is_none() && c.func_name == op::LOGICAL_AND && c.args.len() == 2 {
            collect_conjuncts(&c.args[0].expr, ctx, out);
            collect_conjuncts(&c.args[1].expr, ctx, out);
            return;
        }
        // Either side holding constrains a path both sides constrain to
        // one of their sets; a path only one side constrains stays free.
        if c.target.is_none() && c.func_name == op::LOGICAL_OR && c.args.len() == 2 {
            let sides = c.args.iter().map(|a| {
                let mut side = Vec::new();
                collect_conjuncts(&a.expr, ctx, &mut side);
                side
            });
            out.extend(join_common(&sides.collect::<Vec<_>>()));
            return;
        }
    }
    let bool_path = |e: &Expr| {
        let path = crate::cel_paths::select_path(e)?;
        // The reserved entry flags are engine-written bools (dsl 0.19.0 §5,
        // 0.22.0 §7), undeclared in `state:`.
        (lute_manifest::semantics::cel_paths::reserved_entry_id(&path).is_some()
            || matches!(crate::set_op::resolve_type(&path, ctx.schema)?, Type::Bool))
        .then_some(path)
    };
    let flag = |path: String, value: bool| {
        (
            path,
            SolutionSet::Values(std::iter::once(DomainValue::Bool(value)).collect()),
        )
    };
    if let Some(hit) = comparison_set(expr, ctx.schema) {
        out.push(hit);
    } else if let Some(hits) = membership_sets_polar(expr, ctx.schema, false) {
        out.extend(hits);
    } else if let Some(path) = bool_path(expr).or_else(|| holds_key(expr)) {
        out.push(flag(path, true));
        if let Some(vocab) = ctx.vocab {
            out.extend(schedule_conjuncts(expr, vocab, ctx));
        }
    } else if let Expr::Call(c) = expr {
        if c.target.is_none() && c.func_name == op::LOGICAL_NOT && c.args.len() == 1 {
            let inner = &c.args[0].expr;
            if let Some(path) = bool_path(inner).or_else(|| holds_key(inner)) {
                out.push(flag(path, false));
            } else if let Some(hits) = membership_sets_polar(inner, ctx.schema, true) {
                out.extend(hits);
            }
        }
    }
}

/// A ground list-form `holds('rel', [args…])` or `visited('id')` query as a
/// pseudo-path (`holds('rel', [a,b])`, `visited('id')`), so `holds(P)` and
/// `!holds(P)` are exclusive like `x` and `!x`.
fn holds_key(expr: &Expr) -> Option<String> {
    let Expr::Call(c) = expr else { return None };
    if c.target.is_some() {
        return None;
    }
    if c.func_name == "holds" {
        return crate::fact_env::QueryPattern::from_call(c).map(|q| q.to_string());
    }
    (c.func_name == "visited" && c.args.len() == 1)
        .then(|| term_key(&c.args[0].expr).map(|t| format!("visited({t})")))
        .flatten()
}

fn term_key(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Literal(Val::String(s)) => Some(format!("'{s}'")),
        Expr::Literal(Val::Int(i)) => Some(i.to_string()),
        Expr::Call(c) if c.target.is_none() => {
            let args: Option<Vec<String>> = c.args.iter().map(|a| term_key(&a.expr)).collect();
            Some(format!("{}({})", c.func_name, args?.join(",")))
        }
        _ => None,
    }
}

/// dsl 0.24.0 (T3-3): what a positive list-form `holds('rel', [c…])` query
/// implies about state when the atom is a pure schedule — `rel` is `derive:
/// true` and not engine-`reserved`, no seed fact is the atom, and EVERY rule
/// whose head unifies with it is a ground head over `cel()` guards only
/// (`at(sol, radio) :- cel("run.slot == 'morning'")`). The atom then holds
/// exactly when one of those guards does, so a path every guard constrains is
/// constrained to the union of their sets. Anything else (a variable head, a
/// body atom, a comparison literal, no rule at all) implies nothing.
fn schedule_conjuncts(
    expr: &Expr,
    vocab: &crate::rel_schema::RelVocab,
    ctx: &ConjunctCtx<'_>,
) -> Vec<(String, SolutionSet)> {
    use lute_syntax::datalog::{BodyLiteral, FactTerm, RuleTerm};
    let Expr::Call(c) = expr else {
        return Vec::new();
    };
    if c.target.is_some() || c.func_name != "holds" {
        return Vec::new();
    }
    let Some(query) = crate::fact_env::QueryPattern::from_call(c) else {
        return Vec::new();
    };
    let Some(consts) = query.args.iter().cloned().collect::<Option<Vec<String>>>() else {
        return Vec::new();
    };
    let rel = query.relation.as_str();
    if !vocab
        .relations
        .get(rel)
        .is_some_and(|d| d.derive && !d.reserved)
    {
        return Vec::new();
    }
    let seeded = vocab.facts.iter().any(|f| {
        f.fact.relation == rel
            && f.fact.args.len() == consts.len()
            && f.fact.args.iter().zip(&consts).all(|(a, k)| match &a.term {
                FactTerm::Ident(i) => i == k,
                FactTerm::Bool(b) => b.to_string() == *k,
                FactTerm::Wildcard | FactTerm::Param(_) | FactTerm::Target => true,
            })
    });
    if seeded {
        return Vec::new();
    }
    // One unifying rule is one disjunct: the conjunction of its guards.
    let inner = ConjunctCtx {
        vocab: None,
        ..*ctx
    };
    let mut per_rule: Vec<Vec<(String, SolutionSet)>> = Vec::new();
    for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
        let head = &r.rule.head.terms;
        if head.len() != consts.len() {
            continue;
        }
        let mut unifies = true;
        for (t, k) in head.iter().zip(&consts) {
            match t {
                // A variable head (bound by a body atom) is not a schedule.
                RuleTerm::Var(_) => return Vec::new(),
                RuleTerm::Const(v) => unifies &= v == k,
                RuleTerm::Bool(b) => unifies &= b.to_string() == *k,
            }
        }
        if !unifies {
            continue;
        }
        if r.rule.body.is_empty() {
            return Vec::new();
        }
        let mut sets = Vec::new();
        for lit in &r.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                return Vec::new();
            };
            if let Some(e) = parse_expanded(cel, ctx.defs) {
                collect_conjuncts(&e, &inner, &mut sets);
            }
        }
        per_rule.push(sets);
    }
    join_common(&per_rule)
}

/// What a disjunction of `alternatives` (each a conjunct list) implies: a
/// path every alternative constrains, to the union of their sets (the first
/// set each gives it); a path some alternative leaves free is dropped, as is
/// one whose sets have no representable union. No alternative, no conjunct.
fn join_common(alternatives: &[Vec<(String, SolutionSet)>]) -> Vec<(String, SolutionSet)> {
    let Some((first, rest)) = alternatives.split_first() else {
        return Vec::new();
    };
    let mut acc: Vec<(String, SolutionSet)> = Vec::new();
    for (path, set) in first {
        if acc.iter().any(|(p, _)| p == path) {
            continue;
        }
        let mut joined = Some(set.clone());
        for other in rest {
            joined = match (joined, other.iter().find(|(p, _)| p == path)) {
                (Some(j), Some((_, s))) => join(&j, s),
                _ => None,
            };
        }
        if let Some(j) = joined {
            acc.push((path.clone(), j));
        }
    }
    acc
}

/// The union of two solution sets of one path, when representable: two
/// finite value sets, or two number sets. `None` otherwise (the path is then
/// not constrained).
fn join(a: &SolutionSet, b: &SolutionSet) -> Option<SolutionSet> {
    use crate::solution::number_spans;
    match (a, b) {
        (SolutionSet::Values(x), SolutionSet::Values(y)) => {
            Some(SolutionSet::Values(x.union(y).cloned().collect()))
        }
        (
            SolutionSet::Interval { .. } | SolutionSet::Union(_),
            SolutionSet::Interval { .. } | SolutionSet::Union(_),
        ) => {
            let mut spans = number_spans(Some(a));
            spans.extend(number_spans(Some(b)));
            Some(SolutionSet::Union(spans))
        }
        _ => None,
    }
}

/// dsl 0.26.0 §8 (T3-2): a condition in disjunctive normal form over the
/// literals [`collect_conjuncts`] reads — every `!` pushed inward first (De
/// Morgan: `!(a && b)` is `!a || !b`; `!!a` is `a`; `!(x < 1)` is `x >= 1`),
/// so `!(A && B)` against `A && B` is a pair of literals on one path.
/// Each [`Disjunct`] holds the in-domain conjuncts of one alternative. A
/// condition too large to expand (more than [`DNF_LIMIT`] alternatives) is
/// one unconstrained, inexact disjunct — which only makes exclusivity (and
/// implication) harder to prove.
#[derive(Clone, Debug)]
pub(crate) struct Dnf(Vec<Disjunct>);

/// One alternative of a [`Dnf`].
#[derive(Clone, Debug, Default)]
pub(crate) struct Disjunct {
    conjuncts: Vec<(String, SolutionSet)>,
    /// Every literal of the alternative is among `conjuncts` (so they say
    /// exactly when it holds, not merely something it implies).
    exact: bool,
}

/// The alternatives a [`Dnf`] expands to at most.
const DNF_LIMIT: usize = 64;

impl Default for Dnf {
    /// No condition: one alternative constraining nothing.
    fn default() -> Self {
        Dnf(vec![Disjunct::default()])
    }
}

impl Disjunct {
    /// The paths (and pseudo-paths: `holds(…)`, `visited('…')`) it constrains.
    pub(crate) fn paths(&self) -> impl Iterator<Item = &str> {
        self.conjuncts.iter().map(|(p, _)| p.as_str())
    }

    /// `path` is constrained to `true` (a `holds(F)`, a flag, a bool path).
    pub(crate) fn requires_true(&self, path: &str) -> bool {
        let yes = DomainValue::Bool(true);
        self.conjuncts.iter().any(|(p, s)| {
            p == path && matches!(s, SolutionSet::Values(v) if v.len() == 1 && v.contains(&yes))
        })
    }

    /// No conjunct of `self` meets one of `other` on a path with a disjoint
    /// solution set.
    fn meets(&self, other: &Disjunct) -> bool {
        !self.conjuncts.iter().any(|(pa, sa)| {
            other
                .conjuncts
                .iter()
                .any(|(pb, sb)| pa == pb && disjoint(sa, sb))
        })
    }
}

/// [`Dnf`] of `raw` after `@def` expansion, typed against `schema`; `vocab`
/// lets a positive `holds(A)` contribute its schedule's state constraints
/// ([`schedule_conjuncts`]).
pub(crate) fn when_dnf(
    raw: &str,
    defs: &DefTable<'_>,
    schema: &crate::meta::StateSchema,
    vocab: Option<&crate::rel_schema::RelVocab>,
) -> Dnf {
    let Some(expr) = parse_expanded(raw, defs) else {
        return Dnf::default();
    };
    let ctx = ConjunctCtx {
        defs,
        schema,
        vocab,
    };
    Dnf(dnf_of(&expr, false, &ctx).unwrap_or_else(|| vec![Disjunct::default()]))
}

/// The alternatives of `expr` (of `!expr` when `negated`); `None` past
/// [`DNF_LIMIT`]. An alternative whose own conjuncts are disjoint on a path
/// can never hold and is dropped.
fn dnf_of(expr: &Expr, negated: bool, ctx: &ConjunctCtx<'_>) -> Option<Vec<Disjunct>> {
    if let Expr::Call(c) = expr {
        if c.target.is_none() {
            if c.func_name == op::LOGICAL_NOT && c.args.len() == 1 {
                return dnf_of(&c.args[0].expr, !negated, ctx);
            }
            let and = c.func_name == op::LOGICAL_AND;
            if (and || c.func_name == op::LOGICAL_OR) && c.args.len() == 2 {
                let l = dnf_of(&c.args[0].expr, negated, ctx)?;
                let r = dnf_of(&c.args[1].expr, negated, ctx)?;
                // `!(a && b)` is `!a || !b`; `!(a || b)` is `!a && !b`.
                if and != negated {
                    if l.len() * r.len() > DNF_LIMIT {
                        return None;
                    }
                    let mut out = Vec::new();
                    for a in &l {
                        for b in &r {
                            if a.meets(b) {
                                let mut conjuncts = a.conjuncts.clone();
                                conjuncts.extend(b.conjuncts.iter().cloned());
                                out.push(Disjunct {
                                    conjuncts,
                                    exact: a.exact && b.exact,
                                });
                            }
                        }
                    }
                    return Some(out);
                }
                if l.len() + r.len() > DNF_LIMIT {
                    return None;
                }
                return Some(l.into_iter().chain(r).collect());
            }
        }
    }
    if let Expr::Literal(Val::Boolean(b)) = expr {
        return Some(if *b != negated {
            vec![Disjunct {
                conjuncts: Vec::new(),
                exact: true,
            }]
        } else {
            Vec::new()
        });
    }
    let mut conjuncts = Vec::new();
    let exact = literal_conjuncts(expr, negated, ctx, &mut conjuncts);
    Some(vec![Disjunct { conjuncts, exact }])
}

/// One literal (`expr`, or `!expr` when `negated`) as [`collect_conjuncts`]
/// reads it. `false` when it is not an in-domain comparison, flag or ground
/// query (it then constrains nothing).
fn literal_conjuncts(
    expr: &Expr,
    negated: bool,
    ctx: &ConjunctCtx<'_>,
    out: &mut Vec<(String, SolutionSet)>,
) -> bool {
    if let Some(hit) = comparison_set_polar(expr, ctx.schema, negated) {
        out.push(hit);
        return true;
    }
    if let Some(hits) = membership_sets_polar(expr, ctx.schema, negated) {
        out.extend(hits);
        return true;
    }
    let bool_path = crate::cel_paths::select_path(expr).filter(|path| {
        lute_manifest::semantics::cel_paths::reserved_entry_id(path).is_some()
            || matches!(
                crate::set_op::resolve_type(path, ctx.schema),
                Some(Type::Bool)
            )
    });
    let Some(path) = bool_path.or_else(|| holds_key(expr)) else {
        return false;
    };
    out.push((
        path,
        SolutionSet::Values(std::iter::once(DomainValue::Bool(!negated)).collect()),
    ));
    if !negated {
        if let Some(vocab) = ctx.vocab {
            out.extend(schedule_conjuncts(expr, vocab, ctx));
        }
    }
    true
}

/// Two conditions that cannot both hold: every alternative of one meets
/// every alternative of the other on a path they constrain to disjoint
/// solution sets. Sound, never complete.
pub(crate) fn provably_exclusive(a: &Dnf, b: &Dnf) -> bool {
    non_exclusive_witness(a, b).is_none()
}

/// A pair of alternatives, one of each, that no path separates — why `a`
/// and `b` are not [`provably_exclusive`].
pub(crate) fn non_exclusive_witness<'d>(
    a: &'d Dnf,
    b: &'d Dnf,
) -> Option<(&'d Disjunct, &'d Disjunct)> {
    a.0.iter()
        .flat_map(|da| b.0.iter().map(move |db| (da, db)))
        .find(|(da, db)| da.meets(db))
}

/// dsl 0.26.0 §8 (T3-3): `b` holds only where `a` does — every alternative
/// of `b` has an [exact](Disjunct::exact) alternative of `a` each of whose
/// conjuncts `b`'s alternative constrains to a subset. Sound, never
/// complete: an alternative of `a` the checker cannot read in full implies
/// nothing.
pub(crate) fn implies(b: &Dnf, a: &Dnf) -> bool {
    b.0.iter().all(|db| {
        a.0.iter().any(|da| {
            da.exact
                && da.conjuncts.iter().all(|(pa, sa)| {
                    db.conjuncts
                        .iter()
                        .any(|(pb, sb)| pa == pb && crate::solution::subset(sb, sa))
                })
        })
    })
}

