//! Per-path solution sets (dsl 0.10.0 §5.2, dsl 0.23.0 §9): the values of
//! ONE path — a state path, the `<match>` subject `$`, a component param, or
//! a relational call such as `holds(P)` — that make one comparison TRUE.
//!
//! Two consumers ask two questions of them:
//!
//! - [`disjoint`]: can two comparisons over one path never both hold
//!   (`E-OBJECTIVE-CONTRADICTION`, dsl 0.10.0 §5.2)?
//! - [`covers`]: is every value the path can take — `unset` included when
//!   the path may be unset — inside the union of several sets? `decide()`
//!   asks it of a disjunction's operands (true in every state: the
//!   disjunction decides **true**) and of a conjunction's NEGATED operands
//!   (false in every state: the conjunction decides **false**) — dsl 0.23.0
//!   §9. CEL's `!` maps true↔false and leaves an error an error, so "some
//!   operand is FALSE for every value" is exactly "the negations' TRUE sets
//!   cover the domain"; an operand that ERRS on a value (`x > 5` on an unset
//!   `x`) is never counted as false, which keeps a decided `false` the
//!   expression's actual value rather than merely "never true".
//!
//! Numbers range over the REALS, not the integers: a literal is an `f64` and
//! the language has no integer scalar, so `x > 1 && x < 2` is satisfiable.

use std::collections::BTreeSet;

use cel_parser::ast::operators as op;
use cel_parser::reference::Val;
use lute_manifest::types::Type;

use crate::decide::Decided;
use crate::match_check::{Domain, DomainInfo, DomainValue};

/// The values of one path that make one comparison true.
#[derive(Clone, Debug)]
pub(crate) enum SolutionSet {
    /// The numbers of a real interval. `lo`/`hi` may be infinite (then
    /// exclusive); `*_inc` is endpoint inclusion.
    Interval {
        lo: f64,
        lo_inc: bool,
        hi: f64,
        hi_inc: bool,
    },
    /// Exactly these strings / booleans (`== 'a'`, a bare `bool`, `in [..]`
    /// over a finite domain).
    Values(BTreeSet<DomainValue>),
    /// Every value but one (`!= c`) — values of any other kind included.
    Except(Decided),
    /// The numbers of a union of real intervals — a nested `||` / `&&` of
    /// comparisons on one number path (`run.n > 3 || run.n < 3`), built by
    /// [`number_spans`], [`meet_spans`], and concatenation.
    Union(Vec<Span>),
}

/// A real interval `(lo, lo_inc, hi, hi_inc)` with [`SolutionSet::Interval`]'s
/// field meanings.
pub(crate) type Span = (f64, bool, f64, bool);

/// The whole real line.
pub(crate) const REALS: Span = (f64::NEG_INFINITY, false, f64::INFINITY, false);

/// The numbers `set` holds (`None` is every value) as a union of spans. A
/// `Values` set holds strings / booleans only, so no number.
pub(crate) fn number_spans(set: Option<&SolutionSet>) -> Vec<Span> {
    match set {
        None | Some(SolutionSet::Except(Decided::Str(_) | Decided::Bool(_))) => vec![REALS],
        Some(SolutionSet::Except(Decided::Num(c))) => {
            vec![(f64::NEG_INFINITY, false, *c, false), (*c, false, f64::INFINITY, false)]
        }
        Some(SolutionSet::Interval {
            lo,
            lo_inc,
            hi,
            hi_inc,
        }) => vec![(*lo, *lo_inc, *hi, *hi_inc)],
        Some(SolutionSet::Values(_)) => Vec::new(),
        Some(SolutionSet::Union(spans)) => spans.clone(),
    }
}

/// The intersection of two span unions, empty spans dropped.
pub(crate) fn meet_spans(a: &[Span], b: &[Span]) -> Vec<Span> {
    let mut out = Vec::new();
    for &(l1, li1, h1, hi1) in a {
        for &(l2, li2, h2, hi2) in b {
            let (lo, lo_inc) = match l1.total_cmp(&l2) {
                std::cmp::Ordering::Greater => (l1, li1),
                std::cmp::Ordering::Less => (l2, li2),
                std::cmp::Ordering::Equal => (l1, li1 && li2),
            };
            let (hi, hi_inc) = match h1.total_cmp(&h2) {
                std::cmp::Ordering::Less => (h1, hi1),
                std::cmp::Ordering::Greater => (h2, hi2),
                std::cmp::Ordering::Equal => (h1, hi1 && hi2),
            };
            if lo < hi || (lo == hi && lo_inc && hi_inc) {
                out.push((lo, lo_inc, hi, hi_inc));
            }
        }
    }
    out
}

fn span_contains(&(lo, lo_inc, hi, hi_inc): &Span, n: f64) -> bool {
    (n > lo || (n == lo && lo_inc)) && (n < hi || (n == hi && hi_inc))
}

/// A comparison's solution set plus whether the path's `unset` value makes
/// it true (`!= c` and `== null` do; `== c`, an ordering, and a bare bool
/// read do not). `set: None` is every value (`isSet(p)`, `p != null`).
#[derive(Clone, Debug)]
pub(crate) struct Truth {
    pub(crate) set: Option<SolutionSet>,
    pub(crate) unset: bool,
}

/// What one path ranges over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathDomain {
    pub(crate) kind: Kind,
    pub(crate) maybe_unset: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    /// A finite member set (`bool`, `enum`, a choice record, `holds(P)`).
    Finite(Vec<DomainValue>),
    /// Every value is a number (`number`, `count(P)`).
    Number,
    /// Anything — an undeclared path, a `string`, an opaque type. A value of
    /// any kind may turn up, so only equality reasoning applies.
    Open,
}

impl PathDomain {
    /// The domain `decide()`'s R2 infers for a subject. An unresolved
    /// subject (no schema knowledge) may always be unset.
    pub(crate) fn of(info: &DomainInfo) -> Self {
        PathDomain {
            kind: match &info.domain {
                Domain::Finite(values) => Kind::Finite(values.clone()),
                Domain::Number | Domain::IntRange { .. } => Kind::Number,
                Domain::Infinite => Kind::Open,
            },
            maybe_unset: info.maybe_unset || !info.resolved,
        }
    }

    /// A never-unset `bool` (a relational query such as `holds(P)`).
    pub(crate) fn boolean() -> Self {
        PathDomain {
            kind: Kind::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)]),
            maybe_unset: false,
        }
    }

    /// Whether `value` is one the path can hold.
    fn admits(&self, value: &Decided) -> bool {
        match &self.kind {
            Kind::Finite(members) => domain_value(value).is_some_and(|v| members.contains(&v)),
            Kind::Number => matches!(value, Decided::Num(_)),
            Kind::Open => true,
        }
    }
}

/// A decided scalar as a finite-domain member (`None` for a number).
pub(crate) fn domain_value(d: &Decided) -> Option<DomainValue> {
    match d {
        Decided::Str(s) => Some(DomainValue::Str(s.clone())),
        Decided::Bool(b) => Some(DomainValue::Bool(*b)),
        Decided::Num(_) => None,
    }
}

fn contains(set: &SolutionSet, value: &Decided) -> bool {
    match set {
        SolutionSet::Interval {
            lo,
            lo_inc,
            hi,
            hi_inc,
        } => matches!(value, Decided::Num(n) if span_contains(&(*lo, *lo_inc, *hi, *hi_inc), *n)),
        SolutionSet::Values(values) => domain_value(value).is_some_and(|v| values.contains(&v)),
        SolutionSet::Except(hole) => hole != value,
        SolutionSet::Union(spans) => {
            matches!(value, Decided::Num(n) if spans.iter().any(|s| span_contains(s, *n)))
        }
    }
}

/// Whether `set` holds the finite-domain member `m`.
pub(crate) fn holds_member(set: &SolutionSet, m: &DomainValue) -> bool {
    let v = match m {
        DomainValue::Str(s) => Decided::Str(s.clone()),
        DomainValue::Bool(b) => Decided::Bool(*b),
    };
    contains(set, &v)
}

/// The union of `truths` holds every value `dom` can take (see the module
/// doc). Sound, never complete: an unrecognised shape covers nothing.
pub(crate) fn covers(dom: &PathDomain, truths: &[Truth]) -> bool {
    if dom.maybe_unset && !truths.iter().any(|t| t.unset) {
        return false;
    }
    let mut sets = Vec::with_capacity(truths.len());
    for t in truths {
        match &t.set {
            None => return true,
            Some(s) => sets.push(s),
        }
    }
    // `Except(c)` misses exactly `c`: covered iff `c` is not a value of the
    // path at all, or another set holds it.
    if let Some((i, hole)) = sets.iter().enumerate().find_map(|(i, s)| match s {
        SolutionSet::Except(c) => Some((i, c)),
        _ => None,
    }) {
        return !dom.admits(hole)
            || sets
                .iter()
                .enumerate()
                .any(|(j, s)| j != i && contains(s, hole));
    }
    match &dom.kind {
        Kind::Finite(members) => members
            .iter()
            .all(|m| sets.iter().any(|s| holds_member(s, m))),
        // Every `Except` returned above; a `Values` set holds no number.
        Kind::Number => intervals_cover_reals(sets.iter().flat_map(|s| number_spans(Some(s)))),
        Kind::Open => false,
    }
}

/// Whether the intervals' union is the whole real line: sweep them in
/// ascending start order (an inclusive start first on a tie), extending the
/// covered prefix `(-∞, reach)` / `(-∞, reach]` until a gap or `+∞`.
fn intervals_cover_reals(intervals: impl Iterator<Item = Span>) -> bool {
    let mut ivs: Vec<_> = intervals.collect();
    ivs.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut reach = f64::NEG_INFINITY;
    let mut reach_inc = false;
    for (lo, lo_inc, hi, hi_inc) in ivs {
        let starts_inside = if reach == f64::NEG_INFINITY {
            lo == f64::NEG_INFINITY
        } else {
            lo < reach || (lo == reach && (lo_inc || reach_inc))
        };
        if !starts_inside {
            return false;
        }
        if hi > reach || (hi == reach && hi_inc) {
            reach = hi;
            reach_inc = hi_inc;
        }
    }
    reach == f64::INFINITY
}

/// Do the two solution sets fail to intersect? `Except` pairs always share
/// a value; mixed kinds never do; a `Union` misses when each span does.
pub(crate) fn disjoint(a: &SolutionSet, b: &SolutionSet) -> bool {
    use SolutionSet::*;
    match (a, b) {
        (Union(spans), other) | (other, Union(spans)) => {
            spans.iter().all(|&(lo, lo_inc, hi, hi_inc)| {
                disjoint(
                    &Interval {
                        lo,
                        lo_inc,
                        hi,
                        hi_inc,
                    },
                    other,
                )
            })
        }
        (
            Interval {
                lo: l1,
                lo_inc: li1,
                hi: h1,
                hi_inc: hi1,
            },
            Interval {
                lo: l2,
                lo_inc: li2,
                hi: h2,
                hi_inc: hi2,
            },
        ) => {
            // The intervals miss iff one ends before the other begins, or
            // they touch at a point one of them excludes.
            (h1 < l2 || (h1 == l2 && !(*hi1 && *li2))) || (h2 < l1 || (h2 == l1 && !(*hi2 && *li1)))
        }
        (Except(c), other) | (other, Except(c)) => match other {
            Except(_) => false,
            Values(vs) => vs.iter().all(|v| domain_value(c).as_ref() == Some(v)),
            Interval { lo, hi, .. } => {
                matches!(c, Decided::Num(n) if lo == n && hi == n) && contains(other, c)
            }
            Union(_) => false, // matched by the first arm
        },
        (Values(x), Values(y)) => x.is_disjoint(y),
        (Values(_), Interval { .. }) | (Interval { .. }, Values(_)) => true,
    }
}

/// The satisfying set of `path <opname> lit` over the declared type, or
/// `None` when the pair is out of domain (an ordering on an unordered type,
/// a literal of the wrong type, a non-scalar declaration).
pub(crate) fn solution_set(declared: &Type, opname: &str, lit: &Val) -> Option<SolutionSet> {
    match declared {
        Type::Number => {
            let v = match lit {
                Val::Int(i) => *i as f64,
                Val::UInt(u) => *u as f64,
                Val::Double(d) => *d,
                _ => return None,
            };
            number_set(opname, v)
        }
        // Finite domains. An ordering has no meaning over an unordered
        // member set, so `<`/`<=`/`>`/`>=` are OUT OF DOMAIN rather than
        // guessed at — the conservative reading.
        Type::Bool => {
            let Val::Boolean(b) = lit else { return None };
            let all = [DomainValue::Bool(true), DomainValue::Bool(false)];
            finite_set(&all, &DomainValue::Bool(*b), opname)
        }
        Type::Enum(members) => {
            let Val::String(s) = lit else { return None };
            if !members.iter().any(|m| m == s.as_str()) {
                return None; // a foreign member is its own diagnostic's problem
            }
            let all: Vec<DomainValue> = members.iter().cloned().map(DomainValue::Str).collect();
            finite_set(&all, &DomainValue::Str(s.to_string()), opname)
        }
        // §5.2: equality/inequality only for `string`.
        Type::Str => {
            let Val::String(s) = lit else { return None };
            match opname {
                op::EQUALS => Some(SolutionSet::Values(
                    std::iter::once(DomainValue::Str(s.to_string())).collect(),
                )),
                op::NOT_EQUALS => Some(SolutionSet::Except(Decided::Str(s.to_string()))),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `x <opname> v` over the numbers; `None` for a non-comparison.
pub(crate) fn number_set(opname: &str, v: f64) -> Option<SolutionSet> {
    let interval = |lo, lo_inc, hi, hi_inc| SolutionSet::Interval {
        lo,
        lo_inc,
        hi,
        hi_inc,
    };
    Some(match opname {
        op::EQUALS => interval(v, true, v, true),
        op::NOT_EQUALS => SolutionSet::Except(Decided::Num(v)),
        op::LESS => interval(f64::NEG_INFINITY, false, v, false),
        op::LESS_EQUALS => interval(f64::NEG_INFINITY, false, v, true),
        op::GREATER => interval(v, false, f64::INFINITY, false),
        op::GREATER_EQUALS => interval(v, true, f64::INFINITY, false),
        _ => return None,
    })
}

/// `==` / `!=` of `value` over the finite member set `all`.
pub(crate) fn finite_set(all: &[DomainValue], value: &DomainValue, opname: &str) -> Option<SolutionSet> {
    let set: BTreeSet<DomainValue> = match opname {
        op::EQUALS => std::iter::once(value.clone()).collect(),
        op::NOT_EQUALS => all.iter().filter(|m| *m != value).cloned().collect(),
        _ => return None,
    };
    Some(SolutionSet::Values(set))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(opname: &str, v: f64) -> Truth {
        Truth {
            set: number_set(opname, v),
            unset: opname == op::NOT_EQUALS,
        }
    }

    fn numbers(maybe_unset: bool) -> PathDomain {
        PathDomain {
            kind: Kind::Number,
            maybe_unset,
        }
    }

    #[test]
    fn complementary_half_lines_cover_the_reals_only_when_they_meet() {
        let d = numbers(false);
        assert!(covers(&d, &[num(op::LESS, 5.0), num(op::GREATER_EQUALS, 5.0)]));
        assert!(covers(&d, &[num(op::LESS_EQUALS, 5.0), num(op::GREATER, 5.0)]));
        // `x < 5 || x > 5` misses 5 itself; the point closes it.
        assert!(!covers(&d, &[num(op::LESS, 5.0), num(op::GREATER, 5.0)]));
        assert!(covers(
            &d,
            &[num(op::LESS, 5.0), num(op::GREATER, 5.0), num(op::EQUALS, 5.0)]
        ));
        // A gap in the middle.
        assert!(!covers(&d, &[num(op::LESS, 3.0), num(op::GREATER, 5.0)]));
        assert!(!covers(&d, &[num(op::GREATER, 5.0)]));
    }

    #[test]
    fn a_maybe_unset_path_needs_a_set_true_on_unset() {
        let halves = [num(op::LESS, 5.0), num(op::GREATER_EQUALS, 5.0)];
        assert!(!covers(&numbers(true), &halves));
        assert!(covers(&numbers(true), &[num(op::NOT_EQUALS, 5.0), num(op::EQUALS, 5.0)]));
    }

    #[test]
    fn an_except_hole_needs_another_set_holding_it() {
        let open = PathDomain {
            kind: Kind::Open,
            maybe_unset: false,
        };
        let s = |x: &str| Decided::Str(x.to_string());
        let except = |c| Truth {
            set: Some(SolutionSet::Except(c)),
            unset: true,
        };
        assert!(covers(&open, &[except(s("a")), except(s("b"))]));
        assert!(!covers(&open, &[except(s("a")), except(s("a"))]));
        assert!(covers(
            &open,
            &[
                except(s("a")),
                Truth {
                    set: Some(SolutionSet::Values(
                        std::iter::once(DomainValue::Str("a".into())).collect()
                    )),
                    unset: false,
                }
            ]
        ));
        // An open domain is never covered by positive sets alone.
        assert!(!covers(&open, &[num(op::LESS, 5.0), num(op::GREATER_EQUALS, 5.0)]));
    }
}
