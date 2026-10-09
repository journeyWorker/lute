//! The K3 evaluation environment (dsl 0.4.0 §4.3): effective state and the
//! bounded fact scan an expression reads ([`EvalEnv`]); the expression
//! walker itself is `exec::expr`.
//!
//! `holds`/`count` read the supplied fact set — through the Datalog
//! fixpoint under `derive: true` (dsl 0.22.0 §6), a bounded scan otherwise.
//! `visited('<scene id>')` (dsl 0.21.0 §7a.1) is DEFINITE over the supplied
//! presented-scene set the [`FactStore`] carries — closed-world, like a
//! non-derived relation: an id absent from the set was not presented.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lute_manifest::Literal;

use crate::datalog::Program;
use crate::schema::{RelVocab, StateSchema};
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
/// "existence unverified" note [`crate::trace`] attaches to every such read
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
/// the walk's live state.
#[derive(Clone)]
pub struct EffectiveState<'a> {
    schema: &'a StateSchema,
    seed: std::borrow::Cow<'a, BTreeMap<String, Value>>,
    writes: BTreeMap<String, Value>,
    reserved_reads: Rc<RefCell<BTreeMap<String, ReservedReadKind>>>,
    read_log: Option<Rc<RefCell<Vec<(String, Read)>>>>,
}

impl<'a> EffectiveState<'a> {
    /// `seed` is the mock-supplied `--state`/`state:` surface (§4.3); trace
    /// writes accumulate separately via [`EffectiveState::write`] as the
    /// walk (Task 19) proceeds.
    pub fn new(schema: &'a StateSchema, seed: BTreeMap<String, Value>) -> Self {
        Self::with_seed(schema, std::borrow::Cow::Owned(seed))
    }

    /// [`Self::new`] over a seed the caller keeps: no copy of the map.
    pub fn over(schema: &'a StateSchema, seed: &'a BTreeMap<String, Value>) -> Self {
        Self::with_seed(schema, std::borrow::Cow::Borrowed(seed))
    }

    fn with_seed(schema: &'a StateSchema, seed: std::borrow::Cow<'a, BTreeMap<String, Value>>) -> Self {
        Self {
            schema,
            seed,
            writes: BTreeMap::new(),
            reserved_reads: Rc::new(RefCell::new(BTreeMap::new())),
            read_log: None,
        }
    }

    /// Enable the per-evaluation read log used by CEL observers. Ordinary
    /// execution leaves this disabled so reads do not allocate a path/value
    /// pair or a log entry.
    pub(crate) fn with_read_log(mut self) -> Self {
        self.read_log = Some(Rc::new(RefCell::new(Vec::new())));
        self
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
    /// `lute_manifest::semantics::cel_paths::is_reserved_quest_path` — `pub(crate)` to
    /// that crate, so not reusable across the D1 quarantine boundary
    /// ([`expr_path`] carries the same idiom below).
    pub fn read(&self, path: &str) -> Read {
        let result = self.read_inner(path);
        if let Some(read_log) = &self.read_log {
            read_log
                .borrow_mut()
                .push((path.to_string(), result.clone()));
        }
        result
    }

    fn read_inner(&self, path: &str) -> Read {
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
        if lute_manifest::semantics::cel_paths::is_reserved_entry_read(path) {
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

    pub fn reads(&self) -> Vec<(String, Read)> {
        self.read_log
            .as_ref()
            .map_or_else(Vec::new, |read_log| read_log.borrow().clone())
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
/// `lute_manifest::semantics::cel_paths::is_reserved_quest_path` (`pub(crate)` there, so
/// not reusable across the D1 quarantine boundary).
pub fn is_reserved_quest_path(path: &str) -> bool {
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
/// `lute_manifest::semantics::cel_paths::is_reserved_quest_objective_done` (`pub(crate)`
/// there too).
pub fn is_reserved_quest_objective_done_path(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "objectives", _, "done" | "failed"]
    )
}

/// dsl 0.24.0 §2: `quest.<id>.failedBy` — the reason enum of a failed quest.
pub fn is_reserved_quest_failed_by_path(path: &str) -> bool {
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
pub fn reserved_quest_default(path: &str) -> Value {
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
enum FactSet<'a> {
    Borrowed(&'a BTreeSet<(String, Vec<String>)>),
    Owned(BTreeSet<(String, Vec<String>)>),
}

impl<'a> FactSet<'a> {
    fn as_ref(&self) -> &BTreeSet<(String, Vec<String>)> {
        match self {
            Self::Borrowed(facts) => facts,
            Self::Owned(facts) => facts,
        }
    }

    fn as_mut(&mut self) -> &mut BTreeSet<(String, Vec<String>)> {
        if let Self::Borrowed(facts) = self {
            *self = Self::Owned((*facts).clone());
        }
        match self {
            Self::Borrowed(_) => unreachable!(),
            Self::Owned(facts) => facts,
        }
    }
}

#[derive(Clone)]
pub struct FactStore<'a> {
    facts: FactSet<'a>,
    rel_vocab: &'a RelVocab,
    visited: BTreeSet<String>,
    derivation: Option<&'a Program>,
    /// Derived relations queried without derivation — walk-global like
    /// [`EffectiveState`]'s reserved-read log, so shared across snapshots.
    derived_reads: Rc<RefCell<BTreeSet<String>>>,
    /// dsl 0.24 T1-1: derived relations whose materialized fixpoint read an
    /// undecided rule guard — a query over one reads unknown, never a silent
    /// "no such fact".
    undecided: BTreeMap<String, Vec<UnresolvedAtom>>,
}

impl<'a> FactStore<'a> {
    pub fn new(rel_vocab: &'a RelVocab) -> Self {
        Self {
            facts: FactSet::Owned(BTreeSet::new()),
            rel_vocab,
            visited: BTreeSet::new(),
            derivation: None,
            derived_reads: Rc::new(RefCell::new(BTreeSet::new())),
            undecided: BTreeMap::new(),
        }
    }

    /// Use the store's materialized fact set without rebuilding it for every
    /// CEL expression. Mutating methods still detach a private owned copy.
    pub fn with_facts(mut self, facts: &'a BTreeSet<(String, Vec<String>)>) -> Self {
        self.facts = FactSet::Borrowed(facts);
        self
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
        self.facts.as_mut().insert((rel.to_string(), args.to_vec()));
    }

    /// `_` wildcard positions retract every fact matching the GROUND
    /// positions, regardless of what occupies a wildcard slot.
    pub fn retract(&mut self, rel: &str, pattern: &[Pat]) {
        self.facts
            .as_mut()
            .retain(|(r, args)| !(r == rel && pattern_matches(pattern, args)));
    }

    /// The held (base) facts: mocks, seeds and walk deltas, before derivation.
    pub fn base(&self) -> &BTreeSet<(String, Vec<String>)> {
        self.facts.as_ref()
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
                let closure = program.fixpoint(self.facts.as_ref(), state);
                (
                    closure.facts.iter().map(render).collect(),
                    closure.undecided.keys().cloned().collect(),
                )
            }
            _ => (
                self.facts.as_ref().iter().map(render).collect(),
                BTreeSet::new(),
            ),
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
                closure = program.fixpoint(self.facts.as_ref(), state).facts;
                &closure
            }
            _ => self.facts.as_ref(),
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
        if let Some(atoms) = self.undecided.get(rel).filter(|atoms| !atoms.is_empty()) {
            return Err(atoms.clone());
        }
        match self.derivation {
            Some(program) if !program.is_empty() => {
                let closure = program.fixpoint(self.facts.as_ref(), state);
                if let Some(atoms) = closure.undecided.get(rel).filter(|atoms| !atoms.is_empty()) {
                    return Err(atoms.clone());
                }
                Ok(Self::scan(&closure.facts, rel, pattern, column))
            }
            Some(_) => Ok(Self::scan(self.facts.as_ref(), rel, pattern, column)),
            None => {
                let n = Self::scan(self.facts.as_ref(), rel, pattern, column);
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

/// Everything an expression reads from: the effective state and the fact
/// store.
pub struct EvalEnv<'a> {
    pub state: &'a EffectiveState<'a>,
    pub facts: &'a FactStore<'a>,
}

pub(crate) fn literal_to_value(l: &Literal) -> Value {
    match l {
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Int(n) => Value::Int(*n),
        Literal::Double(n) => Value::Double(*n),
        Literal::Str(s) => Value::Str(s.clone()),
        Literal::List(_) | Literal::Map(_) => Value::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::StateDecl;
    use lute_manifest::relations::RelationDecl;
    use lute_manifest::types::Type;

    /// Lower `raw` exactly as `lute compile` does and evaluate its `expr`.
    fn eval_str(raw: &str, env: &EvalEnv<'_>) -> (Value, Vec<UnresolvedAtom>) {
        let lowered = lute_compile::expr::lower_expr(raw).expect("CEL inside the profile");
        let expr = crate::expr::Expr::decode(&serde_json::to_value(lowered).unwrap());
        let mut unresolved = Vec::new();
        let v = crate::expr::eval(&expr, env, None, &mut unresolved);
        (v, unresolved)
    }

    fn schema_with(decls: &[(&str, Type, Option<Literal>)]) -> StateSchema {
        let mut s = StateSchema::default();
        for (path, _, default) in decls {
            s.decls.insert(
                path.to_string(),
                StateDecl {
                    default: default.clone(),
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
        assert_eq!(v, Value::Int(1));
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
        assert_eq!(v, Value::Int(20));
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
        let schema = schema_with(&[("run.tip", Type::Double, Some(Literal::Double(1.0)))]);
        let mut seed = BTreeMap::new();
        seed.insert("run.tip".to_string(), Value::Double(2.0));
        let mut state = EffectiveState::new(&schema, seed);

        // default only
        let schema_no_seed = schema_with(&[("run.other", Type::Double, Some(Literal::Double(9.0)))]);
        let state_default = EffectiveState::new(&schema_no_seed, BTreeMap::new());
        assert_eq!(
            state_default.read("run.other"),
            Read::Value(Value::Double(9.0))
        );

        // seed beats default
        assert_eq!(state.read("run.tip"), Read::Value(Value::Double(2.0)));

        // write beats seed
        state.write("run.tip", Value::Double(3.0));
        assert_eq!(state.read("run.tip"), Read::Value(Value::Double(3.0)));

        // nothing at all -> Unset
        assert_eq!(state.read("run.neverDeclared"), Read::Unset);
    }

    #[test]
    fn read_capture_is_opt_in() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        assert_eq!(state.read("run.missing"), Read::Unset);
        assert!(state.reads().is_empty());

        let observed = EffectiveState::new(&schema, BTreeMap::new()).with_read_log();
        assert_eq!(observed.read("run.missing"), Read::Unset);
        assert_eq!(
            observed.reads(),
            vec![("run.missing".to_string(), Read::Unset)]
        );
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
        let (v, unresolved) = eval_str("has(run.tip)", &env);
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
        seed.insert("run.tip".to_string(), Value::Double(5.0));
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
    fn has_macro_is_definite_like_isset() {
        let mut seed = BTreeMap::new();
        seed.insert("run.tip".to_string(), Value::Double(5.0));
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
        let (v, unresolved) = eval_str("!has(run.fresh)", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty());
    }

    // -- dsl 0.27.0 §3: the bound member as a pattern arg / family index ----

    fn with_target(member: Option<&str>, extra: &[(&str, Value)]) -> BTreeMap<String, Value> {
        let mut seed: BTreeMap<String, Value> = extra
            .iter()
            .map(|(p, v)| (p.to_string(), v.clone()))
            .collect();
        if let Some(m) = member {
            seed.insert(
                lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
                Value::Str(m.to_string()),
            );
        }

        seed
    }
    /// A quoted index reads the dotted path its members name, and a quoted
    /// fact argument is the bare name — either spelling, one answer.
    #[test]
    fn quoted_members_and_fact_args_read_the_dotted_names() {
        let vocab = rel_vocab_with(&[("at", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("at", &["lab-b2".to_string()]);
        let schema = schema_with(&[]);
        let state = EffectiveState::new(
            &schema,
            with_target(
                None,
                &[
                    ("run.visits.lab-b2", Value::Int(2)),
                    ("run.visits.001", Value::Int(1)),
                    ("quest.zero-coke-001.state", Value::Str("complete".into())),
                ],
            ),
        );
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        for (raw, want) in [
            (r#"run.visits["lab-b2"]"#, Value::Int(2)),
            ("run.visits['001'] + 1", Value::Int(2)),
            (
                r#"quest["zero-coke-001"].state == "complete""#,
                Value::Bool(true),
            ),
        ] {
            let (v, unresolved) = eval_str(raw, &env);
            assert_eq!(v, want, "{raw}");
            assert!(unresolved.is_empty(), "{raw}: {unresolved:?}");
        }
        // An unset member is unknown, recorded at its dotted path.
        let (v, unresolved) = eval_str(r#"run.visits["hall"] > 0"#, &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("run.visits.hall".into())]
        );
    }

    #[test]
    fn holds_substitutes_the_bound_occasion_target_as_a_ground_arg() {
        let vocab = rel_vocab_with(&[("owned", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("owned", &["bram".to_string()]);
        let schema = schema_with(&[]);
        for (member, want) in [("bram", true), ("aria", false)] {
            let state = EffectiveState::new(&schema, with_target(Some(member), &[]));
            let env = EvalEnv {
                state: &state,
                facts: &facts,
            };
            let (v, unresolved) = eval_str("holds(\"owned\", [occasion.target])", &env);
            assert_eq!(v, Value::Bool(want), "{member}");
            assert!(unresolved.is_empty(), "{member}: {unresolved:?}");
        }
        // Unbound: unknown, and the missing binding is what would decide it —
        // never a lookup of the literal id `occasion.target`.
        let state = EffectiveState::new(&schema, with_target(None, &[]));
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("holds(\"owned\", [occasion.target])", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("occasion.target".to_string())]
        );
    }

    #[test]
    fn a_family_indexed_by_occasion_target_reads_the_members_path() {
        let schema = schema_with(&[]);
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let seed = with_target(
            Some("bram"),
            &[
                ("user.bond.bram", Value::Int(3)),
                ("user.bond.aria", Value::Int(0)),
            ],
        );
        let state = EffectiveState::new(&schema, seed);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("user.bond[occasion.target] >= 2", &env);
        assert_eq!(v, Value::Bool(true));
        assert!(unresolved.is_empty(), "{unresolved:?}");

        // Bound to a member whose slot has no value: that slot is the atom.
        let state = EffectiveState::new(&schema, with_target(Some("cyra"), &[]));
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("user.bond[occasion.target]", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("user.bond.cyra".to_string())]
        );

        // Unbound: the binding is the atom, not a guessed member.
        let state = EffectiveState::new(&schema, with_target(None, &[]));
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        let (v, unresolved) = eval_str("user.bond[occasion.target]", &env);
        assert_eq!(v, Value::Unknown);
        assert_eq!(
            unresolved,
            vec![UnresolvedAtom::Path("occasion.target".to_string())]
        );
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

        let (v, unresolved) =
            eval_str("holds(\"believesLocation\", [\"player\", \"halsin\", \"grove\"])", &env);
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

        let (v, unresolved) =
            eval_str("holds(\"believesLocation\", [\"player\", \"halsin\", \"grove\"])", &env);
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

        let (v, unresolved) = eval_str("holds(\"inParty\", [\"elena\", \"grove\"])", &env);
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

        let (v, unresolved) = eval_str("validAt('inParty', ['elena', 'grove'], now())", &env);
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
        // as `Defaulted` (never silently untracked) so `crate::trace` can
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
            ("run.day", Type::Int, Some(Literal::Int(14))),
            ("run.half", Type::Double, Some(Literal::Double(2.5))),
        ]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab);
        let env = EvalEnv {
            state: &state,
            facts: &facts,
        };
        assert_eq!(eval_str("run.day % 7 == 0", &env).0, Value::Bool(true));
        assert_eq!(eval_str("(run.day + 1) % 7", &env).0, Value::Int(1));
        assert_eq!(eval_str("-7 % 3", &env).0, Value::Int(-1));
        assert!(matches!(eval_str("run.half % 2", &env).0, Value::Error(_)));
        assert!(matches!(eval_str("run.day % 0", &env).0, Value::Error(_)));
    }
    #[test]
    fn cel_numeric_errors_and_list_host_functions() {
        let schema = schema_with(&[
            ("run.i", Type::Int, Some(Literal::Int(3))),
            ("run.d", Type::Double, Some(Literal::Double(3.0))),
            ("run.visits.hall", Type::Int, Some(Literal::Int(1))),
        ]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = rel_vocab_with(&[("at", false)]);
        let mut facts = FactStore::new(&vocab);
        facts.assert("at", &["hall".into()]);
        let env = EvalEnv { state: &state, facts: &facts };
        assert_eq!(eval_str("3 + 2", &env).0, Value::Int(5));
        assert_eq!(eval_str("3.0 + 2.0", &env).0, Value::Double(5.0));
        assert_eq!(eval_str("int(3.7)", &env).0, Value::Int(3));
        assert_eq!(eval_str("int(-3.7)", &env).0, Value::Int(-3));
        assert_eq!(eval_str("int(4)", &env).0, Value::Int(4));
        assert_eq!(eval_str("double(4.0)", &env).0, Value::Double(4.0));
        assert_eq!(eval_str("double(4)", &env).0, Value::Double(4.0));
        assert_eq!(eval_str("1.0 / 0.0", &env).0, Value::Double(f64::INFINITY));
        assert!(matches!(eval_str("0.0 / 0.0", &env).0, Value::Double(v) if v.is_nan()));
        assert!(matches!(eval_str("1 / 0", &env).0, Value::Error(_)));
        assert_eq!(eval_str("false && (1 / 0 == 0)", &env).0, Value::Bool(false));
        assert_eq!(eval_str("true || (1 / 0 == 0)", &env).0, Value::Bool(true));
        assert_eq!(eval_str("holds('at', ['hall'])", &env).0, Value::Bool(true));
        assert_eq!(eval_str("count('at', ['_'])", &env).0, Value::Int(1));
        assert_eq!(eval_str("has(run.i)", &env).0, Value::Bool(true));
        assert_eq!(eval_str("'hall' in run.visits", &env).0, Value::Bool(true));
        assert_eq!(eval_str("run.visits['hall']", &env).0, Value::Int(1));
        assert_eq!(eval_str("visited('hall')", &env).0, Value::Bool(false));
        assert!(matches!(eval_str("now()", &env).0, Value::Unknown));
        assert!(matches!(eval_str("validAt('at', ['hall'], 1)", &env).0, Value::Unknown));
    }
    #[test]
    fn derived_list_query_ignores_empty_undecided_bookkeeping() {
        let schema = schema_with(&[]);
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let vocab = RelVocab::default();
        let facts = FactStore::new(&vocab)
            .with_undecided(BTreeMap::from([("prime".into(), Vec::new())]));
        let env = EvalEnv { state: &state, facts: &facts };
        assert_eq!(eval_str("holds('prime', ['solt'])", &env).0, Value::Bool(false));
    }
}
