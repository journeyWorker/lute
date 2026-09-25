//! K3 evaluator core (dsl 0.4.0 §4.3): effective state, the bounded fact
//! scan, and [`eval`] — the ONE function that walks a post-expansion CEL
//! AST (`@`/`$`-free, D14) under three-valued logic.
//!
//! Ground operations (comparison, arithmetic, equality, `?:` selection,
//! `in`-list) delegate to [`lute_check::apply_op`] (D3) — the SAME R3
//! semantics `decide()` uses — lifted over [`Value::Unknown`] here:
//! `false && U = false`, `true || U = true`, otherwise a connective is `U`;
//! a comparison/arithmetic node with a `U` operand is `U`; `?:` with a `U`
//! condition is `U` (never guesses a branch, mirrors `decide()`'s own
//! ternary rule). List-literal indexing (`list[i]`, §4.3) is its own
//! function — the index and, when it decides to an in-range integer, the
//! selected element are evaluated; a non-list target or an
//! unknown/non-integer/out-of-range index is `U`. `isSet()`/`has()` are
//! DEFINITE (D19) — presence, not
//! value. `holds`/`count` read the supplied fact set — through the Datalog
//! fixpoint under `derive: true` (dsl 0.22.0 §6), a bounded scan otherwise. `visited('<scene id>')` (dsl
//! 0.21.0 §7a.1) is DEFINITE over the supplied presented-scene set the
//! [`FactStore`] carries — closed-world, like a non-derived relation: an id
//! absent from the set was not presented. `now()`/`validAt(...)` are
//! always `U` — narrative time has no mock surface. Every `U` this module
//! produces records why into `unresolved`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use cel_parser::ast::{operators as op, CallExpr, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_check::{Decided, RelVocab, StateSchema};
use lute_manifest::Literal;

use crate::datalog::Program;
use crate::value::{UnresolvedAtom, Value};

/// The result of an [`EffectiveState::read`] — distinguishes "no effective
/// value at all" (§4.3 unset, itself reported [`Value::Unknown`] by
/// [`eval`]) from a present value that may ITSELF be [`Value::Unknown`] (a
/// trace write whose RHS didn't decide).
#[derive(Clone, Debug, PartialEq)]
pub enum Read {
    Value(Value),
    Unset,
}

/// How a reserved quest path actually READ during the walk resolved (dsl
/// 0.5.1 §1.3): admitted as an explicit `--state`/`--mock` mock (§1.1), or
/// left un-mocked and resolved to its domain DEFAULT (§1.2). Drives the
/// "existence unverified" note [`crate::walk`] attaches to every such read
/// of a FOREIGN quest id (one not defined by an in-document `<quest>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReservedReadKind {
    Mocked,
    Defaulted,
}

/// §4.3 effective state: a trace-applied write, a mock seed, or a schema
/// `default:` — read in exactly that order, falling through to unset.
/// `#[derive(Clone)]` backs Task 20's PRE-EVENT SNAPSHOT (`0.4.0 §4.4`
/// quest walk): an `<on>` guard evaluates against a clone taken BEFORE its
/// event's arms run, never the live, in-flow-mutated state. `reserved_reads`
/// is `Rc`-shared (not merely cloned) across every such snapshot: it is a
/// walk-GLOBAL observation log (dsl 0.5.1 §1.3), not per-snapshot state, so
/// a reserved-path read made only inside an `<on>` guard still surfaces on
/// the live [`Walk`](crate::walk)'s state.
#[derive(Clone)]
pub struct EffectiveState<'a> {
    schema: &'a StateSchema,
    seed: BTreeMap<String, Value>,
    writes: BTreeMap<String, Value>,
    reserved_reads: Rc<RefCell<BTreeMap<String, ReservedReadKind>>>,
}

impl<'a> EffectiveState<'a> {
    /// `seed` is the mock-supplied `--state`/`state:` surface (§4.3); trace
    /// writes accumulate separately via [`EffectiveState::write`] as the
    /// walk (Task 19) proceeds.
    pub fn new(schema: &'a StateSchema, seed: BTreeMap<String, Value>) -> Self {
        Self {
            schema,
            seed,
            writes: BTreeMap::new(),
            reserved_reads: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    /// §4.3 read order: trace write → mock seed → RESERVED default (dsl
    /// 0.5.1 §1.2) → schema `default:` → unset. A RESERVED quest path
    /// (`quest.<id>.state` / `quest.<id>.objectives.<oid>.done`) skips the
    /// schema `default:` tier entirely (`objectives.<oid>.done`'s decl
    /// carries `default: Some(false)` — `match_check.rs`'s implicit fold —
    /// for the REAL engine's benefit) and instead resolves, when neither
    /// written (derived by an in-document `<quest>` walk) nor seeded
    /// (§1.1), to its OWN reserved default ([`reserved_quest_default`]):
    /// `objectives.<oid>.done` → `false`, `quest.<id>.state` → `unset` —
    /// never bare [`Read::Unset`] (dsl 0.5.1 §1.2: "trace MUST resolve it
    /// to the path's default, ... never 'no arm'"). Every such reserved
    /// resolution — seeded OR defaulted — is logged into `reserved_reads`
    /// (§1.3's note surface); a write is never logged (`writes` only ever
    /// holds paths this SAME walk derived, so it is never "foreign").
    /// [`is_reserved_quest_path`] is this crate's own copy of
    /// `lute_check::cel_paths::is_reserved_quest_path` — `pub(crate)` to
    /// that crate, so not reusable across the D1 quarantine boundary
    /// ([`expr_path`] carries the same idiom below).
    pub fn read(&self, path: &str) -> Read {
        if let Some(v) = self.writes.get(path) {
            return Read::Value(v.clone());
        }
        if let Some(v) = self.seed.get(path) {
            if is_reserved_quest_path(path) {
                self.reserved_reads
                    .borrow_mut()
                    .insert(path.to_string(), ReservedReadKind::Mocked);
            }
            return Read::Value(v.clone());
        }
        // dsl 0.19.0 §5: `entry.<id>.read` is engine-written, default
        // `false` — an un-mocked read in trace is a first read.
        if lute_check::is_reserved_entry_read(path) {
            return Read::Value(Value::Bool(false));
        }
        if is_reserved_quest_path(path) {
            self.reserved_reads
                .borrow_mut()
                .insert(path.to_string(), ReservedReadKind::Defaulted);
            return Read::Value(reserved_quest_default(path));
        }
        if let Some(default) = self.schema.decls.get(path).and_then(|d| d.default.as_ref()) {
            return Read::Value(literal_to_value(default));
        }
        Read::Unset
    }

    /// `::set path = v` (§4.4, sequential in-flow visibility). An `Unknown`
    /// RHS marks the path unknown: it still reads as PRESENT
    /// (`isSet`/`has` see it, D19) but its VALUE stays unknown until a
    /// later write decides it.
    pub fn write(&mut self, path: &str, v: Value) {
        self.writes.insert(path.to_string(), v);
    }

    /// Every path that can have an effective value: written by the walk,
    /// seeded by a mock, or declared with a `default:`. [`Self::read`] each
    /// to get its value in §4.3 order.
    pub fn effective_paths(&self) -> BTreeSet<String> {
        self.writes
            .keys()
            .chain(self.seed.keys())
            .chain(
                self.schema
                    .decls
                    .iter()
                    .filter(|(_, d)| d.default.is_some())
                    .map(|(p, _)| p),
            )
            .cloned()
            .collect()
    }

    /// §1.3: every reserved quest path actually READ during the walk (via
    /// [`EffectiveState::read`]), classified by how it resolved. Cloned OUT
    /// (not borrowed) so the caller can inspect it after the walk without
    /// fighting the `RefCell` — this crate's own walk never re-enters
    /// `read()` once it starts consuming this.
    pub(crate) fn reserved_reads(&self) -> BTreeMap<String, ReservedReadKind> {
        self.reserved_reads.borrow().clone()
    }
}

/// `true` for a RESERVED quest path (dsl 0.2.0 §5.2, dsl 0.4.0 §4.4):
/// `quest.<id>.state` (3 segments, segment 2 == `state`) or
/// `quest.<id>.objectives.<oid>.done` (5 segments, segment 2 ==
/// `objectives`, segment 4 == `done`) — plus dsl 0.24.0 §2's
/// `quest.<id>.failedBy` and `quest.<id>.objectives.<oid>.failed` —
/// [`EffectiveState::read`]'s own copy of
/// `lute_check::cel_paths::is_reserved_quest_path` (`pub(crate)` there, so
/// not reusable across the D1 quarantine boundary).
pub(crate) fn is_reserved_quest_path(path: &str) -> bool {
    let segs: Vec<&str> = path.split('.').collect();
    matches!(
        segs.as_slice(),
        ["quest", _, "state" | "failedBy"] | ["quest", _, "objectives", _, "done" | "failed"]
    )
}

/// `true` for a boolean objective flag of [`is_reserved_quest_path`] —
/// `quest.<id>.objectives.<oid>.done` or (dsl 0.24.0 §2) `….failed` — whose
/// reserved default is `false` rather than `"unset"`
/// ([`reserved_quest_default`]). Mirrors
/// `lute_check::cel_paths::is_reserved_quest_objective_done` (`pub(crate)`
/// there too).
pub(crate) fn is_reserved_quest_objective_done_path(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "objectives", _, "done" | "failed"]
    )
}

/// dsl 0.24.0 §2: `quest.<id>.failedBy` — the reason enum of a failed quest.
pub(crate) fn is_reserved_quest_failed_by_path(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "failedBy"]
    )
}

/// dsl 0.5.1 §1.2's reserved-path default: an objective flag (`done`, and
/// dsl 0.24.0 §2's `failed`) → `false` (its schema-decl default, mirrored
/// here since trace bypasses that decl tier for reserved paths);
/// `quest.<id>.state` and `quest.<id>.failedBy` → the literal string
/// `"unset"` (their pre-activation / pre-failure value, dsl 0.2.0 §5.2).
pub(crate) fn reserved_quest_default(path: &str) -> Value {
    if is_reserved_quest_objective_done_path(path) {
        Value::Bool(false)
    } else {
        Value::Str("unset".to_string())
    }
}

/// A single fact-pattern position (§4.3 bounded scan): a ground term or the
/// `_` existential wildcard.
#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Ground(String),
    Wildcard,
}

fn pattern_matches(pattern: &[Pat], args: &[String]) -> bool {
    pattern.len() == args.len()
        && pattern.iter().zip(args.iter()).all(|(p, a)| match p {
            Pat::Wildcard => true,
            Pat::Ground(g) => g == a,
        })
}

fn render_pattern(relation: &str, pattern: &[Pat]) -> String {
    let args = pattern
        .iter()
        .map(|p| match p {
            Pat::Ground(g) => g.clone(),
            Pat::Wildcard => "_".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{relation}({args})")
}

/// The mock fact set as modified by trace-applied `::assert`/`::retract`
/// deltas (§4.3), plus the relational vocabulary needed to tell a
/// `derive:true` relation apart from an ordinary closed-world one, plus the
/// presented-scene set `visited(…)` reads (dsl 0.21.0 §7a.1) — engine
/// knowledge of the save, supplied as a mock (`visited:`) exactly as facts
/// are, and cloned with them into every pre-event snapshot.
///
/// With a [`Program`] attached ([`FactStore::with_derivation`], dsl 0.22.0
/// §6 `derive: true`) every query reads the stratified least fixpoint over
/// the held facts — the runner's own evaluator — so a derived relation is
/// as definite as a base one. Without one (`derive: false`, the 0.21 model)
/// a query is pattern LOOKUP: an unmatched derived relation is unknown, and
/// the relation is logged in [`FactStore::derived_reads`].
#[derive(Clone)]
pub struct FactStore<'a> {
    facts: BTreeSet<(String, Vec<String>)>,
    rel_vocab: &'a RelVocab,
    visited: BTreeSet<String>,
    derivation: Option<&'a Program>,
    /// Derived relations queried without derivation — walk-global like
    /// [`EffectiveState`]'s reserved-read log, so shared across snapshots.
    derived_reads: Rc<RefCell<BTreeSet<String>>>,
    /// dsl 0.24 T1-1: derived relations whose materialized fixpoint read an
    /// undecided rule guard ([`crate::datalog::Closure::undecided`]) — a
    /// query over one is unknown, never a silent "no such fact".
    undecided: BTreeMap<String, Vec<UnresolvedAtom>>,
}

impl<'a> FactStore<'a> {
    pub fn new(rel_vocab: &'a RelVocab) -> Self {
        Self {
            facts: BTreeSet::new(),
            rel_vocab,
            visited: BTreeSet::new(),
            derivation: None,
            derived_reads: Rc::new(RefCell::new(BTreeSet::new())),
            undecided: BTreeMap::new(),
        }
    }

    /// Answer every query over `program`'s fixpoint of the held facts.
    pub fn with_derivation(mut self, program: &'a Program) -> Self {
        self.derivation = Some(program);
        self
    }

    /// A held fact set that is ALREADY a fixpoint (the runner's materialized
    /// `all_facts`): mark the relations whose derivation read an undecided
    /// rule guard, so a query over one reads unknown with the atoms that
    /// would decide it (dsl 0.24 T1-1).
    pub fn with_undecided(mut self, undecided: BTreeMap<String, Vec<UnresolvedAtom>>) -> Self {
        self.undecided = undecided;
        self
    }

    /// Record that the scene `id` has been presented in this save (dsl
    /// 0.21.0 §7a.1): `visited('<id>')` reads true from now on.
    pub fn visit(&mut self, id: &str) {
        self.visited.insert(id.to_string());
    }

    /// `visited('<id>')` — definite, closed-world over the presented set.
    pub fn visited(&self, id: &str) -> bool {
        self.visited.contains(id)
    }

    pub fn assert(&mut self, rel: &str, args: &[String]) {
        self.facts.insert((rel.to_string(), args.to_vec()));
    }

    /// `_` wildcard positions retract every fact matching the GROUND
    /// positions, regardless of what occupies a wildcard slot.
    pub fn retract(&mut self, rel: &str, pattern: &[Pat]) {
        self.facts
            .retain(|(r, args)| !(r == rel && pattern_matches(pattern, args)));
    }

    /// The held (base) facts: mocks, seeds and walk deltas, before
    /// derivation.
    pub fn base(&self) -> &BTreeSet<(String, Vec<String>)> {
        &self.facts
    }

    /// Derived relations a query read without derivation, sorted.
    pub fn derived_reads(&self) -> BTreeSet<String> {
        self.derived_reads.borrow().clone()
    }

    /// Every fact that holds over `state`, rendered `rel(a, b)`: the
    /// fixpoint of the held facts under derivation, else the held facts —
    /// plus every derived relation whose derivation read undecided state
    /// (a fact of it neither holds nor fails to hold).
    pub fn holding(&self, state: &EffectiveState<'_>) -> (BTreeSet<String>, BTreeSet<String>) {
        let render = |(rel, args): &(String, Vec<String>)| format!("{rel}({})", args.join(", "));
        match self.derivation {
            Some(program) if !program.is_empty() => {
                let closure = program.fixpoint(&self.facts, state);
                (
                    closure.facts.iter().map(render).collect(),
                    closure.undecided.keys().cloned().collect(),
                )
            }
            _ => (self.facts.iter().map(render).collect(), BTreeSet::new()),
        }
    }

    /// dsl 0.25.0 §1: every pair of facts holding over `state` (the fixpoint
    /// under derivation, else the held facts) whose relations `excludes:`
    /// each other, rendered `seenAfter(elias) and fell(elias) both hold`.
    pub fn exclusive_violations(&self, state: &EffectiveState<'_>) -> Vec<String> {
        let names = || self.rel_vocab.relations.keys().map(String::as_str);
        let pairs: Vec<(&str, &str)> = names()
            .flat_map(|a| {
                names()
                    .filter(move |b| a < *b && self.rel_vocab.excludes(a, b))
                    .map(move |b| (a, b))
            })
            .collect();
        if pairs.is_empty() {
            return Vec::new();
        }
        let closure;
        let facts = match self.derivation {
            Some(program) if !program.is_empty() => {
                closure = program.fixpoint(&self.facts, state).facts;
                &closure
            }
            _ => &self.facts,
        };
        let render = |rel: &str, args: &[String]| format!("{rel}({})", args.join(", "));
        let mut out = Vec::new();
        for (a, b) in pairs {
            for (_, args) in facts.iter().filter(|(rel, _)| rel == a) {
                if facts.contains(&(b.to_string(), args.clone())) {
                    out.push(format!(
                        "{} and {} both hold",
                        render(a, args),
                        render(b, args)
                    ));
                }
            }
        }
        out
    }

    fn is_derived(&self, rel: &str) -> bool {
        self.rel_vocab
            .relations
            .get(rel)
            .map(|d| d.derive)
            .unwrap_or(false)
    }

    /// Matching facts, or — with `column` — the number of DISTINCT values at
    /// that argument position among them (`countDistinct`, dsl 0.24 T3-9).
    fn scan<'f>(
        facts: impl IntoIterator<Item = &'f (String, Vec<String>)>,
        rel: &str,
        pattern: &[Pat],
        column: Option<usize>,
    ) -> usize {
        let hits = facts
            .into_iter()
            .filter(|(r, args)| r == rel && pattern_matches(pattern, args));
        match column {
            None => hits.count(),
            Some(i) => hits
                .filter_map(|(_, args)| args.get(i))
                .collect::<BTreeSet<_>>()
                .len(),
        }
    }

    /// How many facts match `rel(pattern)` (§4.3: ground positions match,
    /// `_` existential), or why that is unknown. With derivation the count
    /// is over the fixpoint — unknown only when a rule guard feeding `rel`
    /// read undecided state. Without it, a derived relation with zero
    /// matching held facts is unknown (the 0.21 lookup model).
    pub fn lookup(
        &self,
        rel: &str,
        pattern: &[Pat],
        state: &EffectiveState<'_>,
    ) -> Result<usize, Vec<UnresolvedAtom>> {
        self.lookup_distinct(rel, pattern, None, state)
    }

    /// [`FactStore::lookup`], counting distinct values at `column` when
    /// given (`countDistinct(rel(…), V)`, dsl 0.24 T3-9).
    pub fn lookup_distinct(
        &self,
        rel: &str,
        pattern: &[Pat],
        column: Option<usize>,
        state: &EffectiveState<'_>,
    ) -> Result<usize, Vec<UnresolvedAtom>> {
        if let Some(atoms) = self.undecided.get(rel) {
            return Err(atoms.clone());
        }
        match self.derivation {
            Some(program) if !program.is_empty() => {
                let closure = program.fixpoint(&self.facts, state);
                if let Some(atoms) = closure.undecided.get(rel) {
                    return Err(atoms.clone());
                }
                Ok(Self::scan(&closure.facts, rel, pattern, column))
            }
            Some(_) => Ok(Self::scan(&self.facts, rel, pattern, column)),
            None => {
                let n = Self::scan(&self.facts, rel, pattern, column);
                if self.is_derived(rel) {
                    self.derived_reads.borrow_mut().insert(rel.to_string());
                    if n == 0 {
                        return Err(vec![UnresolvedAtom::DerivedFact(render_pattern(
                            rel, pattern,
                        ))]);
                    }
                }
                Ok(n)
            }
        }
    }
}

/// Everything [`eval`] reads from: the effective state and the fact store.
pub struct EvalEnv<'a> {
    pub state: &'a EffectiveState<'a>,
    pub facts: &'a FactStore<'a>,
}

pub(crate) fn literal_to_value(l: &Literal) -> Value {
    match l {
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Num(n) => Value::Num(*n),
        Literal::Str(s) => Value::Str(s.clone()),
        // A scalar `state:` default is bool/number/string/enum (dsl §9.3);
        // `List`/`Map` never occur here in the closed evaluated subset.
        Literal::List(_) | Literal::Map(_) => Value::Unknown,
    }
}

fn val_to_value(v: &Val) -> Value {
    match v {
        Val::Boolean(b) => Value::Bool(*b),
        Val::Int(i) => Value::Num(*i as f64),
        Val::UInt(u) => Value::Num(*u as f64),
        Val::Double(d) => Value::Num(*d),
        Val::String(s) => Value::Str(s.clone()),
        // Outside the closed Lute-CEL profile (dsl §8.4) — never produced by
        // a document that passed `check` (trace refuses check errors, §4.3).
        Val::Null | Val::Bytes(_) => Value::Unknown,
    }
}

fn to_decided(v: Value) -> Option<Decided> {
    match v {
        Value::Bool(b) => Some(Decided::Bool(b)),
        Value::Num(n) => Some(Decided::Num(n)),
        Value::Str(s) => Some(Decided::Str(s)),
        Value::Unknown => None,
    }
}

/// A pure `Ident`/`Select` chain rendered as a dotted state path
/// (`scene.x.y`), mirroring `lute_check::cel_paths::select_path` — that
/// helper is `pub(crate)` to `lute-check`, so `lute-trace` (structurally
/// isolated, D1 rule 4) carries its own copy.
pub(crate) fn expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Select(sel) => Some(format!("{}.{}", expr_path(&sel.operand.expr)?, sel.field)),
        _ => None,
    }
}

/// `holds`/`count`'s pattern `Call` args → [`Pat`]s: `Ident("_")` is the
/// existential wildcard, any other bare `Ident` a ground id, a boolean
/// literal its `to_string()`. Mirrors `lute_check::cel_resolve`'s
/// (`pattern_terms`, private to that crate) shape; `None` means a
/// non-ground arg slipped through — defensive, unreachable against a
/// document `trace` actually accepted (§4.3: trace refuses documents with
/// check errors).
fn pattern_args(c: &CallExpr) -> Option<Vec<Pat>> {
    c.args
        .iter()
        .map(|a| match &a.expr {
            Expr::Ident(name) if name == "_" => Some(Pat::Wildcard),
            Expr::Ident(name) => Some(Pat::Ground(name.clone())),
            Expr::Literal(Val::Boolean(b)) => Some(Pat::Ground(b.to_string())),
            _ => None,
        })
        .collect()
}

pub(crate) fn eval_path_read(
    path: &str,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    match env.state.read(path) {
        Read::Value(v) => {
            if v == Value::Unknown {
                unresolved.push(UnresolvedAtom::Path(path.to_string()));
            }
            v
        }
        Read::Unset => {
            unresolved.push(UnresolvedAtom::Path(path.to_string()));
            Value::Unknown
        }
    }
}

/// K3 `&&`: `false && x = false` — `x` is never evaluated, so no atom is
/// recorded for a branch the connective didn't need to know. Otherwise both
/// sides are evaluated and combined: `true && true = true`, a `false` on
/// either side wins, else unknown (`U && true = U`).
fn eval_and(
    a: &IdedExpr,
    b: &IdedExpr,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let va = eval(&a.expr, env, unresolved);
    if va == Value::Bool(false) {
        return Value::Bool(false);
    }
    let vb = eval(&b.expr, env, unresolved);
    match (va, vb) {
        (Value::Bool(true), Value::Bool(true)) => Value::Bool(true),
        (Value::Bool(false), _) | (_, Value::Bool(false)) => Value::Bool(false),
        _ => Value::Unknown,
    }
}

/// K3 `||`: `true || x = true` (short-circuit, symmetric to [`eval_and`]);
/// otherwise `false || false = false`, a `true` on either side wins, else
/// unknown.
fn eval_or(
    a: &IdedExpr,
    b: &IdedExpr,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let va = eval(&a.expr, env, unresolved);
    if va == Value::Bool(true) {
        return Value::Bool(true);
    }
    let vb = eval(&b.expr, env, unresolved);
    match (va, vb) {
        (Value::Bool(false), Value::Bool(false)) => Value::Bool(false),
        (Value::Bool(true), _) | (_, Value::Bool(true)) => Value::Bool(true),
        _ => Value::Unknown,
    }
}

/// `?:` — an unknown CONDITION is unknown (never guesses a branch, mirrors
/// `lute_check::decide`'s ternary rule); a decided condition evaluates
/// exactly the taken branch, so the untaken side never contributes an atom.
fn eval_conditional(
    cond: &IdedExpr,
    then: &IdedExpr,
    els: &IdedExpr,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    match eval(&cond.expr, env, unresolved) {
        Value::Bool(true) => eval(&then.expr, env, unresolved),
        Value::Bool(false) => eval(&els.expr, env, unresolved),
        _ => Value::Unknown,
    }
}

/// Ordinary R3 ground unary/binary operators (comparison, arithmetic,
/// equality) — ANY unknown operand makes the whole node unknown. Every
/// operand is still evaluated (even once the result is known-unknown) so
/// every atom that could resolve it lands in `unresolved`.
fn eval_ground(
    name: &str,
    args: &[&IdedExpr],
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let values: Vec<Value> = args
        .iter()
        .map(|a| eval(&a.expr, env, unresolved))
        .collect();
    if values.contains(&Value::Unknown) {
        return Value::Unknown;
    }
    let decided: Option<Vec<Decided>> = values.into_iter().map(to_decided).collect();
    decided
        .and_then(|d| lute_check::apply_op(name, &d))
        .map(Value::from)
        .unwrap_or(Value::Unknown)
}

/// `in` over a list LITERAL (the only in-profile shape, dsl §8.4): every
/// element is evaluated (so an unknown list member's atom is still
/// reported); a non-list right side is out of profile — defensive,
/// unreachable post-check.
fn eval_in(
    needle: &IdedExpr,
    list: &IdedExpr,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let Expr::List(elements) = &list.expr else {
        return Value::Unknown;
    };
    let mut idents: Vec<&IdedExpr> = Vec::with_capacity(elements.elements.len() + 1);
    idents.push(needle);
    idents.extend(elements.elements.iter());
    eval_ground(op::IN, &idents, env, unresolved)
}

/// `list[index]` over a list LITERAL (dsl 0.4.0 §4.3: "indexing" is
/// explicitly in the evaluated subset, alongside `in` over list literals —
/// the same list-literal restriction the CEL profile enforces for `in`,
/// `cel_resolve.rs::is_profile_operator`). The index is evaluated first (so
/// an unknown index still records its atom); a non-list target, a
/// non-decided/non-numeric/non-integer index, or an out-of-range index is
/// `Unknown` — never a panic, never a guess.
fn eval_index(
    target: &IdedExpr,
    index: &IdedExpr,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let Expr::List(elements) = &target.expr else {
        return Value::Unknown;
    };
    let idx = match eval(&index.expr, env, unresolved) {
        Value::Num(n) if n.fract() == 0.0 && n >= 0.0 => n as usize,
        _ => return Value::Unknown,
    };
    match elements.elements.get(idx) {
        Some(el) => eval(&el.expr, env, unresolved),
        None => Value::Unknown,
    }
}

/// `holds(pattern)` / `count(pattern)` (§4.3): looks the pattern up via
/// [`FactStore::lookup`] — over the Datalog fixpoint under `derive: true`
/// (dsl 0.22.0 §6), a bounded scan otherwise. An unknown answer records
/// the atoms that would decide it (for an unmatched derived relation
/// without derivation, the rendered pattern as the "supply it as a mock"
/// hint, §4.6). With `column` (`countDistinct`, dsl 0.24 T3-9) that
/// position is a wildcard and the answer counts its distinct values.
fn eval_fact_query(
    kind: &str,
    pattern: &IdedExpr,
    column: Option<usize>,
    env: &EvalEnv<'_>,
    unresolved: &mut Vec<UnresolvedAtom>,
) -> Value {
    let Expr::Call(pat_call) = &pattern.expr else {
        return Value::Unknown; // caller guarantees this; defensive fallback
    };
    let relation = pat_call.func_name.as_str();
    let Some(mut pats) = pattern_args(pat_call) else {
        return Value::Unknown; // non-ground pattern; defensive, unreachable post-check
    };
    if let Some(slot) = column.and_then(|i| pats.get_mut(i)) {
        *slot = Pat::Wildcard;
    }
    match env
        .facts
        .lookup_distinct(relation, &pats, column, env.state)
    {
        Ok(n) if kind == "holds" => Value::Bool(n > 0),
        Ok(n) => Value::Num(n as f64),
        Err(atoms) => {
            unresolved.extend(atoms);
            Value::Unknown
        }
    }
}

/// `countDistinct(rel(…, V, …), V)`: the one pattern position the variable
/// names (dsl 0.24 T3-9; the checker admits nothing else).
fn distinct_column(pattern: &IdedExpr, var: &IdedExpr) -> Option<usize> {
    let (Expr::Call(p), Expr::Ident(v)) = (&pattern.expr, &var.expr) else {
        return None;
    };
    p.args
        .iter()
        .position(|a| matches!(&a.expr, Expr::Ident(n) if n == v))
}

/// `isSet(<path>)`/`has(<path>)` are DEFINITE (D19): true iff an effective
/// value exists (write → seed → default), false on unset — never unknown,
/// so no atom is ever recorded here.
fn eval_definite_presence(path: &str, env: &EvalEnv<'_>) -> Value {
    Value::Bool(!matches!(env.state.read(path), Read::Unset))
}

fn eval_call(c: &CallExpr, env: &EvalEnv<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    match (c.func_name.as_str(), c.args.as_slice()) {
        (op::LOGICAL_NOT, [a]) => eval_ground(op::LOGICAL_NOT, &[a], env, unresolved),
        (op::NEGATE, [a]) => eval_ground(op::NEGATE, &[a], env, unresolved),
        (op::LOGICAL_AND, [a, b]) => eval_and(a, b, env, unresolved),
        (op::LOGICAL_OR, [a, b]) => eval_or(a, b, env, unresolved),
        (op::CONDITIONAL, [c0, t, e]) => eval_conditional(c0, t, e, env, unresolved),
        (op::IN, [needle, list]) => eval_in(needle, list, env, unresolved),
        (op::INDEX, [target, idx]) => eval_index(target, idx, env, unresolved),
        (op::ADD, [a, b])
        | (op::SUBSTRACT, [a, b])
        | (op::MULTIPLY, [a, b])
        | (op::DIVIDE, [a, b])
        | (op::MODULO, [a, b])
        | (op::GREATER, [a, b])
        | (op::GREATER_EQUALS, [a, b])
        | (op::LESS, [a, b])
        | (op::LESS_EQUALS, [a, b])
        | (op::EQUALS, [a, b])
        | (op::NOT_EQUALS, [a, b]) => eval_ground(c.func_name.as_str(), &[a, b], env, unresolved),
        ("holds", [pattern]) | ("count", [pattern]) if matches!(pattern.expr, Expr::Call(_)) => {
            eval_fact_query(c.func_name.as_str(), pattern, None, env, unresolved)
        }
        ("countDistinct", [pattern, var]) => match distinct_column(pattern, var) {
            Some(col) => eval_fact_query("countDistinct", pattern, Some(col), env, unresolved),
            None => Value::Unknown, // ill-shaped; defensive, unreachable post-check
        },
        // dsl 0.21.0 §7a.1: one string-literal scene id, definite (never an
        // unresolved atom — the presented set is the mock).
        ("visited", [arg]) => match &arg.expr {
            Expr::Literal(Val::String(id)) => Value::Bool(env.facts.visited(id)),
            _ => Value::Unknown, // non-literal arg; defensive, unreachable post-check
        },
        ("validAt", [pattern, _]) if matches!(pattern.expr, Expr::Call(_)) => {
            unresolved.push(UnresolvedAtom::Time);
            Value::Unknown
        }
        ("now", []) => {
            unresolved.push(UnresolvedAtom::Time);
            Value::Unknown
        }
        (name, [arg]) if name.eq_ignore_ascii_case("isSet") => match expr_path(&arg.expr) {
            Some(path) => eval_definite_presence(&path, env),
            None => Value::Unknown, // malformed; defensive, unreachable post-check
        },
        // Out of the closed profile (dsl §8.4) — never reached by a document
        // that passed `check` (trace refuses documents with check errors).
        _ => Value::Unknown,
    }
}

/// K3 evaluator over the post-expansion CEL AST (slots are `@`/`$`-free,
/// D14). Ground ops delegate to [`lute_check::apply_op`] (D3) lifted over
/// [`Value::Unknown`]. `isSet`/`has` are definite (D19). `holds`/`count`
/// run through [`FactStore`]. `now()`/`validAt(...)` are always unknown.
/// Every `Unknown` this function produces records its [`UnresolvedAtom`].
pub fn eval(expr: &Expr, env: &EvalEnv<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    match expr {
        Expr::Literal(v) => val_to_value(v),
        // A bare path root — never produced by a validated document's
        // multi-segment state paths; kept for totality (a stray `Ident`
        // reads as an unset path, same as any other unknown path).
        Expr::Ident(name) => eval_path_read(name, env, unresolved),
        Expr::Select(sel) => match expr_path(expr) {
            Some(path) if sel.test => eval_definite_presence(&path, env), // has()
            Some(path) => eval_path_read(&path, env, unresolved),
            None => Value::Unknown, // defensive; unreachable post-check
        },
        Expr::Call(c) => eval_call(c, env, unresolved),
        // Out of the closed evaluated subset (§4.3) — never produced by a
        // document that passed `check`.
        Expr::List(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Comprehension(_)
        | Expr::Unspecified => Value::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_check::meta::{Namespace, StateDecl};
    use lute_manifest::relations::RelationDecl;
    use lute_manifest::types::Type;

    fn parse(raw: &str) -> Expr {
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot(&mut arena, raw, 0).expect("valid test CEL");
        arena.get(handle).expect("handle in arena").expr.clone()
    }

    fn eval_str(raw: &str, env: &EvalEnv<'_>) -> (Value, Vec<UnresolvedAtom>) {
        let expr = parse(raw);
        let mut unresolved = Vec::new();
        let v = eval(&expr, env, &mut unresolved);
        (v, unresolved)
    }

    fn schema_with(decls: &[(&str, Type, Option<Literal>)]) -> StateSchema {
        let mut s = StateSchema::default();
        for (path, ty, default) in decls {
            s.decls.insert(
                path.to_string(),
                StateDecl {
                    ty: ty.clone(),
                    default: default.clone(),
                    namespace: Namespace::Run,
                    owner: None,
                },
            );
        }
        s
    }

    fn rel_vocab_with(rels: &[(&str, bool)]) -> RelVocab {
        let mut v = RelVocab::default();
        for (name, derive) in rels {
            v.relations.insert(
                name.to_string(),
                RelationDecl {
                    args: vec!["entity".to_string(); 3],
                    derive: *derive,
                    ..Default::default()
                },
            );
        }
        v
    }

    // -- K3 propagation --------------------------------------------------

    #[test]
    fn k3_false_and_unknown_is_false() {
        let schema = schema_with(&[]); // run.unseen has no seed/default -> Unset
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("false && run.unseen", &env);
        assert_eq!(v, Value::Bool(false));
        // Short-circuit: the unknown right side is never evaluated, so it
        // never contributes an atom to the (already fully decided) result.
        assert!(
            unresolved.is_empty(),
            "short-circuit must not record {unresolved:?}"
        );
    }

    #[test]
    fn k3_true_or_unknown_is_true() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("true || run.unseen", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(
            unresolved.is_empty(),
            "short-circuit must not record {unresolved:?}"
        );
    }

    #[test]
    fn k3_unknown_and_true_is_unknown() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("run.unseen && true", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.unseen".to_string())]
        );
    }

    #[test]
    fn k3_comparison_with_unknown_operand_is_unknown() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("1 > run.unseen", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.unseen".to_string())]
        );
    }

    #[test]
    fn k3_ternary_with_unknown_condition_is_unknown_and_skips_both_branches() {
        let schema = schema_with(&[("run.cond", Type::Bool, None)]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        // `run.other` would itself be unknown too, but must never be read
        // (and thus never recorded) since the condition alone decides U.
        let (v, unresolved) = eval_str("run.cond ? 1 : run.other", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.cond".to_string())]
        );
    }

    #[test]
    fn k3_ternary_with_decided_condition_evaluates_only_taken_branch() {
        let mut seed = BTreeMap::new();
        seed.insert("run.cond".to_string(), Value::Bool(true));
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, seed);
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("run.cond ? 1 : run.untouched", &env);
        assert_eq!(v, Value::Num(1.0));
        assert!(unresolved.is_empty());
    }

    #[test]
    fn k3_ground_ops_still_report_all_unknown_operands() {
        // Unlike && / ||, a plain arithmetic/comparison node has no
        // short-circuit: both unknown operands must show up in unresolved.
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("run.a + run.b", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![
                UnresolvedAtom::Path("run.a".to_string()),
                UnresolvedAtom::Path("run.b".to_string())
            ]
        );
    }

    // -- Indexing (dsl 0.4.0 §4.3: "indexing" is in the evaluated subset) --

    #[test]
    fn index_into_list_literal_with_concrete_int_returns_element() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("[10, 20, 30][1]", &env);
        assert_eq!(v, Value::Num(20.0));
        assert!(unresolved.is_empty());
    }

    #[test]
    fn index_out_of_range_is_unknown() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, _unresolved) = eval_str("[10, 20][5]", &env);
        assert_eq!(v, Value::Unknown);
    }

    #[test]
    fn index_with_unknown_index_is_unknown_and_records_its_atom() {
        let schema = schema_with(&[]); // run.idx unset -> Unknown
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("[10, 20][run.idx]", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.idx".to_string())]
        );
    }

    #[test]
    fn index_a_check_clean_guard_decides_concretely_not_unknown() {
        // Mirrors the finding: a document that passed `check` (INDEX is
        // admitted by the closed profile, cel_resolve.rs) must not fall
        // through to Unknown/Incomplete at trace time when every operand
        // is decided.
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("[10, 20, 30][1] == 20", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());
    }

    // -- Effective-state precedence --------------------------------------

    #[test]
    fn effective_state_precedence_write_beats_seed_beats_default_beats_unset() {
        let schema = schema_with(&[("run.tip", Type::Number, Some(Literal::Num(1.0)))]);
        let mut seed = BTreeMap::new();
        seed.insert("run.tip".to_string(), Value::Num(2.0));
        let mut state = EffectiveState::new(&schema, seed);

        // default only
        let schema_no_seed = schema_with(&[("run.other", Type::Number, Some(Literal::Num(9.0)))]);
        let state_default = EffectiveState::new(&schema_no_seed, BTreeMap::new());
        assert_eq!(
            state_default.read("run.other"),
            Read::Value(Value::Num(9.0))
        );

        // seed beats default
        assert_eq!(state.read("run.tip"), Read::Value(Value::Num(2.0)));

        // write beats seed
        state.write("run.tip", Value::Num(3.0));
        assert_eq!(state.read("run.tip"), Read::Value(Value::Num(3.0)));

        // nothing at all -> Unset
        assert_eq!(state.read("run.neverDeclared"), Read::Unset);
    }

    #[test]
    fn write_of_unknown_marks_path_unknown_but_still_present_for_isset() {
        let schema = schema_with(&[]);
        let mut state = EffectiveState::new(&schema, BTreeMap::new());
        state.write("run.tip", Value::Unknown);
        assert_eq!(state.read("run.tip"), Read::Value(Value::Unknown));
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        // D19: isSet is definite presence, true even though the VALUE at
        // that path is unknown.
        let (v, unresolved) = eval_str("isSet(run.tip)", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());
        // A plain value read of that same path IS unknown, and records it.
        let (v, unresolved) = eval_str("run.tip", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.tip".to_string())]
        );
    }

    // -- D19: isSet()/has() are definite ---------------------------------

    #[test]
    fn isset_is_definite_true_when_seeded_false_when_unset() {
        let mut seed = BTreeMap::new();
        seed.insert("run.tip".to_string(), Value::Num(5.0));
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, seed);
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("isSet(run.tip)", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());

        let (v, unresolved) = eval_str("isSet(run.neverDeclared)", &env);
        assert_eq!(v, Value::Bool(false));
        assert!(unresolved.is_empty());
    }

    #[test]
    fn has_macro_is_definite_like_isset() {
        let mut seed = BTreeMap::new();
        seed.insert("run.tip".to_string(), Value::Num(5.0));
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, seed);
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("has(run.tip)", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());

        let (v, unresolved) = eval_str("has(run.neverDeclared)", &env);
        assert_eq!(v, Value::Bool(false));
        assert!(unresolved.is_empty());
    }

    #[test]
    fn isset_true_never_reports_the_underlying_value_as_unknown() {
        // !isSet(run.x) must decide on a fresh mock world (D19 interpretation
        // note): the VALUE read is what is unknown, presence never is.
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("!isSet(run.fresh)", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());
    }

    // -- holds()/count() over ground + `_` patterns -----------------------

    #[test]
    fn holds_over_ground_pattern_matches_exact_fact() {
        let vocab = rel_vocab_with(&[("inParty", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("inParty", &["elena".to_string(), "grove".to_string()]);
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("holds(inParty(elena, grove))", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());

        let (v, unresolved) = eval_str("holds(inParty(elena, town))", &env);
        assert_eq!(v, Value::Bool(false));
        assert!(unresolved.is_empty());
    }

    #[test]
    fn holds_over_wildcard_pattern_is_existential_over_supplied_set() {
        let vocab = rel_vocab_with(&[("inParty", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("inParty", &["elena".to_string(), "grove".to_string()]);
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("holds(inParty(elena, _))", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());

        let (v, unresolved) = eval_str("count(inParty(_, _))", &env);
        assert_eq!(v, Value::Num(1.0));
        assert!(unresolved.is_empty());
    }

    // -- derived-unless-supplied -------------------------------------------

    #[test]
    fn derived_relation_unsupplied_is_unknown_with_derived_fact_atom() {
        let vocab = rel_vocab_with(&[("believesLocation", true)]);
        let facts = FactStore::new(&vocab);
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("holds(believesLocation(player, halsin, grove))", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::DerivedFact(
                "believesLocation(player, halsin, grove)".to_string()
            )]
        );
    }

    #[test]
    fn derived_relation_supplied_as_mock_decides_true() {
        // §4.6: the writer previews the derivation's CONSEQUENCE by mocking
        // its output — the rules are never run, the supplied fact just wins
        // the bounded scan.
        let vocab = rel_vocab_with(&[("believesLocation", true)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert(
            "believesLocation",
            &[
                "player".to_string(),
                "halsin".to_string(),
                "grove".to_string(),
            ],
        );
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("holds(believesLocation(player, halsin, grove))", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());
    }

    // -- non-derived closed-world -------------------------------------------

    #[test]
    fn non_derived_relation_absent_fact_is_definitely_false() {
        let vocab = rel_vocab_with(&[("inParty", false)]);
        let facts = FactStore::new(&vocab); // nothing asserted at all
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("holds(inParty(elena, grove))", &env);
        assert_eq!(v, Value::Bool(false));
        assert!(unresolved.is_empty());
        assert_eq!(
            facts.lookup("inParty", &[Pat::Wildcard, Pat::Wildcard], &state),
            Ok(0)
        );
    }

    // -- retract -------------------------------------------------------------

    #[test]
    fn retract_removes_matching_ground_facts() {
        let vocab = rel_vocab_with(&[("inParty", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("inParty", &["elena".to_string(), "grove".to_string()]);
        facts.assert("inParty", &["gale".to_string(), "grove".to_string()]);
        facts.retract(
            "inParty",
            &[Pat::Ground("elena".to_string()), Pat::Wildcard],
        );
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(
            facts.lookup(
                "inParty",
                &[Pat::Ground("elena".to_string()), Pat::Wildcard],
                &state
            ),
            Ok(0)
        );
        assert_eq!(
            facts.lookup(
                "inParty",
                &[Pat::Ground("gale".to_string()), Pat::Wildcard],
                &state
            ),
            Ok(1)
        );
    }

    // -- now()/validAt() ------------------------------------------------------

    #[test]
    fn now_and_valid_at_are_always_unknown_with_time_atom() {
        let vocab = rel_vocab_with(&[("inParty", false)]);
        let facts = FactStore::new(&vocab);
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };

        let (v, unresolved) = eval_str("now()", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(unresolved, vec![UnresolvedAtom::Time]);

        let (v, unresolved) = eval_str("validAt(inParty(elena, grove), now())", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(unresolved, vec![UnresolvedAtom::Time]);
    }

    // -- reserved quest paths (dsl 0.4.0 §4.4's `quest.<id>.state`/
    //    `…objectives.*.done` exception; dsl 0.5.1 §1.2's default fix) ---

    #[test]
    fn reserved_quest_objective_done_defaults_to_false_despite_a_schema_default_decl() {
        // `match_check.rs::check_quest` folds every `quest.<id>.objectives.
        // <oid>.done` decl with `default: Some(false)` for the REAL engine's
        // benefit — trace bypasses that SCHEMA decl tier entirely (§4.4:
        // "trace derives them from its own walk") but, since 0.5.1 §1.2,
        // resolves to its OWN reserved default (`false`) rather than bare
        // `Read::Unset` — the fix for 0.5.0's "no arm" defect.
        let schema = schema_with(&[(
            "quest.rescueHalsin.objectives.reach.done",
            Type::Bool,
            Some(Literal::Bool(false)),
        )]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(
            state.read("quest.rescueHalsin.objectives.reach.done"),
            Read::Value(Value::Bool(false))
        );

        // Same for `quest.<id>.state` (no schema default at all in
        // practice, but the reserved default must win even if one were
        // present) — its reserved default is the literal `"unset"`.
        let schema2 = schema_with(&[(
            "quest.rescueHalsin.state",
            Type::Str,
            Some(Literal::Str("active".to_string())),
        )]);
        let state2 = EffectiveState::new(&schema2, BTreeMap::new());
        assert_eq!(
            state2.read("quest.rescueHalsin.state"),
            Read::Value(Value::Str("unset".to_string()))
        );
    }

    #[test]
    fn reserved_quest_path_default_logs_a_defaulted_reserved_read() {
        // §1.3's note surface: an un-mocked reserved read must log itself
        // as `Defaulted` (never silently untracked) so `crate::walk` can
        // attach the "existence unverified" note.
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(
            state.read("quest.foo.state"),
            Read::Value(Value::Str("unset".to_string()))
        );
        let log = state.reserved_reads();
        assert_eq!(
            log.get("quest.foo.state"),
            Some(&ReservedReadKind::Defaulted)
        );
    }

    #[test]
    fn reserved_quest_path_seed_logs_a_mocked_reserved_read() {
        // §1.1's admitted `--state` mock surface: a seeded reserved path
        // must log itself as `Mocked`, distinct from a defaulted one.
        let schema = schema_with(&[]);
        let mut seed = BTreeMap::new();
        seed.insert(
            "quest.foo.state".to_string(),
            Value::Str("complete".to_string()),
        );
        let state = EffectiveState::new(&schema, seed);
        assert_eq!(
            state.read("quest.foo.state"),
            Read::Value(Value::Str("complete".to_string()))
        );
        let log = state.reserved_reads();
        assert_eq!(log.get("quest.foo.state"), Some(&ReservedReadKind::Mocked));
    }

    #[test]
    fn reserved_quest_path_write_is_never_logged() {
        // A path this SAME walk derived (written) is never "foreign" —
        // §1.3's note is scoped to mocked/defaulted reads only.
        let schema = schema_with(&[]);
        let mut state = EffectiveState::new(&schema, BTreeMap::new());
        state.write("quest.foo.state", Value::Str("active".to_string()));
        assert_eq!(
            state.read("quest.foo.state"),
            Read::Value(Value::Str("active".to_string()))
        );
        assert!(state.reserved_reads().is_empty());
    }

    #[test]
    fn ordinary_unset_path_still_reports_read_unset() {
        // A NON-reserved path with no seed/default/write is still bare
        // `Read::Unset` — the §1.2 defaulting is `is_reserved_quest_path`-
        // gated, never a blanket behavior change.
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(state.read("run.neverDeclared"), Read::Unset);
        assert!(state.reserved_reads().is_empty());
    }

    #[test]
    fn reserved_quest_path_bypass_is_narrowly_scoped() {
        // An ORDINARY path with the identical shape-adjacent name must
        // still use its schema default — the bypass is `is_reserved_quest_path`-
        // gated, never a blanket "quest.*" skip.
        let schema = schema_with(&[(
            "quest.rescueHalsin.objectives.reach.notDone",
            Type::Bool,
            Some(Literal::Bool(true)),
        )]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(
            state.read("quest.rescueHalsin.objectives.reach.notDone"),
            Read::Value(Value::Bool(true))
        );

        // A once-decided objective still overrides via a trace WRITE (the
        // bypass only removes the `default:` tier, never `writes`/`seed`).
        let schema2 = schema_with(&[(
            "quest.q.objectives.o.done",
            Type::Bool,
            Some(Literal::Bool(false)),
        )]);
        let mut state2 = EffectiveState::new(&schema2, BTreeMap::new());
        state2.write("quest.q.objectives.o.done", Value::Bool(true));
        assert_eq!(
            state2.read("quest.q.objectives.o.done"),
            Read::Value(Value::Bool(true))
        );
    }

    /// dsl 0.24.0 §1: integer `%` evaluates (truncated remainder); a
    /// fractional value or a zero divisor is unknown, never a float remainder.
    #[test]
    fn integer_modulo_evaluates() {
        let schema = schema_with(&[
            ("run.day", Type::Number, Some(Literal::Num(14.0))),
            ("run.half", Type::Number, Some(Literal::Num(2.5))),
        ]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        assert_eq!(eval_str("run.day % 7 == 0", &env).0, Value::Bool(true));
        assert_eq!(eval_str("(run.day + 1) % 7", &env).0, Value::Num(1.0));
        assert_eq!(eval_str("-7 % 3", &env).0, Value::Num(-1.0));
        assert_eq!(eval_str("run.half % 2", &env).0, Value::Unknown);
        assert_eq!(eval_str("run.day % 0", &env).0, Value::Unknown);
    }
}
