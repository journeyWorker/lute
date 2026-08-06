//! Project index (`lute compile --all`) — the UNION of every document's
//! artifact vocabulary, plus the document table an engine needs to find them.
//!
//! `docs/runtime/execution-model.md` requires an engine to union `entities` /
//! `enums` / `relations` / `seedFacts` / `rules` / `prereqEdges` across every
//! compiled document before it can evaluate anything: a single artifact carries
//! only the vocabulary ITS own `uses:`/inline declarations folded to, and a
//! relation asserted in one scene is queried from another. Before 0.8.0 every
//! adopter re-implemented that union by hand, each with its own conflict
//! policy. [`build_index`] is that union, computed once by the toolchain that
//! produced the artifacts.
//!
//! ## Determinism
//! Every output array is sorted by a total key and deduplicated:
//! - `documents` by `path`;
//! - `entities` / `enums` / `relations` by `name` — the SAME name-sorted rule a
//!   single artifact already follows (`RelVocab`'s maps are `BTreeMap`s);
//! - `prereqEdges` by `node`, matching the single artifact's own rule;
//! - `seedFacts` by `(relation, args)` and `rules` by `(head relation, raw)`.
//!   A single artifact emits these two in vocabulary (import-then-inline)
//!   order, which is only meaningful WITHIN one document — a union has no such
//!   order, so it gets an explicit total one instead.
//!
//! ## Conflicts are errors, never a silent pick
//! Two documents declaring the same entity kind / enum / relation / prerequisite
//! node with a DIFFERENT signature cannot both be right, and picking one would
//! silently change what half the project validated against. That is an
//! [`IndexError`], reported with the same wording the composition layer uses for
//! a cross-import duplicate (`E-USES-DUP-RELATION`'s "declared by two imports").
//! `seedFacts` and `rules` have no signature to conflict over — an identical
//! tuple/rule from two documents is one fact, deduplicated (spec §4.1: facts and
//! rules always UNION, never dup-checked).

use std::collections::BTreeMap;

use lute_check::meta::canonical_episode_key;
use serde::Serialize;

use crate::ir::{
    Artifact, ArtifactMeta, Command, DocKind, EntityKindEntry, EnumEntry, PrereqEdgeEntry,
    RelationEntry, RuleEntry, SeedFactEntry,
};

/// One document's row in the index. Paths are FORWARD-SLASH relative to the
/// project root, never absolute — an index is a build output that must survive
/// being copied to another machine or shipped inside a game package.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDocument {
    /// Source document, relative to the project root (`quests/a.lute`).
    pub path: String,
    /// Its compiled artifact, relative to the output directory
    /// (`quests/a.lute.json`).
    pub artifact: String,
    pub kind: DocKind,
    /// The document's canonical node key: a scene's `{character}.{episodeId}`
    /// ([`canonical_episode_key`]) or a quest document's first declared
    /// `<quest id>` (document order = addressing order). A quest PACK's
    /// remaining ids stay recoverable from its own artifact's `quest` records —
    /// the index names the document, it does not replace it.
    pub key: String,
}

/// The `project.index.json` envelope. Field DECLARATION ORDER is the serialized
/// order, exactly as [`Artifact`] does it — a `serde_json::Map` would sort the
/// keys alphabetically instead.
///
/// The six vocabulary arrays are ALWAYS emitted, empty included: an engine
/// unions them unconditionally, and an absent key would force it to distinguish
/// "no relations" from "index too old to carry them".
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndex {
    pub ir_version: String,
    pub capability_version: String,
    pub documents: Vec<IndexDocument>,
    pub entities: Vec<EntityKindEntry>,
    pub enums: Vec<EnumEntry>,
    pub relations: Vec<RelationEntry>,
    #[serde(rename = "seedFacts")]
    pub seed_facts: Vec<SeedFactEntry>,
    pub rules: Vec<RuleEntry>,
    #[serde(rename = "prereqEdges")]
    pub prereq_edges: Vec<PrereqEdgeEntry>,
}

impl ProjectIndex {
    /// Pretty-printed + newline terminated, like every other artifact this
    /// toolchain writes.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }
}

/// One document's contribution to the index.
pub struct IndexInput<'a> {
    /// Source path, forward-slash relative to the project root.
    pub path: String,
    /// Artifact path, forward-slash relative to the output directory.
    pub artifact_path: String,
    pub artifact: &'a Artifact,
}

/// Why an index could not be built. Both variants name BOTH offending
/// documents — a conflict report that names one side is not actionable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    /// Two documents resolved DIFFERENT capability snapshots. A project
    /// resolves one snapshot per document by `profile:`, so this means two
    /// profiles are in play and the index has no single stamp to carry.
    CapabilityMismatch {
        first_doc: String,
        first: String,
        other_doc: String,
        other: String,
    },
    /// Two documents declared `name` with different signatures.
    Conflict {
        noun: &'static str,
        name: String,
        first_doc: String,
        other_doc: String,
    },
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::CapabilityMismatch {
                first_doc,
                first,
                other_doc,
                other,
            } => write!(
                f,
                "documents resolve different capability snapshots: `{first_doc}` is `{first}` \
                 but `{other_doc}` is `{other}` — two profiles are in play, so the project \
                 has no single `capabilityVersion` to index (plugin §13)"
            ),
            IndexError::Conflict {
                noun,
                name,
                first_doc,
                other_doc,
            } => write!(
                f,
                "{noun} `{name}` is declared with conflicting signatures by two documents \
                 (`{first_doc}` and `{other_doc}`)"
            ),
        }
    }
}

/// Accumulate one name-keyed vocabulary axis, rejecting a same-name /
/// different-signature redeclaration. "Signature" is the entry's own SERIALIZED
/// form — exactly the bytes that would reach an artifact — so the comparison can
/// never drift from what the union actually emits.
struct Axis<T> {
    noun: &'static str,
    seen: BTreeMap<String, (String, T, String)>,
}

impl<T: Clone + Serialize> Axis<T> {
    fn new(noun: &'static str) -> Self {
        Self {
            noun,
            seen: BTreeMap::new(),
        }
    }

    fn push(&mut self, name: &str, entry: &T, doc: &str, errors: &mut Vec<IndexError>) {
        // A `Serialize` impl over plain owned data cannot fail; the fallback
        // keeps this total rather than panicking on a hypothetical one.
        let sig = serde_json::to_string(entry).unwrap_or_default();
        match self.seen.get(name) {
            Some((prev_sig, _, prev_doc)) => {
                if *prev_sig != sig {
                    errors.push(IndexError::Conflict {
                        noun: self.noun,
                        name: name.to_string(),
                        first_doc: prev_doc.clone(),
                        other_doc: doc.to_string(),
                    });
                }
            }
            None => {
                self.seen
                    .insert(name.to_string(), (sig, entry.clone(), doc.to_string()));
            }
        }
    }

    /// Name-sorted by construction (`BTreeMap`).
    fn finish(self) -> Vec<T> {
        self.seen.into_values().map(|(_, entry, _)| entry).collect()
    }
}

/// Build the project index over already-compiled artifacts.
///
/// `docs` may arrive in any order — `documents` is sorted by `path` and every
/// vocabulary axis by its own total key, so the output is byte-stable. Conflict
/// reporting IS order-sensitive in one harmless way: the "first" document named
/// in an [`IndexError::Conflict`] is whichever came first in `docs`, so callers
/// pass a path-sorted slice to make the message itself deterministic too.
///
/// `Err` carries EVERY problem found, not just the first — a project with three
/// conflicting relations should report three, not force three rebuilds.
pub fn build_index(ir_version: &str, docs: &[IndexInput<'_>]) -> Result<ProjectIndex, Vec<IndexError>> {
    let mut errors = Vec::new();

    let mut capability: Option<(&str, &str)> = None;
    for d in docs {
        match capability {
            None => capability = Some((d.path.as_str(), d.artifact.capability_version.as_str())),
            Some((first_doc, first)) if first != d.artifact.capability_version => {
                errors.push(IndexError::CapabilityMismatch {
                    first_doc: first_doc.to_string(),
                    first: first.to_string(),
                    other_doc: d.path.clone(),
                    other: d.artifact.capability_version.clone(),
                });
            }
            Some(_) => {}
        }
    }

    let mut entities = Axis::new("entity kind");
    let mut enums = Axis::new("enum");
    let mut relations = Axis::new("relation");
    let mut prereqs = Axis::new("prerequisite node");
    // Facts and rules always UNION (spec §4.1) — an identical tuple/rule from
    // two documents is ONE declaration, so these dedupe on the whole value and
    // can never conflict.
    let mut seed_facts: BTreeMap<(String, Vec<String>), SeedFactEntry> = BTreeMap::new();
    let mut rules: BTreeMap<(String, String), RuleEntry> = BTreeMap::new();

    for d in docs {
        let a = d.artifact;
        for e in &a.entities {
            entities.push(&e.name, e, &d.path, &mut errors);
        }
        for e in &a.enums {
            enums.push(&e.name, e, &d.path, &mut errors);
        }
        for r in &a.relations {
            relations.push(&r.name, r, &d.path, &mut errors);
        }
        for p in &a.prereq_edges {
            prereqs.push(&p.node, p, &d.path, &mut errors);
        }
        for f in &a.seed_facts {
            seed_facts
                .entry((f.relation.clone(), f.args.clone()))
                .or_insert_with(|| f.clone());
        }
        for r in &a.rules {
            rules
                .entry((r.head.relation.clone(), r.raw.clone()))
                .or_insert_with(|| r.clone());
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut documents: Vec<IndexDocument> = docs
        .iter()
        .map(|d| IndexDocument {
            path: d.path.clone(),
            artifact: d.artifact_path.clone(),
            kind: d.artifact.kind,
            key: document_key(d.artifact),
        })
        .collect();
    documents.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(ProjectIndex {
        ir_version: ir_version.to_string(),
        capability_version: capability.map(|(_, v)| v.to_string()).unwrap_or_default(),
        documents,
        entities: entities.finish(),
        enums: enums.finish(),
        relations: relations.finish(),
        seed_facts: seed_facts.into_values().collect(),
        rules: rules.into_values().collect(),
        prereq_edges: prereqs.finish(),
    })
}

/// The document's canonical node key (see [`IndexDocument::key`]). A scene's key
/// is recomputed through the SHARED [`canonical_episode_key`] the addressing
/// prefix and `check-project`'s scene-key grouping both use, so the index can
/// never name a scene differently from the graph. A quest document with no
/// `<quest>` at all has no key; that shape never survives the check gate, so
/// the empty string is a total fallback, not a real output.
pub fn document_key(artifact: &Artifact) -> String {
    match &artifact.meta {
        ArtifactMeta::Scene(m) => {
            canonical_episode_key(&m.character, m.season, m.episode, Some(&m.episode_id))
        }
        ArtifactMeta::Quest(_) => artifact
            .commands
            .iter()
            .find_map(|c| match c {
                Command::Quest(q) => Some(q.id.clone()),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AtomEntry, SceneMeta};

    fn scene(character: &str, capability: &str) -> Artifact {
        Artifact {
            kind: DocKind::Scene,
            lute: "0.10.0".to_string(),
            ir_version: "0.10.0".to_string(),
            capability_version: capability.to_string(),
            meta: ArtifactMeta::Scene(SceneMeta {
                character: character.to_string(),
                season: 1,
                episode: 2,
                episode_id: "s01ep02".to_string(),
                title: None,
            }),
            state: Vec::new(),
            entities: Vec::new(),
            enums: Vec::new(),
            relations: Vec::new(),
            seed_facts: Vec::new(),
            rules: Vec::new(),
            commands: Vec::new(),
            prereq_edges: Vec::new(),
            shots: Vec::new(),
        }
    }

    fn relation(name: &str, args: &[&str]) -> RelationEntry {
        RelationEntry {
            name: name.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            tier: Some("run".to_string()),
            derive: false,
            reserved: false,
            key: Vec::new(),
        }
    }

    fn fact(relation: &str, arg: &str) -> SeedFactEntry {
        SeedFactEntry {
            relation: relation.to_string(),
            args: vec![arg.to_string()],
        }
    }

    fn rule(head: &str, raw: &str) -> RuleEntry {
        RuleEntry {
            head: AtomEntry {
                relation: head.to_string(),
                terms: Vec::new(),
            },
            body: Vec::new(),
            raw: raw.to_string(),
        }
    }

    fn inputs<'a>(docs: &'a [(&str, Artifact)]) -> Vec<IndexInput<'a>> {
        docs.iter()
            .map(|(p, a)| IndexInput {
                path: (*p).to_string(),
                artifact_path: format!("{p}.json"),
                artifact: a,
            })
            .collect()
    }

    #[test]
    fn unions_dedupes_and_sorts_every_axis() {
        let mut a = scene("bianca", "cap-1");
        a.relations = vec![relation("knows", &["npc"]), relation("owns", &["item"])];
        a.seed_facts = vec![fact("knows", "kai"), fact("owns", "key")];
        a.rules = vec![rule("trusts", "trusts(X) :- knows(X)")];
        a.prereq_edges = vec![PrereqEdgeEntry {
            node: "bianca.s01ep02".to_string(),
            after: "visited(\"a.b\")".to_string(),
        }];
        let mut b = scene("kai", "cap-1");
        // Same relation + same fact + same rule as `a`: one union entry each.
        b.relations = vec![relation("knows", &["npc"]), relation("aware", &["npc"])];
        b.seed_facts = vec![fact("knows", "kai")];
        b.rules = vec![rule("trusts", "trusts(X) :- knows(X)")];

        // Deliberately UNSORTED input: `documents` must still come out sorted.
        let docs = [("z/b.lute", b), ("a/a.lute", a)];
        let index = build_index("0.9.0", &inputs(&docs)).expect("no conflicts");

        assert_eq!(
            index.documents.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
            vec!["a/a.lute", "z/b.lute"],
            "documents sort by path"
        );
        assert_eq!(index.documents[0].artifact, "a/a.lute.json");
        assert_eq!(index.documents[0].key, "bianca.s01ep02");
        assert_eq!(index.capability_version, "cap-1");
        assert_eq!(
            index.relations.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["aware", "knows", "owns"],
            "relations union, dedupe, and sort by name"
        );
        assert_eq!(index.seed_facts.len(), 2, "the shared fact dedupes");
        assert_eq!(index.rules.len(), 1, "the shared rule dedupes");
        assert_eq!(index.prereq_edges.len(), 1);
    }

    #[test]
    fn a_conflicting_signature_is_an_error_not_a_silent_pick() {
        let mut a = scene("bianca", "cap-1");
        a.relations = vec![relation("knows", &["npc"])];
        let mut b = scene("kai", "cap-1");
        b.relations = vec![relation("knows", &["npc", "item"])];
        let docs = [("a.lute", a), ("b.lute", b)];
        let errors = build_index("0.9.0", &inputs(&docs)).expect_err("arity differs");
        assert_eq!(
            errors,
            vec![IndexError::Conflict {
                noun: "relation",
                name: "knows".to_string(),
                first_doc: "a.lute".to_string(),
                other_doc: "b.lute".to_string(),
            }]
        );
        assert!(errors[0].to_string().contains("conflicting signatures"), "{}", errors[0]);
    }

    #[test]
    fn two_capability_snapshots_are_an_error() {
        let docs = [("a.lute", scene("bianca", "cap-1")), ("b.lute", scene("kai", "cap-2"))];
        let errors = build_index("0.9.0", &inputs(&docs)).expect_err("two profiles");
        assert!(
            matches!(errors[0], IndexError::CapabilityMismatch { .. }),
            "{errors:?}"
        );
    }

    #[test]
    fn empty_vocabulary_arrays_are_still_emitted() {
        let docs = [("a.lute", scene("bianca", "cap-1"))];
        let index = build_index("0.9.0", &inputs(&docs)).unwrap();
        let json = index.to_json().unwrap();
        for key in ["entities", "enums", "relations", "seedFacts", "rules", "prereqEdges"] {
            assert!(json.contains(&format!("\"{key}\": []")), "missing empty `{key}`: {json}");
        }
        // Declaration order, not alphabetical.
        let pos = |k: &str| json.find(k).unwrap_or(usize::MAX);
        assert!(pos("\"irVersion\"") < pos("\"capabilityVersion\""));
        assert!(pos("\"capabilityVersion\"") < pos("\"documents\""));
        assert!(pos("\"documents\"") < pos("\"entities\""));
        assert!(json.ends_with('\n'));
    }
}
