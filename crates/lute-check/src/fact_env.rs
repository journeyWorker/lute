//! Fact envelopes (dsl 0.20.0 §2–§5): the two sets that decide a relational
//! query (`holds(P)` / `count(P)`) at a guard slot.
//!
//! - [`MaySet`] (§3) over-approximates every ground fact that can be live at
//!   any point of any run of one project root: the least fixpoint of the
//!   `facts:` seeds, every `::assert` whose hosting node is not proven
//!   unreachable ([`crate::connectivity::live_assert_sites`]), the reserved
//!   and unbounded relations, and the derived relations' rules evaluated over
//!   `May` itself. It is flow-insensitive: it says *whether* some route can
//!   produce a fact, never *when*.
//! - [`MustMap`] (§4) under-approximates, per guard slot, the facts live on
//!   **every** route reaching it, each with the assert site that establishes
//!   it ([`Provenance`]).
//!
//! [`FactEnv`] pairs the two, and [`FactScope`] binds it to one slot so that
//! `decide()` (`crate::decide`) can resolve a relational call to a constant:
//! `holds(P)` is `false` when no `May` fact matches `P`, `true` when a `Must`
//! fact at the slot matches it, and undecided otherwise; `count(P)` lies in
//! the interval `[|Must ∩ P|, |May ∩ P|]` (an unbounded relation's upper end
//! is ∞).
//!
//! The must set is filled by [`crate::fact_must`] (the forward dataflow of
//! §4); each slot's entry already includes the derived facts its rules
//! produce over the slot's guaranteed base facts ([`derive_guaranteed`]).
//!
//! D1 holds: nothing here runs the engine's Datalog. The may-set rule pass
//! is an over-approximating closure over declared structure — negated atoms
//! are read as satisfiable and a rule-body CEL guard as satisfiable unless
//! `decide()` proves it false (§3 rule 4); [`derive_guaranteed`] is its
//! under-approximating twin (a negated atom holds only outside `May`, a CEL
//! guard only when it decides true).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cel_parser::ast::CallExpr;
use lute_core_span::Span;
use lute_manifest::relations::{KindShape, RelationDecl};
use lute_manifest::snapshot::Domain;
use lute_syntax::datalog::{BodyLiteral, FactPattern, FactTerm, Rule, RuleAtom, RuleTerm};

use crate::rel_schema::RelVocab;

/// A relation applied to members of its argument domains (§2):
/// `knows(vesna, manifest)`. A `bool` argument is stored as `true`/`false`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundFact {
    pub relation: String,
    pub args: Vec<String>,
}

impl GroundFact {
    /// The ground fact an `::assert` / `facts:` pattern names — `None` for the
    /// parse-failed sentinel (empty relation, D13) or a pattern carrying a
    /// wildcard (never ground; already `E-RETRACT-WILDCARD-ASSERT`).
    pub fn from_pattern(p: &FactPattern) -> Option<Self> {
        if p.relation.is_empty() {
            return None;
        }
        let args = p
            .args
            .iter()
            .map(|a| match &a.term {
                FactTerm::Ident(id) => Some(id.clone()),
                FactTerm::Bool(b) => Some(b.to_string()),
                FactTerm::Wildcard | FactTerm::Param(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(GroundFact {
            relation: p.relation.clone(),
            args,
        })
    }
}

impl fmt::Display for GroundFact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.relation, self.args.join(", "))
    }
}

/// The atom inside `holds(…)` / `count(…)` (§2): a relation with each
/// argument either a ground member or the `_` wildcard (`None`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryPattern {
    pub relation: String,
    pub args: Vec<Option<String>>,
}

impl QueryPattern {
    /// The pattern of a well-shaped fact-query call's `args[0]`
    /// (`crate::cel_resolve::is_profile_fact_query`). `None` when the pattern
    /// is not compile-time ground (`E-CEL-PROFILE` owns that) — such a query
    /// is never decided.
    pub fn from_call(pattern: &CallExpr) -> Option<Self> {
        let terms = crate::cel_resolve::pattern_terms(pattern)?;
        Some(QueryPattern {
            relation: pattern.func_name.clone(),
            args: terms
                .into_iter()
                .map(|a| match a.term {
                    FactTerm::Ident(id) => Some(id),
                    FactTerm::Bool(b) => Some(b.to_string()),
                    FactTerm::Wildcard | FactTerm::Param(_) => None,
                })
                .collect(),
        })
    }

    /// The pattern of an `::assert` / `::retract` payload — `None` for the
    /// parse-failed sentinel (empty relation, D13).
    pub fn from_fact_pattern(p: &FactPattern) -> Option<Self> {
        if p.relation.is_empty() {
            return None;
        }
        Some(QueryPattern {
            relation: p.relation.clone(),
            args: p
                .args
                .iter()
                .map(|a| match &a.term {
                    FactTerm::Ident(id) => Some(id.clone()),
                    FactTerm::Bool(b) => Some(b.to_string()),
                    FactTerm::Wildcard | FactTerm::Param(_) => None,
                })
                .collect(),
        })
    }

    /// The ground fact this pattern names, when it carries no wildcard.
    pub fn ground(&self) -> Option<GroundFact> {
        Some(GroundFact {
            relation: self.relation.clone(),
            args: self.args.iter().cloned().collect::<Option<Vec<_>>>()?,
        })
    }

    fn matches_args(&self, args: &[String]) -> bool {
        self.args.len() == args.len()
            && self
                .args
                .iter()
                .zip(args)
                .all(|(q, a)| q.as_ref().is_none_or(|q| q == a))
    }

    /// `true` iff `fact` is an instance of this pattern.
    pub fn matches(&self, fact: &GroundFact) -> bool {
        self.relation == fact.relation && self.matches_args(&fact.args)
    }
}

/// A well-shaped `count(P)` / `countDistinct(P, V)` call (dsl 0.24 T3-9):
/// its pattern — the column `V` names read as `_` — and, for
/// `countDistinct`, that column. `None` for anything else, or a pattern that
/// is not compile-time ground.
pub fn count_query(c: &CallExpr) -> Option<(QueryPattern, Option<usize>)> {
    if !crate::cel_resolve::is_profile_fact_query(c) {
        return None;
    }
    let column = match c.func_name.as_str() {
        "count" => None,
        "countDistinct" => Some(crate::cel_resolve::count_distinct_column(c)?),
        _ => return None,
    };
    let cel_parser::ast::Expr::Call(p) = &c.args[0].expr else {
        return None;
    };
    let mut q = QueryPattern::from_call(p)?;
    if let Some(col) = column {
        q.args[col] = None;
    }
    Some((q, column))
}

/// How many of `facts` there are — or, with `column`, how many distinct
/// values they carry at that position.
fn tally<'f>(facts: impl Iterator<Item = &'f [String]>, column: Option<usize>) -> usize {
    match column {
        None => facts.count(),
        Some(i) => facts
            .filter_map(|a| a.get(i))
            .collect::<BTreeSet<_>>()
            .len(),
    }
}

impl fmt::Display for QueryPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args: Vec<&str> = self
            .args
            .iter()
            .map(|a| a.as_deref().unwrap_or("_"))
            .collect();
        write!(f, "{}({})", self.relation, args.join(", "))
    }
}

/// One argument's (or one rule-body predicate's) member set: closed and
/// enumerable, or open — the engine may mint members (`open:` kinds, open
/// plugin/core domains).
enum Universe<'v> {
    Closed(Vec<&'v str>),
    Open,
}

/// The relational vocabulary of a whole project root, unioned over every
/// document's merged [`RelVocab`] (§3 reads "every seed", "every rule"
/// project-wide). Documents of one root normally fold the same imported
/// schema; where two documents disagree on a relation's declaration the
/// relation is treated as unbounded, and entity kinds / enums union their
/// members — both only ever enlarge `May`, which keeps an *impossible*
/// verdict sound.
#[derive(Clone, Debug, Default)]
pub struct RootVocab {
    pub(crate) relations: BTreeMap<String, RelationDecl>,
    conflicting: BTreeSet<String>,
    kinds: BTreeMap<String, KindShape>,
    enums: BTreeMap<String, BTreeSet<String>>,
    /// Plugin/core/project domains (`Env::domains`): `None` = open.
    domains: BTreeMap<String, Option<BTreeSet<String>>>,
    pub(crate) seeds: BTreeSet<GroundFact>,
    rules: Vec<Rule>,
    rule_texts: BTreeSet<String>,
    /// seven F3 (dsl 0.23.1): some document of the root has a frontmatter
    /// that does not parse, so its `uses:` vocabulary, seeds, rules — and
    /// whether its asserts ever run — are unknown. [`MaySet::build`] then
    /// reads every relation as unbounded, so no fact is impossible, as
    /// connectivity already declines to call a `visited()` key unknown.
    incomplete: bool,
    /// dsl 0.24 T3-6: relations heading a rule that failed to parse — their
    /// derivation is unknown, so they are unbounded (no verdict cascades).
    unparsed_heads: BTreeSet<String>,
}

impl RootVocab {
    /// Fold one document's merged vocabulary and domain catalog in.
    pub fn add(&mut self, vocab: &RelVocab, domains: &BTreeMap<String, Domain>) {
        for (name, decl) in &vocab.relations {
            match self.relations.get(name) {
                None => {
                    self.relations.insert(name.clone(), decl.clone());
                }
                Some(existing) if existing != decl => {
                    self.conflicting.insert(name.clone());
                }
                Some(_) => {}
            }
        }
        for (name, kind) in &vocab.kinds {
            let merged = match (self.kinds.remove(name), &kind.shape) {
                (None, shape) => shape.clone(),
                (Some(KindShape::Members(mut a)), KindShape::Members(b)) => {
                    for m in b {
                        if !a.contains(m) {
                            a.push(m.clone());
                        }
                    }
                    KindShape::Members(a)
                }
                (Some(_), _) => KindShape::Open,
            };
            self.kinds.insert(name.clone(), merged);
        }
        for (name, members) in &vocab.enums {
            self.enums
                .entry(name.clone())
                .or_default()
                .extend(members.iter().cloned());
        }
        for (name, dom) in domains {
            let slot = self
                .domains
                .entry(name.clone())
                .or_insert_with(|| Some(BTreeSet::new()));
            if dom.open {
                *slot = None;
            } else if let Some(set) = slot {
                set.extend(dom.members.iter().cloned());
            }
        }
        for seed in &vocab.facts {
            if let Some(fact) = GroundFact::from_pattern(&seed.fact) {
                self.seeds.insert(fact);
            }
        }
        for rule in &vocab.rules {
            if self.rule_texts.insert(rule.raw.trim().to_string()) {
                self.rules.push(rule.rule.clone());
            }
        }
        self.unparsed_heads
            .extend(vocab.unparsed_heads.iter().cloned());
    }

    /// seven F3 (dsl 0.23.1): mark the root incomplete when any of `docs`
    /// has a frontmatter that does not parse (`E-META-PARSE` —
    /// [`crate::meta::frontmatter_parses`]). Call before [`MaySet::build`].
    pub fn note_unreadable_documents(&mut self, docs: &[(PathBuf, lute_syntax::ast::Document)]) {
        self.incomplete |= docs
            .iter()
            .any(|(_, d)| !crate::meta::frontmatter_parses(&d.meta));
    }

    /// The member universe a domain / predicate name denotes. `bool` is the
    /// two literals; an unresolvable name (already `E-RELATION-DOMAIN`) is
    /// open — never a basis for an impossibility claim.
    fn universe(&self, name: &str) -> Universe<'_> {
        const BOOLS: [&str; 2] = ["true", "false"];
        if name == "bool" {
            return Universe::Closed(BOOLS.to_vec());
        }
        if let Some(kind) = self.kinds.get(name) {
            return match kind {
                KindShape::Members(m) => Universe::Closed(m.iter().map(String::as_str).collect()),
                KindShape::Open | KindShape::Invalid => Universe::Open,
            };
        }
        if let Some(members) = self.enums.get(name) {
            return Universe::Closed(members.iter().map(String::as_str).collect());
        }
        match self.domains.get(name) {
            Some(Some(members)) => Universe::Closed(members.iter().map(String::as_str).collect()),
            _ => Universe::Open,
        }
    }

    /// §2/§3: a relation whose universe is "anything" — `reserved: true`
    /// (the engine populates any fact of it), any argument over an open
    /// domain, or a declaration two documents disagree on.
    pub(crate) fn is_unbounded(&self, name: &str, decl: &RelationDecl) -> bool {
        decl.reserved
            || self.unparsed_heads.contains(name)
            || self.conflicting.contains(name)
            || decl
                .args
                .iter()
                .any(|a| matches!(self.universe(a), Universe::Open))
    }

    /// dsl 0.23.0 §10: the declared relations nothing can produce at all —
    /// not reserved, no seed, no rule deriving them, and none of `asserted`
    /// (every relation some `::assert` anywhere in the root writes).
    pub(crate) fn unproduced(&self, asserted: &BTreeSet<String>) -> BTreeSet<String> {
        self.relations
            .iter()
            .filter(|(name, decl)| {
                !decl.reserved
                    && !self.unparsed_heads.contains(*name)
                    && !asserted.contains(*name)
                    && !self.seeds.iter().any(|s| &s.relation == *name)
                    && !self.rules.iter().any(|r| &r.head.relation == *name)
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// A rule-body predicate name that denotes a member set (entity kind,
    /// enum, or domain) rather than a relation.
    fn is_predicate(&self, name: &str) -> bool {
        self.kinds.contains_key(name)
            || self.enums.contains_key(name)
            || self.domains.contains_key(name)
    }
}

/// The project-wide may set (§3).
#[derive(Clone, Debug, Default)]
pub struct MaySet {
    /// Declared relations' argument universes (`None` = open) — a query over
    /// any other relation, with the wrong arity, or naming a non-member of a
    /// closed argument domain is `E-RELATION-UNKNOWN`/`E-RELATION-ARITY`/
    /// `E-FACT-DOMAIN`'s problem and is never decided here (no cascade).
    signature: BTreeMap<String, Vec<Option<BTreeSet<String>>>>,
    facts: BTreeMap<String, BTreeSet<Vec<String>>>,
    unbounded: BTreeSet<String>,
    /// dsl 0.23.0 §9: the facts that hold at every point of every run
    /// (`crate::fact_must::stable_seeds`, closed under the rules whose every
    /// premise is stable — dsl 0.23.1) — a negated rule atom over one of
    /// them is false, so the clause instance never fires.
    stable: BTreeSet<GroundFact>,
    /// Each derived fact a stable fact defeats, with the first stable fact
    /// a negated premise of one of its clauses denies — why an impossible
    /// derived fact is impossible (`opp(crane)`: `alibi(crane)` is stable).
    defeats: BTreeMap<GroundFact, GroundFact>,
    /// The stable seeds `build` started from — [`Self::widened`]'s stable
    /// set: the rule closure over them read a relation nothing produces yet
    /// as never holding, which `--wip` no longer assumes.
    stable_seeds: BTreeSet<GroundFact>,
}

/// One rule application's result: concrete head tuples, or "the head may be
/// anything" (a body over an unbounded relation / open predicate, or a head
/// variable no positive atom binds).
enum Derived {
    Tuples(Vec<Vec<String>>),
    Unbounded,
}

impl MaySet {
    /// §3's least fixpoint over `vocab`'s seeds and rules plus `asserts` — the
    /// facts of every live assert site in the root. `stable` are the facts
    /// that hold throughout every run (dsl 0.23.0 §9); the set closes them
    /// under the rules that prove a fact from stable facts alone (a negated
    /// premise outside `May`, [`derive_guaranteed`]) and rebuilds until that
    /// closure stops growing (dsl 0.23.1: `not alibi(crane)` with `alibi`
    /// derived from seeds nothing removes never holds either).
    pub fn build(
        vocab: &RootVocab,
        asserts: impl IntoIterator<Item = GroundFact>,
        stable: &BTreeSet<GroundFact>,
    ) -> Self {
        let asserts: Vec<GroundFact> = asserts.into_iter().collect();
        let mut closure = stable.clone();
        loop {
            let mut may = Self::build_once(vocab, &asserts, &closure);
            let base: Vec<MustFact> = closure
                .iter()
                .map(|fact| MustFact {
                    fact: fact.clone(),
                    provenance: Provenance::Seed,
                })
                .collect();
            let derived = derive_guaranteed(vocab, &may, &base);
            if derived.is_empty() {
                may.stable_seeds = stable.clone();
                return may;
            }
            // Sound: every closure fact holds at every point of every run
            // over the current (sound) `May`, and a larger stable set only
            // shrinks the next `May`.
            closure.extend(derived.into_iter().map(|m| m.fact));
        }
    }

    fn build_once(
        vocab: &RootVocab,
        asserts: &[GroundFact],
        stable: &BTreeSet<GroundFact>,
    ) -> Self {
        let mut may = MaySet {
            stable: stable.clone(),
            ..MaySet::default()
        };
        for (name, decl) in &vocab.relations {
            let sig = decl
                .args
                .iter()
                .map(|a| match vocab.universe(a) {
                    Universe::Closed(m) => Some(m.into_iter().map(str::to_string).collect()),
                    Universe::Open => None,
                })
                .collect();
            may.signature.insert(name.clone(), sig);
            // seven F3: an incomplete root may produce any fact of any
            // relation — every relation is unbounded, nothing impossible.
            if vocab.incomplete || vocab.is_unbounded(name, decl) {
                may.unbounded.insert(name.clone());
            }
        }
        for fact in vocab.seeds.iter().chain(asserts).cloned() {
            may.insert(fact);
        }
        may.saturate(vocab);
        if !may.stable.is_empty() {
            let mut defeats = BTreeMap::new();
            for rule in &vocab.rules {
                if vocab
                    .relations
                    .get(&rule.head.relation)
                    .is_some_and(|d| d.derive)
                {
                    may.apply_rule(vocab, rule, Some(&mut defeats));
                }
            }
            defeats.retain(|head, _| !may.contains(head));
            may.defeats = defeats;
        }
        may
    }

    /// Why no fact matching `q` can hold although a rule derives its
    /// relation: the first defeated head matching `q` and the stable fact a
    /// negated premise of its clause denies.
    pub fn defeat(&self, q: &QueryPattern) -> Option<(&GroundFact, &GroundFact)> {
        self.defeats.iter().find(|(head, _)| q.matches(head))
    }

    /// The ground tuples of `relation` this set may hold — `None` for a
    /// relation it holds none of. Meaningless for an
    /// [unbounded](Self::is_unbounded) relation, which may hold anything.
    pub fn instances(&self, relation: &str) -> Option<&BTreeSet<Vec<String>>> {
        self.facts.get(relation)
    }

    /// dsl 0.23.0 §10 (`check-project --wip`): this set with every relation
    /// in `open` unbounded — as if content not yet written could produce any
    /// fact of it — and the rules re-run to their fixpoint. Starting from
    /// this set's facts is sound: widening only ever adds facts.
    pub fn widened(&self, vocab: &RootVocab, open: &BTreeSet<String>) -> Self {
        let mut may = self.clone();
        may.unbounded.extend(
            open.iter()
                .filter(|r| may.signature.contains_key(*r))
                .cloned(),
        );
        may.stable = self.stable_seeds.clone();
        may.defeats.clear();
        may.saturate(vocab);
        may
    }

    /// Apply the derived relations' rules until nothing changes.
    fn saturate(&mut self, vocab: &RootVocab) {
        loop {
            let mut changed = false;
            for rule in &vocab.rules {
                let head = &rule.head.relation;
                let Some(decl) = vocab.relations.get(head) else {
                    continue; // `E-DERIVE-UNDECLARED`'s problem
                };
                if !decl.derive || self.unbounded.contains(head) {
                    continue;
                }
                match self.apply_rule(vocab, rule, None) {
                    Derived::Unbounded => {
                        self.unbounded.insert(head.clone());
                        changed = true;
                    }
                    Derived::Tuples(tuples) => {
                        for args in tuples {
                            changed |= self.insert(GroundFact {
                                relation: head.clone(),
                                args,
                            });
                        }
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }

    /// Insert a fact of a declared relation with the declared arity; anything
    /// else is already diagnosed and never feeds the set. `true` iff new.
    fn insert(&mut self, fact: GroundFact) -> bool {
        if self
            .signature
            .get(&fact.relation)
            .is_none_or(|sig| sig.len() != fact.args.len())
        {
            return false;
        }
        self.facts
            .entry(fact.relation)
            .or_default()
            .insert(fact.args)
    }

    /// Evaluate one clause over the current set: join the positive atoms,
    /// then filter by `=`/`!=` and drop the clause if a CEL guard decides
    /// false. A negated atom is satisfiable (§3 rule 4) unless it denies a
    /// stable fact (dsl 0.23.0 §9: `not alibi(crane, tunnel)` with the alibi
    /// a seed nothing removes) — each head so defeated is recorded in
    /// `defeats` when given. A positive atom over an unbounded relation
    /// may match anything: it is satisfiable and binds nothing, so the head
    /// is unbounded only when it needs a variable no other atom binds.
    fn apply_rule(
        &self,
        vocab: &RootVocab,
        rule: &Rule,
        mut defeats: Option<&mut BTreeMap<GroundFact, GroundFact>>,
    ) -> Derived {
        let mut bindings: Vec<BTreeMap<&str, &str>> = vec![BTreeMap::new()];
        for lit in &rule.body {
            let BodyLiteral::Pos(atom) = lit else {
                continue;
            };
            let Some(rows) = self.atom_rows(vocab, atom) else {
                continue;
            };
            let mut next = Vec::new();
            for b in &bindings {
                for row in &rows {
                    if let Some(extended) = unify(&atom.terms, row, b) {
                        next.push(extended);
                    }
                }
            }
            bindings = next;
            if bindings.is_empty() {
                return Derived::Tuples(Vec::new());
            }
        }
        for lit in &rule.body {
            match lit {
                BodyLiteral::Guard { cel, .. } if guard_is_dead(cel) => {
                    return Derived::Tuples(Vec::new());
                }
                BodyLiteral::Cmp {
                    lhs, rhs, negated, ..
                } => bindings.retain(|b| match (term_value(lhs, b), term_value(rhs, b)) {
                    (Some(l), Some(r)) => (l == r) != *negated,
                    _ => true, // unbound: never a basis for dropping a clause
                }),
                BodyLiteral::Neg(atom) if !self.stable.is_empty() => bindings.retain(|b| {
                    let Some(args) = atom
                        .terms
                        .iter()
                        .map(|t| term_value(t, b).map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                    else {
                        return true; // unbound: never a basis for dropping a clause
                    };
                    let denied = GroundFact {
                        relation: atom.relation.clone(),
                        args,
                    };
                    if !self.stable.contains(&denied) {
                        return true;
                    }
                    if let Some(sink) = defeats.as_deref_mut() {
                        let head = rule
                            .head
                            .terms
                            .iter()
                            .map(|t| term_value(t, b).map(str::to_string))
                            .collect::<Option<Vec<_>>>();
                        if let Some(args) = head {
                            sink.entry(GroundFact {
                                relation: rule.head.relation.clone(),
                                args,
                            })
                            .or_insert(denied);
                        }
                    }
                    false
                }),
                _ => {}
            }
        }
        let mut out = Vec::with_capacity(bindings.len());
        for b in &bindings {
            let Some(args) = rule
                .head
                .terms
                .iter()
                .map(|t| term_value(t, b).map(str::to_string))
                .collect::<Option<Vec<_>>>()
            else {
                return Derived::Unbounded;
            };
            out.push(args);
        }
        Derived::Tuples(out)
    }

    /// The rows a positive body atom ranges over: a declared relation's `May`
    /// tuples, or a one-place domain predicate's members (`K(X)` over an
    /// entity kind / enum / domain). `None` = unbounded (an unbounded
    /// relation, an open predicate, or a name nothing declares).
    fn atom_rows<'s>(&'s self, vocab: &'s RootVocab, atom: &RuleAtom) -> Option<Vec<Vec<&'s str>>> {
        if self.signature.contains_key(&atom.relation) {
            if self.unbounded.contains(&atom.relation) {
                return None;
            }
            return Some(
                self.facts
                    .get(&atom.relation)
                    .map(|set| {
                        set.iter()
                            .map(|row| row.iter().map(String::as_str).collect())
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        if atom.terms.len() != 1 {
            return None;
        }
        match vocab.universe(&atom.relation) {
            Universe::Closed(members) if vocab.is_predicate(&atom.relation) => {
                Some(members.into_iter().map(|m| vec![m]).collect())
            }
            _ => None,
        }
    }

    /// `true` iff `relation` is declared and unbounded.
    pub fn is_unbounded(&self, relation: &str) -> bool {
        self.unbounded.contains(relation)
    }

    /// `true` iff `q` names a declared relation with the declared arity and
    /// every ground argument inside its (closed) domain — the only queries
    /// this set decides.
    pub fn decides(&self, q: &QueryPattern) -> bool {
        self.signature.get(&q.relation).is_some_and(|sig| {
            sig.len() == q.args.len()
                && sig.iter().zip(&q.args).all(|(dom, arg)| match (dom, arg) {
                    (Some(members), Some(a)) => members.contains(a),
                    _ => true,
                })
        })
    }

    /// `|May ∩ q|`; `None` = unbounded (∞).
    pub fn count_matching(&self, q: &QueryPattern) -> Option<usize> {
        self.count_matching_in(q, None)
    }

    /// `|May ∩ q|`, or the number of distinct values at `column` among those
    /// facts (`countDistinct`); `None` = unbounded (∞).
    fn count_matching_in(&self, q: &QueryPattern, column: Option<usize>) -> Option<usize> {
        if self.unbounded.contains(&q.relation) {
            return None;
        }
        Some(self.facts.get(&q.relation).map_or(0, |set| {
            tally(
                set.iter().filter(|a| q.matches_args(a)).map(Vec::as_slice),
                column,
            )
        }))
    }

    /// `true` iff some fact in `May` may match `q`.
    pub fn any_match(&self, q: &QueryPattern) -> bool {
        self.count_matching(q).is_none_or(|n| n > 0)
    }

    /// `true` iff `fact` is in `May` (an unbounded relation may hold any
    /// fact of its universe).
    pub fn contains(&self, fact: &GroundFact) -> bool {
        self.unbounded.contains(&fact.relation)
            || self
                .facts
                .get(&fact.relation)
                .is_some_and(|set| set.contains(&fact.args))
    }
}

fn term_value<'a>(t: &'a RuleTerm, b: &BTreeMap<&'a str, &'a str>) -> Option<&'a str> {
    match t {
        RuleTerm::Var(v) => b.get(v.as_str()).copied(),
        RuleTerm::Const(c) => Some(c.as_str()),
        RuleTerm::Bool(true) => Some("true"),
        RuleTerm::Bool(false) => Some("false"),
    }
}

/// Extend binding `b` so `terms` matches `row`, or `None` on a clash.
fn unify<'a>(
    terms: &'a [RuleTerm],
    row: &[&'a str],
    b: &BTreeMap<&'a str, &'a str>,
) -> Option<BTreeMap<&'a str, &'a str>> {
    if terms.len() != row.len() {
        return None;
    }
    let mut out = b.clone();
    for (t, v) in terms.iter().zip(row) {
        match t {
            RuleTerm::Var(name) => match out.get(name.as_str()) {
                Some(bound) if bound != v => return None,
                Some(_) => {}
                None => {
                    out.insert(name.as_str(), v);
                }
            },
            _ => {
                if term_value(t, &out) != Some(v) {
                    return None;
                }
            }
        }
    }
    Some(out)
}

/// A rule-body CEL guard decided with no state knowledge at all: §3 rule 4
/// reads a guard as satisfiable unless it decides `false` (a literal-false
/// guard, `1 > 2`); the must closure ([`derive_guaranteed`]) only uses a
/// clause whose guard decides `true`.
fn guard_decides(cel: &str, value: bool) -> bool {
    let bodies = BTreeMap::new();
    let def_params = BTreeMap::new();
    let defs = crate::cel_expand::DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    let schema = crate::meta::StateSchema::default();
    let params = BTreeMap::new();
    let ctx = crate::decide::DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    crate::decide::decide_slot(cel, &defs, &ctx) == Some(crate::decide::Decided::Bool(value))
}

fn guard_is_dead(cel: &str) -> bool {
    guard_decides(cel, false)
}

/// Where a guaranteed fact is established — what the `W-FACT-GUARANTEED`
/// message names (§5: "asserted on every route to here
/// (scenes/archive.lute:21)").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// An `::assert` site that every route to the slot passes.
    Assert { path: PathBuf, line: u32 },
    /// An enclosing guard whose positive `holds(F)` conjunct makes `F` true
    /// throughout its region (§4, D-D).
    Guard { path: PathBuf, line: u32 },
    /// A `facts:` seed that no document can retract or displace.
    Seed,
    /// A derived fact whose rule body holds over guaranteed facts, carrying
    /// the provenance of the first fact that supports it.
    Derived(Box<Provenance>),
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provenance::Assert { path, line } | Provenance::Guard { path, line } => {
                write!(f, "{}:{line}", path.display())
            }
            Provenance::Seed => f.write_str("`facts:` seed"),
            Provenance::Derived(inner) => inner.fmt(f),
        }
    }
}

/// §4 "Derived facts are recomputed over `Must` at each query": the derived
/// facts every rule produces over `base` (a slot's guaranteed facts), to a
/// fixpoint — the UNDER-approximating twin of [`MaySet::build`]'s rule pass.
/// A positive atom binds only from guaranteed facts (or a closed domain
/// predicate's members); a negated atom holds only when no `May` fact can
/// match it; `=`/`!=` must be decided on bound values; a CEL guard must
/// decide `true`. Anything else drops the clause. Returns only the facts not
/// already in `base`, in derivation order (round, then rule, then binding),
/// each carrying the provenance of the first fact supporting its first
/// derivation (a seed when only domain predicates support it).
pub fn derive_guaranteed(vocab: &RootVocab, may: &MaySet, base: &[MustFact]) -> Vec<MustFact> {
    Clauses::new(vocab).derive(may, base)
}

/// One derive rule as [`Clauses`] applies it: where each positive body atom
/// binds from, fixed once per root.
#[derive(Clone, Debug)]
struct Clause {
    rule: Rule,
    /// Per positive body atom, in body order: `None` binds from the known
    /// facts of its relation, `Some` from a closed domain predicate's
    /// members.
    sources: Vec<Option<Vec<String>>>,
}

/// The rules [`derive_guaranteed`] applies, prepared once: only clauses that
/// can fire (a derived head, every positive atom bindable, every CEL guard
/// deciding `true` — a guard reads no binding, so it is decided here once).
#[derive(Clone, Debug, Default)]
struct Clauses {
    clauses: Vec<Clause>,
    /// The relations a derivation reads: every positive atom's declared
    /// relation and every head (a fact already known is not derived again).
    /// Facts of any other relation never change the derived facts.
    relevant: BTreeSet<String>,
}

/// One row a positive atom binds from: the arguments, the provenance of the
/// fact (`None` for a domain member), and whether the fact is new since the
/// previous round.
type Row<'a> = (&'a [String], Option<&'a Provenance>, bool);

impl Clauses {
    fn new(vocab: &RootVocab) -> Self {
        let mut out = Clauses::default();
        'rules: for rule in &vocab.rules {
            let head = &rule.head.relation;
            if !vocab.relations.get(head).is_some_and(|d| d.derive) {
                continue;
            }
            let mut sources = Vec::new();
            for lit in &rule.body {
                match lit {
                    BodyLiteral::Pos(atom) => {
                        if vocab.relations.contains_key(&atom.relation) {
                            sources.push(None);
                            continue;
                        }
                        match vocab.universe(&atom.relation) {
                            Universe::Closed(members)
                                if atom.terms.len() == 1 && vocab.is_predicate(&atom.relation) =>
                            {
                                sources
                                    .push(Some(members.into_iter().map(str::to_string).collect()));
                            }
                            _ => continue 'rules,
                        }
                    }
                    BodyLiteral::Guard { cel, .. } => {
                        if !guard_decides(cel, true) {
                            continue 'rules;
                        }
                    }
                    BodyLiteral::Cmp { .. } | BodyLiteral::Neg(_) => {}
                    // An aggregate is never proved over a must set here: the
                    // clause proves nothing (the sound under-approximation).
                    BodyLiteral::Count { .. } => continue 'rules,
                }
            }
            out.relevant.insert(head.clone());
            for (lit, source) in positive_atoms(rule).zip(&sources) {
                if source.is_none() {
                    out.relevant.insert(lit.relation.clone());
                }
            }
            out.clauses.push(Clause {
                rule: rule.clone(),
                sources,
            });
        }
        out
    }

    /// The derived facts over `base` ([`derive_guaranteed`]'s answer), by
    /// rounds: round 1 applies every clause to `base`; a later round only
    /// enumerates the bindings that use a fact the previous round added
    /// (semi-naive) — every other binding proves a fact already known, so the
    /// facts, their order and their provenance are the naive rounds' own.
    fn derive(&self, may: &MaySet, base: &[MustFact]) -> Vec<MustFact> {
        // The relevant part of `known`, fact-sorted (a repeated fact keeps
        // its last provenance, as collecting into a map would).
        let mut sorted: BTreeMap<&GroundFact, &Provenance> = BTreeMap::new();
        for m in base {
            if self.relevant.contains(&m.fact.relation) {
                sorted.insert(&m.fact, &m.provenance);
            }
        }
        let mut base_rows: BTreeMap<&str, Vec<(&[String], &Provenance)>> = BTreeMap::new();
        for (f, p) in sorted {
            base_rows
                .entry(f.relation.as_str())
                .or_default()
                .push((f.args.as_slice(), p));
        }
        let mut derived: Vec<MustFact> = Vec::new();
        let mut fresh = 0..0;
        let mut first = true;
        loop {
            let new = self.round(may, &base_rows, &derived, &fresh, first);
            if new.is_empty() {
                return derived;
            }
            fresh = derived.len()..derived.len() + new.len();
            derived.extend(new);
            first = false;
        }
    }

    /// One naive round over `base_rows` ∪ `derived` (`fresh`: the indices
    /// the previous round added; `first`: nothing is pruned).
    fn round(
        &self,
        may: &MaySet,
        base_rows: &BTreeMap<&str, Vec<(&[String], &Provenance)>>,
        derived: &[MustFact],
        fresh: &std::ops::Range<usize>,
        first: bool,
    ) -> Vec<MustFact> {
        let mut rows: BTreeMap<&str, Vec<Row<'_>>> = base_rows
            .iter()
            .map(|(rel, v)| (*rel, v.iter().map(|&(a, p)| (a, Some(p), first)).collect()))
            .collect();
        let mut grown: BTreeSet<&str> = BTreeSet::new();
        for (i, m) in derived.iter().enumerate() {
            rows.entry(m.fact.relation.as_str()).or_default().push((
                m.fact.args.as_slice(),
                Some(&m.provenance),
                first || fresh.contains(&i),
            ));
            grown.insert(m.fact.relation.as_str());
        }
        for rel in grown {
            if let Some(v) = rows.get_mut(rel) {
                v.sort_by(|a, b| a.0.cmp(b.0));
            }
        }
        let known = |fact: &GroundFact| {
            rows.get(fact.relation.as_str())
                .is_some_and(|v| v.binary_search_by(|r| r.0.cmp(&fact.args[..])).is_ok())
        };
        let mut new: Vec<MustFact> = Vec::new();
        let mut new_set: HashSet<GroundFact> = HashSet::new();
        for clause in &self.clauses {
            for (args, provenance) in clause.apply(may, &rows, !first) {
                let fact = GroundFact {
                    relation: clause.rule.head.relation.clone(),
                    args,
                };
                if !known(&fact) && new_set.insert(fact.clone()) {
                    new.push(MustFact {
                        fact,
                        provenance: Provenance::Derived(Box::new(provenance)),
                    });
                }
            }
        }
        new
    }
}

/// The positive atoms of `rule`'s body, in order.
fn positive_atoms(rule: &Rule) -> impl Iterator<Item = &RuleAtom> {
    rule.body.iter().filter_map(|lit| match lit {
        BodyLiteral::Pos(atom) => Some(atom),
        _ => None,
    })
}

impl Clause {
    /// Every head tuple this clause proves over `rows`, in binding order,
    /// each with the provenance of its first supporting fact (a seed when
    /// only domain predicates support it). `prune`: only bindings that use a
    /// fresh row.
    fn apply<'a>(
        &'a self,
        may: &MaySet,
        rows: &'a BTreeMap<&'a str, Vec<Row<'a>>>,
        prune: bool,
    ) -> Vec<(Vec<String>, Provenance)> {
        let atoms: Vec<&RuleAtom> = positive_atoms(&self.rule).collect();
        let atom_rows: Vec<Cow<'a, [Row<'a>]>> = atoms
            .iter()
            .zip(&self.sources)
            .map(|(atom, source)| match source {
                None => rows
                    .get(atom.relation.as_str())
                    .map_or(Cow::Borrowed(&[][..]), |v| Cow::Borrowed(v.as_slice())),
                Some(members) => Cow::Owned(
                    members
                        .iter()
                        .map(|m| (std::slice::from_ref(m), None, false))
                        .collect(),
                ),
            })
            .collect();
        // `fresh_from[i]`: a fresh row remains at some atom `>= i`.
        let mut fresh_from = vec![false; atoms.len() + 1];
        for i in (0..atoms.len()).rev() {
            fresh_from[i] = fresh_from[i + 1] || atom_rows[i].iter().any(|r| r.2);
        }
        let mut bindings: Vec<(BTreeMap<&str, &str>, Option<&Provenance>, bool)> =
            vec![(BTreeMap::new(), None, false)];
        for (i, atom) in atoms.iter().enumerate() {
            let mut next = Vec::new();
            for (b, p, used) in &bindings {
                for &(args, rp, is_fresh) in atom_rows[i].iter() {
                    if prune && !used && !is_fresh && !fresh_from[i + 1] {
                        continue;
                    }
                    if let Some(extended) = unify_args(&atom.terms, args, b) {
                        next.push((extended, p.or(rp), *used || is_fresh));
                    }
                }
            }
            bindings = next;
            if bindings.is_empty() {
                return Vec::new();
            }
        }
        if prune {
            bindings.retain(|(_, _, used)| *used);
        }
        for lit in &self.rule.body {
            match lit {
                BodyLiteral::Pos(_) | BodyLiteral::Guard { .. } => {}
                // Never kept by `Clauses::new`; proves nothing.
                BodyLiteral::Count { .. } => bindings.clear(),
                BodyLiteral::Cmp {
                    lhs, rhs, negated, ..
                } => bindings.retain(|(b, _, _)| match (term_value(lhs, b), term_value(rhs, b)) {
                    (Some(l), Some(r)) => (l == r) != *negated,
                    _ => false, // unbound: never a proof
                }),
                BodyLiteral::Neg(atom) => bindings.retain(|(b, _, _)| {
                    let q = QueryPattern {
                        relation: atom.relation.clone(),
                        args: atom
                            .terms
                            .iter()
                            .map(|t| term_value(t, b).map(str::to_string))
                            .collect(),
                    };
                    may.decides(&q) && !may.any_match(&q)
                }),
            }
        }
        bindings
            .into_iter()
            .filter_map(|(b, p, _)| {
                let args = self
                    .rule
                    .head
                    .terms
                    .iter()
                    .map(|t| term_value(t, &b).map(str::to_string))
                    .collect::<Option<Vec<_>>>()?;
                Some((args, p.cloned().unwrap_or(Provenance::Seed)))
            })
            .collect()
    }
}

/// [`unify`] over a fact's owned arguments.
fn unify_args<'a>(
    terms: &'a [RuleTerm],
    row: &'a [String],
    b: &BTreeMap<&'a str, &'a str>,
) -> Option<BTreeMap<&'a str, &'a str>> {
    if terms.len() != row.len() {
        return None;
    }
    let mut out = b.clone();
    for (t, v) in terms.iter().zip(row) {
        match t {
            RuleTerm::Var(name) => match out.get(name.as_str()) {
                Some(bound) if *bound != v.as_str() => return None,
                Some(_) => {}
                None => {
                    out.insert(name.as_str(), v.as_str());
                }
            },
            _ => {
                if term_value(t, &out) != Some(v.as_str()) {
                    return None;
                }
            }
        }
    }
    Some(out)
}

/// The derived closure of one root's must sets (§4, T2-1), prepared once per
/// root: the clauses, the may set negated atoms are decided against, and the
/// closure of the seeds every must walk starts from. A slot's derived facts
/// are computed only when the slot is read ([`MustMap::at`]): a slot whose
/// facts of the relevant relations are exactly the seeds shares the seed
/// closure, and every other such shape is derived once and shared by every
/// slot of that shape.
#[derive(Debug)]
pub struct MustClosure {
    clauses: Clauses,
    may: MaySet,
    /// The seeds of relevant relations, fact-sorted (provenance `Seed`).
    seeds: Vec<GroundFact>,
    seed_derived: Arc<Vec<MustFact>>,
    memo: Mutex<HashMap<ClosureKey, Arc<Vec<MustFact>>>>,
}

/// A must set's relevant facts as they differ from the seeds: the facts
/// that are not a `Seed`-provenance seed, and the seeds absent.
#[derive(Debug, PartialEq, Eq, Hash)]
struct ClosureKey {
    extra: Vec<MustFact>,
    missing: Vec<usize>,
}

impl MustClosure {
    /// Prepare the closure over `vocab`'s rules, deciding negated atoms
    /// against `may`; `seeds` are the facts every walk starts from.
    pub fn new(
        vocab: &RootVocab,
        may: &MaySet,
        seeds: impl IntoIterator<Item = GroundFact>,
    ) -> Self {
        let clauses = Clauses::new(vocab);
        let seeds: BTreeSet<GroundFact> = seeds
            .into_iter()
            .filter(|f| clauses.relevant.contains(&f.relation))
            .collect();
        let base: Vec<MustFact> = seeds
            .iter()
            .map(|fact| MustFact {
                fact: fact.clone(),
                provenance: Provenance::Seed,
            })
            .collect();
        let seed_derived = Arc::new(clauses.derive(may, &base));
        MustClosure {
            clauses,
            may: may.clone(),
            seeds: seeds.into_iter().collect(),
            seed_derived,
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// The derived facts over `base` — a must set: fact-sorted, each fact
    /// once ([`derive_guaranteed`]'s answer).
    pub fn derived(&self, base: &[MustFact]) -> Arc<Vec<MustFact>> {
        let mut extra = Vec::new();
        let mut missing = Vec::new();
        let mut s = 0;
        for m in base
            .iter()
            .filter(|m| self.clauses.relevant.contains(&m.fact.relation))
        {
            while s < self.seeds.len() && self.seeds[s] < m.fact {
                missing.push(s);
                s += 1;
            }
            if s < self.seeds.len() && self.seeds[s] == m.fact {
                s += 1;
                if m.provenance == Provenance::Seed {
                    continue;
                }
            }
            extra.push(m.clone());
        }
        missing.extend(s..self.seeds.len());
        if extra.is_empty() && missing.is_empty() {
            return Arc::clone(&self.seed_derived);
        }
        let key = ClosureKey { extra, missing };
        if let Some(hit) = self
            .memo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return Arc::clone(hit);
        }
        let derived = Arc::new(self.clauses.derive(&self.may, base));
        Arc::clone(
            self.memo
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(key)
                .or_insert(derived),
        )
    }
}

/// One fact of a slot's must set, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MustFact {
    pub fact: GroundFact,
    pub provenance: Provenance,
}

/// The path-sensitive must sets (§4), keyed by slot identity: the document
/// path and the guard slot's byte range. A slot with no entry has an empty
/// must set.
#[derive(Clone, Debug, Default)]
pub struct MustMap {
    slots: BTreeMap<PathBuf, BTreeMap<(usize, usize), Arc<SlotFacts>>>,
}

/// One slot's must set, assembled when first read: the facts it shares with
/// other slots, its own, and their derived closure.
#[derive(Debug)]
struct SlotFacts {
    /// Facts shared with other slots (fact-sorted); an `own` fact overrides
    /// the shared one's provenance.
    shared: Option<Arc<Vec<MustFact>>>,
    /// The slot's own facts, fact-sorted.
    own: Vec<MustFact>,
    closure: Option<Arc<MustClosure>>,
    /// The whole set, filled on the first read.
    full: OnceLock<Vec<MustFact>>,
}

impl SlotFacts {
    fn facts(&self) -> &[MustFact] {
        if self.shared.is_none() && self.closure.is_none() {
            return &self.own;
        }
        self.full.get_or_init(|| {
            let mut all = match &self.shared {
                Some(shared) => merge_facts(shared, &self.own),
                None => self.own.clone(),
            };
            if let Some(closure) = &self.closure {
                let derived = closure.derived(&all);
                all.extend(derived.iter().cloned());
            }
            all
        })
    }
}

/// Two fact-sorted must sets as one; a fact in both keeps `over`'s entry.
fn merge_facts(under: &[MustFact], over: &[MustFact]) -> Vec<MustFact> {
    let mut out = Vec::with_capacity(under.len() + over.len());
    let (mut u, mut o) = (under.iter().peekable(), over.iter().peekable());
    loop {
        let next = match (u.peek(), o.peek()) {
            (None, None) => return out,
            (Some(_), None) => u.next(),
            (None, Some(_)) => o.next(),
            (Some(a), Some(b)) => match a.fact.cmp(&b.fact) {
                std::cmp::Ordering::Less => u.next(),
                std::cmp::Ordering::Greater => o.next(),
                std::cmp::Ordering::Equal => {
                    u.next();
                    o.next()
                }
            },
        };
        out.extend(next.cloned());
    }
}

impl MustMap {
    /// Record the facts guaranteed at the slot `span` of document `path`
    /// (appending to anything already recorded there).
    pub fn insert(&mut self, path: &Path, span: Span, facts: impl IntoIterator<Item = MustFact>) {
        self.put(
            path,
            span,
            SlotFacts {
                shared: None,
                own: facts.into_iter().collect(),
                closure: None,
                full: OnceLock::new(),
            },
        );
    }

    /// Record the must set `shared` ∪ `own` at the slot (both fact-sorted;
    /// an `own` fact overrides a shared one's provenance). It is assembled,
    /// with the facts `closure` derives over it, when the slot is first read
    /// — a slot nothing reads never computes them (T2-1).
    pub fn insert_closed(
        &mut self,
        path: &Path,
        span: Span,
        shared: &Arc<Vec<MustFact>>,
        own: Vec<MustFact>,
        closure: &Arc<MustClosure>,
    ) {
        self.put(
            path,
            span,
            SlotFacts {
                shared: Some(Arc::clone(shared)),
                own,
                closure: Some(Arc::clone(closure)),
                full: OnceLock::new(),
            },
        );
    }

    fn put(&mut self, path: &Path, span: Span, new: SlotFacts) {
        match self
            .slots
            .entry(path.to_path_buf())
            .or_default()
            .entry((span.byte_start, span.byte_end))
        {
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(Arc::new(new));
            }
            std::collections::btree_map::Entry::Occupied(mut o) => {
                let mut all = o.get().facts().to_vec();
                all.extend_from_slice(new.facts());
                o.insert(Arc::new(SlotFacts {
                    shared: None,
                    own: all,
                    closure: None,
                    full: OnceLock::new(),
                }));
            }
        }
    }

    /// The facts guaranteed at the slot `span` of document `path`.
    pub fn at(&self, path: &Path, span: Span) -> &[MustFact] {
        self.slots
            .get(path)
            .and_then(|slots| slots.get(&(span.byte_start, span.byte_end)))
            .map_or(&[], |slot| slot.facts())
    }
}

/// The two sets of one project root (§3, §4).
#[derive(Clone, Debug, Default)]
pub struct FactEnv {
    pub may: MaySet,
    pub must: MustMap,
    /// dsl 0.23.0 §10 (`check-project --wip` only): `may` widened so every
    /// relation nothing produces yet may hold anything
    /// ([`MaySet::widened`]). A guard that is dead under `may` but not under
    /// this set is dead only because content is not written yet.
    pub wip: Option<MaySet>,
}

/// The §5 verdict for `holds(P)` at one slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldsVerdict<'e> {
    /// No fact in `May` matches `P`.
    Impossible,
    /// A fact in `Must(slot)` matches `P` — the first such fact.
    Guaranteed(&'e MustFact),
    /// dsl 0.25.0 §1: a fact in `Must(slot)` of a relation that excludes
    /// `P`'s, on `P`'s arguments — the first such fact. `holds(P)` is false.
    Excluded(&'e MustFact),
    /// Both outcomes are reachable, or the analysis cannot separate them
    /// (also: an undeclared relation or a wrong arity, owned elsewhere).
    Possible,
}

/// `count(P)` at one slot lies in `[lo, hi]`; `hi == None` is ∞.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountInterval {
    pub lo: usize,
    pub hi: Option<usize>,
}

impl CountInterval {
    /// Decide `count(P) <op> n` over the interval, `op` being a CEL
    /// comparison operator name; `None` when the interval straddles the
    /// threshold (or `op` is not a comparison).
    pub fn compare(&self, op: &str, n: f64) -> Option<bool> {
        use cel_parser::ast::operators as o;
        let lo = self.lo as f64;
        let hi = self.hi.map_or(f64::INFINITY, |h| h as f64);
        let decided = |always: bool, never: bool| {
            if always {
                Some(true)
            } else if never {
                Some(false)
            } else {
                None
            }
        };
        match op {
            o::GREATER => decided(lo > n, hi <= n),
            o::GREATER_EQUALS => decided(lo >= n, hi < n),
            o::LESS => decided(hi < n, lo >= n),
            o::LESS_EQUALS => decided(hi <= n, lo > n),
            o::EQUALS => decided(lo == hi && lo == n, n < lo || n > hi || n.fract() != 0.0),
            o::NOT_EQUALS => self.compare(o::EQUALS, n).map(|eq| !eq),
            _ => None,
        }
    }
}

impl FactEnv {
    pub fn new(may: MaySet, must: MustMap) -> Self {
        FactEnv {
            may,
            must,
            wip: None,
        }
    }

    /// dsl 0.23.0 §10: this envelope plus its work-in-progress twin, where
    /// every relation in `unproduced` (no seed, assert, rule, or reserved
    /// declaration — [`RootVocab::unproduced`]) is unbounded.
    pub fn with_wip(mut self, vocab: &RootVocab, unproduced: &BTreeSet<String>) -> Self {
        self.wip = Some(self.may.widened(vocab, unproduced));
        self
    }

    /// The may set a scope reads: the work-in-progress twin when asked for
    /// and present.
    fn may_set(&self, wip: bool) -> &MaySet {
        match &self.wip {
            Some(widened) if wip => widened,
            _ => &self.may,
        }
    }

    /// §5's `holds(P)` verdict at the slot `span` of document `path`.
    fn holds(&self, path: &Path, span: Span, q: &QueryPattern, wip: bool) -> HoldsVerdict<'_> {
        let may = self.may_set(wip);
        if !may.decides(q) {
            return HoldsVerdict::Possible;
        }
        if let Some(m) = self.must.at(path, span).iter().find(|m| q.matches(&m.fact)) {
            return HoldsVerdict::Guaranteed(m);
        }
        if may.any_match(q) {
            HoldsVerdict::Possible
        } else {
            HoldsVerdict::Impossible
        }
    }

    /// §5's `count(P)` interval at the slot — with `column`, the interval of
    /// `countDistinct(P, V)` (distinct values at that position, dsl 0.24
    /// T3-9); `None` for a query this set does not decide.
    fn count(
        &self,
        path: &Path,
        span: Span,
        q: &QueryPattern,
        column: Option<usize>,
        wip: bool,
    ) -> Option<CountInterval> {
        let may = self.may_set(wip);
        if !may.decides(q) {
            return None;
        }
        let mut guaranteed: Vec<&GroundFact> = self
            .must
            .at(path, span)
            .iter()
            .map(|m| &m.fact)
            .filter(|f| q.matches(f))
            .collect();
        guaranteed.sort();
        guaranteed.dedup();
        Some(CountInterval {
            lo: tally(guaranteed.iter().map(|f| f.args.as_slice()), column),
            hi: may.count_matching_in(q, column),
        })
    }
}

/// A [`FactEnv`] bound to one guard slot of one document — what
/// `DecideCtx::facts` carries so `decide()` can resolve the relational calls
/// of that slot. `vocab` is the document's own merged vocabulary: a query
/// over a relation it does not declare (or with the wrong arity) is that
/// document's `E-RELATION-UNKNOWN`/`E-RELATION-ARITY` and is never decided,
/// even when a sibling document declares the relation.
#[derive(Clone, Copy, Debug)]
pub struct FactScope<'a> {
    pub env: &'a FactEnv,
    pub vocab: &'a RelVocab,
    pub path: &'a Path,
    pub span: Span,
    /// Read the envelope's work-in-progress twin ([`FactEnv::wip`]).
    pub wip: bool,
}

impl<'a> FactScope<'a> {
    fn in_vocab(&self, q: &QueryPattern) -> bool {
        self.vocab
            .relations
            .get(&q.relation)
            .is_some_and(|d| d.args.len() == q.args.len())
    }

    /// [`MaySet::defeat`] over the may set this scope reads.
    pub fn defeat(&self, q: &QueryPattern) -> Option<(&'a GroundFact, &'a GroundFact)> {
        self.env.may_set(self.wip).defeat(q)
    }

    pub fn holds(&self, q: &QueryPattern) -> HoldsVerdict<'a> {
        if !self.in_vocab(q) {
            return HoldsVerdict::Possible;
        }
        match self.env.holds(self.path, self.span, q, self.wip) {
            HoldsVerdict::Possible => self
                .excluding(q)
                .map_or(HoldsVerdict::Possible, HoldsVerdict::Excluded),
            v => v,
        }
    }

    /// dsl 0.25.0 §1: a guaranteed fact that rules out every instance of `q`
    /// — for a ground `q`, a must fact of an excluding relation on the same
    /// arguments; for a pattern with `_`, every `May` instance is so ruled
    /// out (a pattern over an unbounded relation never is).
    fn excluding(&self, q: &QueryPattern) -> Option<&'a MustFact> {
        let must = self.env.must.at(self.path, self.span);
        let rules_out = |args: &[String]| {
            must.iter()
                .find(|m| m.fact.args == args && self.vocab.excludes(&m.fact.relation, &q.relation))
        };
        if let Some(g) = q.ground() {
            return rules_out(&g.args);
        }
        let may = self.env.may_set(self.wip);
        if !may.decides(q) || may.is_unbounded(&q.relation) {
            return None;
        }
        let mut first = None;
        for args in may.instances(&q.relation)? {
            if q.matches_args(args) {
                let m = rules_out(args)?;
                first.get_or_insert(m);
            }
        }
        first
    }

    pub fn count(&self, q: &QueryPattern) -> Option<CountInterval> {
        self.count_in(q, None)
    }

    /// [`FactScope::count`]; with `column`, `countDistinct`'s interval.
    pub fn count_in(&self, q: &QueryPattern, column: Option<usize>) -> Option<CountInterval> {
        if !self.in_vocab(q) {
            return None;
        }
        self.env.count(self.path, self.span, q, column, self.wip)
    }
}
