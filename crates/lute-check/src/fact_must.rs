//! The path-sensitive must set (dsl 0.20.0 §4): for every guard slot of one
//! project root, the ground facts live on **every** route reaching it, each
//! with the site that establishes it ([`Provenance`]). The result fills the
//! [`MustMap`] half of [`crate::fact_env::FactEnv`].
//!
//! **Stability.** A fact crosses time the author does not control — a
//! document boundary, an engine-chosen quest/entry moment — only when it is
//! *monotone*: no `::retract` pattern anywhere in the root matches it, no
//! other assert (or seed) shares its `key:` tuple with a different value, and
//! its relation is neither reserved, unbounded, nor derived. `tier: scene`
//! facts never cross a document boundary at all, nor do `tier: quest` facts
//! (the engine clears them with their quest, at a time no document controls).
//!
//! **Within a document** the walk is a forward must-dataflow over the node
//! stream with the fork/join shape of definite assignment
//! ([`crate::defassign`]), except that a join here always INTERSECTS the
//! incoming set too whenever a construct may be skipped (defassign's
//! write-only lattice can keep the pre-block set, but a retract inside an arm
//! can shrink a must set, so the fall-through route must be met, not kept):
//!
//! - `::assert{F}` adds `F` and removes every fact `F` displaces through
//!   `key:`; `::retract{P}` removes every fact matching `P`. Either drops the
//!   derived facts a guard assumed (their support may have changed).
//! - `<branch>` / `<match>` arms fork and meet; a branch without an
//!   unconditional choice, or a non-exhaustive match, also meets the
//!   pre-block set. A `<hub>` body runs zero or more times: its entry set is
//!   the greatest fixpoint `X = pre ∩ ⋂ arm_out(X)`, which is also its exit.
//! - `::end` sends the current set to the document's exit; `::next{to}` sends
//!   it (with its own guard assumed, when guarded) to its label, where it is
//!   met with the fall-through route; an unguarded `::next` ends the
//!   fall-through route.
//! - A guard is an assumption inside its region (D-D): every positive
//!   top-level conjunct `holds(F)` of a `<when test>`, `<choice when>`,
//!   guarded `::next`, `<on when>`, `<objective done>`, entry `when`, or scene
//!   beat `when` (dsl 0.21.0 §3.1) with a ground `F` is in the set inside it.
//!   A content line's `when=` guards nothing after it.
//!
//! At every guard slot the set BEFORE the slot's own assumption is recorded
//! (a slot visited more than once — a hub body — keeps the intersection of
//! its visits), and the derived facts its rules produce over it are added
//! ([`derive_guaranteed`]) when the map is filled.
//!
//! **Entry points.** A scene starts from its `Must_in`: the monotone seeds
//! plus the monotone facts its `after:` formula guarantees (`visited(A)` →
//! `Must_out(A)`, `&&` unions, `||` intersects, `completed`/`active`
//! nothing), memoized over the connectivity graph's topological order like
//! the scalar envelope (`crate::envelope::propagate`); a scene on or past a
//! cycle, or one the graph does not host, starts from the seeds alone. A
//! beat scene's `when` slot sees `Must_in` (a beat is eligible only once its
//! `after:` holds) and its assumptions hold throughout the scene, which runs
//! as soon as the beat is chosen (dsl 0.21.0 §4).
//! `Must_out` is the meet of every route to the document's end (`::end`
//! included), restricted to crossing facts. Quest bodies start from the
//! seeds plus the crossing facts `start` assumes (plus `<on when>` /
//! `<objective done>` inside their own bodies); a lore entry body starts from
//! the seeds plus its `when`'s assumptions. Quest `start`/`fail` and entry
//! `when` slots see the seeds.
//!
//! **Entry reads (dsl 0.24.0 §6).** A guard conjunct `entry.X.read` (or
//! `== true`) assumes the crossing facts entry X's body guarantees on every
//! route through it — its effects run on the first read of each run, and a
//! crossing fact nothing removes still holds afterwards. `entry.X.everRead`
//! assumes only the `tier: user` / `tier: app` ones: a later run's
//! `everRead` does not re-run the effects, and the run tier has been reset.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cel_parser::ast::{operators as op, Expr};
use lute_core_span::Span;
use lute_syntax::ast::{
    Arm, Assert, Attr, AttrValue, CelSlot, Choice, Directive, Document, Match, Node, Retract,
};
use lute_syntax::datalog::FactTerm;

use crate::cel_expand::{expand_cel, DefTable};
use crate::check::FoldedEnv;
use crate::connectivity::{ConnGraph, NodeId, PrereqState};
use crate::fact_env::{
    GroundFact, MaySet, MustClosure, MustFact, MustMap, Provenance, QueryPattern, RootVocab,
};
use crate::meta::StateSchema;
use crate::prereq::PrereqFormula;
use crate::rel_schema::RelVocab;

/// Guaranteed facts, each with where it is established.
type FactMap = BTreeMap<GroundFact, Provenance>;

/// The must set on one route: the root's seeds — every route holds them (a
/// seed is monotone: no retract matches it, no produced fact displaces it,
/// its relation is not derived — so no transfer below removes it) — plus
/// `own`: every other fact, and a seed whose provenance on this route is
/// not `Seed` (an `::assert` of it). The seeds are shared, so copying a set
/// costs what the route added, not the seed count (T2-1).
#[derive(Clone, Debug)]
struct Facts {
    seeds: Arc<FactMap>,
    own: FactMap,
}

impl Facts {
    fn contains_key(&self, f: &GroundFact) -> bool {
        self.own.contains_key(f) || self.seeds.contains_key(f)
    }

    /// Add `fact` unless it already holds here (a map's `or_insert`).
    fn add(&mut self, fact: GroundFact, provenance: Provenance) {
        if !self.seeds.contains_key(&fact) {
            self.own.entry(fact).or_insert(provenance);
        }
    }

    /// Hold `fact`, established at `provenance` (never `Seed`).
    fn insert(&mut self, fact: GroundFact, provenance: Provenance) {
        self.own.insert(fact, provenance);
    }

    /// Keep the facts `keep` accepts; every seed is kept — each caller's
    /// predicate accepts it (see the type doc).
    fn retain(&mut self, mut keep: impl FnMut(&GroundFact) -> bool) {
        debug_assert!(
            self.seeds.keys().all(&mut keep),
            "a transfer removed a seed"
        );
        self.own.retain(|f, _| keep(f));
    }

    /// The facts that are not seeds, with their provenance.
    fn beyond_seeds(self) -> FactMap {
        let seeds = self.seeds;
        let mut own = self.own;
        own.retain(|f, _| !seeds.contains_key(f));
        own
    }

    /// Same facts (provenance ignored): the seeds are shared, so the
    /// non-seed facts decide.
    fn same_facts(&self, other: &Facts) -> bool {
        fn extra(s: &Facts) -> impl Iterator<Item = &GroundFact> {
            s.own.keys().filter(|f| !s.seeds.contains_key(*f))
        }
        extra(self).eq(extra(other))
    }

    /// `own` as must facts (a seed here overrides the shared one).
    fn own_facts(self) -> Vec<MustFact> {
        self.own
            .into_iter()
            .map(|(fact, provenance)| MustFact { fact, provenance })
            .collect()
    }
}

/// The must set on the current route; `None` = no route reaches here (the
/// identity of [`meet`]).
type Flow = Option<Facts>;

/// The must analysis of one project root.
#[derive(Clone, Debug, Default)]
pub struct FactMust {
    /// Every guard slot's guaranteed facts, derived closure included.
    pub slots: MustMap,
    /// Per scene key (`NodeId::Scene`) and bundle beat key (`NodeId::Beat`,
    /// an `after`-less entry: the seeds): the facts guaranteed on arrival
    /// (`Must_in`), derived closure included — `lute scenario`'s fact
    /// envelope.
    pub scene_entry: BTreeMap<String, Vec<MustFact>>,
}

/// Compute the root's must sets. `docs` and `foldeds` are parallel (one
/// resolved root); `graph` is its connectivity graph; `vocab`/`may` its root
/// vocabulary and may set (negated rule atoms are decided against `may`, so
/// any sound — i.e. not smaller than the true — may set keeps this sound).
pub fn compute_must(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    graph: &ConnGraph,
    vocab: &RootVocab,
    may: &MaySet,
) -> FactMust {
    let mut root = Root::new(docs, vocab, may);
    root.entry_reads = entry_outcomes(&root, docs, foldeds);
    // T2-1: the derived closure, prepared once for the root (its seed
    // closure included) and applied to a slot only when the slot is read.
    let closure = Arc::new(MustClosure::new(vocab, may, root.seeds.keys().cloned()));
    let mut out = FactMust::default();
    let mut must_out: BTreeMap<String, FactMap> = BTreeMap::new();
    let mut walked = vec![false; docs.len()];
    // First index per path — `position`'s answer, without a scan per node.
    let mut doc_ix: std::collections::HashMap<&std::path::Path, usize> =
        std::collections::HashMap::with_capacity(docs.len());
    for (idx, (p, _)) in docs.iter().enumerate() {
        doc_ix.entry(p.as_path()).or_insert(idx);
    }
    for id in &graph.topo_order {
        let NodeId::Scene(key) = id else {
            continue;
        };
        let Some(info) = graph.nodes.get(id) else {
            continue;
        };
        let Some(&idx) = doc_ix.get(info.path.as_path()) else {
            continue;
        };
        if walked[idx] {
            continue;
        }
        walked[idx] = true;
        let mut entry = root.start();
        if let PrereqState::Valid(f) = &info.prereq {
            for (fact, provenance) in after_facts(f, &must_out) {
                entry.add(fact, provenance);
            }
        }
        out.scene_entry
            .insert(key.clone(), with_derived(&closure, entry.clone()));
        let end = walk_doc(
            &root,
            &closure,
            &docs[idx],
            foldeds[idx],
            entry,
            &mut out.slots,
        );
        // Only what the route adds to the seeds matters downstream: an
        // `after:` set only ever adds to a walk's seeds.
        let mut end = end.map(Facts::beyond_seeds).unwrap_or_default();
        end.retain(|f, _| root.crosses(f));
        must_out.insert(key.clone(), end);
    }
    // Scenes on or past a cycle (absent from `topo_order`), duplicates and
    // unidentifiable scenes, bundle beats, quest and lore documents: seeds
    // only.
    for (idx, doc) in docs.iter().enumerate() {
        if !walked[idx] {
            walk_doc(
                &root,
                &closure,
                doc,
                foldeds[idx],
                root.start(),
                &mut out.slots,
            );
        }
    }
    for (key, info) in &graph.nodes {
        if let NodeId::Scene(k) | NodeId::Beat(k) = key {
            if !out.scene_entry.contains_key(k) && doc_ix.contains_key(info.path.as_path()) {
                out.scene_entry
                    .insert(k.clone(), with_derived(&closure, root.start()));
            }
        }
    }
    out
}

/// The non-seed facts an `after:` formula guarantees on arrival (§4 entry
/// points; `must_out` holds each scene's non-seed crossing facts).
fn after_facts(f: &PrereqFormula, must_out: &BTreeMap<String, FactMap>) -> FactMap {
    match f {
        PrereqFormula::Visited(key) => must_out.get(key).cloned().unwrap_or_default(),
        PrereqFormula::Completed(_) | PrereqFormula::Active(_) => FactMap::new(),
        PrereqFormula::And(l, r) => {
            let mut out = after_facts(l, must_out);
            for (fact, provenance) in after_facts(r, must_out) {
                out.entry(fact).or_insert(provenance);
            }
            out
        }
        PrereqFormula::Or(l, r) => {
            let mut out = after_facts(l, must_out);
            let right = after_facts(r, must_out);
            out.retain(|fact, _| right.contains_key(fact));
            out
        }
    }
}

/// `facts` materialized, then their derived facts (the scene entry sets).
fn with_derived(closure: &MustClosure, facts: Facts) -> Vec<MustFact> {
    let mut base: Vec<MustFact> = facts
        .seeds
        .iter()
        .filter(|(f, _)| !facts.own.contains_key(*f))
        .chain(facts.own.iter())
        .map(|(fact, provenance)| MustFact {
            fact: fact.clone(),
            provenance: provenance.clone(),
        })
        .collect();
    base.sort_by(|a, b| a.fact.cmp(&b.fact));
    let derived = closure.derived(&base);
    base.extend(derived.iter().cloned());
    base
}

/// `acc ∩= other`, `None` being the identity (an unreached route).
fn meet(acc: &mut Flow, other: Flow) {
    match (acc.as_mut(), other) {
        (_, None) => {}
        (None, other) => *acc = other,
        (Some(a), Some(o)) => a.retain(|f| o.contains_key(f)),
    }
}

/// Same facts (provenance ignored) — the hub fixpoint's convergence test.
fn same(a: &Flow, b: &Flow) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_facts(b),
        _ => false,
    }
}

fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.as_str()),
            _ => None,
        })
}

/// Root-wide stability facts (see the module doc).
struct Root<'a> {
    vocab: &'a RootVocab,
    may: &'a MaySet,
    /// Every `::retract` pattern anywhere in the root.
    retracts: Vec<QueryPattern>,
    /// Per `key:`-declaring relation: each `key:` tuple of an asserted or
    /// seeded ground fact → the distinct argument tuples carrying it (at most
    /// two kept: a fact is displaced iff one of them is not its own). Built
    /// once, so [`Root::monotone`] costs a lookup instead of a scan of every
    /// produced tuple — which made the must walk quadratic in the seed count
    /// (T2-1).
    key_groups: BTreeMap<String, HashMap<Vec<String>, Vec<Vec<String>>>>,
    /// Every relation some `::assert` writes, ground or not.
    asserted: BTreeSet<String>,
    /// dsl 0.26.0 §2.6: every relation some component `::assert` writes with
    /// an unbound `@param` argument (`hasBadge(@badge)` in the component's
    /// own body) — a producer of whatever its future `::use` sites pass.
    param_asserted: BTreeSet<String>,
    /// The monotone, crossing seeds (all `Seed`), shared by every must set.
    seeds: Arc<FactMap>,
    /// Entry id → the non-seed crossing facts its body guarantees on every
    /// route (§6); empty until [`entry_outcomes`] fills it.
    entry_reads: BTreeMap<String, FactMap>,
}

/// dsl 0.24.0 §6: for every lore entry of the root, the crossing facts its
/// body guarantees on every route (seeds and `when` assumptions included —
/// they held when it was read and nothing removes a crossing fact) beyond
/// the seeds every route holds anyway. An entry id declared twice keeps the
/// facts both guarantee.
fn entry_outcomes(
    root: &Root<'_>,
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> BTreeMap<String, FactMap> {
    let mut out: BTreeMap<String, FactMap> = BTreeMap::new();
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        for entry in doc.entries.iter().filter(|e| !e.id.is_empty()) {
            let mut w = Walk::new(root, path, folded);
            let mut flow = Some(root.start());
            if let Some(when) = &entry.when {
                w.assume(when, &mut flow);
            }
            w.body_base = flow.clone().unwrap_or_else(|| root.start());
            w.walk(&entry.body, &mut flow);
            let mut end = w.exit.take();
            meet(&mut end, flow);
            let mut end = end.map(Facts::beyond_seeds).unwrap_or_default();
            end.retain(|f, _| root.crosses(f));
            match out.entry(entry.id.clone()) {
                Entry::Vacant(v) => {
                    v.insert(end);
                }
                Entry::Occupied(mut o) => o.get_mut().retain(|f, _| end.contains_key(f)),
            }
        }
    }
    out
}

/// dsl 0.23.0 §9: the seeds that hold at every point of every run of the
/// root — monotone and crossing, the set every must walk starts from.
/// [`MaySet::build`] reads a negated rule atom over one of them as false.
/// Stability reads the vocabulary and the root's writes, never a may set.
pub fn stable_seeds(docs: &[(PathBuf, Document)], vocab: &RootVocab) -> BTreeSet<GroundFact> {
    Root::new(docs, vocab, &MaySet::default())
        .seeds
        .keys()
        .cloned()
        .collect()
}

/// dsl 0.23.0 §10 / 0.26.0 §2.6 (`check-project --wip`): the relations of
/// the root whose facts content not yet written may still produce — those
/// nothing produces at all (no seed, no `::assert` anywhere, no rule, not
/// reserved — [`RootVocab::unproduced`]), plus those a component `::assert`
/// writes with an unbound `@param` (`hasBadge(@badge)`): a specific atom
/// (`hasBadge(stone)`) no `::use` produces yet has only that producer, so
/// it counts as unproduced for its arguments.
pub fn unproduced_relations(docs: &[(PathBuf, Document)], vocab: &RootVocab) -> BTreeSet<String> {
    let may = MaySet::default();
    let root = Root::new(docs, vocab, &may);
    let mut open = vocab.unproduced(&root.asserted);
    open.extend(
        root.param_asserted
            .into_iter()
            .filter(|r| vocab.relations.get(r).is_some_and(|d| !d.reserved)),
    );
    open
}

impl<'a> Root<'a> {
    fn new(docs: &[(PathBuf, Document)], vocab: &'a RootVocab, may: &'a MaySet) -> Self {
        let mut retracts = Vec::new();
        let mut produced: BTreeMap<String, BTreeSet<Vec<String>>> = BTreeMap::new();
        let mut asserted = BTreeSet::new();
        let mut param_asserted = BTreeSet::new();
        for seed in &vocab.seeds {
            produced
                .entry(seed.relation.clone())
                .or_default()
                .insert(seed.args.clone());
        }
        for (_, doc) in docs {
            let bodies = doc
                .shots
                .iter()
                .map(|s| &s.body)
                .chain(doc.quests.iter().map(|q| &q.body))
                .chain(doc.entries.iter().map(|e| &e.body))
                .chain(doc.beats.iter().map(|b| &b.body));
            for body in bodies {
                scan(body, &mut |node| match node {
                    Node::Assert(a) => {
                        asserted.insert(a.pattern.relation.clone());
                        if a.pattern
                            .args
                            .iter()
                            .any(|x| matches!(x.term, FactTerm::Param(_)))
                        {
                            param_asserted.insert(a.pattern.relation.clone());
                        }
                        if let Some(f) = GroundFact::from_pattern(&a.pattern) {
                            produced.entry(f.relation).or_default().insert(f.args);
                        }
                    }
                    Node::Retract(r) => {
                        if let Some(q) = QueryPattern::from_fact_pattern(&r.pattern) {
                            retracts.push(q);
                        }
                    }
                    _ => {}
                });
            }
        }
        let mut key_groups: BTreeMap<String, HashMap<Vec<String>, Vec<Vec<String>>>> =
            BTreeMap::new();
        for (relation, tuples) in &produced {
            let Some(decl) = vocab.relations.get(relation) else {
                continue;
            };
            if decl.key.is_empty() {
                continue;
            }
            let groups = key_groups.entry(relation.clone()).or_default();
            for args in tuples {
                let key = key_of(&decl.key, args)
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                let group = groups.entry(key).or_default();
                if group.len() < 2 {
                    group.push(args.clone());
                }
            }
        }
        let mut root = Root {
            vocab,
            may,
            retracts,
            key_groups,
            asserted,
            param_asserted,
            seeds: Arc::default(),
            entry_reads: BTreeMap::new(),
        };
        root.seeds = Arc::new(
            vocab
                .seeds
                .iter()
                .filter(|f| root.crosses(f))
                .map(|f| (f.clone(), Provenance::Seed))
                .collect(),
        );
        root
    }

    /// The must set every walk starts from: the seeds alone.
    fn start(&self) -> Facts {
        Facts {
            seeds: Arc::clone(&self.seeds),
            own: FactMap::new(),
        }
    }

    /// The seeds as must facts, fact-sorted — the part every slot shares.
    fn seed_facts(&self) -> Arc<Vec<MustFact>> {
        Arc::new(
            self.seeds
                .iter()
                .map(|(fact, provenance)| MustFact {
                    fact: fact.clone(),
                    provenance: provenance.clone(),
                })
                .collect(),
        )
    }

    fn is_derived(&self, relation: &str) -> bool {
        self.vocab.relations.get(relation).is_some_and(|d| d.derive)
    }

    /// The `key:` tuple of `f`: `None` when its relation declares no key (see
    /// [`key_of`]).
    fn key_tuple<'f>(&self, f: &'f GroundFact) -> Option<Vec<&'f str>> {
        let decl = self.vocab.relations.get(&f.relation)?;
        if decl.key.is_empty() {
            return None;
        }
        Some(key_of(&decl.key, &f.args))
    }

    /// `true` iff asserting `new` removes `old` through `key:`.
    fn displaces(&self, new: &GroundFact, old: &GroundFact) -> bool {
        new.relation == old.relation
            && new.args != old.args
            && matches!((self.key_tuple(new), self.key_tuple(old)), (Some(a), Some(b)) if a == b)
    }

    /// §4 stability: nothing anywhere can remove `f` once it holds.
    fn monotone(&self, f: &GroundFact) -> bool {
        let Some(decl) = self.vocab.relations.get(&f.relation) else {
            return false;
        };
        if decl.derive || decl.reserved || self.vocab.is_unbounded(&f.relation, decl) {
            return false;
        }
        if self.retracts.iter().any(|q| q.matches(f)) {
            return false;
        }
        // Displaced iff some produced tuple of `f`'s relation shares its
        // `key:` tuple with other arguments (`Self::displaces`).
        !self.key_groups.get(&f.relation).is_some_and(|groups| {
            let key: Vec<String> = key_of(&decl.key, &f.args)
                .into_iter()
                .map(str::to_string)
                .collect();
            groups
                .get(&key)
                .is_some_and(|group| group.iter().any(|args| *args != f.args))
        })
    }

    /// Monotone and neither `tier: scene` nor `tier: quest` — may cross a
    /// document boundary.
    fn crosses(&self, f: &GroundFact) -> bool {
        self.monotone(f)
            && self
                .vocab
                .relations
                .get(&f.relation)
                .is_some_and(|d| !matches!(d.tier.as_deref(), Some("scene" | "quest")))
    }
}

/// The `key:` tuple (`key`, non-empty) of a fact with `args`; an
/// out-of-range index yields the empty tuple, which every fact of the
/// relation shares (the malformed key is `E-RELATION-DOMAIN`'s; treating it
/// as maximally displacing keeps the must set sound).
fn key_of<'f>(key: &[i64], args: &'f [String]) -> Vec<&'f str> {
    let mut out = Vec::with_capacity(key.len());
    for &i in key {
        match usize::try_from(i).ok().and_then(|i| args.get(i)) {
            Some(a) => out.push(a.as_str()),
            None => return Vec::new(),
        }
    }
    out
}

/// Visit every node of `nodes`, nested bodies included.
fn scan<'n>(nodes: &'n [Node], f: &mut impl FnMut(&'n Node)) {
    for node in nodes {
        f(node);
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| scan(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| scan(&c.body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    scan(body, f);
                }
            }
            Node::On(o) => scan(&o.body, f),
            Node::Objective(o) => scan(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// Walk one document from `entry` (its shots) and its quest / entry bodies
/// from their own bases; record every guard slot into `slots` (its derived
/// facts under `closure` are added when the slot is read). Returns the meet
/// of every route to the end of the shots (`None`: no route reaches it).
fn walk_doc(
    root: &Root<'_>,
    closure: &Arc<MustClosure>,
    (path, doc): &(PathBuf, Document),
    folded: &FoldedEnv,
    entry: Facts,
    slots: &mut MustMap,
) -> Flow {
    let mut w = Walk::new(root, path, folded);
    let mut flow = Some(entry);
    if let Some(when) = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()) {
        w.guard(when, &mut flow);
    }
    for shot in &doc.shots {
        w.walk(&shot.body, &mut flow);
    }
    let mut end = w.exit.take();
    meet(&mut end, flow);

    for quest in &doc.quests {
        let seeds = Some(root.start());
        for slot in [&quest.start, &quest.fail].into_iter().flatten() {
            w.record(slot, &seeds);
        }
        let mut base = root.start();
        if let Some(start) = &quest.start {
            for (fact, provenance) in w.assumptions(start) {
                if root.crosses(&fact) {
                    base.add(fact, provenance);
                }
            }
        }
        w.body_base = base.clone();
        let mut flow = Some(base);
        w.walk(&quest.body, &mut flow);
    }
    for entry in &doc.entries {
        let mut base = Some(root.start());
        if let Some(when) = &entry.when {
            w.guard(when, &mut base);
        }
        w.body_base = base.clone().unwrap_or_else(|| root.start());
        w.walk(&entry.body, &mut base);
    }
    // dsl 0.23.0 §4: a bundle beat is presented on its own like an entry
    // (it has no `after:`, so nothing but the seeds is known at its start).
    for beat in &doc.beats {
        let mut base = Some(root.start());
        if let Some(when) = &beat.when {
            w.guard(when, &mut base);
        }
        w.body_base = base.clone().unwrap_or_else(|| root.start());
        w.walk(&beat.body, &mut base);
    }

    let shared = root.seed_facts();
    for (span, facts) in w.slots.into_values() {
        slots.insert_closed(path, span, &shared, facts.own_facts(), closure);
    }
    end
}

/// One document's walk state.
struct Walk<'a> {
    root: &'a Root<'a>,
    path: &'a Path,
    defs: DefTable<'a>,
    schema: &'a StateSchema,
    /// The document's own vocabulary: a fact of a relation it does not
    /// declare is never tracked (its queries are never decided either).
    vocab: &'a RelVocab,
    /// Guard slot → (span, the meet of its visits' sets).
    slots: BTreeMap<(usize, usize), (Span, Facts)>,
    /// `::next{to}` label → the meet of the sets jumping to it.
    pending: BTreeMap<String, Facts>,
    /// The meet of every `::end` route.
    exit: Flow,
    /// Where an `<on>` / `<objective>` body starts (engine-chosen time).
    body_base: Facts,
}

impl<'a> Walk<'a> {
    fn new(root: &'a Root<'a>, path: &'a Path, folded: &'a FoldedEnv) -> Self {
        Walk {
            root,
            path,
            defs: DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            },
            schema: &folded.env.state,
            vocab: &folded.env.rel_vocab,
            slots: BTreeMap::new(),
            pending: BTreeMap::new(),
            exit: None,
            body_base: root.start(),
        }
    }

    fn guard_provenance(&self, slot: &CelSlot) -> Provenance {
        Provenance::Guard {
            path: self.path.to_path_buf(),
            line: slot.span.line,
        }
    }

    /// Record the set reaching guard `slot` (before its own assumption).
    fn record(&mut self, slot: &CelSlot, flow: &Flow) {
        self.record_span(slot.span, flow);
    }

    /// Record the set reaching the slot at `span`.
    fn record_span(&mut self, span: Span, flow: &Flow) {
        let Some(facts) = flow else {
            return;
        };
        match self.slots.entry((span.byte_start, span.byte_end)) {
            Entry::Vacant(v) => {
                v.insert((span, facts.clone()));
            }
            Entry::Occupied(mut o) => o.get_mut().1.retain(|f| facts.contains_key(f)),
        }
    }

    /// Add `slot`'s assumptions to `flow` (D-D).
    fn assume(&self, slot: &CelSlot, flow: &mut Flow) {
        let Some(facts) = flow else {
            return;
        };
        for (fact, provenance) in self.assumptions(slot) {
            facts.add(fact, provenance);
        }
    }

    /// Record, then assume.
    fn guard(&mut self, slot: &CelSlot, flow: &mut Flow) {
        self.record(slot, flow);
        self.assume(slot, flow);
    }

    /// The ground facts `slot`'s positive top-level `holds(F)` conjuncts
    /// require (provenance: this guard), plus what its `entry.X.read` /
    /// `entry.X.everRead` conjuncts guarantee (§6; provenance: where X's body
    /// establishes them), restricted to [`Self::trackable`] ones.
    fn assumptions(&self, slot: &CelSlot) -> Vec<(GroundFact, Provenance)> {
        if slot.raw.trim().is_empty() {
            return Vec::new();
        }
        let mut stack = Vec::new();
        let expanded = expand_cel(&slot.raw, &self.defs, Some("$"), &mut stack)
            .unwrap_or_else(|_| slot.raw.clone());
        let mut arena = lute_cel::CelArena::default();
        let Some(handle) = lute_cel::parse_slot_marked_refs(&mut arena, &expanded) else {
            return Vec::new();
        };
        let Some(node) = arena.get(handle) else {
            return Vec::new();
        };
        let mut conj = Vec::new();
        conjuncts(&node.expr, &mut conj);
        let mut facts: Vec<(GroundFact, Provenance)> = Vec::new();
        for c in conj {
            if let Some(f) = held_fact(c) {
                if self.guard_trackable(&f) {
                    facts.push((f, self.guard_provenance(slot)));
                }
            } else if let Some((id, ever)) = entry_read(c) {
                let Some(read) = self.root.entry_reads.get(&id) else {
                    continue;
                };
                facts.extend(
                    read.iter()
                        .filter(|(f, _)| (!ever || self.user_tier(f)) && self.trackable(f))
                        .map(|(f, p)| (f.clone(), p.clone())),
                );
            }
        }
        facts
    }

    /// A fact a guard's own `holds(F)` conjunct may put in the must set of
    /// its region: [`Self::trackable`], except that a reserved (or otherwise
    /// unbounded) relation counts too (crown M1, dsl 0.25.0 §1) — the engine
    /// changes such facts only between presentations (on its occasions), never
    /// inside the region the guard opens, and a region never outlives its
    /// presentation (only [`Root::crosses`] facts leave a document).
    fn guard_trackable(&self, f: &GroundFact) -> bool {
        let Some(decl) = self.vocab.relations.get(&f.relation) else {
            return false;
        };
        let q = QueryPattern {
            relation: f.relation.clone(),
            args: f.args.iter().cloned().map(Some).collect(),
        };
        decl.args.len() == f.args.len() && self.root.may.decides(&q)
    }

    /// A fact of a `tier: user` / `tier: app` relation — one a new run keeps.
    fn user_tier(&self, f: &GroundFact) -> bool {
        self.root
            .vocab
            .relations
            .get(&f.relation)
            .is_some_and(|d| matches!(d.tier.as_deref(), Some("user" | "app")))
    }

    /// A fact the must set may carry: a relation this document declares
    /// with this arity, arguments inside their closed domains, and neither
    /// reserved nor unbounded (the engine may change such facts at will).
    fn trackable(&self, f: &GroundFact) -> bool {
        let Some(decl) = self.vocab.relations.get(&f.relation) else {
            return false;
        };
        let q = QueryPattern {
            relation: f.relation.clone(),
            args: f.args.iter().cloned().map(Some).collect(),
        };
        decl.args.len() == f.args.len()
            && !decl.reserved
            && self.root.may.decides(&q)
            && !self.root.may.is_unbounded(&f.relation)
    }

    /// A label site: meet the jumps that target it.
    fn label(&self, id: &str, flow: &mut Flow) {
        if let Some(jumped) = self.pending.get(id) {
            meet(flow, Some(jumped.clone()));
        }
    }

    fn jump(&mut self, to: &str, flow: Flow) {
        let Some(facts) = flow else {
            return;
        };
        match self.pending.entry(to.to_string()) {
            Entry::Vacant(v) => {
                v.insert(facts);
            }
            Entry::Occupied(mut o) => o.get_mut().retain(|f| facts.contains_key(f)),
        }
    }

    fn walk(&mut self, nodes: &[Node], flow: &mut Flow) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    if let Some(id) = attr_str(&l.attrs, "id") {
                        self.label(id, flow);
                    }
                    // dsl 0.24.0 §4: an unguarded line is its own slot —
                    // `W-CAST-ABSENT` reads the facts guaranteed there.
                    match &l.when {
                        Some(when) => self.record(when, flow),
                        None => self.record_span(l.span, flow),
                    }
                }
                Node::Directive(d) => self.directive(d, flow),
                Node::Assert(a) => {
                    // dsl 0.25.0 §1: the set an `::assert` meets —
                    // `E-FACT-EXCLUSIVE` reads it.
                    self.record_span(a.span, flow);
                    match &a.when {
                        // dsl 0.26.0 §4: a guarded assert may be skipped —
                        // never a Must fact; what it displaces is gone
                        // either way (the meet of both routes).
                        Some(when) => {
                            self.record(when, flow);
                            let mut taken = flow.clone();
                            self.assume(when, &mut taken);
                            self.assert(a, &mut taken);
                            meet(flow, taken);
                        }
                        None => self.assert(a, flow),
                    }
                }
                Node::Retract(r) => {
                    // dsl 0.26.0 §4: a guarded retract may run — it removes
                    // what it matches from the Must set exactly as one that
                    // always runs.
                    if let Some(when) = &r.when {
                        self.record(when, flow);
                    }
                    self.retract(r, flow)
                }
                Node::Branch(b) => self.branch(&b.choices, flow),
                Node::Hub(h) => self.hub(&h.choices, flow),
                Node::Match(m) => self.match_arms(m, flow),
                Node::On(o) => {
                    let mut body = Some(self.body_base.clone());
                    if let Some(when) = &o.when {
                        self.assume(when, &mut body);
                    }
                    self.walk(&o.body, &mut body);
                }
                Node::Objective(o) => {
                    let base = Some(self.body_base.clone());
                    self.record(&o.done, &base);
                    if let Some(when) = &o.when {
                        self.record(when, &base);
                    }
                    for deadline in o.by.iter().chain(&o.until) {
                        self.record(deadline, &base);
                    }
                    let mut body = base;
                    self.assume(&o.done, &mut body);
                    self.walk(&o.body, &mut body);
                }
                Node::Set(_) | Node::Timeline(_) => {}
            }
        }
    }

    fn directive(&mut self, d: &Directive, flow: &mut Flow) {
        use lute_manifest::core::{END_DIRECTIVE, MARK_DIRECTIVE, NEXT_DIRECTIVE};
        match d.tag.as_str() {
            END_DIRECTIVE => meet(&mut self.exit, flow.take()),
            MARK_DIRECTIVE => {
                if let Some(id) = attr_str(&d.attrs, "id") {
                    self.label(id, flow);
                }
            }
            NEXT_DIRECTIVE => {
                let to = attr_str(&d.attrs, "to");
                match (&d.when, to) {
                    (Some(when), to) => {
                        self.record(when, flow);
                        if let Some(to) = to {
                            let mut jumped = flow.clone();
                            self.assume(when, &mut jumped);
                            self.jump(to, jumped);
                        }
                    }
                    (None, Some(to)) => {
                        let jumped = flow.take();
                        self.jump(to, jumped);
                    }
                    (None, None) => {}
                }
            }
            _ => {
                if let Some(when) = &d.when {
                    self.record(when, flow);
                }
                // dsl 0.26.0 §3.2: a `::use` is the slot of the `@@p:` lines
                // it speaks (`W-CAST-ABSENT`), under its own guard.
                if d.tag == "use" {
                    match &d.when {
                        Some(when) => {
                            let mut inner = flow.clone();
                            self.assume(when, &mut inner);
                            self.record_span(d.span, &inner);
                        }
                        None => self.record_span(d.span, flow),
                    }
                }
            }
        }
    }

    fn assert(&self, a: &Assert, flow: &mut Flow) {
        let Some(facts) = flow else {
            return;
        };
        facts.retain(|g| !self.root.is_derived(&g.relation));
        let Some(fact) = GroundFact::from_pattern(&a.pattern) else {
            return;
        };
        if self.root.is_derived(&fact.relation) {
            return; // `E-DERIVED-WRITE`'s problem
        }
        facts.retain(|g| !self.root.displaces(&fact, g));
        if self.trackable(&fact) {
            facts.insert(
                fact,
                Provenance::Assert {
                    path: self.path.to_path_buf(),
                    line: a.span.line,
                },
            );
        }
    }

    fn retract(&self, r: &Retract, flow: &mut Flow) {
        let Some(facts) = flow else {
            return;
        };
        facts.retain(|g| !self.root.is_derived(&g.relation));
        if let Some(q) = QueryPattern::from_fact_pattern(&r.pattern) {
            facts.retain(|g| !q.matches(g));
        }
    }

    fn branch(&mut self, choices: &[Choice], flow: &mut Flow) {
        let pre = flow.clone();
        let mut join: Flow = None;
        let mut unconditional = false;
        for c in choices {
            let mut arm = pre.clone();
            match &c.when {
                Some(when) => self.guard(when, &mut arm),
                None => unconditional = true,
            }
            self.walk(&c.body, &mut arm);
            meet(&mut join, arm);
        }
        if !unconditional {
            meet(&mut join, pre);
        }
        *flow = join;
    }

    /// Zero or more rounds of the hub body: the greatest fixpoint
    /// `X = pre ∩ ⋂ arm_out(X)`. Every transfer function is monotone and the
    /// sets finite, so the descending sequence converges.
    fn hub(&mut self, choices: &[Choice], flow: &mut Flow) {
        let pre = flow.clone();
        let mut x = pre.clone();
        loop {
            let mut next = pre.clone();
            for c in choices {
                let mut arm = x.clone();
                if let Some(when) = &c.when {
                    self.guard(when, &mut arm);
                }
                self.walk(&c.body, &mut arm);
                meet(&mut next, arm);
            }
            if same(&next, &x) {
                break;
            }
            x = next;
        }
        *flow = x;
    }

    fn match_arms(&mut self, m: &Match, flow: &mut Flow) {
        let pre = flow.clone();
        let mut join: Flow = None;
        for arm in &m.arms {
            let mut a = pre.clone();
            match arm {
                Arm::When { test, body, .. } => {
                    self.guard(test, &mut a);
                    self.walk(body, &mut a);
                }
                Arm::Otherwise { body, .. } => self.walk(body, &mut a),
            }
            meet(&mut join, a);
        }
        if m.arms.is_empty() || !crate::match_check::is_exhaustive(m, self.schema) {
            meet(&mut join, pre);
        }
        *flow = join;
    }
}

/// The top-level `&&` conjuncts of `e`, in order.
fn conjuncts<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_AND && c.target.is_none() && c.args.len() == 2 {
            conjuncts(&c.args[0].expr, out);
            conjuncts(&c.args[1].expr, out);
            return;
        }
    }
    out.push(e);
}

/// The ground fact of a `holds(F)` conjunct.
fn held_fact(e: &Expr) -> Option<GroundFact> {
    let Expr::Call(c) = e else {
        return None;
    };
    if c.func_name != "holds" || !crate::cel_resolve::is_profile_fact_query(c) {
        return None;
    }
    let Expr::Call(p) = &c.args[0].expr else {
        return None;
    };
    QueryPattern::from_call(p)?.ground()
}

/// `entry.X.read` / `entry.X.everRead` as a conjunct — bare or `== true` —
/// as `(X, is_ever_read)`.
fn entry_read(e: &Expr) -> Option<(String, bool)> {
    let e = match e {
        Expr::Call(c)
            if c.func_name == op::EQUALS
                && c.target.is_none()
                && c.args.len() == 2
                && matches!(
                    c.args[1].expr,
                    Expr::Literal(cel_parser::reference::Val::Boolean(true))
                ) =>
        {
            &c.args[0].expr
        }
        other => other,
    };
    let path = crate::cel_paths::select_path(e)?;
    let id = crate::cel_paths::reserved_entry_id(&path)?.to_string();
    Some((id, crate::cel_paths::is_entry_ever_read(&path)))
}
