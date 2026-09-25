//! `lute scenario <dir> knowledge [--for <node>]` (dsl 0.23.0 §1, 0.24.0
//! T3-1): the knowledge map of a project. For every fact-guarded condition —
//! one that queries a relation (`holds(…)`, `count(…)`, `countDistinct(…)`,
//! `validAt(…)`) — in every guard slot (scene and bundle beat `when`, entry
//! `when`, quest `start`/`fail`, objective `done`/`when`/`by`/`until`, reward
//! `when`, `<on when>`, line `when=`, `<choice when>`, `<when>` arm tests,
//! `::next{when}`, `::set{when}`),
//! grouped by document: the relations it reads, and for each relation who
//! makes it true — the documents that `::assert` it, the seed facts, the
//! engine (a `reserved` relation), or the rules that derive it, traced
//! through each rule's premises to their own producers. A derived atom's
//! rules are printed once per report and referenced after that.
//!
//! A negated premise (`not X` in a rule, `!holds(X)` in a guard) is read as
//! a premise that holds until something defeats it: it names the facts that
//! can defeat it and who produces them, or says it cannot be defeated.
//! Constants the rule's positive premises force (over the may set) are
//! propagated into it, so `alibied(S) :- sawAt(W, S, …), not liar(W)` under
//! `S = tobias` reads `not liar(hollis)` when only Hollis saw Tobias.
//!
//! Read-only, over the same per-root document collection `lute scenario`
//! and `check-project` use; documents need not check clean.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cel_parser::ast::operators as op;
use lute_check::{GroundFact, MaySet, ProjectBeatKind, RelVocab};
use lute_syntax::ast::{Arm, CelSlot, Choice, Document, Node, Objective, Reward};
use lute_syntax::datalog::{BodyLiteral, Rule, RuleAtom, RuleTerm};
use serde_json::{json, Map, Value as Json};

use crate::{ByRoot, DocGroup};

/// One fact-guarded element: what it is, where, its conditions and the
/// atoms they query.
struct Guarded {
    /// ``scene `k` `` / ``entry `id` `` / ``beat `k` `` / ``quest `q` `` /
    /// ``objective `q.o` `` / ``choice `b.c` `` / ``line `@s` `` / …
    name: String,
    /// The source line of a guard inside a body (lines, choices, arms, …).
    line: Option<u32>,
    /// The `--for` handles that select it: its container (a scene key, a
    /// bundle beat key, an entry id, `quest:<id>`), an objective's
    /// `<quest>.<objective>`, a choice's `<container>#<branch>.<choice>`.
    handles: Vec<String>,
    document: String,
    /// `(slot, as authored, after @def expansion)`: `when`, `start`, …
    slots: Vec<(&'static str, String, String)>,
    reads: BTreeSet<Read>,
}

impl Guarded {
    fn label(&self) -> String {
        match self.line {
            Some(n) => format!("{} (line {n})", self.name),
            None => self.name.clone(),
        }
    }
}

/// An atom as queried, asserted, seeded or concluded: its relation and, per
/// argument, the constant it names (`None`: a variable or `_`, matching
/// anything). Tracing a query through a rule binds the head's variables, so
/// `holds(trusted(ada))` follows `trusted(P) :- met(P)` to `met(ada)`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Pattern {
    rel: String,
    args: Vec<Option<String>>,
}

impl Pattern {
    fn text(&self) -> String {
        let args: Vec<&str> = self.args.iter().map(|a| a.as_deref().unwrap_or("_")).collect();
        format!("{}({})", self.rel, args.join(", "))
    }

    fn of(g: &GroundFact) -> Pattern {
        Pattern {
            rel: g.relation.clone(),
            args: g.args.iter().cloned().map(Some).collect(),
        }
    }

    /// Some ground instance satisfies both: same relation and arity, and no
    /// argument where both name different constants.
    fn unifies(&self, rel: &str, args: &[Option<String>]) -> bool {
        self.rel == rel
            && self.args.len() == args.len()
            && self.args.iter().zip(args).all(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            })
    }
}

/// A queried atom and whether the guard reads it under `!` (`!holds(X)`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Read {
    pattern: Pattern,
    negated: bool,
}

fn fact_args(p: &lute_syntax::datalog::FactPattern) -> Vec<Option<String>> {
    use lute_syntax::datalog::FactTerm;
    p.args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) => Some(s.clone()),
            FactTerm::Bool(b) => Some(b.to_string()),
            FactTerm::Wildcard | FactTerm::Param(_) => None,
        })
        .collect()
}

/// A rule atom's arguments under `bound` (head variable -> constant).
fn rule_args(
    terms: &[lute_syntax::datalog::RuleTerm],
    bound: &BTreeMap<&str, String>,
) -> Vec<Option<String>> {
    terms
        .iter()
        .map(|t| match t {
            RuleTerm::Var(v) => bound.get(v.as_str()).cloned(),
            RuleTerm::Const(c) => Some(c.clone()),
            RuleTerm::Bool(b) => Some(b.to_string()),
        })
        .collect()
}

/// Who asserts what: a producer label per asserting site, with the pattern
/// its `::assert` writes.
type Asserters = Vec<(String, Pattern)>;

/// The atoms a condition queries: the pattern of every `holds(R(…))`,
/// `count(R(…))`, `validAt(R(…), t)`; a `holds` under an odd number of `!`
/// (through `&&`/`||` only) is read negated.
fn queried(raw: &str) -> BTreeSet<Read> {
    use cel_parser::ast::Expr;
    use cel_parser::reference::Val;
    fn walk(expr: &Expr, negated: bool, out: &mut BTreeSet<Read>) {
        match expr {
            Expr::Call(c) => {
                let name = c.func_name.as_str();
                if c.target.is_none() && name == op::LOGICAL_NOT && c.args.len() == 1 {
                    return walk(&c.args[0].expr, !negated, out);
                }
                let keeps = c.target.is_none() && (name == op::LOGICAL_AND || name == op::LOGICAL_OR);
                if c.target.is_none() && matches!(name, "holds" | "count" | "countDistinct" | "validAt") {
                    // `countDistinct(P, V…)`: the column variables `V` are
                    // not constants of `P`.
                    let columns: Vec<&str> = if name == "countDistinct" {
                        c.args
                            .iter()
                            .skip(1)
                            .filter_map(|a| match &a.expr {
                                Expr::Ident(s) => Some(s.as_str()),
                                _ => None,
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if let Some(Expr::Call(pattern)) = c.args.first().map(|a| &a.expr) {
                        let args = pattern
                            .args
                            .iter()
                            .map(|a| match &a.expr {
                                Expr::Ident(s) if s != "_" && !columns.contains(&s.as_str()) => Some(s.clone()),
                                Expr::Literal(Val::String(s)) => Some(s.to_string()),
                                Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
                                _ => None,
                            })
                            .collect();
                        out.insert(Read {
                            pattern: Pattern {
                                rel: pattern.func_name.clone(),
                                args,
                            },
                            negated: negated && name == "holds",
                        });
                    }
                }
                if let Some(t) = &c.target {
                    walk(&t.expr, false, out);
                }
                for a in &c.args {
                    walk(&a.expr, keeps && negated, out);
                }
            }
            Expr::List(l) => l.elements.iter().for_each(|e| walk(&e.expr, false, out)),
            Expr::Select(s) => walk(&s.operand.expr, false, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    let mut arena = lute_cel::CelArena::default();
    if let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, raw).and_then(|h| arena.get(h)) {
        walk(&root.expr, false, &mut out);
    }
    out
}

/// A condition after `@def` expansion in its own document.
fn expanded(slot: &CelSlot, folded: &lute_check::FoldedEnv) -> String {
    let defs = lute_check::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let mut stack = Vec::new();
    lute_check::expand_cel(&slot.raw, &defs, None, &mut stack).unwrap_or_else(|_| slot.raw.clone())
}

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// One document's guard walk: every slot that can hold a condition, in
/// source order, each tagged with the handles of what encloses it.
struct Walk<'a> {
    document: String,
    folded: &'a lute_check::FoldedEnv,
    /// The quest whose body is being walked (objectives name it).
    quest: Option<String>,
    out: Vec<Guarded>,
}

impl Walk<'_> {
    fn push(&mut self, name: String, line: Option<u32>, handles: Vec<String>, slots: Vec<(&'static str, &CelSlot)>) {
        let slots: Vec<(&'static str, String, String)> = slots
            .into_iter()
            .filter(|(_, s)| !s.raw.trim().is_empty())
            .map(|(k, s)| (k, s.raw.trim().to_string(), expanded(s, self.folded)))
            .collect();
        self.push_expanded(name, line, handles, slots);
    }

    fn push_expanded(
        &mut self,
        name: String,
        line: Option<u32>,
        handles: Vec<String>,
        slots: Vec<(&'static str, String, String)>,
    ) {
        let reads: BTreeSet<Read> = slots.iter().flat_map(|(_, _, c)| queried(c)).collect();
        if !reads.is_empty() {
            self.out.push(Guarded {
                name,
                line,
                handles,
                document: self.document.clone(),
                slots,
                reads,
            });
        }
    }

    fn body(&mut self, nodes: &[Node], handles: &[String]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    if let Some(w) = &l.when {
                        let name = format!("line `@{}`", l.speaker);
                        self.push(name, Some(w.span.line), handles.to_vec(), vec![("when", w)]);
                    }
                }
                Node::Directive(d) => {
                    if let Some(w) = &d.when {
                        let name = format!("`::{}`", d.tag);
                        self.push(name, Some(w.span.line), handles.to_vec(), vec![("when", w)]);
                    }
                }
                Node::Branch(b) => self.choices(&b.choices, Some(&b.id), handles),
                Node::Hub(h) => self.choices(&h.choices, None, handles),
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { test, body, .. } => {
                                let line = Some(test.span.line);
                                self.push("`<when>` arm".to_string(), line, handles.to_vec(), vec![("test", test)]);
                                self.body(body, handles);
                            }
                            Arm::Otherwise { body, .. } => self.body(body, handles),
                        }
                    }
                }
                Node::On(o) => {
                    if let Some(w) = &o.when {
                        let name = format!("`<on {}>` handler", o.event);
                        self.push(name, Some(w.span.line), handles.to_vec(), vec![("when", w)]);
                    }
                    self.body(&o.body, handles);
                }
                Node::Objective(o) => self.objective(o, handles),
                Node::Set(s) => {
                    if let Some(w) = &s.when {
                        let name = format!("`::set` of `{}`", s.path);
                        self.push(name, Some(w.span.line), handles.to_vec(), vec![("when", w)]);
                    }
                }
                Node::Timeline(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    fn choices(&mut self, choices: &[Choice], branch: Option<&str>, handles: &[String]) {
        for c in choices {
            let local = match branch {
                Some(b) => format!("{b}.{}", c.id),
                None => c.id.clone(),
            };
            let mut inner = handles.to_vec();
            if let Some(container) = handles.first() {
                inner.push(format!("{container}#{local}"));
            }
            if let Some(w) = &c.when {
                self.push(format!("choice `{local}`"), Some(c.span.line), inner.clone(), vec![("when", w)]);
            }
            self.body(&c.body, &inner);
        }
    }

    fn rewards(&mut self, rewards: &[Reward], handles: &[String]) {
        for r in rewards {
            if let Some(w) = &r.when {
                let name = format!("reward `{}`", r.kind);
                self.push(name, Some(r.span.line), handles.to_vec(), vec![("when", w)]);
            }
        }
    }

    fn objective(&mut self, o: &Objective, handles: &[String]) {
        let quest = self.quest.clone().unwrap_or_default();
        let id = format!("{quest}.{}", o.id);
        let mut inner = vec![id.clone()];
        inner.extend(handles.iter().cloned());
        let mut slots = vec![("done", &o.done)];
        slots.extend(o.when.iter().map(|s| ("when", s)));
        slots.extend(o.by.iter().map(|s| ("by", s)));
        slots.extend(o.until.iter().map(|s| ("until", s)));
        self.push(format!("objective `{id}`"), None, inner.clone(), slots);
        self.rewards(&o.rewards, &inner);
        self.body(&o.body, &inner);
    }
}

/// A scene document's frontmatter `when:` as authored.
fn frontmatter_when(doc: &Document) -> Option<String> {
    let meta = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml).ok()?;
    meta.get(serde_yaml::Value::String("when".into()))?
        .as_str()
        .map(|s| s.trim().to_string())
}

/// Every fact-guarded element of one root, grouped by document (file
/// order), each document in source order: the scene beat's `when:`, the
/// scene body, entries and bundle beats (each guard, then its body), then
/// quests (`start`/`fail`, rewards, objectives, handlers).
fn guarded(root: &Path, group: &DocGroup, docs: &[(PathBuf, Document)]) -> Vec<Guarded> {
    let foldeds: Vec<&lute_check::FoldedEnv> = group.iter().map(|(_, _, f)| f).collect();
    let beats = lute_check::project_beats(docs, &foldeds);
    let mut out = Vec::new();
    for (path, doc, folded) in group {
        let mut w = Walk {
            document: rel_path(root, path),
            folded,
            quest: None,
            out: Vec::new(),
        };
        for b in beats.iter().filter(|b| b.path == path && b.kind == ProjectBeatKind::Scene) {
            if let Some(when) = &b.when {
                let authored = frontmatter_when(doc).unwrap_or_else(|| when.clone());
                w.push_expanded(b.name(), None, vec![b.id.clone()], vec![("when", authored, when.clone())]);
            }
        }
        let scene: Vec<String> = lute_check::connectivity::scene_key(doc).into_iter().collect();
        for shot in &doc.shots {
            w.body(&shot.body, &scene);
        }
        // Entries and bundle beats interleave in source order.
        enum Top<'d> {
            Entry(&'d lute_syntax::ast::Entry),
            Beat(&'d lute_syntax::ast::BundleBeat),
        }
        let mut tops: Vec<(usize, Top<'_>)> = doc
            .entries
            .iter()
            .map(|e| (e.span.byte_start, Top::Entry(e)))
            .chain(doc.beats.iter().map(|b| (b.span.byte_start, Top::Beat(b))))
            .collect();
        tops.sort_by_key(|(at, _)| *at);
        let bundle = lute_check::connectivity::bundle_id(doc);
        for (_, top) in tops {
            match top {
                Top::Entry(e) => {
                    let handles = vec![e.id.clone()];
                    if let Some(when) = &e.when {
                        w.push(format!("entry `{}`", e.id), None, handles.clone(), vec![("when", when)]);
                    }
                    w.body(&e.body, &handles);
                }
                Top::Beat(b) => {
                    let key = match &bundle {
                        Some(d) => lute_check::bundle_beat_key(d, &b.id),
                        None => b.id.clone(),
                    };
                    let handles = vec![key.clone()];
                    if let Some(when) = &b.when {
                        w.push(format!("beat `{key}`"), None, handles.clone(), vec![("when", when)]);
                    }
                    w.body(&b.body, &handles);
                }
            }
        }
        for q in &doc.quests {
            let handles = vec![format!("quest:{}", q.id)];
            let slots: Vec<(&'static str, &CelSlot)> = q
                .start
                .iter()
                .map(|s| ("start", s))
                .chain(q.fail.iter().map(|s| ("fail", s)))
                .collect();
            w.push(format!("quest `{}`", q.id), None, handles.clone(), slots);
            w.rewards(&q.rewards, &handles);
            w.quest = Some(q.id.clone());
            w.body(&q.body, &handles);
            w.quest = None;
        }
        out.extend(w.out);
    }
    out
}

/// Every asserting site in one root: ``scene `key` (path)``, ``quest `id` ``,
/// ``entry `id` ``, ``beat `doc.id` ``, with the pattern it asserts.
fn asserters(root: &Path, group: &DocGroup) -> Asserters {
    let mut out = Asserters::new();
    let mut record = |nodes: &[Node], label: String| {
        let mut sites = Vec::new();
        lute_check::connectivity::collect_asserts(nodes, &mut sites);
        for a in sites {
            if !a.pattern.relation.is_empty() {
                let p = Pattern {
                    rel: a.pattern.relation.clone(),
                    args: fact_args(&a.pattern),
                };
                out.push((label.clone(), p));
            }
        }
    };
    for (path, doc, _) in group {
        let document = rel_path(root, path);
        let scene = lute_check::connectivity::scene_key(doc)
            .map(|k| format!("scene `{k}`"))
            .unwrap_or_else(|| "scene".to_string());
        for shot in &doc.shots {
            record(&shot.body, format!("{scene} ({document})"));
        }
        for q in &doc.quests {
            record(&q.body, format!("quest `{}` ({document})", q.id));
        }
        for e in &doc.entries {
            record(&e.body, format!("entry `{}` ({document})", e.id));
        }
        let bundle = lute_check::connectivity::bundle_id(doc);
        for b in &doc.beats {
            let id = match &bundle {
                Some(d) => lute_check::bundle_beat_key(d, &b.id),
                None => b.id.clone(),
            };
            record(&b.body, format!("beat `{id}` ({document})"));
        }
    }
    out
}

/// The root's relational vocabulary: every document of one root folds the
/// same imported vocabulary (the `lute scenario` facts section's rule).
fn vocab(group: &DocGroup) -> RelVocab {
    group
        .iter()
        .map(|(_, _, f)| &*f.env.rel_vocab)
        .find(|v| !v.relations.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// The immediate producers of `p` — asserting sites, seed facts, the
/// engine, the rules whose head can conclude it — each restricted to what
/// can match `p`; the rules are traced by the caller. Empty: nothing
/// produces it.
fn producer_parts(p: &Pattern, vocab: &RelVocab, asserted: &Asserters) -> Vec<String> {
    let sites: BTreeSet<&str> = asserted
        .iter()
        .filter(|(_, a)| p.unifies(&a.rel, &a.args))
        .map(|(label, _)| label.as_str())
        .collect();
    producers(p, vocab, &sites.into_iter().collect::<Vec<_>>())
}

/// [`producer_parts`] over already-chosen asserting `sites`.
fn producers(p: &Pattern, vocab: &RelVocab, sites: &[&str]) -> Vec<String> {
    let Some(decl) = vocab.relations.get(&p.rel) else {
        return vec!["undeclared relation".to_string()];
    };
    let mut parts = Vec::new();
    if !sites.is_empty() {
        parts.push(format!("asserted by {}", sites.join(", ")));
    }
    let seeds: Vec<&str> = vocab
        .facts
        .iter()
        .filter(|f| p.unifies(&f.fact.relation, &fact_args(&f.fact)))
        .map(|f| f.raw.as_str())
        .collect();
    if !seeds.is_empty() {
        parts.push(format!("seed facts {}", seeds.join(", ")));
    }
    if decl.reserved {
        parts.push("reserved — the engine asserts it".to_string());
    }
    if decl.derive {
        let rules = vocab.rules.iter().filter(|r| concludes(p, &r.rule.head)).count();
        parts.push(match rules {
            0 => "derived, but no rule can conclude it".to_string(),
            1 => "derived by 1 rule".to_string(),
            n => format!("derived by {n} rules"),
        });
    }
    parts
}

/// The words for "nothing produces it", read positively.
const NO_PRODUCER: &str = "NO PRODUCER — nothing asserts it, no seed fact, not reserved, no rule";

/// [`producer_parts`] as one line for a queried (positive) atom.
fn producer_line(p: &Pattern, vocab: &RelVocab, asserted: &Asserters) -> String {
    let parts = producer_parts(p, vocab, asserted);
    if parts.is_empty() {
        NO_PRODUCER.to_string()
    } else {
        parts.join("; ")
    }
}

/// A whole relation's producers in this view's words — `lute scenario
/// envelope`'s facts section (T3-2: one wording across both reports),
/// `sites` being the documents that assert it.
pub(crate) fn relation_producers(rel: &str, vocab: &RelVocab, sites: &[&str]) -> String {
    let arity = vocab.relations.get(rel).map_or(0, |d| d.args.len());
    let p = Pattern {
        rel: rel.to_string(),
        args: vec![None; arity],
    };
    let parts = producers(&p, vocab, sites);
    if parts.is_empty() {
        NO_PRODUCER.to_string()
    } else {
        parts.join("; ")
    }
}

/// A rule with this head can conclude `p`.
fn concludes(p: &Pattern, head: &lute_syntax::datalog::RuleAtom) -> bool {
    p.unifies(&head.relation, &rule_args(&head.terms, &BTreeMap::new()))
}

/// One root's view: its fact-guarded elements, vocabulary, asserting sites
/// and may set.
pub(crate) struct RootKnowledge {
    root: PathBuf,
    elements: Vec<Guarded>,
    vocab: RelVocab,
    asserted: Asserters,
    /// Every ground fact that may hold at some point of some run — built as
    /// `check-project` builds it, with every assert site counted live (this
    /// view does not run reachability).
    may: MaySet,
}

/// The root's may set (`compute_conn_fixpoint`'s construction).
fn may_set(group: &DocGroup, docs: &[(PathBuf, Document)]) -> MaySet {
    let mut vocab = lute_check::RootVocab::default();
    for (_, _, folded) in group {
        vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    }
    vocab.note_unreadable_documents(docs);
    let stable = lute_check::stable_seeds(docs, &vocab);
    let none = BTreeSet::new();
    let facts = lute_check::connectivity::live_assert_sites(docs, &BTreeMap::new(), &none, &none)
        .into_iter()
        .filter_map(|(_, a)| GroundFact::from_pattern(&a.pattern));
    MaySet::build(&vocab, facts, &stable)
}

/// A clause variable binding.
type Subst = BTreeMap<String, String>;

/// More bindings than this and a clause is not enumerated.
const JOIN_CAP: usize = 512;

/// Defeating facts printed per negated premise; the rest are counted.
const DEFEAT_SHOWN: usize = 3;

fn term_value(t: &RuleTerm, s: &Subst) -> Option<String> {
    match t {
        RuleTerm::Var(v) => s.get(v).cloned(),
        RuleTerm::Const(c) => Some(c.clone()),
        RuleTerm::Bool(b) => Some(b.to_string()),
    }
}

/// Every binding of `rule`'s positive premises to facts `may` holds,
/// extending `seed`, kept when each `=`/`!=` decided on bound values holds.
/// `None`: not enumerable — a positive premise over an unbounded relation,
/// or more than [`JOIN_CAP`] bindings.
fn joins(rule: &Rule, seed: Subst, may: &MaySet) -> Option<Vec<Subst>> {
    let mut out = vec![seed];
    for lit in &rule.body {
        let BodyLiteral::Pos(atom) = lit else { continue };
        if may.is_unbounded(&atom.relation) {
            return None;
        }
        let mut next = Vec::new();
        for s in &out {
            for tuple in may.instances(&atom.relation).into_iter().flatten() {
                if let Some(s) = bind(&atom.terms, tuple, s) {
                    next.push(s);
                    if next.len() > JOIN_CAP {
                        return None;
                    }
                }
            }
        }
        out = next;
    }
    out.retain(|s| {
        rule.body.iter().all(|lit| match lit {
            BodyLiteral::Cmp { lhs, rhs, negated, .. } => match (term_value(lhs, s), term_value(rhs, s)) {
                (Some(a), Some(b)) => (a == b) != *negated,
                _ => true,
            },
            _ => true,
        })
    });
    Some(out)
}

/// `s` extended so `terms` match `tuple`; `None` when they cannot.
fn bind(terms: &[RuleTerm], tuple: &[String], s: &Subst) -> Option<Subst> {
    if terms.len() != tuple.len() {
        return None;
    }
    let mut s = s.clone();
    for (term, v) in terms.iter().zip(tuple) {
        match term {
            RuleTerm::Var(name) => match s.get(name) {
                Some(b) if b != v => return None,
                Some(_) => {}
                None => {
                    s.insert(name.clone(), v.clone());
                }
            },
            RuleTerm::Const(c) if c != v => return None,
            RuleTerm::Bool(b) if b.to_string() != *v => return None,
            _ => {}
        }
    }
    Some(s)
}

/// What can make a negated premise false: the facts the may set holds that
/// match it, or `Err` (why they cannot be listed).
type Defeat = Result<Vec<GroundFact>, String>;

/// Every fact `may` holds that matches one of `instances`.
fn matching(may: &MaySet, rel: &str, instances: &[Pattern]) -> Defeat {
    if may.is_unbounded(rel) {
        return Err(format!("any `{rel}` tuple may hold"));
    }
    Ok(may
        .instances(rel)
        .into_iter()
        .flatten()
        .filter(|tuple| {
            let args: Vec<Option<String>> = tuple.iter().cloned().map(Some).collect();
            instances.iter().any(|inst| inst.unifies(rel, &args))
        })
        .map(|args| GroundFact {
            relation: rel.to_string(),
            args: args.clone(),
        })
        .collect())
}

/// The negated premise `atom` of `rule` under `bound` (the head's
/// constants), with every constant the rule's positive premises force
/// propagated into it (T3-1: `not liar(W)` reads `not liar(hollis)` when
/// every binding of the positive premises binds `W` to `hollis`), and the
/// facts that defeat it: each binding instantiates the premise, and every
/// held fact matching an instance defeats it.
fn negated(rule: &Rule, atom: &RuleAtom, bound: &Subst, may: &MaySet) -> (Pattern, Defeat) {
    let under = |s: &Subst| Pattern {
        rel: atom.relation.clone(),
        args: atom.terms.iter().map(|t| term_value(t, s)).collect(),
    };
    let head = under(bound);
    match joins(rule, bound.clone(), may) {
        Some(substs) if !substs.is_empty() => {
            let instances: Vec<Pattern> = substs.iter().map(under).collect();
            let args = (0..head.args.len())
                .map(|i| {
                    let first = &instances[0].args[i];
                    instances
                        .iter()
                        .all(|p| &p.args[i] == first)
                        .then(|| first.clone())
                        .flatten()
                })
                .collect();
            let p = Pattern {
                rel: atom.relation.clone(),
                args,
            };
            let defeat = matching(may, &atom.relation, &instances);
            (p, defeat)
        }
        // The positive premises never hold together: nothing can reach
        // the negation, so nothing defeats it.
        Some(_) if !may.is_unbounded(&atom.relation) => (head, Ok(Vec::new())),
        // Not enumerable: the premise under the head's constants.
        _ => {
            let defeat = matching(may, &atom.relation, std::slice::from_ref(&head));
            (head, defeat)
        }
    }
}

fn ground_text(g: &GroundFact) -> String {
    format!("{}({})", g.relation, g.args.join(", "))
}

/// Where a ground fact can come from: its asserting sites, `seed`, the
/// engine, or `derived`.
fn fact_source(g: &GroundFact, k: &RootKnowledge) -> String {
    let p = Pattern::of(g);
    let mut parts: Vec<String> = k
        .asserted
        .iter()
        .filter(|(_, a)| p.unifies(&a.rel, &a.args))
        .map(|(label, _)| label.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if k.vocab
        .facts
        .iter()
        .any(|f| p.unifies(&f.fact.relation, &fact_args(&f.fact)))
    {
        parts.push("seed".to_string());
    }
    if let Some(decl) = k.vocab.relations.get(&g.relation) {
        if decl.reserved {
            parts.push("engine".to_string());
        }
        if decl.derive && parts.is_empty() {
            parts.push("derived".to_string());
        }
    }
    parts.join(", ")
}

/// How `p` (a defeating fact, or the pattern of one) comes to hold, each a
/// whole "defeated …" clause: its asserting sites, a seed, the engine.
fn defeat_ways(p: &Pattern, k: &RootKnowledge) -> Vec<String> {
    let text = p.text();
    let mut ways = Vec::new();
    let sites: BTreeSet<&str> = k
        .asserted
        .iter()
        .filter(|(_, a)| p.unifies(&a.rel, &a.args))
        .map(|(label, _)| label.as_str())
        .collect();
    if !sites.is_empty() {
        ways.push(format!(
            "defeated when {text} is asserted by {}",
            sites.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if k.vocab
        .facts
        .iter()
        .any(|f| p.unifies(&f.fact.relation, &fact_args(&f.fact)))
    {
        ways.push(format!("defeated from the start: {text} is a seed fact"));
    }
    if k.vocab.relations.get(&p.rel).is_some_and(|d| d.reserved) {
        ways.push(format!("defeated when the engine asserts {text} (reserved)"));
    }
    ways
}

/// How one defeating fact defeats: how it is asserted, else the premises of
/// the first rule instance that derives it.
fn defeated_when(g: &GroundFact, k: &RootKnowledge) -> String {
    let ways = defeat_ways(&Pattern::of(g), k);
    if !ways.is_empty() {
        return ways.join("; or ");
    }
    match derivations(g, k).first() {
        Some(d) => format!("defeated when {} is derived ⇐ {}", ground_text(g), derivation_text(d, k)),
        None => format!("defeated when {} is derived", ground_text(g)),
    }
}

/// Why nothing defeats `not p`, inside "always holds (…)".
fn never_produced(p: &Pattern, k: &RootKnowledge) -> String {
    let parts = producer_parts(p, &k.vocab, &k.asserted);
    let text = p.text();
    let rules = k.vocab.rules.iter().filter(|r| concludes(p, &r.rule.head)).count();
    if parts.is_empty() {
        format!("nothing produces {text}: no assert, seed fact, engine write or rule")
    } else if parts.len() == 1 && rules > 0 {
        let its = if rules == 1 { "its rule".to_string() } else { format!("none of its {rules} rules") };
        let verb = if rules == 1 { "never concludes" } else { "concludes" };
        format!("nothing can produce {text}: {its} {verb} it from what the project asserts")
    } else {
        format!(
            "nothing can produce {text}: {}, but none of it yields these arguments from what the \
             project asserts",
            parts.join("; ")
        )
    }
}

/// An entity-kind atom (`suitor(sol)`) as a premise: membership, never a
/// fact — whether the member belongs, or the kind's members when the
/// argument is unbound. `negated`: read under `not`.
fn kind_membership(p: &Pattern, vocab: &RelVocab, negated: bool) -> String {
    use lute_manifest::relations::KindShape;
    let members = match vocab.kinds.get(&p.rel).map(|d| &d.shape) {
        Some(KindShape::Members(ms)) => Some(ms.as_slice()),
        _ => None,
    };
    let kind = format!("entity kind `{}`", p.rel);
    match (p.args.as_slice(), members) {
        ([Some(id)], Some(ms)) => {
            let member = ms.contains(id);
            let verdict = match (member, negated) {
                (true, false) => "",
                (true, true) => " — never holds",
                (false, false) => " — never holds",
                (false, true) => " — always holds",
            };
            let is = if member { "is a member" } else { "is not a member" };
            format!("{kind}; {id} {is}{verdict}")
        }
        ([Some(id)], None) => format!("{kind} (open — holds when the engine registers {id})"),
        (_, Some(ms)) => format!("{kind} (members: {})", ms.join(", ")),
        (_, None) => format!("{kind} (open — the engine registers its members)"),
    }
}

/// What a rule's `cel()` premise reads: the state paths it names.
fn guard_reads(cel: &str) -> String {
    use cel_parser::ast::Expr;
    /// A pure `Ident`/`Select` chain as a dotted path.
    fn path(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Ident(s) => Some(s.clone()),
            Expr::Select(s) => Some(format!("{}.{}", path(&s.operand.expr)?, s.field)),
            _ => None,
        }
    }
    fn walk(expr: &Expr, out: &mut BTreeSet<String>) {
        if let Some(p) = path(expr) {
            let root = p.split('.').next().unwrap_or_default();
            if p.contains('.') && matches!(root, "scene" | "run" | "user" | "app" | "quest" | "entry" | "prev" | "clock") {
                out.insert(p);
            }
            return;
        }
        match expr {
            Expr::Call(c) => {
                if let Some(t) = &c.target {
                    walk(&t.expr, out);
                }
                c.args.iter().for_each(|a| walk(&a.expr, out));
            }
            Expr::List(l) => l.elements.iter().for_each(|e| walk(&e.expr, out)),
            Expr::Select(s) => walk(&s.operand.expr, out),
            _ => {}
        }
    }
    let mut paths = BTreeSet::new();
    let mut arena = lute_cel::CelArena::default();
    if let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, cel).and_then(|h| arena.get(h)) {
        walk(&root.expr, &mut paths);
    }
    if paths.is_empty() {
        "state condition, decided at run time".to_string()
    } else {
        format!("state condition on {}, decided at run time", paths.into_iter().collect::<Vec<_>>().join(", "))
    }
}

/// Where a derived atom's rules were printed: the element and its document.
#[derive(Clone, PartialEq)]
struct Here {
    label: String,
    document: String,
}

/// Prints the knowledge trees of one root. A derived atom's rules are
/// printed the first time it is reached; every later mention references
/// that place instead of re-expanding (T3-1).
struct Tracer<'a> {
    k: &'a RootKnowledge,
    /// Derived atoms already expanded -> the element that expanded them.
    expanded: BTreeMap<Pattern, Here>,
}

impl Tracer<'_> {
    /// Print `p` (read positively, or negated with its defeaters) and, when
    /// derived and not yet expanded, each rule that can conclude it with
    /// its premises — the head's variables bound by `p` — traced
    /// recursively. `here` names the element being traced.
    fn trace(&mut self, out: &mut String, p: &Pattern, neg: Option<Defeat>, depth: usize, here: &Here) {
        let k = self.k;
        let pad = "  ".repeat(depth + 3);
        let rules: Vec<_> = k.vocab.rules.iter().filter(|r| concludes(p, &r.rule.head)).collect();
        let seen = if rules.is_empty() { None } else { self.expanded.get(p).cloned() };
        let reference = match &seen {
            Some(w) if w == here => " — traced above".to_string(),
            Some(w) if w.document == here.document => format!(" — traced above under {}", w.label),
            Some(w) => format!(" — traced above under {} in {}", w.label, w.document),
            None => String::new(),
        };
        match &neg {
            None if k.vocab.kinds.contains_key(&p.rel) && !k.vocab.relations.contains_key(&p.rel) => {
                outln!(out, "{pad}{} — {}", p.text(), kind_membership(p, &k.vocab, false));
                return;
            }
            None => outln!(out, "{pad}{} — {}{reference}", p.text(), producer_line(p, &k.vocab, &k.asserted)),
            Some(Ok(facts)) if facts.is_empty() => {
                outln!(
                    out,
                    "{pad}not {} — always holds ({}) — cannot be defeated",
                    p.text(),
                    never_produced(p, k)
                );
                return;
            }
            Some(Ok(facts)) => {
                outln!(out, "{pad}not {} — holds unless defeated{reference}", p.text());
                for g in facts.iter().take(DEFEAT_SHOWN) {
                    outln!(out, "{pad}  {}", defeated_when(g, k));
                }
                if facts.len() > DEFEAT_SHOWN {
                    outln!(out, "{pad}  … and {} more defeating facts", facts.len() - DEFEAT_SHOWN);
                }
            }
            Some(Err(why)) => {
                outln!(out, "{pad}not {} — holds unless defeated{reference}", p.text());
                let ways = defeat_ways(p, k);
                let when = if ways.is_empty() {
                    format!("defeated when {} holds", p.text())
                } else {
                    ways.join("; or ")
                };
                outln!(out, "{pad}  {when} — {why}");
            }
        }
        if seen.is_some() || rules.is_empty() {
            return;
        }
        self.expanded.insert(p.clone(), here.clone());
        for r in rules {
            outln!(out, "{pad}  rule: {}", r.raw.trim());
            let mut bound: BTreeMap<&str, String> = BTreeMap::new();
            for (term, arg) in r.rule.head.terms.iter().zip(&p.args) {
                if let (RuleTerm::Var(v), Some(c)) = (term, arg) {
                    bound.insert(v.as_str(), c.clone());
                }
            }
            let subst: Subst = bound.iter().map(|(v, c)| (v.to_string(), c.clone())).collect();
            for lit in &r.rule.body {
                match lit {
                    BodyLiteral::Pos(a) => {
                        let premise = Pattern {
                            rel: a.relation.clone(),
                            args: rule_args(&a.terms, &bound),
                        };
                        self.trace(out, &premise, None, depth + 2, here);
                    }
                    BodyLiteral::Neg(a) if k.vocab.kinds.contains_key(&a.relation) => {
                        let premise = Pattern {
                            rel: a.relation.clone(),
                            args: rule_args(&a.terms, &bound),
                        };
                        outln!(out, "{pad}    not {} — {}", premise.text(), kind_membership(&premise, &k.vocab, true));
                    }
                    BodyLiteral::Neg(a) => {
                        let (premise, defeat) = negated(&r.rule, a, &subst, &k.may);
                        self.trace(out, &premise, Some(defeat), depth + 2, here);
                    }
                    BodyLiteral::Guard { cel, .. } => {
                        let at: BTreeMap<&str, &str> = bound.iter().map(|(v, c)| (*v, c.as_str())).collect();
                        let cel = lute_check::rule_index::ground_guard(cel, &at);
                        outln!(out, "{pad}    cel({cel:?}) — {}", guard_reads(&cel));
                    }
                    BodyLiteral::Cmp { .. } => {}
                }
            }
        }
    }

    /// A guard's own read: a `!holds(X)` is defeated by any fact matching X.
    fn read(&mut self, out: &mut String, r: &Read, here: &Here) {
        let neg = r
            .negated
            .then(|| matching(&self.k.may, &r.pattern.rel, std::slice::from_ref(&r.pattern)));
        self.trace(out, &r.pattern, neg, 0, here);
    }
}

/// The `--for` selection over every root's elements; `Err` names the miss.
fn select<'a>(
    roots: &'a [RootKnowledge],
    for_node: Option<&str>,
) -> Result<Vec<(&'a RootKnowledge, Vec<&'a Guarded>)>, String> {
    let picked: Vec<_> = roots
        .iter()
        .map(|k| {
            let chosen = k
                .elements
                .iter()
                .filter(|g| for_node.is_none_or(|n| g.handles.iter().any(|h| h == n)))
                .collect::<Vec<_>>();
            (k, chosen)
        })
        .collect();
    if let Some(n) = for_node {
        if picked.iter().all(|(_, chosen)| chosen.is_empty()) {
            let handles: BTreeSet<&str> = roots
                .iter()
                .flat_map(|k| k.elements.iter().flat_map(|g| g.handles.iter().map(String::as_str)))
                .collect();
            let hint = lute_manifest::suggest::nearest(n, handles.iter().copied(), 3)
                .map(|s| format!(" — did you mean `{s}`?"))
                .unwrap_or_default();
            return Err(format!(
                "`--for {n}` names no fact-guarded condition (a scene, bundle beat, entry, \
                 `quest:<id>`, `<quest>.<objective>` or `<scene>#<branch>.<choice>`){hint}"
            ));
        }
    }
    Ok(picked)
}

pub(crate) fn collect(by_root: &ByRoot) -> Vec<RootKnowledge> {
    by_root
        .iter()
        .map(|(root, group)| {
            let docs: Vec<(PathBuf, Document)> = group.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
            RootKnowledge {
                root: root.clone(),
                elements: guarded(root, group, &docs),
                vocab: vocab(group),
                asserted: asserters(root, group),
                may: may_set(group, &docs),
            }
        })
        .collect()
}

/// The text view (`lute scenario <dir> knowledge`).
pub(crate) fn run_text(
    out: &mut String,
    by_root: &ByRoot,
    _file_results: &[(PathBuf, lute_check::CheckResult)],
    for_node: Option<&str>,
) -> ExitCode {
    let roots = collect(by_root);
    let picked = match select(&roots, for_node) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lute scenario knowledge: {e}");
            return ExitCode::from(2);
        }
    };
    if by_root.is_empty() {
        outln!(out, "lute: no .lute files found");
        return ExitCode::SUCCESS;
    }
    for (k, elements) in picked {
        outln!(out, "project root: {}", k.root.display());
        outln!(
            out,
            "  knowledge (every fact-guarded condition, by document -> relations read -> producers):"
        );
        if elements.is_empty() {
            outln!(out, "    (no condition queries a relation)");
            continue;
        }
        let mut tracer = Tracer {
            k,
            expanded: BTreeMap::new(),
        };
        let mut document: Option<&str> = None;
        for g in elements {
            outln!(out);
            if document != Some(g.document.as_str()) {
                outln!(out, "  {}", g.document);
                document = Some(&g.document);
            }
            outln!(out, "    {}", g.label());
            for (slot, authored, _) in &g.slots {
                outln!(out, "      {slot}: {}", authored.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            let here = Here {
                label: g.label(),
                document: g.document.clone(),
            };
            for r in &g.reads {
                tracer.read(out, r, &here);
            }
        }
    }
    ExitCode::SUCCESS
}

/// One derived atom the may set holds, as `lute lore`'s "Derived" section
/// lists it (T3-2).
#[derive(serde::Serialize)]
pub(crate) struct DerivedFact {
    pub fact: String,
    /// Each rule instance that can conclude it: its positive premises with
    /// where each comes from (`[entry `x` (doc)]`, `[seed]`, `[derived]`).
    pub from: Vec<String>,
    /// Every asserting site, seed or engine write the conclusion rests on,
    /// through every derivation (the evidence that can open it).
    pub evidence: Vec<String>,
    /// The fact-guarded conditions that read it (not under `!`).
    pub gates: Vec<String>,
}

/// One rule instance over the may set: its ground positive premises, and
/// its other premises as written under the same binding (`not liar(tobias)`,
/// `cel("run.day >= 2")`, `X != S` with values; `_` where unbound).
type Derivation = (Vec<GroundFact>, Vec<String>);

/// The rule instances that can conclude `g` over the may set.
fn derivations(g: &GroundFact, k: &RootKnowledge) -> Vec<Derivation> {
    let mut out: Vec<Derivation> = Vec::new();
    for r in k.vocab.rules.iter().filter(|r| r.rule.head.relation == g.relation) {
        let Some(seed) = bind(&r.rule.head.terms, &g.args, &Subst::new()) else {
            continue;
        };
        for s in joins(&r.rule, seed, &k.may).unwrap_or_default() {
            let value = |t: &RuleTerm| term_value(t, &s).unwrap_or_else(|| "_".to_string());
            let mut pos = Vec::new();
            let mut other = Vec::new();
            for lit in &r.rule.body {
                match lit {
                    BodyLiteral::Pos(a) => {
                        if let Some(args) = a.terms.iter().map(|t| term_value(t, &s)).collect() {
                            pos.push(GroundFact {
                                relation: a.relation.clone(),
                                args,
                            });
                        }
                    }
                    BodyLiteral::Neg(a) => {
                        let args: Vec<String> = a.terms.iter().map(value).collect();
                        other.push(format!("not {}({})", a.relation, args.join(", ")));
                    }
                    BodyLiteral::Guard { cel, .. } => other.push(format!("cel({cel:?})")),
                    BodyLiteral::Cmp { lhs, rhs, negated, .. } => {
                        let op = if *negated { "!=" } else { "=" };
                        other.push(format!("{} {op} {}", value(lhs), value(rhs)));
                    }
                }
            }
            if !out.contains(&(pos.clone(), other.clone())) {
                out.push((pos, other));
            }
        }
    }
    out
}

/// A derivation's premises with where each positive one comes from.
fn derivation_text((pos, other): &Derivation, k: &RootKnowledge) -> String {
    let premises: Vec<String> = pos
        .iter()
        .map(|f| format!("{} [{}]", ground_text(f), fact_source(f, k)))
        .chain(other.iter().cloned())
        .collect();
    if premises.is_empty() {
        "(a rule with no premises)".to_string()
    } else {
        premises.join(", ")
    }
}

/// Every non-derived source `g` rests on, through every derivation.
fn evidence(g: &GroundFact, k: &RootKnowledge, seen: &mut BTreeSet<GroundFact>, out: &mut BTreeSet<String>) {
    if !seen.insert(g.clone()) {
        return;
    }
    for source in fact_source(g, k).split(", ") {
        match source {
            "" | "derived" => {}
            "seed" => {
                out.insert("seed facts".to_string());
            }
            "engine" => {
                out.insert("the engine".to_string());
            }
            site => {
                out.insert(site.to_string());
            }
        }
    }
    if k.vocab.relations.get(&g.relation).is_some_and(|d| d.derive) {
        for (premises, _) in derivations(g, k) {
            for f in &premises {
                evidence(f, k, seen, out);
            }
        }
    }
}

/// Derived atoms the may set holds, per derived relation (byte-sorted),
/// each with how it can be concluded, the evidence it rests on and the
/// conditions it gates — `lute lore`'s "Derived" section (T3-2).
pub(crate) fn derived(k: &RootKnowledge) -> Vec<(String, Vec<DerivedFact>)> {
    k.vocab
        .relations
        .iter()
        .filter(|(_, d)| d.derive)
        .map(|(rel, _)| {
            let facts = k
                .may
                .instances(rel)
                .into_iter()
                .flatten()
                .map(|tuple| {
                    let g = GroundFact {
                        relation: rel.clone(),
                        args: tuple.clone(),
                    };
                    let args: Vec<Option<String>> = tuple.iter().cloned().map(Some).collect();
                    let from = derivations(&g, k).iter().map(|d| derivation_text(d, k)).collect();
                    let mut sources = BTreeSet::new();
                    evidence(&g, k, &mut BTreeSet::new(), &mut sources);
                    let gates = k
                        .elements
                        .iter()
                        .filter(|e| e.reads.iter().any(|r| !r.negated && r.pattern.unifies(rel, &args)))
                        .map(|e| format!("{} ({})", e.label(), e.document))
                        .collect();
                    DerivedFact {
                        fact: ground_text(&g),
                        from,
                        evidence: sources.into_iter().collect(),
                        gates,
                    }
                })
                .collect();
            (rel.clone(), facts)
        })
        .collect()
}

/// The `--format json` view: per root, the elements (with the relations
/// they read) and one producer record per relation reached from them.
pub(crate) fn json(by_root: &ByRoot, for_node: Option<&str>) -> Result<Json, String> {
    let roots = collect(by_root);
    let picked = select(&roots, for_node)?;
    let roots_json: Vec<Json> = picked
        .into_iter()
        .map(|(k, elements)| {
            let (root, vocab, asserted) = (&k.root, &k.vocab, &k.asserted);
            let mut reached: BTreeSet<String> = BTreeSet::new();
            let mut stack: Vec<String> = elements
                .iter()
                .flat_map(|g| g.reads.iter().map(|r| r.pattern.rel.clone()))
                .collect();
            while let Some(rel) = stack.pop() {
                if !reached.insert(rel.clone()) {
                    continue;
                }
                for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
                    for lit in &r.rule.body {
                        if let BodyLiteral::Pos(a) | BodyLiteral::Neg(a) = lit {
                            if !vocab.kinds.contains_key(&a.relation) {
                                stack.push(a.relation.clone());
                            }
                        }
                    }
                }
            }
            let relations: Map<String, Json> = reached
                .iter()
                .map(|rel| {
                    let decl = vocab.relations.get(rel);
                    let rules: Vec<Json> = vocab
                        .rules
                        .iter()
                        .filter(|r| &r.rule.head.relation == rel)
                        .map(|r| {
                            let premises: Vec<Json> = r
                                .rule
                                .body
                                .iter()
                                .filter_map(|lit| match lit {
                                    BodyLiteral::Pos(a) if vocab.kinds.contains_key(&a.relation) => {
                                        Some(json!({ "entityKind": a.relation }))
                                    }
                                    BodyLiteral::Neg(a) if vocab.kinds.contains_key(&a.relation) => {
                                        Some(json!({ "entityKind": a.relation, "negated": true }))
                                    }
                                    BodyLiteral::Pos(a) => Some(json!({ "relation": a.relation })),
                                    BodyLiteral::Neg(a) => {
                                        Some(json!({ "relation": a.relation, "negated": true }))
                                    }
                                    BodyLiteral::Guard { cel, .. } => Some(json!({ "cel": cel })),
                                    BodyLiteral::Cmp { .. } => None,
                                })
                                .collect();
                            json!({ "rule": r.raw.trim(), "premises": premises })
                        })
                        .collect();
                    let seeds: Vec<&str> = vocab
                        .facts
                        .iter()
                        .filter(|f| &f.fact.relation == rel)
                        .map(|f| f.raw.as_str())
                        .collect();
                    (
                        rel.clone(),
                        json!({
                            "declared": decl.is_some(),
                            "derived": decl.is_some_and(|d| d.derive),
                            "reserved": decl.is_some_and(|d| d.reserved),
                            "assertedBy": asserted
                                .iter()
                                .filter(|(_, a)| &a.rel == rel)
                                .map(|(label, _)| label.as_str())
                                .collect::<BTreeSet<_>>(),
                            "seedFacts": seeds,
                            "rules": rules,
                        }),
                    )
                })
                .collect();
            let elements: Vec<Json> = elements
                .iter()
                .map(|g| {
                    let slots: Map<String, Json> =
                        g.slots.iter().map(|(s, _, c)| (s.to_string(), json!(c))).collect();
                    let reads: Vec<String> = g
                        .reads
                        .iter()
                        .map(|r| {
                            let t = r.pattern.text();
                            if r.negated {
                                format!("!{t}")
                            } else {
                                t
                            }
                        })
                        .collect();
                    let mut e = json!({
                        "node": g.name,
                        "document": g.document,
                        "for": g.handles,
                        "conditions": slots,
                        "reads": reads,
                    });
                    if let (Some(line), Json::Object(m)) = (g.line, &mut e) {
                        m.insert("line".into(), json!(line));
                    }
                    e
                })
                .collect();
            json!({
                "root": root.display().to_string(),
                "elements": elements,
                "relations": relations,
            })
        })
        .collect();
    Ok(json!({ "roots": roots_json }))
}
