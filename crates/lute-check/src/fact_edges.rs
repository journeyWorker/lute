//! dsl 0.26.0 §8 (T3-11): fact-producer edges for `lute scenario --facts`.
//!
//! Progress gated by facts (`hasItem(spectralLens)`, `hasBadge(tide)`,
//! `canPass(…)`) is invisible to the `after:` graph: every gym sits in layer
//! 0. A fact edge `A -> B [F]` says node `B`'s gate reads
//! `holds('rel', [args])` as a top-level conjunct (positive, ground or with
//! `_`) and unit `A` asserts a fact that unifies with it — directly, or
//! (through the root's rules, a bounded depth) a fact a rule deriving `F`
//! needs positively; the edge then names the derived fact it serves (`via`).
//!
//! Gates read: a scene beat's frontmatter `when:`, a bundle beat's or lore
//! entry's `when=`, a quest's `start=` — and (HW27-09) the `raisedWhen` of
//! the occasion a beat answers, ground with its member target
//! (`holds('canEnter', [occasion.target])` for `room.office`). Readers are
//! graph nodes; producers are any unit that asserts (a scene, a quest, a lore
//! entry or a bundle beat — `::use` sites included, the host carrying the
//! component's bound writes). A producer is necessary for no reader: a reader
//! with several producers needs only one of them, as with a `||` in `after:`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cel_parser::ast::{operators as op, Expr};
use lute_syntax::ast::Document;
use lute_syntax::datalog::{BodyLiteral, RuleTerm};

use crate::cast::FactProducers;
use crate::cel_expand::{expand_cel, DefTable};
use crate::check::FoldedEnv;
use crate::connectivity::{ConnGraph, NodeId};
use crate::rel_schema::RelVocab;

/// How many rules deep a derived gate is followed to the facts it needs.
const RULE_DEPTH: u8 = 4;

/// One fact-producer edge: `from` asserts `fact`, which `to`'s gate needs —
/// read directly, or through the rules deriving `via`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactEdge {
    pub from: NodeId,
    pub to: NodeId,
    /// The asserted fact, `_` for an argument the site leaves open.
    pub fact: String,
    /// The fact the gate reads, when it is derived from `fact`.
    pub via: Option<String>,
}

/// A query argument: a constant, or open (`_`, a variable).
type Args = Vec<Option<String>>;

fn show(rel: &str, args: &[Option<String>]) -> String {
    let args: Vec<&str> = args.iter().map(|a| a.as_deref().unwrap_or("_")).collect();
    format!("{rel}({})", args.join(", "))
}

/// Every fact-producer edge of one resolved root (`docs` parallel to
/// `foldeds`), readers restricted to `graph`'s nodes, in a stable order and
/// without self-edges.
pub fn fact_edges(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    graph: &ConnGraph,
) -> Vec<FactEdge> {
    // dsl 0.27.0 §4: a directive call's declared asserts produce too.
    let effects = crate::directive_facts::root_table(foldeds.iter().copied());
    let producers = crate::cast::fact_producers(docs, &effects);
    let units = unit_nodes(docs, foldeds, graph);
    let mut out = BTreeSet::new();
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        for (reader, raw) in gates(path, doc, folded, graph) {
            let defs = DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            };
            for (rel, args) in required_holds(&raw, &defs) {
                let mut found = Vec::new();
                let read = show(&rel, &args);
                let ask = Ask {
                    producers: &producers,
                    vocab: &folded.env.rel_vocab,
                };
                ask.sources(&rel, &args, RULE_DEPTH, false, &mut found);
                for (site, fact, via) in found {
                    let Some(from) = units.get(&site) else {
                        continue;
                    };
                    if *from == reader {
                        continue;
                    }
                    out.insert(FactEdge {
                        from: from.clone(),
                        to: reader.clone(),
                        via: via.then(|| read.clone()),
                        fact,
                    });
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Each asserting unit's node: `(document, unit key)` as
/// [`FactProducers`] keys a site (`0` a scene's shots, else the quest's,
/// entry's or bundle beat's span start).
fn unit_nodes(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    graph: &ConnGraph,
) -> BTreeMap<(PathBuf, usize), NodeId> {
    let mut out = BTreeMap::new();
    for (id, info) in &graph.nodes {
        if matches!(id, NodeId::Scene(_)) {
            out.entry((info.path.clone(), 0))
                .or_insert_with(|| id.clone());
        }
    }
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        for q in doc.quests.iter().filter(|q| !q.id.is_empty()) {
            out.insert(
                (path.clone(), q.span.byte_start),
                NodeId::Quest(q.id.clone()),
            );
        }
        for e in doc.entries.iter().filter(|e| !e.id.is_empty()) {
            out.insert(
                (path.clone(), e.span.byte_start),
                NodeId::Entry(e.id.clone()),
            );
        }
        if let Some(doc_id) = folded.typed.id.as_deref() {
            for b in doc.beats.iter().filter(|b| !b.id.is_empty()) {
                out.insert(
                    (path.clone(), b.span.byte_start),
                    NodeId::Beat(crate::bundles::bundle_beat_key(doc_id, &b.id)),
                );
            }
        }
    }
    out
}

/// The gate conditions of `doc`'s graph nodes: each one's own `when` /
/// `start`, and the gate of the occasion it answers ([`occasion_gate`]).
fn gates(
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    graph: &ConnGraph,
) -> Vec<(NodeId, String)> {
    let node = |id: NodeId| graph.nodes.get(&id).filter(|i| i.path == path).map(|_| id);
    let on = |on: &Option<(String, lute_core_span::Span)>,
              target: &Option<(String, lute_core_span::Span)>| {
        on.as_ref()
            .and_then(|(on, _)| occasion_gate(folded, on, target.as_ref().map(|(t, _)| t.as_str())))
    };
    let mut out = Vec::new();
    for (id, info) in &graph.nodes {
        if let NodeId::Scene(_) = id {
            if info.path == path {
                if let Some(b) = folded.typed.beat.as_ref() {
                    out.extend(b.when.as_ref().map(|w| (id.clone(), w.raw.clone())));
                    out.extend(
                        occasion_gate(folded, &b.on, b.target.as_deref()).map(|g| (id.clone(), g)),
                    );
                }
            }
        }
    }
    for q in &doc.quests {
        if let (Some(id), Some(start)) = (node(NodeId::Quest(q.id.clone())), q.start.as_ref()) {
            out.push((id, start.raw.clone()));
        }
    }
    for e in &doc.entries {
        if let Some(id) = node(NodeId::Entry(e.id.clone())) {
            out.extend(e.when.as_ref().map(|w| (id.clone(), w.raw.clone())));
            out.extend(on(&e.on, &e.target).map(|g| (id, g)));
        }
    }
    if let Some(doc_id) = folded.typed.id.as_deref() {
        for b in &doc.beats {
            let key = crate::bundles::bundle_beat_key(doc_id, &b.id);
            if let Some(id) = node(NodeId::Beat(key)) {
                out.extend(b.when.as_ref().map(|w| (id.clone(), w.raw.clone())));
                out.extend(on(&b.on, &b.target).map(|g| (id, g)));
            }
        }
    }
    out
}

/// The `raisedWhen` of occasion `on` as a beat answering it for `target`
/// reads it: ground with the member a concrete target names. `None` without
/// a gate, or for a gate reading `occasion.target` when the beat names no
/// single member (untargeted, or a whole kind).
fn occasion_gate(folded: &FoldedEnv, on: &str, target: Option<&str>) -> Option<String> {
    let gate = crate::gates::gate_of(&folded.occasions, on)?;
    if !crate::occasion_bind::mentions_target(gate) {
        return Some(gate.to_string());
    }
    let member = crate::gates::target_member(folded.occasions.get(on)?, target?)?;
    Some(crate::occasion_bind::instantiate(gate, &member))
}

/// Every positive top-level conjunct `holds('rel', [args…])` of `raw` (after
/// `@def` expansion).
fn required_holds(raw: &str, defs: &DefTable<'_>) -> Vec<(String, Args)> {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, None, &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let Some(expr) = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)
        .and_then(|h| arena.get(h).map(|e| e.expr.clone()))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    conjuncts(&expr, &mut out);
    out
}

fn conjuncts(expr: &Expr, out: &mut Vec<(String, Args)>) {
    let Expr::Call(c) = expr else {
        return;
    };
    if c.target.is_some() {
        return;
    }
    if c.func_name == op::LOGICAL_AND && c.args.len() == 2 {
        conjuncts(&c.args[0].expr, out);
        conjuncts(&c.args[1].expr, out);
        return;
    }
    if c.func_name != "holds" {
        return;
    }
    if let Some(query) = crate::fact_env::QueryPattern::from_call(c) {
        out.push((query.relation, query.args));
    }
}

/// What [`Ask::sources`] reads: the root's assert sites and rules.
struct Ask<'a> {
    producers: &'a FactProducers,
    vocab: &'a RelVocab,
}

/// A producing site, the fact it asserts, and whether the gate reads it
/// through a rule.
type Found = ((PathBuf, usize), String, bool);

impl Ask<'_> {
    /// The assert sites that can produce a fact matching `rel(args)` —
    /// directly, or through a rule deriving it (`depth` rules deep; `via`
    /// once inside one) — each with the fact it asserts.
    fn sources(
        &self,
        rel: &str,
        args: &[Option<String>],
        depth: u8,
        via: bool,
        out: &mut Vec<Found>,
    ) {
        for (path, key, site) in self.producers.sites(rel) {
            if site.len() == args.len()
                && site
                    .iter()
                    .zip(args)
                    .all(|(s, a)| s.is_none() || a.is_none() || s == a)
            {
                let fact: Args = site
                    .iter()
                    .zip(args)
                    .map(|(s, a)| s.clone().or_else(|| a.clone()))
                    .collect();
                out.push(((path.clone(), *key), show(rel, &fact), via));
            }
        }
        if depth == 0 || !self.vocab.relations.get(rel).is_some_and(|d| d.derive) {
            return;
        }
        for r in self
            .vocab
            .rules
            .iter()
            .filter(|r| r.rule.head.relation == rel)
        {
            let head = &r.rule.head.terms;
            if head.len() != args.len() {
                continue;
            }
            let mut bound: BTreeMap<&str, &str> = BTreeMap::new();
            let mut unifies = true;
            for (t, a) in head.iter().zip(args) {
                match (t, a) {
                    (RuleTerm::Var(v), Some(a)) => {
                        if bound.insert(v, a).is_some_and(|prev| prev != a) {
                            unifies = false;
                        }
                    }
                    (RuleTerm::Const(c), Some(a)) => unifies &= c == a,
                    (RuleTerm::Bool(b), Some(a)) => unifies &= b.to_string() == *a,
                    (_, None) => {}
                }
            }
            if !unifies {
                continue;
            }
            for lit in &r.rule.body {
                let BodyLiteral::Pos(atom) = lit else {
                    continue;
                };
                let sub: Args = atom
                    .terms
                    .iter()
                    .map(|t| match t {
                        RuleTerm::Var(v) => bound.get(v.as_str()).map(|s| s.to_string()),
                        RuleTerm::Const(c) => Some(c.clone()),
                        RuleTerm::Bool(b) => Some(b.to_string()),
                    })
                    .collect();
                self.sources(&atom.relation, &sub, depth - 1, true, out);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive_facts::EffectDirectives;
    use crate::rel_schema::RelVocab;
    use std::path::PathBuf;

    #[test]
    fn list_form_gate_extracts_required_fact_atom_and_producer() {
        let defs = DefTable {
            bodies: &BTreeMap::new(),
            params: &BTreeMap::new(),
        };
        let atoms = required_holds("holds('met', ['maud'])", &defs);
        assert_eq!(atoms, vec![("met".to_string(), vec![Some("maud".to_string())])]);

        let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
                   ## One\n::assert{met(maud)}\n";
        let (doc, _) = lute_syntax::parse(text);
        let path = PathBuf::from("producer.lute");
        let docs = vec![(path.clone(), doc)];
        let producers = crate::cast::fact_producers(&docs, &EffectDirectives::default());
        let vocab = RelVocab::default();
        let ask = Ask {
            producers: &producers,
            vocab: &vocab,
        };
        let mut found = Vec::new();
        ask.sources(
            "met",
            &[Some("maud".to_string())],
            RULE_DEPTH,
            false,
            &mut found,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, (path, 0));
    }
}
