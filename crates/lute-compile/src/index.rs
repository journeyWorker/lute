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
//! - `entries` (dsl 0.19.0 §7) by document `path`, then document order
//!   within each lore document — the declaration order an engine uses as
//!   its eligibility tiebreak;
//! - `beats` (dsl 0.21.0 §8) by the same rule — document `path`, then
//!   declaration order — the selection tiebreak after priority;
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

use serde::Serialize;

use crate::ir::{
    Artifact, ArtifactMeta, BeatOnce, Command, DocKind, EntityKindEntry, EnumEntry,
    PrereqEdgeEntry, RelationEntry, RuleEntry, SeedFactEntry,
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
    /// ([`canonical_episode_key`]); a quest or lore document's authored `id:`
    /// (dsl 0.19.0 §2.1), else its first declared `<quest id>` / `<entry
    /// id>` (document order = addressing order). A quest PACK's / lore
    /// document's ids stay recoverable from its own artifact's `quest` /
    /// `entry` records (and, for entries, [`ProjectIndex::entries`]) — the
    /// index names the document, it does not replace it.
    pub key: String,
}

/// One `<entry>` row of [`ProjectIndex::entries`] (dsl 0.19.0 §7): enough for
/// an engine to build its `target → entries` / `series → entries` tables
/// without loading every lore artifact. `document` is the SAME string as the
/// owning [`IndexDocument::path`].
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub document: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
}

/// What a [`ProjectIndex::beats`] row declares (dsl 0.21.0 §8): a scene beat
/// (`SceneMeta.beat`) or an entry beat (`EntryCmd.on`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatKind {
    Scene,
    Entry,
}

/// One row of [`ProjectIndex::beats`] (dsl 0.21.0 §4/§8): every beat in the
/// project, so an engine can build its `occasion → candidates` table without
/// loading every artifact. Row order IS the selection tiebreak after
/// priority. `id` is the scene's canonical id ([`SceneMeta::id`]) or the
/// entry id; `document` is the owning [`IndexDocument::path`]; `priority` is
/// resolved (unauthored → `0`); `once` is the scene's policy, or an entry's
/// authored `once` (dsl 0.22.0 §7) — absent on an entry row = repeatable.
///
/// [`SceneMeta::id`]: crate::ir::SceneMeta::id
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexBeat {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub priority: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<BeatOnce>,
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
    /// dsl 0.19.0 §7: every `<entry>` in the project, document order. Unlike
    /// the vocabulary arrays above this is OMITTED when empty, so an index
    /// over a project without lore stays byte-identical to 0.18.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<IndexEntry>,
    /// dsl 0.21.0 §8: every beat in the project — documents in `documents`
    /// (path) order, declaration order within each — the selection
    /// tiebreak after priority. OMITTED when empty, so an index over a
    /// project without beats stays byte-identical to 0.20.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub beats: Vec<IndexBeat>,
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
pub fn build_index(
    ir_version: &str,
    docs: &[IndexInput<'_>],
) -> Result<ProjectIndex, Vec<IndexError>> {
    let mut errors = Vec::new();

    let capability: Option<(&str, &str)> = docs
        .first()
        .map(|d| (d.path.as_str(), d.artifact.capability_version.as_str()));
    errors.extend(capability_mismatches(
        docs.iter()
            .map(|d| (d.path.as_str(), d.artifact.capability_version.as_str())),
    ));

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

    // Entries follow the SAME path order as `documents`, then each lore
    // artifact's own command (= document) order.
    let mut by_path: Vec<&IndexInput<'_>> = docs.iter().collect();
    by_path.sort_by(|a, b| a.path.cmp(&b.path));
    let entries = by_path
        .iter()
        .flat_map(|d| {
            d.artifact.commands.iter().filter_map(move |c| match c {
                Command::Entry(e) => Some(IndexEntry {
                    id: e.id.clone(),
                    document: d.path.clone(),
                    target: e.target.clone(),
                    category: e.category.clone(),
                    series: e.series.clone(),
                    order: e.order,
                }),
                _ => None,
            })
        })
        .collect();

    // Beats (dsl 0.21.0 §8) follow the same path order: a scene document
    // contributes at most its one `meta.beat`, a lore document every entry
    // that names an occasion, in command (= declaration) order.
    let beats = by_path
        .iter()
        .flat_map(|d| {
            let scene = match &d.artifact.meta {
                ArtifactMeta::Scene(m) => m.beat.as_ref().map(|b| IndexBeat {
                    id: m.id.clone(),
                    kind: BeatKind::Scene,
                    document: d.path.clone(),
                    on: b.on.clone(),
                    target: b.target.clone(),
                    priority: b.priority,
                    once: Some(b.once),
                }),
                ArtifactMeta::Quest(_) | ArtifactMeta::Lore(_) => None,
            };
            let entries = d.artifact.commands.iter().filter_map(move |c| match c {
                Command::Entry(e) => e.on.as_ref().map(|on| IndexBeat {
                    id: e.id.clone(),
                    kind: BeatKind::Entry,
                    document: d.path.clone(),
                    on: on.clone(),
                    target: e.target.clone(),
                    priority: e.priority.unwrap_or(0),
                    once: e.once,
                }),
                _ => None,
            });
            scene.into_iter().chain(entries)
        })
        .collect();

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
        entries,
        beats,
    })
}

/// `E-CAPABILITY-MISMATCH`: the code `check-project` reports an
/// [`IndexError::CapabilityMismatch`] under (0.21.1 T1-11).
pub const E_CAPABILITY_MISMATCH: &str = "E-CAPABILITY-MISMATCH";

/// The single-snapshot gate [`build_index`] enforces, over `(document,
/// capabilityVersion)` pairs: one [`IndexError::CapabilityMismatch`] per
/// document whose snapshot differs from the FIRST document's. Shared with
/// `check-project` (0.21.1 T1-11), which runs it over each project root's
/// resolved snapshots so a project `play`/`compile --all` will refuse cannot
/// pass the check first.
pub fn capability_mismatches<'a>(
    docs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<IndexError> {
    let mut docs = docs.into_iter();
    let Some((first_doc, first)) = docs.next() else {
        return Vec::new();
    };
    docs.filter(|(_, v)| *v != first)
        .map(|(other_doc, other)| IndexError::CapabilityMismatch {
            first_doc: first_doc.to_string(),
            first: first.to_string(),
            other_doc: other_doc.to_string(),
            other: other.to_string(),
        })
        .collect()
}

/// `E-DUP-VOICEKEY` (0.21.1 T1-9): two voiced lines with DIFFERENT text
/// compiled to one `voiceKey`.
pub const E_DUP_VOICEKEY: &str = "E-DUP-VOICEKEY";

/// One voice asset key shared by lines that say different things. `lines`
/// holds every `(document, lineId, text)` carrying `key`, in `docs` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceKeyCollision {
    pub key: String,
    pub lines: Vec<(String, String, String)>,
}

impl std::fmt::Display for VoiceKeyCollision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "voiceKey `{}` is shared by {} lines with different text, so one recording would \
             voice all of them:",
            self.key,
            self.lines.len()
        )?;
        for (doc, line_id, text) in &self.lines {
            write!(f, " `{doc}` {line_id} \"{text}\";")?;
        }
        write!(
            f,
            " a voiceKey template without `{{prefix}}` (such as the 0.21 default \
             `{{speaker}}-{{code}}`) repeats across documents — use the default \
             `{{prefix}}.{{speaker}}-{{code}}` in lute.project.yaml's `identity.voiceKey` \
             (renames every voice asset) or give the lines distinct `code=`s"
        )
    }
}

/// Every `voiceKey` carried by two or more voiced lines whose `text` differs,
/// across ALL of `docs` (a project's documents compile independently, so
/// only a project-wide pass can see the collision). Lines repeating the same
/// text under one key share a recording legitimately and are not reported.
/// Sorted by key.
pub fn voice_key_collisions(docs: &[IndexInput<'_>]) -> Vec<VoiceKeyCollision> {
    let mut by_key: BTreeMap<&str, Vec<(&str, &str, &str)>> = BTreeMap::new();
    for d in docs {
        for c in &d.artifact.commands {
            if let Command::Line(l) = c {
                if let Some(key) = l.voice_key.as_deref() {
                    by_key
                        .entry(key)
                        .or_default()
                        .push((d.path.as_str(), l.line_id.as_str(), l.text.as_str()));
                }
            }
        }
    }
    by_key
        .into_iter()
        .filter(|(_, lines)| lines.iter().any(|(_, _, t)| *t != lines[0].2))
        .map(|(key, lines)| VoiceKeyCollision {
            key: key.to_string(),
            lines: lines
                .into_iter()
                .map(|(d, id, t)| (d.to_string(), id.to_string(), t.to_string()))
                .collect(),
        })
        .collect()
}

/// The document's canonical node key (see [`IndexDocument::key`]). A scene's
/// key is [`SceneMeta::id`] verbatim — the shared canonical scene key
/// ([`lute_check::meta::canonical_scene_key`], dsl 0.15.0 §2) already
/// stamped into the artifact by `artifact_meta`, so the index can never
/// name a scene differently from the addressing prefix or `check-project`'s
/// scene-key grouping. A quest or lore document with an authored `id:` is
/// keyed by it (dsl 0.19.0 §2.1); without one, by its first declared
/// `<quest id>` (`<entry id>`). A quest/lore document with neither has no
/// key; that shape never survives the check gate, so the empty string is a
/// total fallback, not a real output.
pub fn document_key(artifact: &Artifact) -> String {
    let authored = match &artifact.meta {
        ArtifactMeta::Scene(m) => return m.id.clone(),
        ArtifactMeta::Quest(m) => m.id.as_ref(),
        ArtifactMeta::Lore(m) => m.id.as_ref(),
    };
    if let Some(id) = authored {
        return id.clone();
    }
    artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Quest(q) => Some(q.id.clone()),
            Command::Entry(e) => Some(e.id.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AtomEntry, BeatIr, SceneMeta};

    fn scene(character: &str, capability: &str) -> Artifact {
        Artifact {
            kind: DocKind::Scene,
            lute: "0.11.0".to_string(),
            ir_version: "0.11.0".to_string(),
            capability_version: capability.to_string(),
            meta: ArtifactMeta::Scene(SceneMeta {
                id: format!("{character}.s01ep02"),
                character: Some(character.to_string()),
                season: Some(1),
                episode: Some(2),
                episode_id: Some("s01ep02".to_string()),
                title: None,
                extra: BTreeMap::new(),
                plugin: BTreeMap::new(),
                beat: None,
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
        let mut a = scene("marina", "cap-1");
        a.relations = vec![relation("knows", &["npc"]), relation("owns", &["item"])];
        a.seed_facts = vec![fact("knows", "kai"), fact("owns", "key")];
        a.rules = vec![rule("trusts", "trusts(X) :- knows(X)")];
        a.prereq_edges = vec![PrereqEdgeEntry {
            node: "marina.s01ep02".to_string(),
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
            index
                .documents
                .iter()
                .map(|d| d.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a/a.lute", "z/b.lute"],
            "documents sort by path"
        );
        assert_eq!(index.documents[0].artifact, "a/a.lute.json");
        assert_eq!(index.documents[0].key, "marina.s01ep02");
        assert_eq!(index.capability_version, "cap-1");
        assert_eq!(
            index
                .relations
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["aware", "knows", "owns"],
            "relations union, dedupe, and sort by name"
        );
        assert_eq!(index.seed_facts.len(), 2, "the shared fact dedupes");
        assert_eq!(index.rules.len(), 1, "the shared rule dedupes");
        assert_eq!(index.prereq_edges.len(), 1);
    }

    #[test]
    fn a_conflicting_signature_is_an_error_not_a_silent_pick() {
        let mut a = scene("marina", "cap-1");
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
        assert!(
            errors[0].to_string().contains("conflicting signatures"),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn two_capability_snapshots_are_an_error() {
        let docs = [
            ("a.lute", scene("marina", "cap-1")),
            ("b.lute", scene("kai", "cap-2")),
        ];
        let errors = build_index("0.9.0", &inputs(&docs)).expect_err("two profiles");
        assert!(
            matches!(errors[0], IndexError::CapabilityMismatch { .. }),
            "{errors:?}"
        );
    }

    #[test]
    fn empty_vocabulary_arrays_are_still_emitted() {
        let docs = [("a.lute", scene("marina", "cap-1"))];
        let index = build_index("0.9.0", &inputs(&docs)).unwrap();
        let json = index.to_json().unwrap();
        for key in [
            "entities",
            "enums",
            "relations",
            "seedFacts",
            "rules",
            "prereqEdges",
        ] {
            assert!(
                json.contains(&format!("\"{key}\": []")),
                "missing empty `{key}`: {json}"
            );
        }
        // Declaration order, not alphabetical.
        let pos = |k: &str| json.find(k).unwrap_or(usize::MAX);
        assert!(pos("\"irVersion\"") < pos("\"capabilityVersion\""));
        assert!(pos("\"capabilityVersion\"") < pos("\"documents\""));
        assert!(pos("\"documents\"") < pos("\"entities\""));
        assert!(json.ends_with('\n'));
        // dsl 0.19.0: no lore → no `entries` key (0.18 byte-identity);
        // dsl 0.21.0: no beats → no `beats` key (0.20 byte-identity).
        assert!(!json.contains("\"entries\""), "{json}");
        assert!(!json.contains("\"beats\""), "{json}");
    }

    fn lore(capability: &str, entries: &[(&str, Option<&str>, Option<u32>)]) -> Artifact {
        use crate::ir::{EntryCmd, LoreMeta, Stamp};
        let mut a = scene("unused", capability);
        a.kind = DocKind::Lore;
        a.meta = ArtifactMeta::Lore(LoreMeta {
            id: None,
            title: None,
            series: None,
            content_lang: None,
            extra: BTreeMap::new(),
            plugin: BTreeMap::new(),
        });
        a.commands = entries
            .iter()
            .enumerate()
            .map(|(i, (id, series, order))| {
                Command::Entry(EntryCmd {
                    addr: format!("{:03}-0100", i + 1),
                    id: (*id).to_string(),
                    target: Some(format!("item.{id}")),
                    category: Some("note".to_string()),
                    title: None,
                    title_line_id: None,
                    series: series.map(str::to_string),
                    order: *order,
                    when: None,
                    body: format!("{:03}-0200", i + 1),
                    on: None,
                    priority: None,
                    once: None,
                    stamp: Stamp::default(),
                })
            })
            .collect();
        a
    }

    /// dsl 0.19.0 §7: `entries` rows follow `documents`' path order, then
    /// each lore document's own declaration order (NOT id order — the
    /// engine's eligibility tiebreak); `document` is the row's
    /// `documents[].path`; a lore document's key is its first entry id.
    #[test]
    fn entries_rows_follow_document_path_then_declaration_order() {
        let docs = [
            (
                "lore/z.lute",
                lore("cap-1", &[("zeta", None, None), ("alpha", None, None)]),
            ),
            ("scene.lute", scene("marina", "cap-1")),
            (
                "lore/a.lute",
                lore(
                    "cap-1",
                    &[("log2", Some("log"), Some(2)), ("log1", Some("log"), Some(1))],
                ),
            ),
        ];
        let index = build_index("0.19.0", &inputs(&docs)).expect("no conflicts");
        let rows: Vec<(&str, &str)> = index
            .entries
            .iter()
            .map(|e| (e.document.as_str(), e.id.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("lore/a.lute", "log2"),
                ("lore/a.lute", "log1"),
                ("lore/z.lute", "zeta"),
                ("lore/z.lute", "alpha"),
            ]
        );
        let lore_a = index
            .documents
            .iter()
            .find(|d| d.path == "lore/a.lute")
            .unwrap();
        assert_eq!(lore_a.key, "log2", "first declared entry id");
        assert_eq!(lore_a.kind, DocKind::Lore);
        let json = index.to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["entries"][0],
            serde_json::json!({
                "id": "log2",
                "document": "lore/a.lute",
                "target": "item.log2",
                "category": "note",
                "series": "log",
                "order": 2
            })
        );
        assert_eq!(
            v["entries"][2],
            serde_json::json!({
                "id": "zeta",
                "document": "lore/z.lute",
                "target": "item.zeta",
                "category": "note"
            }),
            "unauthored series/order are omitted"
        );
        assert_eq!(v["documents"][0]["kind"], "lore");
    }

    fn beat_scene(id: &str, beat: Option<BeatIr>) -> Artifact {
        let mut a = scene("unused", "cap-1");
        if let ArtifactMeta::Scene(m) = &mut a.meta {
            m.id = id.to_string();
            m.beat = beat;
        }
        a
    }

    fn beat(on: &str, target: Option<&str>, priority: i64, once: BeatOnce) -> Option<BeatIr> {
        Some(BeatIr {
            on: on.to_string(),
            target: target.map(str::to_string),
            when: None,
            priority,
            once,
        })
    }

    /// `(id, on, priority)` per entry, in declaration order.
    fn beat_lore(entries: &[(&str, Option<&str>, Option<i64>)]) -> Artifact {
        let plain: Vec<(&str, Option<&str>, Option<u32>)> =
            entries.iter().map(|(id, _, _)| (*id, None, None)).collect();
        let mut a = lore("cap-1", &plain);
        for (c, (_, on, priority)) in a.commands.iter_mut().zip(entries) {
            if let Command::Entry(e) = c {
                e.on = on.map(str::to_string);
                e.priority = *priority;
            }
        }
        a
    }

    /// Rows follow `documents`' path order, then declaration order within a
    /// document (NOT id or priority order — priority is the engine's first
    /// key, this order its tiebreak); non-beat scenes and entries without
    /// `on` contribute nothing; `once` rides on scene rows only; an
    /// unauthored entry priority resolves to `0`.
    #[test]
    fn beats_rows_follow_path_then_declaration_order() {
        let docs = [
            (
                "scenes/b.lute",
                beat_scene(
                    "hades.achilles.run12",
                    beat("talk", Some("npc.achilles"), 50, BeatOnce::User),
                ),
            ),
            ("scenes/a.lute", beat_scene("plain.scene", None)),
            (
                "lore/barks.lute",
                beat_lore(&[
                    ("zBark", Some("talk"), Some(10)),
                    ("lookedUpOnly", None, None),
                    ("aBark", Some("hubVisit"), None),
                ]),
            ),
            (
                "scenes/c.lute",
                beat_scene("dawn", beat("dayStart", None, 0, BeatOnce::None)),
            ),
        ];
        let index = build_index("0.21.0", &inputs(&docs)).expect("no conflicts");
        let rows: Vec<(&str, BeatKind, &str)> = index
            .beats
            .iter()
            .map(|b| (b.document.as_str(), b.kind, b.id.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("lore/barks.lute", BeatKind::Entry, "zBark"),
                ("lore/barks.lute", BeatKind::Entry, "aBark"),
                ("scenes/b.lute", BeatKind::Scene, "hades.achilles.run12"),
                ("scenes/c.lute", BeatKind::Scene, "dawn"),
            ]
        );
        let v: serde_json::Value = serde_json::from_str(&index.to_json().unwrap()).unwrap();
        assert_eq!(
            v["beats"][0],
            serde_json::json!({
                "id": "zBark",
                "kind": "entry",
                "document": "lore/barks.lute",
                "on": "talk",
                "target": "item.zBark",
                "priority": 10
            }),
            "entry rows carry no `once`"
        );
        assert_eq!(v["beats"][1]["priority"], 0, "unauthored priority is 0");
        assert_eq!(
            v["beats"][2],
            serde_json::json!({
                "id": "hades.achilles.run12",
                "kind": "scene",
                "document": "scenes/b.lute",
                "on": "talk",
                "target": "npc.achilles",
                "priority": 50,
                "once": "user"
            })
        );
        assert_eq!(
            v["beats"][3],
            serde_json::json!({
                "id": "dawn",
                "kind": "scene",
                "document": "scenes/c.lute",
                "on": "dayStart",
                "priority": 0,
                "once": "none"
            }),
            "an untargeted beat omits `target`"
        );
    }
}
