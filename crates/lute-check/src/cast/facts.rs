use super::*;

pub fn occasions_before(
    docs: &[(std::path::PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    graph: &crate::connectivity::ConnGraph,
) -> BTreeMap<std::path::PathBuf, BTreeMap<usize, BTreeSet<String>>> {
    use crate::connectivity::{EdgeKind, NodeId};
    let mut out: BTreeMap<std::path::PathBuf, BTreeMap<usize, BTreeSet<String>>> = BTreeMap::new();
    let named: BTreeSet<&str> = foldeds
        .iter()
        .flat_map(|f| f.env.rel_vocab.relations.values())
        .filter(|d| d.reserved)
        .flat_map(|d| d.changed_on.iter().map(String::as_str))
        .collect();
    if named.is_empty() {
        return out;
    }
    // A node's document index, unit key and the occasion it is presented on.
    let unit_of = |id: &NodeId| -> Option<(usize, usize, Option<&str>)> {
        let info = graph.nodes.get(id)?;
        let i = docs.iter().position(|(p, _)| *p == info.path)?;
        let doc = &docs[i].1;
        match id {
            NodeId::Scene(_) => Some((
                i,
                0,
                foldeds.get(i)?.typed.beat.as_ref().map(|b| b.on.as_str()),
            )),
            NodeId::Quest(q) => doc
                .quests
                .iter()
                .find(|x| x.id == *q)
                .map(|x| (i, x.span.byte_start, None)),
            NodeId::Beat(key) => {
                let doc_id = crate::connectivity::bundle_id(doc)?;
                doc.beats
                    .iter()
                    .find(|b| crate::bundles::bundle_beat_key(&doc_id, &b.id) == *key)
                    .map(|b| (i, b.span.byte_start, b.on.as_ref().map(|(o, _)| o.as_str())))
            }
            NodeId::Entry(e) => doc
                .entries
                .iter()
                .find(|x| x.id == *e)
                .map(|x| (i, x.span.byte_start, x.on.as_ref().map(|(o, _)| o.as_str()))),
        }
    };
    let orders = |from: &NodeId, to: &NodeId| {
        graph.edge_kinds_for(from, to).is_some_and(|ks| {
            ks.iter().any(|k| {
                matches!(
                    k,
                    EdgeKind::Visited | EdgeKind::Completed | EdgeKind::Active | EdgeKind::Start
                )
            })
        })
    };
    for id in graph.nodes.keys() {
        let Some(occasion) = unit_of(id)
            .and_then(|(_, _, on)| on)
            .filter(|o| named.contains(o))
        else {
            continue;
        };
        let mut seen = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(from) = stack.pop() {
            for to in graph.edges.get(from).into_iter().flatten() {
                if orders(from, to) && seen.insert(to) {
                    stack.push(to);
                }
            }
        }
        for to in seen {
            if let Some((i, key, _)) = unit_of(to) {
                out.entry(docs[i].0.clone())
                    .or_default()
                    .entry(key)
                    .or_default()
                    .insert(occasion.to_string());
            }
        }
    }
    out
}

/// Every `::assert` site of one project root, by relation — and (dsl 0.27.0
/// §4) every directive call's declared `effects.asserts`: its document,
/// its unit (`0` for a scene's shots, else the span start of the quest,
/// entry or bundle beat) and its arguments (`None` for anything but a
/// constant — a component param, `_`).
#[derive(Default)]
pub struct FactProducers(BTreeMap<String, Vec<(std::path::PathBuf, usize, Vec<Option<String>>)>>);

impl FactProducers {
    /// Every assert site of `relation`: document, unit, arguments.
    pub(crate) fn sites(
        &self,
        relation: &str,
    ) -> impl Iterator<Item = &(std::path::PathBuf, usize, Vec<Option<String>>)> {
        self.0.get(relation).into_iter().flatten()
    }
}

/// The [`FactProducers`] of `docs` (one resolved root). A component
/// document's own sites are not producers: its writes happen where a `::use`
/// performs them, bound — the host documents carry them spliced
/// ([`crate::component_effects::splice_component_effects`]) — so an unused
/// component produces nothing and a used one only its bound arguments.
pub fn fact_producers(
    docs: &[(std::path::PathBuf, Document)],
    effects: &crate::directive_facts::EffectDirectives,
) -> FactProducers {
    let mut out = FactProducers::default();
    for (path, doc) in docs {
        if crate::meta::infer_meta_kind_from_shape(&doc.meta, true)
            == Some(crate::meta::MetaKind::Component)
        {
            continue;
        }
        let units = std::iter::once((0, doc.shots.iter().map(|s| &s.body[..]).collect::<Vec<_>>()))
            .chain(
                doc.quests
                    .iter()
                    .map(|q| (q.span.byte_start, vec![&q.body[..]])),
            )
            .chain(
                doc.entries
                    .iter()
                    .map(|e| (e.span.byte_start, vec![&e.body[..]])),
            )
            .chain(
                doc.beats
                    .iter()
                    .map(|b| (b.span.byte_start, vec![&b.body[..]])),
            );
        for (key, bodies) in units {
            for body in bodies {
                visit(body, &mut |node| match node {
                    Node::Assert(a) if !a.pattern.relation.is_empty() => {
                        out.0.entry(a.pattern.relation.clone()).or_default().push((
                            path.clone(),
                            key,
                            pattern_args(&a.pattern),
                        ));
                    }
                    Node::Directive(d) => {
                        for p in crate::directive_facts::lookup(effects, &d)
                            .map(|f| f.asserts)
                            .unwrap_or_default()
                        {
                            out.0.entry(p.relation.clone()).or_default().push((
                                path.clone(),
                                key,
                                pattern_args(&p),
                            ));
                        }
                    }
                    _ => {}
                });
            }
        }
    }
    out
}

/// One fact write of a project root: an `::assert` / `::retract`, or one
/// fact a directive call's declared `effects.asserts` / `retracts` writes.
pub(crate) struct FactWrite {
    pub(crate) rel: String,
    /// A constant argument, else `None` (`_`, a component param).
    pub(crate) args: Vec<Option<String>>,
    /// `true` for an assert.
    pub(crate) up: bool,
    /// How the author wrote it: `` `::retract{solved(drawer)}` ``, or
    /// `` `::sell` (it retracts `caught(marlin)`) ``.
    pub(crate) text: String,
    pub(crate) path: std::path::PathBuf,
    pub(crate) span: Span,
}

/// Every fact write of `docs` (one resolved root), asserts and retracts
/// alike, in document order. A component document's own sites are left out
/// as in [`fact_producers`]: its host documents carry them spliced.
pub(crate) fn fact_writes(
    docs: &[(std::path::PathBuf, Document)],
    effects: &crate::directive_facts::EffectDirectives,
) -> Vec<FactWrite> {
    let mut out = Vec::new();
    for (path, doc) in docs {
        if crate::meta::infer_meta_kind_from_shape(&doc.meta, true)
            == Some(crate::meta::MetaKind::Component)
        {
            continue;
        }
        for body in doc_bodies(doc) {
            visit(body, &mut |node| {
                let mut push = |p: &FactPattern, up: bool, text: String, span: Span| {
                    if !p.relation.is_empty() {
                        out.push(FactWrite {
                            rel: p.relation.clone(),
                            args: pattern_args(p),
                            up,
                            text,
                            path: path.clone(),
                            span,
                        });
                    }
                };
                let written = |verb: &str, p: &FactPattern| {
                    format!("`::{verb}{{{}}}`", crate::directive_facts::pattern_text(p))
                };
                match node {
                    Node::Assert(a) => {
                        push(&a.pattern, true, written("assert", &a.pattern), a.span)
                    }
                    Node::Retract(r) => {
                        push(&r.pattern, false, written("retract", &r.pattern), r.span)
                    }
                    Node::Directive(d) => {
                        let Some(facts) = crate::directive_facts::lookup(effects, &d) else {
                            return;
                        };
                        for (p, up) in facts.writes() {
                            let verb = if up { "asserts" } else { "retracts" };
                            let text = format!(
                                "`::{}` (it {verb} `{}`)",
                                d.tag,
                                crate::directive_facts::pattern_text(p)
                            );
                            push(p, up, text, d.span);
                        }
                    }
                    _ => {}
                }
            });
        }
    }
    out
}

/// A fact pattern's arguments: a constant, else `None`.
pub(crate) fn pattern_args(pattern: &FactPattern) -> Vec<Option<String>> {
    pattern
        .args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) => Some(s.clone()),
            FactTerm::Bool(b) => Some(b.to_string()),
            _ => None,
        })
        .collect()
}

/// Two argument lists that may name the same fact.
fn unifiable(a: &[Option<String>], b: &[Option<String>]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.is_none() || y.is_none() || x == y)
}

/// One ground fact a unit asserts ([`unit_facts`]).
pub(crate) struct UnitFact {
    /// Authored ground fact used in diagnostic explanations (`hasItem(key)`).
    pub(crate) key: String,
    /// Canonical list-form pattern shared with reachability's fact pseudo-path.
    pub(crate) pattern: String,
    /// List-form host query used when constructing a guard (`holds('rel', [...])`).
    pub(crate) query: String,
    /// `None` when the fact cannot hold before the unit is spent: no other
    /// unit can assert it (no unifiable site elsewhere, a `::use` site's
    /// bound component writes included), no seed names it, and it cannot
    /// survive from an earlier presentation of the unit — a `tier: run`
    /// relation in a unit presented at most once per run (`once: run` or
    /// `user`), a `tier: user`/`app` one in a `once: user` unit; derived
    /// and engine-`reserved` relations never qualify. Otherwise why it may.
    pub(crate) persists: Option<String>,
}

/// Every ground fact unit `key` of document `path` asserts, with whether it
/// can hold while the unit (spent by `once`) is still unplayed — what
/// `W-CAST-ABSENT` assumes absent at the unit's start (dsl 0.24.0 §4) and
/// `W-BEAT-PRIORITY-TIE` conjoins to the unit's eligibility (dsl 0.26.0 §8).
pub(crate) fn unit_facts(
    producers: &FactProducers,
    vocab: &crate::rel_schema::RelVocab,
    path: &Path,
    key: usize,
    once: &BeatOnce,
) -> Vec<UnitFact> {
    let mut out = Vec::new();
    let here = |p: &std::path::PathBuf, k: usize| p.as_path() == path && k == key;
    for (rel, sites) in &producers.0 {
        let Some(decl) = vocab.relations.get(rel) else {
            continue;
        };
        let tier = decl.tier.as_deref().unwrap_or("run");
        for (_, _, args) in sites.iter().filter(|(p, k, _)| here(p, *k)) {
            let Some(ground) = args.iter().cloned().collect::<Option<Vec<String>>>() else {
                continue;
            };
            let fact = format!("{rel}({})", ground.join(", "));
            let spent_by_run = matches!(once, BeatOnce::Run | BeatOnce::User);
            let persists = if decl.derive {
                Some(format!("rules may derive `{fact}` too"))
            } else if decl.reserved {
                Some(format!(
                    "the engine may assert `{fact}` (a reserved relation)"
                ))
            } else if !spent_by_run {
                Some(match once {
                    BeatOnce::None => format!(
                        "it is not spent by being presented, so it may play again after \
                         asserting `{fact}`"
                    ),
                    period => format!(
                        "`once: {}` spends it for one period, not one run, so it may play again \
                         after asserting `{fact}`",
                        period.as_str()
                    ),
                })
            } else if matches!(tier, "user" | "app") && *once != BeatOnce::User {
                Some(format!(
                    "`{fact}` is `tier: {tier}`, which persists across runs; `once: run` does \
                     not, so in a later run `{fact}` holds before it plays again"
                ))
            } else if !matches!(tier, "run" | "user" | "app") {
                Some(format!("`{fact}` is `tier: {tier}`"))
            } else if let Some((other, _, _)) = sites
                .iter()
                .find(|(p, k, a)| !here(p, *k) && unifiable(a, args))
            {
                Some(format!("`{fact}` is also asserted in {}", other.display()))
            } else if vocab
                .facts
                .iter()
                .any(|f| f.fact.relation == *rel && unifiable(&pattern_args(&f.fact), args))
            {
                Some(format!("`{fact}` is a `facts:` seed"))
            } else {
                None
            };
            let pattern = crate::fact_env::QueryPattern {
                relation: rel.clone(),
                args: ground.into_iter().map(Some).collect(),
            }
            .to_string();
            out.push(UnitFact {
                key: fact,
                query: format!("holds{pattern}"),
                pattern,
                persists,
            });
        }
    }
    out
}
