//! The declared clock's positions as a model of guards that read it. A guard
//! is read as a formula whose atoms over clock paths (the clock's day and
//! slot paths, `clock.index`, `clock.day`, `clock.slot`, `clock.weekday`,
//! `clock.weekdayLabel`) are decided at each position the clock can stand
//! at; every other atom stays unknown unless the decider already settles it.
//!
//! Two uses: a conjunction of clock atoms that no position satisfies is
//! provably false (`run.day == 3 && run.slot == 'afternoon' && clock.index
//! > 7`, or a slot on the clock's last day that comes after its last slot),
//! and an objective whose `done` can only hold once its `by=` deadline
//! already does fails before it can be done ([`W_DEADLINE_BEFORE_WINDOW`]).
//!
//! The positions run from where the clock starts (its paths' defaults) to
//! its last position; a clock that never ends is cut off past every day and
//! index its atoms compare against plus one week, beyond which every atom
//! repeats. Bounded: a model needing more than [`MAX_POSITIONS`] positions
//! is not built and nothing is claimed.

use std::collections::BTreeSet;
use std::path::PathBuf;

use cel_parser::ast::{operators as op, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::clock::{Advance, ClockAt, ClockDecl, ClockValue};
use lute_syntax::ast::{Document, Node, Objective};

use crate::cel_expand::{expand_cel, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{decide, literal_truth, DecideCtx, Decided};
use crate::meta::StateSchema;
use crate::rel_schema::RelVocab;
use crate::solution::{contains, number_spans, Truth};

/// `W-DEADLINE-BEFORE-WINDOW`: an objective's `done` can only hold at clock
/// positions where its `by=` deadline already holds, so the deadline fails
/// it before it can be done.
pub const W_DEADLINE_BEFORE_WINDOW: &str = "W-DEADLINE-BEFORE-WINDOW";

/// The most positions a model enumerates.
const MAX_POSITIONS: usize = 20_000;

/// A guard read over clock positions.
enum Formula {
    And(Vec<Formula>),
    Or(Vec<Formula>),
    Not(Box<Formula>),
    /// A comparison over one clock path: the path, the values that make it
    /// true, and how it reads (`None` when it has no simple rendering).
    Clock {
        key: String,
        truth: Truth,
        text: Option<String>,
    },
    /// `visited('<beat>')`: false until the beat has been presented, which
    /// is at the earliest at this position; unknown from then on.
    Visited(ClockAt),
    /// `clock.ended` (read only with [`Reads::ended`]): false at every
    /// position but the last, where it is unknown — the clock stands there
    /// both before and after the advance that ends it.
    Ended,
    /// Any other atom: its decided value, `None` when undecided.
    Const(Option<bool>),
}

/// Which settle of a position a formula is judged at: on arrival (before any
/// beat of the position is presented) or within it (after one may have
/// been). Only `visited` tells them apart.
#[derive(Clone, Copy, PartialEq)]
enum Settle {
    Arrival,
    Within,
}

/// How relational calls read while building a formula.
struct Reads<'r> {
    /// A pure-schedule derived `holds(rel(c…))` reads as its rules'
    /// `cel()` guards.
    schedule: Option<(&'r RelVocab, &'r DefTable<'r>)>,
    /// `visited('<id>')` reads as the earliest position its beat can be
    /// eligible at; `None` leaves it unknown.
    visited: Option<&'r dyn Fn(&str) -> Option<ClockAt>>,
    /// `clock.ended` reads as [`Formula::Ended`].
    ended: bool,
}

const PLAIN: Reads<'static> = Reads {
    schedule: None,
    visited: None,
    ended: false,
};

/// The clock paths a formula decides at a position.
fn is_clock_key(clock: &ClockDecl, key: &str) -> bool {
    use lute_manifest::clock::{
        CLOCK_DAY, CLOCK_INDEX, CLOCK_SLOT, CLOCK_WEEKDAY, CLOCK_WEEKDAY_LABEL,
    };
    key == clock.day
        || clock.slot.as_deref() == Some(key)
        || [
            CLOCK_INDEX,
            CLOCK_DAY,
            CLOCK_SLOT,
            CLOCK_WEEKDAY,
            CLOCK_WEEKDAY_LABEL,
        ]
        .contains(&key)
}

/// The value clock path `key` holds at `at`.
fn value_at(clock: &ClockDecl, at: ClockAt, key: &str) -> Option<Decided> {
    if key == clock.day {
        return Some(Decided::Num(at.day as f64));
    }
    if clock.slot.as_deref() == Some(key) {
        return clock
            .slot_name(at.slot)
            .map(|s| Decided::Str(s.to_string()));
    }
    clock
        .values(at)
        .into_iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| match v {
            ClockValue::Num(n) => Decided::Num(n as f64),
            ClockValue::Str(s) => Decided::Str(s),
        })
}

fn build(expr: &Expr, ctx: &DecideCtx<'_>, clock: &ClockDecl, reads: &Reads<'_>) -> Formula {
    if let Expr::Call(c) = expr {
        if c.target.is_none() {
            match (c.func_name.as_str(), c.args.as_slice()) {
                (op::LOGICAL_NOT, [a]) => {
                    return Formula::Not(Box::new(build(&a.expr, ctx, clock, reads)))
                }
                (op::LOGICAL_AND, [a, b]) => {
                    return Formula::And(vec![
                        build(&a.expr, ctx, clock, reads),
                        build(&b.expr, ctx, clock, reads),
                    ])
                }
                (op::LOGICAL_OR, [a, b]) => {
                    return Formula::Or(vec![
                        build(&a.expr, ctx, clock, reads),
                        build(&b.expr, ctx, clock, reads),
                    ])
                }
                ("holds", [a]) => {
                    if let Some(f) = reads
                        .schedule
                        .and_then(|(vocab, defs)| schedule(&a.expr, vocab, defs, ctx, clock))
                    {
                        return f;
                    }
                }
                (crate::cel_resolve::VISITED_FN, [a]) => {
                    if let (Some(resolve), Expr::Literal(Val::String(id))) =
                        (reads.visited, &a.expr)
                    {
                        return match resolve(&id) {
                            Some(at) => Formula::Visited(at),
                            None => Formula::Const(None),
                        };
                    }
                }
                _ => {}
            }
        }
    }
    if reads.ended
        && matches!(expr, Expr::Select(_))
        && crate::cel_paths::select_path(expr).as_deref() == Some(lute_manifest::clock::CLOCK_ENDED)
    {
        return Formula::Ended;
    }
    if let Some((key, dom, truth)) = literal_truth(expr, true, ctx) {
        if is_clock_key(clock, &key) && !dom.maybe_unset && !ctx.schema.is_faulty(&key) {
            return Formula::Clock {
                key,
                truth,
                text: render(expr),
            };
        }
    }
    Formula::Const(match decide(expr, ctx) {
        Some(Decided::Bool(b)) => Some(b),
        _ => None,
    })
}

/// A positive `holds(rel(c…))` of a pure-schedule atom — `rel` is `derive:
/// true` and not engine-reserved, no seed fact is the atom, and every rule
/// whose head unifies with it is a ground head over `cel()` guards only —
/// holds exactly when one of those rules' guards all do. `None` otherwise.
fn schedule(
    atom: &Expr,
    vocab: &RelVocab,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    clock: &ClockDecl,
) -> Option<Formula> {
    use lute_syntax::datalog::{BodyLiteral, FactTerm, RuleTerm};
    let Expr::Call(atom) = atom else { return None };
    if atom.target.is_some() {
        return None;
    }
    let consts = atom
        .args
        .iter()
        .map(|a| match &a.expr {
            Expr::Ident(n) => Some(n.clone()),
            Expr::Literal(Val::String(s)) => Some(s.to_string()),
            Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
            _ => None,
        })
        .collect::<Option<Vec<String>>>()?;
    let rel = atom.func_name.as_str();
    if !vocab
        .relations
        .get(rel)
        .is_some_and(|d| d.derive && !d.reserved)
    {
        return None;
    }
    let seeded = vocab.facts.iter().any(|f| {
        f.fact.relation == rel
            && f.fact.args.len() == consts.len()
            && f.fact.args.iter().zip(&consts).all(|(a, k)| match &a.term {
                FactTerm::Ident(i) => i == k,
                FactTerm::Bool(b) => b.to_string() == *k,
                _ => true,
            })
    });
    if seeded {
        return None;
    }
    let mut rules = Vec::new();
    for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
        let head = &r.rule.head.terms;
        if head.len() != consts.len() {
            continue;
        }
        let mut unifies = true;
        for (t, k) in head.iter().zip(&consts) {
            match t {
                RuleTerm::Const(v) => unifies &= v == k,
                RuleTerm::Bool(b) => unifies &= b.to_string() == *k,
                _ => return None,
            }
        }
        if !unifies {
            continue;
        }
        if r.rule.body.is_empty() {
            return None;
        }
        let mut guards = Vec::new();
        for lit in &r.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                return None;
            };
            let mut arena = lute_cel::CelArena::default();
            let ided = parse(cel, defs, &mut arena)?;
            guards.push(build(&ided.expr, ctx, clock, &PLAIN));
        }
        rules.push(Formula::And(guards));
    }
    (!rules.is_empty()).then_some(Formula::Or(rules))
}

/// Expand `raw`'s `@def`s and parse it (marked, as the decider does).
fn parse<'a>(
    raw: &str,
    defs: &DefTable<'_>,
    arena: &'a mut lute_cel::CelArena,
) -> Option<&'a IdedExpr> {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let handle = lute_cel::parse_slot_marked_refs(arena, &expanded)?;
    arena.get(handle)
}

impl Formula {
    /// Kleene evaluation at `at`: `None` when unknown.
    fn eval(&self, clock: &ClockDecl, at: ClockAt, settle: Settle) -> Option<bool> {
        match self {
            Formula::And(fs) => {
                let mut all = Some(true);
                for f in fs {
                    match f.eval(clock, at, settle) {
                        Some(false) => return Some(false),
                        None => all = None,
                        Some(true) => {}
                    }
                }
                all
            }
            Formula::Or(fs) => {
                let mut any = Some(false);
                for f in fs {
                    match f.eval(clock, at, settle) {
                        Some(true) => return Some(true),
                        None => any = None,
                        Some(false) => {}
                    }
                }
                any
            }
            Formula::Not(f) => f.eval(clock, at, settle).map(|b| !b),
            Formula::Clock { key, truth, .. } => {
                let v = value_at(clock, at, key)?;
                Some(truth.set.as_ref().is_none_or(|s| contains(s, &v)))
            }
            Formula::Visited(from) => {
                let before = match settle {
                    Settle::Arrival => at <= *from,
                    Settle::Within => at < *from,
                };
                if before {
                    Some(false)
                } else {
                    None
                }
            }
            Formula::Ended => {
                if clock.last_at() == Some(at) {
                    None
                } else {
                    Some(false)
                }
            }
            Formula::Const(b) => *b,
        }
    }

    fn visit<'f>(&'f self, f: &mut impl FnMut(&'f Formula)) {
        f(self);
        match self {
            Formula::And(fs) | Formula::Or(fs) => fs.iter().for_each(|x| x.visit(f)),
            Formula::Not(x) => x.visit(f),
            _ => {}
        }
    }

    fn clock_atoms(&self) -> usize {
        let mut n = 0;
        self.visit(&mut |f| n += usize::from(matches!(f, Formula::Clock { .. })));
        n
    }

    /// The days past which every atom of this formula reads the same: one
    /// past the largest day, or day of an index, it compares against.
    fn horizon(&self, clock: &ClockDecl) -> Option<i64> {
        let per_day = clock.slot_count() as f64;
        let mut out: Option<i64> = None;
        let mut ok = true;
        self.visit(&mut |f| {
            let day = match f {
                Formula::Clock { key, truth, .. } => {
                    let index = key == lute_manifest::clock::CLOCK_INDEX;
                    if !index && *key != clock.day && key != lute_manifest::clock::CLOCK_DAY {
                        return;
                    }
                    let ends = number_spans(truth.set.as_ref())
                        .into_iter()
                        .flat_map(|(lo, _, hi, _)| [lo, hi])
                        .filter(|x| x.is_finite())
                        .fold(None::<f64>, |m, x| Some(m.map_or(x, |m| m.max(x))));
                    match ends {
                        None => return,
                        Some(x) if index => x / per_day + 2.0,
                        Some(x) => x + 1.0,
                    }
                }
                Formula::Visited(at) => at.day as f64 + 1.0,
                _ => return,
            };
            if day.abs() > 1e9 {
                ok = false;
                return;
            }
            let day = day.ceil() as i64;
            out = Some(out.map_or(day, |d| d.max(day)));
        });
        ok.then_some(out.unwrap_or(1))
    }
}

/// Where the clock starts, and the positions it can stand at — `None` when
/// it has more than [`MAX_POSITIONS`]. A clock that never ends (or
/// `unbounded`, as if it did not) is cut off one week past `horizon`.
fn positions(
    clock: &ClockDecl,
    schema: &StateSchema,
    formulas: &[&Formula],
    unbounded: bool,
) -> Option<Vec<ClockAt>> {
    let first = crate::clock::first_at(clock, schema);
    let last = match clock.last_at().filter(|_| !unbounded) {
        Some(last) => last,
        None => {
            let mut horizon = first.day;
            for f in formulas {
                horizon = horizon.max(f.horizon(clock)?);
            }
            let period = clock
                .week
                .as_ref()
                .map_or(1, |w| i64::from(w.length.max(1)));
            ClockAt {
                day: horizon.checked_add(period)?,
                slot: clock.slot_count() - 1,
            }
        }
    };
    if first > last {
        return Some(Vec::new());
    }
    let span = (last.day - first.day + 1).checked_mul(clock.slot_count() as i64)?;
    if span > MAX_POSITIONS as i64 {
        return None;
    }
    let mut out = Vec::with_capacity(span as usize);
    let mut at = first;
    while at <= last {
        out.push(at);
        at = clock.advance(at, Advance::Slots(1));
    }
    Some(out)
}

/// A connective (`name`, `&&` or `||`, over `args`) decided by
/// the clock's positions — `Some(b)` when it reads `b` at every position
/// the clock can stand at. Only with at least two atoms over clock paths
/// (one alone is the per-path rules' concern).
pub(crate) fn decide_connective(
    name: &str,
    args: &[IdedExpr],
    ctx: &DecideCtx<'_>,
) -> Option<bool> {
    let clock = ctx.schema.clock.as_ref()?;
    let parts = args
        .iter()
        .map(|a| build(&a.expr, ctx, clock, &PLAIN))
        .collect();
    let f = match name {
        op::LOGICAL_AND => Formula::And(parts),
        op::LOGICAL_OR => Formula::Or(parts),
        _ => return None,
    };
    if f.clock_atoms() < 2 {
        return None;
    }
    uniform(&f, clock, ctx.schema)
}

/// The value `f` has at every position, when it has one.
fn uniform(f: &Formula, clock: &ClockDecl, schema: &StateSchema) -> Option<bool> {
    let all = positions(clock, schema, &[f], false)?;
    let (first, rest) = all.split_first()?;
    let v = f.eval(clock, *first, Settle::Within)?;
    rest.iter()
        .all(|at| f.eval(clock, *at, Settle::Within) == Some(v))
        .then_some(v)
}

/// Why guard `raw` is provably false when no clock position satisfies it —
/// the reason an unreachable `when`, a dead arm or a deadline that never
/// holds gives. `None` when the clock's positions do not decide it false.
pub fn position_reason(raw: &str, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Option<String> {
    let clock = ctx.schema.clock.as_ref()?;
    let mut arena = lute_cel::CelArena::default();
    let ided = parse(raw, defs, &mut arena)?;
    let f = build(&ided.expr, ctx, clock, &PLAIN);
    if f.clock_atoms() < 2 || uniform(&f, clock, ctx.schema) != Some(false) {
        return None;
    }
    // The conjuncts over clock paths, when the guard is a conjunction whose
    // clock reads are all plain atoms.
    let mut conjuncts = Vec::new();
    let listed = flat_conjuncts(&f, &mut conjuncts);
    let ends = clock.last_at().filter(|_| {
        positions(clock, ctx.schema, &[&f], true).is_some_and(|all| {
            all.iter()
                .any(|at| f.eval(clock, *at, Settle::Within) != Some(false))
        })
    });
    if let (Some(last), true) = (ends, listed) {
        if let Some(why) = last_day_slots(clock, ctx.schema, last, &conjuncts) {
            return Some(why);
        }
    }
    let what = match (
        listed,
        conjuncts
            .iter()
            .map(|(_, t)| t.clone())
            .collect::<Option<Vec<_>>>(),
    ) {
        (true, Some(texts)) => format!("no clock position has {}", join_and(&texts)),
        _ => "no clock position satisfies it".to_string(),
    };
    Some(match ends {
        Some(last) => format!(
            "the clock ends at its last position ({}), so {what}",
            clock.describe(last)
        ),
        None => what,
    })
}

/// Collect the clock atoms of a conjunction (as `(atom, text)`); `false`
/// when a clock read sits under anything but `&&` (a `||`, a `!` over a
/// connective) or has no rendering.
fn flat_conjuncts<'f>(f: &'f Formula, out: &mut Vec<(&'f Formula, Option<String>)>) -> bool {
    match f {
        Formula::And(fs) => fs.iter().all(|x| flat_conjuncts(x, out)),
        Formula::Clock { text, .. } => {
            out.push((f, text.clone().map(|t| format!("`{t}`"))));
            text.is_some()
        }
        Formula::Not(inner) => match &**inner {
            Formula::Clock { text, .. } => {
                out.push((f, text.clone().map(|t| format!("`!({t})`"))));
                text.is_some()
            }
            other => other.clock_atoms() == 0,
        },
        other => other.clock_atoms() == 0,
    }
}

/// The wording for a day equal to the clock's last day beside reads of one
/// slot path: on that day the slot only reaches the last slot.
fn last_day_slots(
    clock: &ClockDecl,
    schema: &StateSchema,
    last: ClockAt,
    conjuncts: &[(&Formula, Option<String>)],
) -> Option<String> {
    let slot_key =
        |k: &str| clock.slot.as_deref() == Some(k) || k == lute_manifest::clock::CLOCK_SLOT;
    let mut day_eq = 0;
    let mut slot: Option<&str> = None;
    for (f, _) in conjuncts {
        let Formula::Clock { key, truth, .. } = f else {
            return None;
        };
        if *key == clock.day || key == lute_manifest::clock::CLOCK_DAY {
            let holds = |d: i64| {
                truth
                    .set
                    .as_ref()
                    .is_none_or(|s| contains(s, &Decided::Num(d as f64)))
            };
            if !(holds(last.day) && !holds(last.day - 1) && !holds(last.day + 1)) {
                return None;
            }
            day_eq += 1;
        } else if slot_key(key) && slot.is_none_or(|s| s == key) {
            slot = Some(key);
        } else {
            return None;
        }
    }
    let slot = slot.filter(|_| day_eq > 0)?;
    let first = crate::clock::first_at(clock, schema);
    let from = if first.day == last.day { first.slot } else { 0 };
    let members = clock.slots.get(from..=last.slot)?;
    Some(format!(
        "the clock ends at its last position, so on day {} `{slot}` only holds {}",
        last.day,
        members.join(", ")
    ))
}

fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// A plain rendering of an atom: paths, literals, comparisons, `in`, `!`.
fn render(expr: &Expr) -> Option<String> {
    Some(match expr {
        Expr::Ident(n) if !n.starts_with(lute_cel::REF_MARKER) && n != "_" => n.clone(),
        Expr::Select(sel) if !sel.test => format!("{}.{}", render(&sel.operand.expr)?, sel.field),
        Expr::Literal(Val::String(s)) => format!("'{s}'"),
        Expr::Literal(Val::Int(i)) => i.to_string(),
        Expr::Literal(Val::UInt(u)) => u.to_string(),
        Expr::Literal(Val::Double(d)) => d.to_string(),
        Expr::Literal(Val::Boolean(b)) => b.to_string(),
        Expr::List(list) => {
            let items: Option<Vec<String>> =
                list.elements.iter().map(|e| render(&e.expr)).collect();
            format!("[{}]", items?.join(", "))
        }
        Expr::Call(c) if c.target.is_none() => match (c.func_name.as_str(), c.args.as_slice()) {
            (op::LOGICAL_NOT, [a]) => format!("!{}", render(&a.expr)?),
            (name, [a, b]) => {
                let sym = match name {
                    op::EQUALS => "==",
                    op::NOT_EQUALS => "!=",
                    op::LESS => "<",
                    op::LESS_EQUALS => "<=",
                    op::GREATER => ">",
                    op::GREATER_EQUALS => ">=",
                    op::IN => "in",
                    _ => return None,
                };
                format!("{} {sym} {}", render(&a.expr)?, render(&b.expr)?)
            }
            (name, [a]) if name.eq_ignore_ascii_case("isSet") => {
                format!("{name}({})", render(&a.expr)?)
            }
            _ => return None,
        },
        _ => return None,
    })
}

/// `W-DEADLINE-BEFORE-WINDOW` across one project root: `docs` parallel to
/// `foldeds`. An objective (not an `on=` one, whose `done` is judged only at
/// its occasion) whose `done` can first hold at some clock position, with a
/// `by=` deadline that holds before then — on arrival at a position, before
/// any of its beats is presented — and at every position `done` could
/// hold. `done` reads clock paths directly, a pure-schedule `holds(…)`
/// through its rules' guards, and `visited('<beat>')` from the earliest
/// position that beat's `when` can hold. Anchored at the `by`.
pub fn check_project_deadline_windows(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let params = Default::default();
    let beats = crate::beats::project_beats(docs, foldeds);
    let mut out = Vec::new();
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        let Some(clock) = folded.env.state.clock.as_ref() else {
            continue;
        };
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let ctx = DecideCtx {
            schema: &folded.env.state,
            dollar: None,
            params: &params,
            facts: None,
        };
        // The earliest position beat `id` can be eligible at, by its `when`
        // read in its own document.
        let window = |id: &str| -> Option<ClockAt> {
            let mut earliest: Option<ClockAt> = None;
            for pb in beats
                .iter()
                .filter(|pb| pb.id == id && pb.kind != crate::beats::ProjectBeatKind::Entry)
            {
                let bctx = DecideCtx {
                    schema: &pb.folded.env.state,
                    dollar: None,
                    params: &params,
                    facts: None,
                };
                let bdefs = DefTable {
                    bodies: &pb.folded.def_bodies,
                    params: &pb.folded.env.def_params,
                };
                let at = match pb.when_slot.filter(|w| !w.raw.trim().is_empty()) {
                    None => crate::clock::first_at(clock, &folded.env.state),
                    Some(w) => {
                        let mut arena = lute_cel::CelArena::default();
                        let ided = parse(&w.raw, &bdefs, &mut arena)?;
                        let reads = Reads {
                            schedule: Some((&pb.folded.env.rel_vocab, &bdefs)),
                            visited: None,
                            ended: false,
                        };
                        let f = build(&ided.expr, &bctx, clock, &reads);
                        let all = positions(clock, &folded.env.state, &[&f], false)?;
                        *all.iter()
                            .find(|at| f.eval(clock, **at, Settle::Within) != Some(false))?
                    }
                };
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            earliest
        };
        let reads = Reads {
            schedule: Some((&folded.env.rel_vocab, &defs)),
            visited: Some(&window),
            ended: false,
        };
        for quest in &doc.quests {
            for node in &quest.body {
                let Node::Objective(o) = node else { continue };
                if let Some(message) = deadline_window(o, clock, &defs, &ctx, &reads) {
                    let by = o.by.as_ref().expect("a deadline window needs a `by`");
                    out.push((
                        path.clone(),
                        crate::reachability::diag(
                            W_DEADLINE_BEFORE_WINDOW,
                            Severity::Warning,
                            message,
                            by.span,
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// The `W-DEADLINE-BEFORE-WINDOW` message for objective `o`, when its
/// deadline holds before its `done` can.
fn deadline_window(
    o: &Objective,
    clock: &ClockDecl,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    reads: &Reads<'_>,
) -> Option<String> {
    let by = o.by.as_ref().filter(|b| !b.raw.trim().is_empty())?;
    if o.on.is_some() || o.done.raw.trim().is_empty() {
        return None;
    }
    let (mut a1, mut a2) = (lute_cel::CelArena::default(), lute_cel::CelArena::default());
    let done = build(&parse(&o.done.raw, defs, &mut a1)?.expr, ctx, clock, reads);
    let deadline = build(&parse(&by.raw, defs, &mut a2)?.expr, ctx, clock, reads);
    let all = positions(clock, ctx.schema, &[&done, &deadline], false)?;
    let may = |at: ClockAt| {
        done.eval(clock, at, Settle::Arrival) != Some(false)
            || done.eval(clock, at, Settle::Within) != Some(false)
    };
    let first_done = *all.iter().find(|at| may(**at))?;
    // Where the deadline first certainly holds: before `done` can.
    let fails = *all
        .iter()
        .find(|at| deadline.eval(clock, **at, Settle::Arrival) == Some(true))?;
    let before = all.iter().take_while(|at| **at < fails).all(|at| !may(*at))
        && done.eval(clock, fails, Settle::Arrival) == Some(false);
    let every = all
        .iter()
        .filter(|at| may(**at))
        .all(|at| deadline.eval(clock, *at, Settle::Within) == Some(true));
    (before && every).then(|| {
        format!(
            "objective `{}` fails before it can be done: its `done` `{}` can first hold at {}, \
             but its deadline `by: {}` already holds at {} — move the deadline after that window",
            o.id,
            o.done.raw.trim(),
            clock.describe(first_done),
            by.raw.trim(),
            clock.describe(fails),
        )
    })
}

/// `W-BEAT-UNRAISED`: a beat answers an occasion the clock raises, and its
/// `when` holds only where the clock does not raise it — or only at the
/// last `dayEnd`, which the game's end closes. Only where the run starts
/// the engine may raise it itself (`raiseAtStart: true` says it does).
pub const W_BEAT_UNRAISED: &str = "W-BEAT-UNRAISED";

/// Where the clock's `raise:` map raises one occasion, as `lute play`'s
/// `advance:` does: the `slot` occasion where an advance stops; `dayStart`
/// at the first slot of each day an advance enters; `dayEnd` before each
/// midnight an advance crosses — at the day's last slot, or wherever an
/// `advance: day` finds the clock — and, on a clock that ends, once at its
/// last position, raised by the advance that ends it (after `clock.ended`
/// turns true, [`RaiseRule::ending`]). The run starts with no raise, so
/// nothing is raised where it starts and `dayStart` never on its day —
/// unless the clock declares `raiseAtStart: true`: then the engine raises
/// the slot occasion and `dayStart` at the run's first position.
#[derive(Clone, Copy, Debug)]
pub struct RaiseRule {
    slot: bool,
    day_start: bool,
    day_end: bool,
    /// `raiseAtStart: true` and the occasion is the slot or `dayStart` one.
    at_start: bool,
}

impl RaiseRule {
    /// The rule of `occasion`; `None` when the clock does not raise it.
    pub fn of(clock: &ClockDecl, occasion: &str) -> Option<Self> {
        let m = clock.raises();
        let is = |o: &Option<String>| o.as_deref() == Some(occasion);
        let rule = RaiseRule {
            slot: is(&m.slot),
            day_start: is(&m.day_start),
            day_end: is(&m.day_end),
            at_start: clock.raise_at_start && (is(&m.slot) || is(&m.day_start)),
        };
        (rule.slot || rule.day_start || rule.day_end).then_some(rule)
    }

    /// Whether an advance raises the occasion at `at`, on a run that started
    /// at `start`. `any_slot`: a `dayEnd` counts at every slot of a day
    /// before the last (where an `advance: day` may find the clock), not
    /// only at the day's last slot.
    pub fn raises(&self, clock: &ClockDecl, start: ClockAt, at: ClockAt, any_slot: bool) -> bool {
        let last = clock.last_at();
        if at < start || last.is_some_and(|l| at > l) {
            return false;
        }
        (self.slot && at > start)
            || (self.at_start && at == start)
            || (self.day_start && at.slot == 0 && at.day > start.day)
            || (self.day_end
                && match last {
                    // On the last day only the advance that ends the clock
                    // raises `dayEnd`, where it stops: the last position.
                    Some(l) if at.day == l.day => at == l,
                    _ => any_slot || at.slot + 1 == clock.slot_count(),
                })
    }

    /// Whether the raise at `at` is the last `dayEnd`, which the advance that
    /// ends the clock makes after `clock.ended` turns true.
    pub fn ending(&self, clock: &ClockDecl, at: ClockAt) -> bool {
        self.day_end && clock.last_at() == Some(at)
    }

    /// Where the clock raises `occasion`, in words.
    pub fn describe(&self, clock: &ClockDecl, occasion: &str, start: ClockAt) -> String {
        let mut parts = Vec::new();
        if self.slot {
            parts.push(if self.at_start {
                "where an `advance:` stops or where the run starts".to_string()
            } else {
                format!(
                    "where an `advance:` stops (not at {}, where the run starts)",
                    clock.describe(start)
                )
            });
        }
        if self.day_start {
            parts.push(if self.at_start {
                format!(
                    "at a day's first slot, and at {}, where the run starts",
                    clock.describe(start)
                )
            } else {
                format!(
                    "at a day's first slot (not on day {}, the day the run starts)",
                    start.day
                )
            });
        }
        if self.day_end {
            parts.push("at a day's last slot".to_string());
        }
        format!("the clock raises `{occasion}` only {}", parts.join(", or "))
    }
}

/// The project's `terminal:` as written, when it holds whenever
/// `clock.ended` does (`clock.ended`, or a `||` with it; `@def`s expanded)
/// — so the game is over before the last `dayEnd` is raised.
pub fn terminal_on_end(terminal: Option<&str>, defs: &DefTable<'_>) -> Option<String> {
    fn implied(expr: &Expr) -> bool {
        match expr {
            Expr::Select(_) => {
                crate::cel_paths::select_path(expr).as_deref()
                    == Some(lute_manifest::clock::CLOCK_ENDED)
            }
            Expr::Call(c) if c.target.is_none() => {
                match (c.func_name.as_str(), c.args.as_slice()) {
                    (op::LOGICAL_OR, [a, b]) => implied(&a.expr) || implied(&b.expr),
                    (op::LOGICAL_AND, [a, b]) => implied(&a.expr) && implied(&b.expr),
                    _ => false,
                }
            }
            _ => false,
        }
    }
    let t = terminal?.trim();
    let mut arena = lute_cel::CelArena::default();
    let ided = parse(t, defs, &mut arena)?;
    implied(&ided.expr).then(|| t.to_string())
}

/// One `when` read over the clock's raises of one occasion.
struct RaiseModel<'c> {
    clock: &'c ClockDecl,
    start: ClockAt,
    /// `None`: the clock does not raise the occasion — the engine may raise
    /// it at any position.
    rule: Option<RaiseRule>,
    /// The project's `terminal:` when it closes the last `dayEnd`.
    closed_end: Option<String>,
    when: Formula,
    all: Vec<ClockAt>,
}

impl<'c> RaiseModel<'c> {
    /// The model of `when` (and `prev`, a guard whose positions it must
    /// cover too) for `occasion`, in `folded`'s schema. `None` without a
    /// clock, when `when` reads no clock path, or past [`MAX_POSITIONS`].
    fn new(
        when: &str,
        prev: Option<&str>,
        occasion: &str,
        folded: &'c FoldedEnv,
    ) -> Option<(Self, Formula)> {
        let schema = &folded.env.state;
        let clock = schema.clock.as_ref()?;
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let params = Default::default();
        let ctx = DecideCtx {
            schema,
            dollar: None,
            params: &params,
            facts: None,
        };
        let reads = Reads {
            schedule: None,
            visited: None,
            ended: true,
        };
        let formula = |raw: &str| {
            let mut arena = lute_cel::CelArena::default();
            parse(raw, &defs, &mut arena).map(|ided| build(&ided.expr, &ctx, clock, &reads))
        };
        let when = formula(when)?;
        let mut clocked = false;
        when.visit(&mut |f| clocked |= matches!(f, Formula::Clock { .. } | Formula::Ended));
        if !clocked {
            return None;
        }
        let prev = match prev {
            Some(p) => formula(p)?,
            None => Formula::Const(Some(true)),
        };
        let all = positions(clock, schema, &[&when, &prev], false)?;
        let rule = RaiseRule::of(clock, occasion);
        let outside = folded
            .occasions
            .get(occasion)
            .is_some_and(|d| d.outside_run);
        let closed_end = terminal_on_end(folded.env.terminal.as_deref(), &defs)
            .filter(|_| rule.is_some() && !outside);
        Some((
            RaiseModel {
                clock,
                start: crate::clock::first_at(clock, schema),
                rule,
                closed_end,
                when,
                all,
            },
            prev,
        ))
    }

    /// Whether the occasion is raised at `at` with the game still on.
    fn open(&self, at: ClockAt) -> bool {
        match self.rule {
            None => true,
            Some(r) => {
                r.raises(self.clock, self.start, at, true)
                    && !(self.closed_end.is_some() && r.ending(self.clock, at))
            }
        }
    }

    fn may(&self, at: ClockAt) -> bool {
        self.when.eval(self.clock, at, Settle::Within) != Some(false)
    }

    /// Why the `when` holds at no open raise of `occasion` though it can
    /// hold somewhere, and what to do: `None` when some open raise may meet
    /// it, or it can hold nowhere (that is `E-BEAT-UNREACHABLE`'s).
    fn unraised(&self, occasion: &str) -> Option<Unraised> {
        let rule = self.rule?;
        if self.all.iter().any(|at| self.open(*at) && self.may(*at)) {
            return None;
        }
        let first = *self.all.iter().find(|at| self.may(**at))?;
        let last = self.clock.last_at();
        if let (Some(t), Some(l)) = (&self.closed_end, last) {
            if rule.ending(self.clock, l) && self.may(l) {
                return Some(Unraised {
                    why: format!(
                        "it can hold at {}, where only the advance that ends the clock raises \
                         `{occasion}`: after `clock.ended` turns true, when `terminal: {t}` \
                         already holds and the game is over",
                        self.clock.describe(l)
                    ),
                    fix: "raise it before the clock ends, or write `terminal:` so it holds only \
                          once this has played"
                        .to_string(),
                    at_start: false,
                });
            }
        }
        // Where the run starts the clock raises nothing, but the engine may
        // raise the slot occasion or `dayStart` there itself.
        if (rule.slot || rule.day_start)
            && !rule.at_start
            && self.all.contains(&self.start)
            && self.may(self.start)
        {
            return Some(Unraised {
                why: format!(
                    "the clock does not raise `{occasion}` at {}, where the run starts",
                    self.clock.describe(self.start)
                ),
                fix: "if the engine raises it when a run starts, declare `raiseAtStart: true` on \
                      the clock; otherwise answer an occasion raised where it holds"
                    .to_string(),
                at_start: true,
            });
        }
        let start_day = rule.day_start
            && self
                .all
                .iter()
                .filter(|at| self.may(**at))
                .all(|at| at.day == self.start.day);
        Some(Unraised {
            why: format!(
                "it can hold at {}, but {}",
                self.clock.describe(first),
                rule.describe(self.clock, occasion, self.start)
            ),
            fix: if start_day {
                "answer the occasion the run starts with instead".to_string()
            } else {
                format!("answer an occasion raised where it holds, or let its `when` hold where the clock raises `{occasion}`")
            },
            at_start: false,
        })
    }
}

/// Why a `when` meets no raise of its occasion, and what to do.
struct Unraised {
    why: String,
    fix: String,
    /// It can hold where the run starts, where the clock raises nothing but
    /// the engine may: so it may still play.
    at_start: bool,
}

/// [`W_BEAT_UNRAISED`] across one project root: `docs` parallel to
/// `foldeds`. A beat on an occasion the clock raises whose `when` can hold
/// only where the clock does not raise it ([`RaiseRule`]), or only at the
/// last `dayEnd` when the project's `terminal:` holds with `clock.ended`.
/// A scene a `chapters:` chain waits on is left to `W-CHAPTER-STALL`.
/// Anchored at the beat's `on`.
pub fn check_project_unraised(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for pb in crate::beats::project_beats(docs, foldeds) {
        let Some(when) = pb.when_slot.map(|w| w.raw.trim()).filter(|w| !w.is_empty()) else {
            continue;
        };
        if pb.kind == crate::beats::ProjectBeatKind::Scene
            && crate::chapters::waited_on(docs, &pb.id)
        {
            continue;
        }
        let Some((model, _)) = RaiseModel::new(when, None, pb.on, pb.folded) else {
            continue;
        };
        let Some(u) = model.unraised(pb.on) else {
            continue;
        };
        // Where the run starts the engine may raise it: no "never plays".
        let (raise, never) = if u.at_start {
            ("the clock makes", "")
        } else {
            ("of it", ", so it never plays")
        };
        out.push((
            pb.path.clone(),
            crate::reachability::diag(
                W_BEAT_UNRAISED,
                Severity::Warning,
                format!(
                    "{} answers `{}`, but its `when` `{when}` holds at no raise {raise}: {}{never} \
                     — {}",
                    pb.name(),
                    pb.on,
                    u.why,
                    u.fix
                ),
                pb.anchor,
            ),
        ));
    }
    out
}

/// Why a `chapters:` chain on `occasion` can stop for good at a listed
/// scene whose `when` reads the clock, and what to do — `None` when it
/// cannot. `prev` is the scene listed before it (its id and own `when`),
/// `None` for the first. It stops when no raise of `occasion` meets the
/// `when` ([`RaiseModel::unraised`]), or when the scene before it may play
/// at a raise after which the occasion is still raised but never again
/// where the `when` holds — a window that closes. On a clock that never
/// ends, only arrivals a full week before the model's end are judged
/// (past it every atom repeats).
pub(crate) fn chapter_window(
    when: &str,
    prev: Option<(&str, Option<&str>)>,
    occasion: &str,
    folded: &FoldedEnv,
) -> Option<(String, String)> {
    let (model, prev_when) = RaiseModel::new(when, prev.and_then(|(_, w)| w), occasion, folded)?;
    if let Some(u) = model.unraised(occasion) {
        let fix = if u.at_start {
            format!(
                "declare `raiseAtStart: true` on the clock if the engine raises `{occasion}` when \
                 a run starts; otherwise answer an occasion raised where it holds"
            )
        } else {
            u.fix
        };
        return Some((
            format!("its `when` holds at no raise of `{occasion}`: {}", u.why),
            fix,
        ));
    }
    let (prev_id, _) = prev?;
    let clock = model.clock;
    let tail = match clock.last_at() {
        Some(_) => i64::MAX,
        None => {
            let period = clock
                .week
                .as_ref()
                .map_or(1, |w| i64::from(w.length.max(1)));
            model.all.last()?.day - period
        }
    };
    // A raise the clock makes comes once per position; one the engine makes
    // may come again where the scene before played.
    let later = |a: ClockAt, q: ClockAt| if model.rule.is_some() { q > a } else { q >= a };
    let raises: Vec<ClockAt> = model
        .all
        .iter()
        .copied()
        .filter(|at| model.open(*at))
        .collect();
    let stuck = raises.iter().copied().find(|a| {
        a.day <= tail
            && prev_when.eval(clock, *a, Settle::Within) != Some(false)
            && raises.iter().any(|q| later(*a, *q))
            && !raises.iter().any(|q| later(*a, *q) && model.may(*q))
    })?;
    let ends = match clock.last_at() {
        Some(l) => format!(" before the clock ends at {}", clock.describe(l)),
        None => " ever again".to_string(),
    };
    Some((
        format!(
            "if `{prev_id}` plays at {} or later, no later raise of `{occasion}` meets it{ends}",
            clock.describe(stuck)
        ),
        format!("let its `when` hold at a later raise of `{occasion}` too"),
    ))
}

/// `W-OBJECTIVE-STRANDED`: a required objective whose only known completion
/// beats all have finite clock windows, but no deadline turns a missed window
/// into a failure. Conservative: an unclocked `when` leaves an open path.
pub const W_OBJECTIVE_STRANDED: &str = "W-OBJECTIVE-STRANDED";

/// `W-SLOT-CONTENTION`: required objectives in one run compete for the same
/// single clock position, and every beat that can complete either consumes time.
pub const W_SLOT_CONTENTION: &str = "W-SLOT-CONTENTION";
/// `E-ADVANCE-CASCADE`: an unguarded beat answers the clock occasion that
/// raises at each position and advances by a period that lets it answer again.
/// The runtime still bounds every cascade, but this is an authoring error: a
/// clock raise should hand off to another beat or have a guard/spend policy.
pub const E_ADVANCE_CASCADE: &str = "E-ADVANCE-CASCADE";


fn visited_ids(raw: &str, defs: &DefTable<'_>) -> BTreeSet<String> {
    let mut arena = lute_cel::CelArena::default();
    let Some(ided) = parse(raw, defs, &mut arena) else {
        return BTreeSet::new();
    };
    fn walk(e: &Expr, out: &mut BTreeSet<String>) {
        match e {
            Expr::Call(c) => {
                if let Some(id) = crate::cel_resolve::visited_call_target(c) {
                    out.insert(id.to_string());
                }
                if let Some(t) = &c.target {
                    walk(&t.expr, out);
                }
                for a in &c.args {
                    walk(&a.expr, out);
                }
            }
            Expr::Select(s) => walk(&s.operand.expr, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(&ided.expr, &mut out);
    out
}

fn done_paths(raw: &str, defs: &DefTable<'_>) -> BTreeSet<String> {
    let mut arena = lute_cel::CelArena::default();
    let Some(ided) = parse(raw, defs, &mut arena) else {
        return BTreeSet::new();
    };
    crate::cel_paths::collect_path_uses(&ided.expr)
        .into_iter()
        .map(|u| u.path)
        .collect()
}

fn collect_set_paths(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Set(s) => {
                out.insert(s.path.clone());
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_set_paths(&c.body, out);
                }
            }
            Node::Hub(h) => {
                for body in h.bodies() {
                    collect_set_paths(body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        lute_syntax::ast::Arm::When { body, .. }
                        | lute_syntax::ast::Arm::Otherwise { body, .. } => {
                            collect_set_paths(body, out)
                        }
                    }
                }
            }
            Node::Objective(o) => collect_set_paths(&o.body, out),
            Node::On(o) => collect_set_paths(&o.body, out),
            _ => {}
        }
    }
}

fn beat_set_paths(
    pb: &crate::beats::ProjectBeat<'_>,
    docs: &[(PathBuf, Document)],
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (path, doc) in docs {
        if path != pb.path {
            continue;
        }
        match pb.kind {
            crate::beats::ProjectBeatKind::Scene => {
                for shot in &doc.shots {
                    collect_set_paths(&shot.body, &mut out);
                }
            }
            crate::beats::ProjectBeatKind::Entry => {
                if let Some(entry) = doc
                    .entries
                    .iter()
                    .find(|e| e.span.byte_start == pb.unit)
                {
                    collect_set_paths(&entry.body, &mut out);
                }
            }
            crate::beats::ProjectBeatKind::Bundle => {
                if let Some(beat) = doc.beats.iter().find(|b| b.span.byte_start == pb.unit) {
                    collect_set_paths(&beat.body, &mut out);
                }
            }
        }
    }
    out
}

fn bounded_window<'a>(
    pb: &crate::beats::ProjectBeat<'a>,
    clock: &ClockDecl,
) -> Option<Vec<ClockAt>> {
    let raw = pb.when.as_deref().filter(|w| !w.trim().is_empty())?;
    let defs = DefTable {
        bodies: &pb.folded.def_bodies,
        params: &pb.folded.env.def_params,
    };
    let params = Default::default();
    let ctx = DecideCtx {
        schema: &pb.folded.env.state,
        dollar: None,
        params: &params,
        facts: None,
    };
    let mut arena = lute_cel::CelArena::default();
    let ided = parse(raw, &defs, &mut arena)?;
    let f = build(&ided.expr, &ctx, clock, &PLAIN);
    if f.clock_atoms() == 0 {
        return None;
    }
    let all = positions(clock, &pb.folded.env.state, &[&f], false)?;
    let may: Vec<_> = all
        .into_iter()
        .filter(|at| f.eval(clock, *at, Settle::Within) != Some(false))
        .collect();
    (!may.is_empty()).then_some(may)
}

fn objective_completion_beats<'a>(
    o: &Objective,
    folded: &'a FoldedEnv,
    beats: &'a [crate::beats::ProjectBeat<'a>],
    docs: &[(PathBuf, Document)],
) -> Vec<&'a crate::beats::ProjectBeat<'a>> {
    let defs = DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let visited = visited_ids(&o.done.raw, &defs);
    let paths = done_paths(&o.done.raw, &defs);
    beats
        .iter()
        .filter(|pb| {
            o.on
                .as_ref()
                .is_some_and(|(on, _)| pb.on == on)
                || visited.contains(&pb.id)
                || !paths.is_disjoint(&beat_set_paths(pb, docs))
        })
        .collect()
}
/// Find an unguarded beat that can be raised again after its own declared
/// clock movement. A `once: run`/`user`/`week`/`season:*` beat is naturally
/// spent for the current cascade; `once: day` is also safe for a slot move.
pub fn check_project_advance_cascades(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for pb in crate::beats::project_beats(docs, foldeds) {
        let Some(spec) = pb.advances else { continue };
        let Some(clock) = pb.folded.env.state.clock.as_ref() else {
            continue;
        };
        if RaiseRule::of(clock, pb.on).is_none() || pb.spent_by.is_some() {
            continue;
        }
        let repeats = match pb.once {
            crate::beats::BeatOnce::None | crate::beats::BeatOnce::Slot => true,
            crate::beats::BeatOnce::Day => matches!(spec, crate::beats::AdvanceSpec::Day),
            _ => false,
        };
        if !repeats {
            continue;
        }
        let can_repeat = match pb.when.as_deref().map(str::trim) {
            None | Some("") => true,
            Some(raw) => match RaiseModel::new(raw, None, pb.on, pb.folded) {
                Some((model, _)) => {
                    let by = match spec {
                        crate::beats::AdvanceSpec::Slot => Advance::Slots(1),
                        crate::beats::AdvanceSpec::Day => Advance::Day,
                        crate::beats::AdvanceSpec::Slots(n) => Advance::Slots(n),
                    };
                    model.all.iter().any(|at| {
                        model.open(*at)
                            && model.may(*at)
                            && {
                                let next = clock.advance(*at, by);
                                model.open(next) && model.may(next)
                            }
                    })
                }
                None => false,
            },
        };
        if !can_repeat {
            continue;
        }
        let first = crate::clock::first_at(clock, &pb.folded.env.state);
        if clock.last_at().is_some_and(|last| first >= last) {
            continue;
        }
        let amount = match spec {
            crate::beats::AdvanceSpec::Slot => "one slot".to_string(),
            crate::beats::AdvanceSpec::Day => "one day".to_string(),
            crate::beats::AdvanceSpec::Slots(n) => format!("{n} slots"),
        };
        out.push((
            pb.path.clone(),
            crate::reachability::diag(
                E_ADVANCE_CASCADE,
                Severity::Error,
                format!(
                    "{} answers the clock's `{}` raise and advances {amount}, but has no \
                     `when`, `spentBy`, or repetition limit that stops it being eligible at \
                     the next raised position; this creates a repeating `advances:` cascade — \
                     add a guard or make the beat hand off to another beat",
                    pb.name(),
                    pb.on,
                ),
                pb.anchor,
            ),
        ));
    }
    out
}


fn run_tier(q: &lute_syntax::ast::Quest) -> bool {
    q.tier
        .as_ref()
        .is_some_and(|(t, _)| t == "run" || t.starts_with("season:"))
}

/// Run W-OBJECTIVE-STRANDED and W-SLOT-CONTENTION over one project root.
pub fn check_project_objective_clock_windows(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let beats = crate::beats::project_beats(docs, foldeds);
    let mut out = Vec::new();
    let mut parents = std::collections::BTreeMap::<String, String>::new();
    for (_, doc) in docs {
        for q in &doc.quests {
            for node in &q.body {
                if let Node::Objective(o) = node {
                    if let Some(child) = &o.quest {
                        parents.entry(child.clone()).or_insert_with(|| q.id.clone());
                    }
                }
            }
        }
    }
    let tree_root = |id: &str| {
        let mut root = id.to_string();
        let mut seen = BTreeSet::new();
        while let Some(parent) = parents.get(&root) {
            if !seen.insert(root.clone()) {
                break;
            }
            root = parent.clone();
        }
        root
    };
    let mut singletons: Vec<(PathBuf, String, bool, Span, ClockAt)> = Vec::new();
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        let Some(clock) = folded.env.state.clock.as_ref() else {
            continue;
        };
        for q in &doc.quests {
            for node in &q.body {
                let Node::Objective(o) = node else { continue };
                if o.optional || o.id.is_empty() {
                    continue;
                }
                let has_deadline = o.until.as_ref().is_some_and(|x| !x.raw.trim().is_empty())
                    || o.by.as_ref().is_some_and(|x| !x.raw.trim().is_empty());
                let candidates = objective_completion_beats(o, folded, &beats, docs);
                if candidates.is_empty() {
                    continue;
                }
                let mut windows = Vec::new();
                let mut bounded = true;
                for pb in candidates {
                    let Some(window) = bounded_window(pb, clock) else {
                        bounded = false;
                        break;
                    };
                    windows.push((pb, window));
                }
                if !bounded || windows.is_empty() {
                    continue;
                }
                let positions: BTreeSet<ClockAt> =
                    windows.iter().flat_map(|(_, w)| w.iter().copied()).collect();
                let Some(first) = positions.iter().next().copied() else {
                    continue;
                };
                let reset = run_tier(q);
                if !has_deadline {
                    out.push((
                        path.clone(),
                        crate::reachability::diag(
                            W_OBJECTIVE_STRANDED,
                            if reset { Severity::Info } else { Severity::Warning },
                            format!(
                                "required objective `{}` can only be completed by clock-bounded beats \
                                 whose windows close at {}; it has no `until=` or `by=` — {} \
                                 (dsl 0.31.0 §3)",
                                o.id,
                                clock.describe(first),
                                if reset {
                                    "the next run retries"
                                } else {
                                    "add `until=` or `by=` so a missed window fails the objective"
                                }
                            ),
                            o.span,
                        ),
                    ));
                }
                if positions.len() == 1
                    && windows.iter().all(|(pb, w)| pb.advances.is_some() && w.len() == 1)
                {
                    singletons.push((path.clone(), q.id.clone(), reset, o.span, first));
                }
            }
        }
    }
    for i in 0..singletons.len() {
        for j in i + 1..singletons.len() {
            let (path_a, quest_a, run_a, span_a, pos_a) = &singletons[i];
            let (path_b, quest_b, run_b, span_b, pos_b) = &singletons[j];
            if tree_root(quest_a) != tree_root(quest_b) || !run_a || !run_b || pos_a != pos_b {
                continue;
            }
            let Some(clock) = foldeds
                .iter()
                .find_map(|f| f.env.state.clock.as_ref())
            else {
                continue;
            };
            let message = format!(
                "required objectives in quest `{}` contend for the same only clock position {}: \
                 each can complete only by an `advances` beat there (dsl 0.31.0 §4)",
                quest_a,
                clock.describe(*pos_a)
            );
            for (path, span) in [(path_a, span_a), (path_b, span_b)] {
                out.push((
                    path.clone(),
                    crate::reachability::diag(
                        W_SLOT_CONTENTION,
                        Severity::Warning,
                        message.clone(),
                        *span,
                    ),
                ));
            }
        }
    }
    out
}
