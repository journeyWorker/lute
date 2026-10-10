//! The `project.index.json` envelope and its rows.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AdvanceSpec, BeatOnce, CelPair, DocKind, EntityKindEntry, EnumEntry, ForKind, GateEntry,
    PrereqEdgeEntry, RelationEntry, RuleEntry, SeasonEntry, SeedFactEntry, TargetKind,
};

/// One document's row in the index. Paths are FORWARD-SLASH relative to the
/// project root, never absolute — an index is a build output that must survive
/// being copied to another machine or shipped inside a game package.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDocument {
    /// Source document, relative to the project root (`quests/a.lute`).
    pub path: String,
    /// Its compiled artifact, relative to the output directory
    /// (`quests/a.lute.json`).
    pub artifact: String,
    #[serde(default)]
    pub kind: DocKind,
    /// The document's canonical node key: a scene's `{character}.{episodeId}`
    /// (`canonical_episode_key`); a quest or lore document's authored `id:`
    /// (dsl 0.19.0 §2.1), else its first declared `<quest id>` / `<entry
    /// id>` (document order = addressing order). A quest PACK's / lore
    /// document's ids stay recoverable from its own artifact's `quest` /
    /// `entry` records (and, for entries, [`ProjectIndex::entries`]) — the
    /// index names the document, it does not replace it.
    #[serde(default)]
    pub key: String,
}

/// One `<entry>` row of [`ProjectIndex::entries`] (dsl 0.19.0 §7): enough for
/// an engine to build its `target → entries` / `series → entries` tables
/// without loading every lore artifact. `document` is the SAME string as the
/// owning [`IndexDocument::path`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub document: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
}

/// What a [`ProjectIndex::beats`] row declares (dsl 0.21.0 §8): a scene beat
/// (`SceneMeta.beat`), an entry beat (`EntryCmd.on`), or a bundle beat (a
/// lore document's `beat` record, dsl 0.23.0 §4).
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatKind {
    #[default]
    Scene,
    Entry,
    Bundle,
}

/// One row of [`ProjectIndex::beats`] (dsl 0.21.0 §4/§8): every beat in the
/// project, so an engine can build its `occasion → candidates` table without
/// loading every artifact. Row order IS the selection tiebreak after
/// priority. `id` is the scene's canonical id (`SceneMeta::id`), the
/// entry id, or a bundle beat's canonical `<document id>.<beat id>`;
/// `document` is the owning [`IndexDocument::path`]; `priority` is
/// resolved (unauthored → `0`); `once` is the scene's / bundle beat's policy,
/// or an entry's authored `once` (dsl 0.22.0 §7) — absent on an entry row =
/// repeatable.
/// `when` is the beat's condition after `@def` expansion (a scene's
/// `meta.beat.when`, an entry's own `when`) and `title` the scene's / entry's
/// title — enough to list the beats and label a `select: all` menu without
/// loading every artifact (dsl 0.23.0 §1, §11). Both are omitted when absent.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexBeat {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once: Option<BeatOnce>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// dsl 0.25.0 §2: the beat's `share` key — every row of one key is
    /// spent when any of them is presented. Omitted when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// dsl 0.26.0 §5: a `target="kind:<kind>"` beat's members, as the
    /// artifact's `targetKind`. Omitted for any other target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<TargetKind>,
    /// dsl 0.27.0 §3 (T2-10): a `for="kind:<kind>"` beat's members, as the
    /// artifact's `forKind`. Omitted when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_kind: Option<ForKind>,
    /// dsl 0.27.0 §5, 0.28.0 §6: the beat's `spentBy` condition (raw,
    /// `@def`-expanded) — once it has held the beat is spent for its `once`
    /// period. Omitted when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spent_by: Option<String>,
    /// dsl 0.31.0 §1: clock movement performed when this beat is presented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advances: Option<AdvanceSpec>,
}

impl IndexBeat {
    /// dsl 0.26.0 §5: whether the beat answers a raise of `occasion` for
    /// `target` — untargeted, the same target, or a kind listing it — and
    /// the member it binds to `occasion.target` (a kind beat's).
    pub fn answers<'t>(&self, occasion: &str, target: Option<&'t str>) -> Option<Option<&'t str>> {
        if self.on != occasion {
            return None;
        }
        match (&self.target_kind, self.target.as_deref()) {
            (Some(k), _) => target.and_then(|t| k.member_of(t)).map(Some),
            (None, None) => Some(None),
            (None, Some(t)) => (Some(t) == target).then_some(None),
        }
    }
}

/// The `project.index.json` envelope. Field DECLARATION ORDER is the serialized
/// order, exactly as the execution IR does it — a `serde_json::Map` would
/// sort keys alphabetically instead.
///
/// The six vocabulary arrays are ALWAYS emitted, empty included: an engine
/// unions them unconditionally, and an absent key would force it to distinguish
/// "no relations" from "index too old to carry them".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndex {
    pub ir_version: String,
    #[serde(default)]
    pub capability_snapshot: String,
    #[serde(default)]
    pub required_semantics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identity_renames: Vec<lute_manifest::project::IdentityRename>,
    #[serde(default)]
    pub documents: Vec<IndexDocument>,
    #[serde(default)]
    pub entities: Vec<EntityKindEntry>,
    #[serde(default)]
    pub enums: Vec<EnumEntry>,
    #[serde(default)]
    pub relations: Vec<RelationEntry>,
    #[serde(rename = "seedFacts", default)]
    pub seed_facts: Vec<SeedFactEntry>,
    #[serde(default)]
    pub rules: Vec<RuleEntry>,
    #[serde(rename = "prereqEdges", default)]
    pub prereq_edges: Vec<PrereqEdgeEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<IndexEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beats: Vec<IndexBeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<lute_manifest::clock::ClockDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<GateEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<CelPair>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal_persists: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<SeasonEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside_run: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub occasions: BTreeMap<String, IndexOccasion>,
    #[serde(rename = "worldEvents", default, skip_serializing_if = "Vec::is_empty")]
    pub world_events: Vec<String>,
    #[serde(rename = "bridgeResults", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bridge_results: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cast: BTreeMap<String, String>,
    #[serde(rename = "stateDomains", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub state_domains: BTreeMap<String, StateDomain>,
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

/// The serializable subset of a capability-declared occasion carried by the
/// project index. The human description and raisedWhen gate are document
/// metadata, not part of the engine's occasion shape.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOccasion {
    pub select: lute_manifest::schema::OccasionSelect,
    pub target: lute_manifest::schema::OccasionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<lute_manifest::schema::OccasionJudge>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub payload: BTreeMap<String, lute_manifest::types::Type>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside_run: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StateDomain {
    pub kind: String,
    pub members: Vec<String>,
}

impl From<&lute_manifest::schema::OccasionDecl> for IndexOccasion {
    fn from(decl: &lute_manifest::schema::OccasionDecl) -> Self {
        Self {
            select: decl.select,
            target: decl.target.clone(),
            judge: (!decl.judge.is_after()).then_some(decl.judge),
            payload: decl.payload.clone(),
            outside_run: decl.outside_run,
        }
    }
}
