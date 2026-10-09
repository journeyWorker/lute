//! Typed JSON IR (spec §4): tagged records with camelCase fields; only
//! relevant fields present (D3). Field DECLARATION ORDER is the serialized
//! order — part of the byte-stability contract; never reorder.

use std::collections::BTreeMap;

use serde::Serialize;

use lute_ir::*;

/// Compiler-side resolution of a [`TargetKind`]: it needs the checker.
pub trait TargetKindExt {
    /// Resolve a beat's authored `target` on occasion `on`: `Some` only for
    /// a well-formed `kind:<kind>` target the checker accepted
    /// ([`lute_check::beats::kind_target_members`]).
    fn resolve(
        on: &str,
        target: Option<&str>,
        occasions: &std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>,
        kinds: &std::collections::BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
    ) -> Option<TargetKind>;
}

impl TargetKindExt for TargetKind {
    fn resolve(
        on: &str,
        target: Option<&str>,
        occasions: &std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>,
        kinds: &std::collections::BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
    ) -> Option<TargetKind> {
        let kind = lute_check::kind_target(target?)?;
        let (prefix, members) =
            lute_check::beats::kind_target_members(occasions.get(on)?, kind, kinds).ok()?;
        Some(TargetKind {
            kind: kind.to_string(),
            prefix,
            members,
        })
    }
}

/// Compiler-side resolution of a [`ForKind`]: it needs the checker.
pub trait ForKindExt {
    /// Resolve a beat's authored `for` on occasion `on`: `Some` only for a
    /// value the checker accepted ([`lute_check::occasion_bind::for_kind_members`]).
    fn resolve(
        on: &str,
        for_kind: Option<&str>,
        has_target: bool,
        occasions: &std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>,
        kinds: &std::collections::BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
    ) -> Option<ForKind>;
}

impl ForKindExt for ForKind {
    fn resolve(
        on: &str,
        for_kind: Option<&str>,
        has_target: bool,
        occasions: &std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>,
        kinds: &std::collections::BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
    ) -> Option<ForKind> {
        let (kind, members) = lute_check::occasion_bind::for_kind_members(
            on, for_kind?, has_target, occasions, kinds,
        )
        .ok()?;
        Some(ForKind { kind, members })
    }
}

/// Envelope (§4.1 + A9): language-version pin + IR schema version + capability
/// snapshot stamp + meta + folded state schema + flat command array. Field
/// DECLARATION ORDER is the serialized order (byte-stability contract).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionIr {
    /// Document kind discriminator (dsl 0.2.0 §2/§3.1) — FIRST field, the
    /// byte-stability contract (IR addendum §1): most fundamental
    /// discriminator, read before anything else to know `meta`'s shape.
    pub kind: DocKind,
    /// Language-version pin (DSL 0.1.0), serialized as `lute`.
    pub lute: String,
    /// IR schema version (A9), independent of `lute`; engines gate parsing on it.
    pub ir_version: String,
    /// Plugin-system §13 capability snapshot stamp (A9): `snapshot.version`,
    /// serialized as `capabilitySnapshot` (dsl 0.37.0 §5.3).
    pub capability_snapshot: String,
    /// Resolved authored rename ledger relevant to this artifact. Empty
    /// ledgers are omitted from the wire shape.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub identity_renames: Vec<lute_manifest::project::IdentityRename>,
    /// Exact engine semantic capabilities required by this lowered artifact.
    /// Compiler-derived, sorted, and duplicate-free; serialized immediately
    /// after the plugin capability snapshot.
    pub required_semantics: Vec<String>,
    pub meta: ArtifactMeta,
    pub state: Vec<StateEntry>,
    /// Merged relational entity kinds (dsl 0.3.0 §3.1), name-sorted
    /// (`RelVocab.kinds` is a `BTreeMap` — deterministic order). Omitted
    /// entirely for a document with no relational declarations (D15 — byte-
    /// identical to 0.2.0 minus the version strings).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityKindEntry>,
    /// Merged relational `enums:` (dsl 0.3.0 §3), name-sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<EnumEntry>,
    /// Merged relation declarations (dsl 0.3.0 §4), name-sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationEntry>,
    /// Merged seed `facts:` (dsl 0.3.0 §4), in vocabulary (import-then-
    /// inline) order.
    #[serde(rename = "seedFacts", skip_serializing_if = "Vec::is_empty")]
    pub seed_facts: Vec<SeedFactEntry>,
    /// Merged Datalog `rules:` (dsl 0.3.0 §7.1), in vocabulary order. Emitted
    /// as DATA for the engine's fixpoint — Lute performs NO evaluation (D1).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<RuleEntry>,
    pub commands: Vec<Command>,
    /// Advisory connectivity graph (connectivity spec §2.6, A-hybrid, T13):
    /// this document's OWN raw declared `after` prerequisite formula(s) —
    /// unresolved, unvalidated, single-document-scoped (`compile` has no
    /// project root to resolve `visited`/`completed` targets against or to
    /// assemble a project-wide graph). Never a project-assembled graph,
    /// flattened/validated edge set, or cycle/reachability/envelope data —
    /// those stay entirely in `check-project`/`lute scenario`. An engine
    /// reconstructs the whole graph by unioning `prereqEdges` across every
    /// document's artifact, exactly as it already unions `relations`/`rules`.
    /// Name-sorted by `node` (determinism). APPENDED LAST — after `commands`,
    /// the prior last field — so this is a true append-only change; nothing
    /// above moved (byte-stability contract, file header).
    #[serde(rename = "prereqEdges", skip_serializing_if = "Vec::is_empty")]
    pub prereq_edges: Vec<PrereqEdgeEntry>,
    /// Authored document sections (dsl 0.37.0 §5.3): the 1-based document-
    /// position `section` number, its verbatim `## ` heading text, and its
    /// optional stable `{#id}`. Purely descriptive — control flow never
    /// references it; a `position`'s first segment is the join. Emitted only
    /// for sections with a non-empty heading or an id; omitted entirely when
    /// none qualifies.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SectionEntry>,
    /// dsl 0.24.0 §1: the project's declared clock, verbatim — the `day` /
    /// `slot` paths, the slot order, the occasion raised after an advance,
    /// the week. An engine derives `clock.index` / `clock.weekday` /
    /// `clock.weekdayLabel` from it and spends `once: day|slot` by it.
    /// Omitted without a clock. APPENDED LAST — after `shots`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock: Option<lute_manifest::clock::ClockDecl>,
    /// dsl 0.27.0 §4 (T2-3): every occasion's declared `raisedWhen` gate,
    /// occasion-sorted, after `@def` expansion — the engine raises the
    /// occasion only while its gate holds (`occasion.target` reads the
    /// member it is raised for). Omitted when no occasion declares one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<GateEntry>,
    /// dsl 0.27.0 §4 (T2-4): the project's `terminal:` condition after
    /// `@def` expansion (several declarations joined by `||`) — once it
    /// holds the engine raises no occasion. Omitted without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<CelPair>,
    /// Every `terminal:` declaration says `persists: true`: the ending
    /// outlives runs on purpose — a new run does not reopen the game.
    /// Omitted when false.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub terminal_persists: bool,
    /// dsl 0.27.0 §5: the project's declared seasons, name-sorted — each
    /// season's `live` condition after `@def` expansion. An engine opens a
    /// season when `live` goes false→true: `season.<name>.*` back to the
    /// declared defaults (the old values to `prev.season.<name>.*`), its
    /// `once: season:<name>` beats and `tier="season:<name>"` quests reset.
    /// Omitted without seasons. After `clock`, `gates`, `terminal`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<SeasonEntry>,
    /// dsl 0.28.0 (T2-9): the occasions declared `outsideRun: true`,
    /// name-sorted — the engine raises these even after `terminal` holds (a
    /// title screen, a gallery). Omitted when none. After `seasons`.
    pub outside_run: Vec<String>,
    pub cel_env: CelEnv,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CelEnv {
    pub variables: Vec<CelVariable>,
    pub functions: Vec<CelFunction>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CelVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CelFunction {
    pub name: String,
    pub overloads: Vec<CelOverload>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CelOverload {
    pub params: Vec<&'static str>,
    pub result: &'static str,
}

/// One authored document section (dsl 0.37.0 §5.3): the 1-based document-
/// position section number, its verbatim `## ` heading text (the `{#id}`
/// suffix excluded), and the optional stable section id. Purely descriptive —
/// control flow never references it; a `position`'s first segment is the join.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionEntry {
    pub section: i64,
    pub heading: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Kind-polymorphic envelope `meta` (dsl 0.2.0, IR addendum §1; dsl 0.15.0
/// §2): untagged so the wire shape is exactly `SceneMeta`'s, `QuestMeta`'s,
/// or `LoreMeta`'s own fields — the consumer reads `ExecutionIr.kind` to know
/// which. Since IR
/// `0.15.0` the discriminator for a scene is `SceneMeta.id` (always present,
/// the resolved canonical scene key); legacy `character`/`season`/`episode`/
/// `episodeId` demote to optional (skipped when unauthored on an authored-
/// `id:` document, retained resolved-as-today on the derived-key path — dsl
/// 0.15.0 §2 wire contract).
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ArtifactMeta {
    Scene(SceneMeta),
    Quest(QuestMeta),
    /// dsl 0.19.0 §7: lore documents carry [`LoreMeta`].
    Lore(LoreMeta),
}

/// Scene-kind envelope meta (dsl 0.15.0 §2/§3). Field DECLARATION ORDER is
/// the serialized order (byte-stability contract):
///
///   `id` -> `character` -> `season` -> `episode` -> `episodeId` -> `title`
///   -> `meta` -> `plugin` -> `beat`
///
/// `id` is ALWAYS present — either the authored canonical scene key
/// (`TypedMeta.id`) or the derived `{character}.{episodeId}` join
/// ([`lute_check::meta::canonical_scene_key`]). The four legacy identity
/// fields (`character`/`season`/`episode`/`episodeId`) demote to optional
/// per dsl 0.15.0 §2; the derived-key path still emits all four verbatim
/// (with `episodeId` resolved as in 0.14.0) so a document that has not
/// migrated to `id:` stays wire-compatible field-for-field beyond the added
/// `id`. On the authored-`id:` path only the AUTHORED legacy keys survive
/// into the artifact (the raw frontmatter is the source of truth — a
/// project-level `defaults:` fallback SHOULD NOT resurrect a key the author
/// dropped, dsl 0.15.0 §4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMeta {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// dsl 0.15.0 §3: free descriptive `extra:` block, JSON-ready
    /// (`TypedMeta.extra_block`), key-sorted (`BTreeMap`) and skipped when
    /// empty so a document without the block stays byte-identical to
    /// pre-0.15 output.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// plugin-owned top-level frontmatter keys the checker admitted past
    /// `E-META-UNKNOWN-KEY` and validated against the declaring plugin's
    /// `frontmatter/*.yaml` schema (`E-FRONTMATTER-SCHEMA`, plugin-system
    /// 0.0.1 §6.8 / 0.0.2 §3), passed through verbatim as JSON
    /// (plugin-system 0.0.4 §2). `BTreeMap` for deterministic (key-sorted)
    /// serialization; skipped entirely when the document authors no
    /// plugin-owned key, so a document with no active plugin's frontmatter
    /// stays byte-identical to pre-0.0.4 output.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin: BTreeMap<String, serde_json::Value>,
    /// dsl 0.21.0 §3.1/§8: the scene's beat declaration, present only when
    /// the frontmatter names an occasion (`on:`) — a scene that answers no
    /// occasion is byte-identical to its 0.20 artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beat: Option<BeatIr>,
}

/// A scene beat (dsl 0.21.0 §3.1, `docs/runtime/beats-and-occasions.md`): the
/// occasion the scene answers, an optional target restriction, the
/// `@def`-expanded `when` eligibility slot, the resolved priority (unauthored
/// → `0`), and the repetition policy (unauthored → `run`). Field declaration
/// order is serialized order (byte-stability contract).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatIr {
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub priority: i64,
    pub once: BeatOnce,
    /// dsl 0.23.0 §3: `also: true` — presented after the `select: first`
    /// winner, in addition to it. Serialized only when true, so every other
    /// beat is byte-identical to its 0.22 artifact.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub also: bool,
    /// dsl 0.25.0 §2: the shared-spend key — presenting any beat of the key
    /// spends every beat of it for their (common) `once` period. Omitted
    /// when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// dsl 0.26.0 §5: with `target: "kind:<kind>"`, the members the beat
    /// answers. Omitted for any other target (byte-stability).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<TargetKind>,
    /// dsl 0.27.0 §3 (T2-10): a scene's `for: "kind:<kind>"` — presented
    /// once per member, as [`EntryCmd::for_kind`]. Omitted when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_kind: Option<ForKind>,
    /// `spentBy` (dsl 0.27.0 §5, 0.28.0 §6): the beat is spent by this
    /// `@def`-expanded condition instead of by being presented — once it
    /// has held, for the beat's `once` period (`run` unless written), even
    /// if it turns false again. Omitted when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spent_by: Option<CelPair>,
    /// dsl 0.31.0 §1: clock movement performed when the beat is presented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advances: Option<AdvanceSpec>,
}

/// Quest-kind envelope meta (dsl 0.2.0 §6.1, IR addendum §1; dsl 0.15.0 §3
/// adds the descriptive `extra:` block; dsl 0.19.0 §2.1 the optional document
/// `id`): MAY serialize as `{}` when none of id/title/contentLang/extra/plugin
/// are authored. Every field is skipped when unauthored, so `id` — declared
/// first, as on [`SceneMeta`] — leaves a quest document without an `id:`
/// byte-identical to its 0.18 artifact.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestMeta {
    /// dsl 0.19.0 §2.1: the document's authored `id:` verbatim (the bundle
    /// name, `[A-Za-z0-9_.-]+`); also its `ProjectIndex` key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_lang: Option<String>,
    /// See [`SceneMeta::meta`] — same predicate, same shape, same
    /// skip-when-empty discipline. dsl 0.15.0 §3 sanctions the block on
    /// quest roots too.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// See [`SceneMeta::plugin`] — same predicate, same shape, same
    /// skip-when-empty discipline.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin: BTreeMap<String, serde_json::Value>,
}

/// Lore-kind envelope meta (dsl 0.19.0 §7). Field DECLARATION ORDER is the
/// serialized order: `id` -> `title` -> `series` -> `contentLang` -> `extra`
/// -> `plugin`, each skipped when unauthored exactly as on [`QuestMeta`].
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoreMeta {
    /// dsl 0.19.0 §2.1: the document's authored `id:` verbatim; also its
    /// `ProjectIndex` key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// dsl 0.19.0 §2.1 (D-K): the document-level `series:` every entry
    /// belongs to. Each `entry` record already carries its resolved
    /// `series`/`order`; this names the bundle's series for a consumer that
    /// reads the envelope alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_lang: Option<String>,
    /// See [`QuestMeta::extra`].
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
    /// See [`QuestMeta::plugin`].
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin: BTreeMap<String, serde_json::Value>,
}

/// One folded state slot (§4.1): the engine's init/type table.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// dsl 0.22.0 §1.2: `engine` when the engine, rather than content, writes
    /// this state path. Omitted for content-owned paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<lute_manifest::types::Owner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    /// dsl 0.24.0 §1: display label per value, from the named enum the path
    /// is typed against (`{ domain: weekday }` + `enums: { weekday: {
    /// labels } }`). A `{{path}}` interpolation renders `labels[value]` when
    /// present, the value itself otherwise. Omitted when no label is declared.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// The label forms of the entity kind the path is typed against
    /// ([`EntityKindEntry::label_forms`]), so a `{{path:start}}` /
    /// `{{path:indefinite}}` renders them. Omitted when none is declared.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub label_forms: BTreeMap<String, LabelForms>,
    /// dsl 0.27.0 §2 (T1-2): the named domain `K` of a path typed
    /// `{ domain: K }` / `{ entity: K }` whose `K` is closed, and its members
    /// — what a play or test seed / `engine:` write of the path is
    /// member-checked against. In memory only: the wire entry keeps the
    /// `string` type the IR has always carried for these paths.
    #[serde(skip)]
    pub member_domain: Option<(String, Vec<String>)>,
}

/// Cross-cutting optional stamps (dsl 0.37.0 §5.2), flattened into every
/// stamped record: the nested `timing` object, injection provenance,
/// component source, and plugin cross-cutting attrs.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    /// Resolved blocking and timing; omitted when every member is absent.
    #[serde(skip_serializing_if = "Timing::is_empty")]
    pub timing: Timing,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<lute_check::Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The directive as authored — `::bg{location="hall"}`, a plugin
    /// `::portrait{…}` — for the record a directive lowered to. Not wire
    /// data: `lute play` prints it so its transcript reads like the source
    /// (0.23.1); the lowered record is what an engine consumes.
    #[serde(skip)]
    pub authored: Option<String>,
    /// Plugin-declared CROSS-CUTTING attrs (plugin 0.0.2 §14.1 `stampAttrs:`),
    /// flattened beside `timing`. An engine reads these exactly like a
    /// directive `fields` entry — typed by the declaring plugin's `AttrDecl`,
    /// absent when unauthored. Assembly REJECTS a `stampAttrs` name colliding
    /// with any reserved key (`E-PLUGIN-RESERVED-STAMP-ATTR`), so this map can
    /// never shadow them. Serialized LAST within the stamp.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A record's `timing` object (dsl 0.37.0 §5.2). `duration`, `delay`, and
/// `at` are seconds (finite, >= 0); `timeline` is the zero-based ordinal of
/// the `<timeline>` the record was emitted from — an ordinal, not seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<u32>,
}

impl Timing {
    /// No member present — the whole `timing` object is omitted.
    pub fn is_empty(&self) -> bool {
        *self == Timing::default()
    }
}

/// `source { component }` on component-expanded records (§4.3, D8).
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub component: String,
    /// Complete enclosing component scope, outermost first.
    #[serde(skip)]
    pub scope: String,
    /// False when one or more enclosing uses use the transitional positional
    /// fallback. This metadata is internal; line joins still expose the scope.
    #[serde(skip)]
    pub stable: bool,
}

/// `:line` role (dsl 0.37.0 D6): the authored delivery name. Every role
/// carries a `voiceKey` — a key is a join, not a recording obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Dialogue,
    Narration,
    Mono,
    Os,
    Vo,
}

/// One record (dsl 0.37.0 §5.3). Serialized as `kind`, then `family`
/// ([`Family`]), then the record's own fields — see the `Serialize` impl.
/// Each kind is named after the authored directive that produces it;
/// `Plugin` is the plugin-directive passthrough (`kind: "plugin"`).
#[derive(Clone, Debug)]
pub enum Command {
    Line(LineCmd),
    Bg(BgCmd),
    Music(MusicCmd),
    Sfx(SfxCmd),
    Vfx(VfxCmd),
    Actor(ActorCmd),
    Camera(CameraCmd),
    Cg(CgCmd),
    Video(VideoCmd),
    Sequence(SequenceCmd),
    Set(SetCmd),
    Assert(AssertCmd),
    Retract(RetractCmd),
    Choice(ChoiceCmd),
    Match(MatchCmd),
    Hub(HubCmd),
    Jump(JumpCmd),
    End(EndCmd),
    Barrier(BarrierCmd),
    Quest(QuestCmd),
    On(OnCmd),
    Plugin(PluginCmd),
    /// dsl 0.19.0 §7: `<entry>` declaration head.
    Entry(EntryCmd),
    /// dsl 0.21.0 §7a.3: `::accept{quest}`.
    Accept(AcceptCmd),
    /// dsl 0.23.0 §4: a bundle `<beat>` declaration head in a lore artifact.
    Beat(BeatCmd),
}

/// A record's `family` (dsl 0.37.0 §5.1) — the normative kind grouping a
/// runtime dispatches on before `kind`. Named `family`, not `category`, so it
/// never collides with a lore `<entry category=…>` record field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Content,
    Staging,
    State,
    Control,
    Declaration,
    Plugin,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Content => "content",
            Family::Staging => "staging",
            Family::State => "state",
            Family::Control => "control",
            Family::Declaration => "declaration",
            Family::Plugin => "plugin",
        }
    }
}

/// `kind`, `family`, then the record's own (flattened) fields.
#[derive(Serialize)]
struct TaggedRecord<'a, T: Serialize> {
    kind: &'static str,
    family: Family,
    #[serde(flatten)]
    record: &'a T,
}

impl Serialize for Command {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        fn tagged<S: serde::Serializer, T: Serialize>(
            cmd: &Command,
            record: &T,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            TaggedRecord { kind: cmd.kind(), family: cmd.family(), record }.serialize(s)
        }
        match self {
            Command::Line(c) => tagged(self, c, s),
            Command::Bg(c) => tagged(self, c, s),
            Command::Music(c) => tagged(self, c, s),
            Command::Sfx(c) => tagged(self, c, s),
            Command::Vfx(c) => tagged(self, c, s),
            Command::Actor(c) => tagged(self, c, s),
            Command::Camera(c) => tagged(self, c, s),
            Command::Cg(c) => tagged(self, c, s),
            Command::Video(c) => tagged(self, c, s),
            Command::Sequence(c) => tagged(self, c, s),
            Command::Set(c) => tagged(self, c, s),
            Command::Assert(c) => tagged(self, c, s),
            Command::Retract(c) => tagged(self, c, s),
            Command::Choice(c) => tagged(self, c, s),
            Command::Match(c) => tagged(self, c, s),
            Command::Hub(c) => tagged(self, c, s),
            Command::Jump(c) => tagged(self, c, s),
            Command::End(c) => tagged(self, c, s),
            Command::Barrier(c) => tagged(self, c, s),
            Command::Quest(c) => tagged(self, c, s),
            Command::On(c) => tagged(self, c, s),
            Command::Plugin(c) => tagged(self, c, s),
            Command::Entry(c) => tagged(self, c, s),
            Command::Accept(c) => tagged(self, c, s),
            Command::Beat(c) => tagged(self, c, s),
        }
    }
}

/// One `{{…}}` interpolation placeholder (IR A3): the runtime substitutes it
/// against live state, while `text`/`label` keep the verbatim `{{…}}` marker.
/// Kind-keyed referent — `{"kind":"path","path":…}`, `{"kind":"ref","ref":…}`,
/// `{"kind":"reserved","token":…}`, `{"kind":"occasionTarget","entityKind":…}`
/// — matching the A3 example and the C1 `ExprNode` kind-keyed convention.
/// Entries appear in left-to-right order.
///
/// dsl 0.24.0 §4: a placeholder carries the interpolation's format hint as
/// `format` (`{{user.deaths:ordinal}}` → `"format":"ordinal"`), omitted when
/// the author wrote none — the engine renders the value in that format
/// (runtime/state-lifecycle.md). The number hints are `ordinal`,
/// `ordinalWord` (dsl 0.25.0 §8), `cardinalWord` (`one` … `twenty`) and
/// `plural` (dsl 0.27.0 §7); the text hints — on a `path`, `ref`,
/// `reserved` or `occasionTarget` placeholder — are `capitalize`, `start`
/// (a label's declared `start` form, else capitalized) and `indefinite` (a
/// label's declared `indefinite` form, else `a` / `an` + the text; the
/// forms ride on `entities[].labelForms` / `state[].labelForms`). The
/// checker rejects any other hint, a number hint on a non-number and a text
/// hint on a number or bool. A `plural` placeholder carries its `forms`
/// (`["lantern", "lanterns"]`: the singular, then the plural; in a form `#`
/// stands for the number, `#word` for it as a cardinal word and `#Word` for
/// that word capitalized) so the engine can localize the count.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Placeholder {
    /// A state-path read (`{{run.coins}}` → `{"kind":"path","path":"run.coins"}`).
    Path {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        forms: Option<Vec<String>>,
    },
    /// A `@def` / `@fn(args)` reference; the referent includes the leading `@`.
    /// `expr` is the def body inlined at compile time (the artifact carries no
    /// defs table), so an engine renders the value by evaluating it like any
    /// other `{cel, expr}` slot. Filled by `expand::inline_ref_placeholders`;
    /// always present in a compiled artifact. A family read indexed by the
    /// raised member (`user.bond[occasion.target]`, dsl 0.27.0 §3) is a `ref`
    /// too: its referent has no `@` and `expr` is the read itself.
    Ref {
        #[serde(rename = "ref")]
        reference: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expr: Option<CelPair>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        forms: Option<Vec<String>>,
    },
    /// A reserved token (only `userName` in 0.1), with its text hint.
    Reserved {
        token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    /// dsl 0.26.0 §5 (prerelease N8): `{{occasion.target}}` in a beat or
    /// entry targeting a kind — the member the occasion was raised for, a
    /// member of `entityKind` (the beat's `target="kind:<kind>"`, as its
    /// `targetKind.kind`). The engine renders the member's display text —
    /// its kind label, else its cast `name:`, else the id — in `format`
    /// when a text hint is written.
    #[serde(rename = "occasionTarget", rename_all = "camelCase")]
    OccasionTarget {
        entity_kind: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
}

/// Map one syntactic [`Interp`](lute_syntax::ast::Interp) to its typed IR
/// [`Placeholder`]. Shared by the content-line lowering ([`crate::lower`]) and
/// the option-label lowering ([`crate::stage`]) — the single kind→referent
/// match, never duplicated. The referent is the interp's verbatim trimmed `raw`.
/// A `Ref`'s `expr` is left empty here; the CEL-expansion pass inlines it.
pub(crate) fn placeholder_from_interp(i: &lute_syntax::ast::Interp) -> Placeholder {
    use lute_syntax::ast::InterpKind;
    match i.kind {
        // dsl 0.27.0 §3: `{{user.bond[occasion.target]}}` is a computed read,
        // so it ships as CEL an engine evaluates like any `ref` body.
        InterpKind::Path if lute_check::cel_paths::occasion_indexed_family(&i.raw).is_some() => {
            let slot = lute_syntax::ast::CelSlot::raw(
                lute_syntax::ast::CelKind::AttrValue,
                i.raw.clone(),
                i.span,
            );
            Placeholder::Ref {
                reference: i.raw.clone(),
                expr: Some(CelPair::from_slot(&slot)),
                format: i.format.clone(),
                forms: i.forms.clone(),
            }
        }
        // `{{run.visits["lab-b2"]}}` ships its canonical dotted path.
        InterpKind::Path => Placeholder::Path {
            path: lute_cel::path::parse_path_text(&i.raw)
                .map_or_else(|| i.raw.clone(), |segs| lute_manifest::text::render_path(&segs)),
            format: i.format.clone(),
            forms: i.forms.clone(),
        },
        InterpKind::Ref => Placeholder::Ref {
            reference: i.raw.clone(),
            expr: None,
            format: i.format.clone(),
            forms: i.forms.clone(),
        },
        InterpKind::Reserved => Placeholder::Reserved {
            token: i.raw.clone(),
            format: i.format.clone(),
        },
    }
}

/// One run of a modified line's presentation (dsl 0.37.0 §3.6): coalesced
/// text carrying the active text styles (outermost first) and the innermost
/// `speed` rate, or a `pause` leaf in seconds. Absent members are omitted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Segment {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        styles: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rate: Option<f64>,
    },
    Pause { pause: f64 },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCmd {
    pub position: String,
    pub role: Role,
    pub speaker: String,
    /// The plain-text derivation of the authored text (dsl 0.37.0 §3.6):
    /// modifier delimiters and attributes removed, escapes decoded, `{{…}}`
    /// markers verbatim.
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog_motion: Option<String>,
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub as_label: Option<String>,
    pub line_id: String,
    /// The voice asset join (dsl 0.37.0 D6): present on EVERY line, any role.
    pub voice_key: String,
    /// IR A3: `{{…}}` interpolations found in `text`, in left-to-right order.
    /// Absent when the line has no interpolation (byte-stability: skip-if-empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// dsl 0.37.0 §3.6: the line's presentation runs — present only when the
    /// authored text carries an inline modifier.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
    /// Merged locale texts (dsl 0.8.0 §7, 0.37.0 §6), locale tag → the
    /// translation's plain-text derivation, keyed on this record's `lineId`.
    /// Populated only when `compile --locales` was given a locale bundle;
    /// `text` always remains the SOURCE-language string. Absent when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    /// dsl 0.37.0 §6: locale tag → the translation's [`Segment`]s, for a
    /// line whose source carries an inline modifier. Absent when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub locale_segments: BTreeMap<String, Vec<Segment>>,
    /// The source text's inline modifier multiset
    /// ([`lute_syntax::ast::inline_modifier_multiset`]) — what a translation
    /// must match (`E-L10N-MODIFIERS`). Internal, NEVER serialized.
    #[serde(skip)]
    pub modifiers: BTreeMap<String, usize>,
    /// Authored (or back-filled) per-speaker `code` — feeds `lineId`/`voiceKey`
    /// in the addressing pass, NEVER serialized (3-id model, §4.2).
    #[serde(skip)]
    pub code: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SfxCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VfxCmd {
    pub position: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// Authored `::actor` OR an injected actor record (§7.4) — injected records
/// are SEPARATE records with `provenance` in their stamp; `posReset` and
/// `preload` appear only on injected ones.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorCmd {
    pub position: String,
    pub character: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub costume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preload: Option<bool>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `::camera` (dsl 0.37.0 D1): opaque project-domain members; at least one
/// is present after checking. No numeric transform is synthesized.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framing: Option<String>,
    #[serde(rename = "move", skip_serializing_if = "Option::is_none")]
    pub camera_move: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `::cg` (dsl 0.37.0 D2): `display` is the resolved value (`show` by
/// default) and always present.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CgCmd {
    pub position: String,
    pub asset_id: String,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `::video` (dsl 0.37.0 D2): `display` is the resolved value (`show` by
/// default) and always present.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCmd {
    pub position: String,
    pub asset_id: String,
    pub display: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `::sequence{name}` (dsl 0.37.0 D3): a reference to a project-declared
/// sequence. Carries only its name and resolved timing (`wait` defaults
/// `true`); no bound characters.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceCmd {
    pub position: String,
    pub name: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCmd {
    pub position: String,
    pub path: String,
    pub op: String,
    pub value: CelPair,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One asserted delta (dsl 0.3.0 §5): the engine applies it as a positive
/// write to the relation's fact set. Emitted as DATA only (D1) — Lute
/// performs no evaluation, no fact store, no timestamps.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertCmd {
    pub position: String,
    pub relation: String,
    /// Ground literals; bools as "true"/"false". Never "_" (checker-enforced
    /// `E-RETRACT-WILDCARD-ASSERT`).
    pub args: Vec<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One retracted delta (dsl 0.3.0 §5 RetractPattern): the engine applies it
/// as a negative write. `_` positions are a bulk wildcard the engine
/// resolves; Lute emits the pattern verbatim, no evaluation (D1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetractCmd {
    pub position: String,
    pub relation: String,
    /// Ground literals or "_" wildcards (§5 RetractPattern).
    pub args: Vec<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `::accept{quest}` (dsl 0.21.0 §7a.3): the player accepts an accept-driven
/// quest (one with no `start`) at this point. The engine activates the quest
/// if it is `unset` and ignores the record otherwise. Declaration data only —
/// Lute evaluates nothing (D1). Mirrors [`AssertCmd`]'s shape.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptCmd {
    pub position: String,
    pub quest: String,
    /// dsl 0.24.0 §2: `"nextRun"` for `::accept{… at="nextRun"}` (named
    /// `applies` on the wire: `at` is the flattened `Stamp` offset) — the
    /// acceptance is queued and applies after the next run-start reset.
    /// Omitted for an immediate accept (byte-stable 0.23 records).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applies: Option<AcceptAt>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// When a queued `::accept` applies (dsl 0.24.0 §2) — only the non-default
/// `nextRun` is ever serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AcceptAt {
    NextRun,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceCmd {
    pub position: String,
    pub branch_id: String,
    pub selection_key: String,
    pub options: Vec<ChoiceOption>,
    pub converge: String,
    /// dsl 0.11.0: the choice-situation sentence for the UI. Absent unless
    /// authored (skip-if-empty, matching `ChoiceOption::when`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// dsl 0.11.0: the countdown, in whole seconds (dsl 0.37.0 §5.3
    /// `timeout`). Absent unless authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub id: String,
    pub text: String,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub target: String,
    /// IR A3: `{{…}}` interpolations in `text`, in left-to-right order. Absent
    /// when the text has none (skip-if-empty). The text stays verbatim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Merged locale texts (dsl 0.8.0 §7), locale tag → translated option
    /// text, keyed on this option's `lineId`. See `LineCmd::texts`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
}

/// `<hub>` (§7.3.2, IR A2): structurally a `choice` plus revisit flags. The
/// hub record is the loop head; re-presentation is a RUNTIME property of the
/// `hub` kind, so no backward jump is emitted (D2/§3.2).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCmd {
    pub position: String,
    pub id: String,
    pub selection_key: String,
    pub options: Vec<HubOption>,
    pub converge: String,
    /// dsl 0.23.0 §4: the prompt line shown with the hub's options. Absent
    /// unless authored, so every other hub record is byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// dsl 0.28.0 §5: the start of the hub's `<return>` segment — run each
    /// time a non-`exit` option's segment ends, before the hub is judged
    /// and presented again. It is a segment boundary like an option
    /// target. Absent unless authored, so every other hub record is
    /// byte-identical.
    #[serde(rename = "return", skip_serializing_if = "Option::is_none")]
    pub on_return: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One `<hub>` option: a `<choice>` option plus always-present `once`/`exit`
/// revisit flags. `when` is a single CEL slot object when the choice is guarded.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubOption {
    pub id: String,
    pub text: String,
    pub line_id: String,
    pub once: bool,
    pub exit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub target: String,
    /// IR A3: `{{…}}` interpolations in `text`, in left-to-right order. Absent
    /// when the text has none (skip-if-empty). The text stays verbatim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Merged locale texts (dsl 0.8.0 §7), locale tag → translated option
    /// text, keyed on this option's `lineId`. See `LineCmd::texts`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<CelPair>,
    pub arms: Vec<MatchArm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<String>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct MatchArm {
    /// The authored `is=` shorthand, when this arm came from that form.
    /// Unlike `test.authored`, this is semantic IR data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is: Option<String>,
    pub test: CelPair,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct JumpCmd {
    pub position: String,
    pub target: String,
}

/// `::end{reason?}` (dsl 0.8.0 §5): terminate the walk at this record.
/// A NEW command kind, so an engine that does not implement 0.8 must refuse
/// the artifact rather than silently fall through (execution-model version
/// policy: an unknown `kind` is a hard error). `reason` is a free-form
/// author string the host may surface (`completed` / `error` / a custom
/// ending id); absent when unauthored.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndCmd {
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct BarrierCmd {
    pub position: String,
    pub timeline: u32,
    pub at: f64,
}

/// A resolved plugin state-write binding (IR A12): where a bridge result /
/// increment / literal lands, with `fromAttr` templates already substituted at
/// compile time. The runtime applies these to its state store after the bridge
/// call — no manifest lookup, no per-plugin knowledge.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    /// Fully-resolved dotted state path (scope + segments, `fromAttr` substituted),
    /// e.g. `scene.serve.debut.rank`.
    pub path: String,
    pub from: EffectSource,
}

/// The origin of an [`Effect`]'s value.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum EffectSource {
    /// `{ "bridgeResult": "<key>" }` — read the named key off the bridge result.
    BridgeResult {
        #[serde(rename = "bridgeResult")]
        bridge_result: String,
    },
    /// `{ "op": "increment", "by": 1 }` — a state mutation; `by` is integral-collapsed.
    Op { op: String, by: serde_json::Value },
    /// A bare literal value (scalar/array/object), integral-collapsed.
    Literal(serde_json::Value),
}

/// dsl 0.27.0 §4: one fact a plugin call's declared `effects.asserts` /
/// `retracts` writes, `@attr`s already substituted — the `relation` + `args`
/// shape of an [`AssertCmd`] / [`RetractCmd`] (`"_"` a retract wildcard).
#[derive(Clone, Debug, Serialize)]
pub struct FactRecord {
    pub relation: String,
    pub args: Vec<String>,
}

/// Plugin-directive passthrough (plan spec-gap note 1): `kind: "plugin"`,
/// the authored tag, and its attrs typed via the manifest `AttrDecl`s.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCmd {
    pub position: String,
    pub tag: String,
    /// Owning plugin id for a resolved plugin directive. Core and unknown
    /// directives omit this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    pub fields: BTreeMap<String, serde_json::Value>,
    /// IR A12: resolved plugin state-write bindings from the manifest directive's
    /// `effects.writes`. Absent when the directive declares none (skip-if-empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    /// dsl 0.27.0 §4: the facts the call retracts, applied after `effects`
    /// (skip-if-empty, appended: an artifact without them is byte-identical).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<FactRecord>,
    /// dsl 0.27.0 §4: the facts the call asserts, applied after `retracts`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asserts: Vec<FactRecord>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `<quest>` declaration head (dsl 0.2.0 §6.3, IR addendum §3.1). A
/// declaration head like `HubCmd`: carries no executable body of its own —
/// the objective table is inlined (mirrors `HubCmd.options`); objective
/// completion bodies + `<on>` arms follow as their own addressed records,
/// referenced by `ObjectiveEntry.body`/`OnCmd.body` targets.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestCmd {
    pub position: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_line_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<CelPair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail: Option<CelPair>,
    pub objectives: Vec<ObjectiveEntry>,
    /// Owner-declared `<reward/>` entries (dsl 0.16.0 §2/§3) in declaration
    /// order. Grants at quest complete/failed transitions per spec D-D;
    /// serialized only when authored so pre-0.16.0 rewardless quests stay
    /// byte-identical (`Vec::is_empty`, append-only field placement — after
    /// `objectives`, before the flattened `stamp`, preserving every prior
    /// field's index — the file-header byte-stability contract).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rewards: Vec<RewardEntry>,
    /// dsl 0.22.0 §7: `"run"` for a `<quest tier="run">` — its status and
    /// objectives reset to `unset` when a run starts. Omitted for the default
    /// `user` tier (status persists across runs), so every 0.21 quest record
    /// is byte-identical. Appended after `rewards` (byte-stability contract).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<QuestTier>,
    /// dsl 0.24.0 §2: `"accept"` for a `<quest activate="accept">` subquest
    /// child — it does not activate with its parent but waits for an
    /// accept while the parent is active. Omitted by default (byte-stable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activate: Option<QuestActivate>,
    /// dsl 0.24.0 §2: `"any"` for a `<quest complete="any">` — ANY required
    /// objective done completes the quest and its other still-active
    /// children fail `superseded`. Omitted for the default `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<QuestComplete>,
    /// dsl 0.25.0 §5: `"external"` for a `<quest accept="external">` — the
    /// quest is accepted outside the script (a quest board, a menu, a UI):
    /// the engine offers it and activates it whenever the player takes it
    /// (while its parent is active, for a child). Omitted by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept: Option<QuestAccept>,
    /// dsl 0.27.0 §5: `rearm` — when this `@def`-expanded condition goes
    /// false→true the quest returns to `unset` (objectives, `failedBy`
    /// cleared) and can be taken again. Omitted when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rearm: Option<CelPair>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// A quest's lifetime tier (dsl 0.22.0 §7, 0.27.0 §5) — only the
/// non-default ones are ever serialized: `"run"` (reset at `newRun`) and
/// `"season:<name>"` (reset when the season opens).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestTier {
    Run,
    Season(String),
}

impl Serialize for QuestTier {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            QuestTier::Run => s.serialize_str("run"),
            QuestTier::Season(name) => {
                s.serialize_str(&format!("{}{name}", lute_manifest::season::SEASON_PREFIX))
            }
        }
    }
}

/// A subquest child's activation mode (dsl 0.24.0 §2) — only the
/// non-default `accept` is ever serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestActivate {
    Accept,
}

/// A quest's completion mode (dsl 0.24.0 §2) — only the non-default `any`
/// is ever serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestComplete {
    Any,
}

/// Who accepts an accept-driven quest besides `::accept` (dsl 0.25.0 §5) —
/// only `external` exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestAccept {
    External,
}

/// One objective inlined in `QuestCmd.objectives` (dsl 0.2.0 §6.4, IR
/// addendum §3.1). Declaration data only — the engine derives the lifecycle
/// (all non-`optional` objectives `done` ⇒ quest `complete`); the compiler
/// emits no control flow for completion. `body` targets the objective's
/// completion-body segment (§3.2); `null` (always serialized, never
/// omitted) when the body is empty.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_line_id: Option<String>,
    pub done: CelPair,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<CelPair>,
    pub optional: bool,
    /// dsl 0.2.0 IR addendum §3.1/§3.2: `body` is ALWAYS present in the
    /// inlined objective entry — `null` (never omitted) when the objective
    /// body is empty. Unlike the sibling `Option` fields above (which are
    /// genuinely optional-authored attrs, omitted when absent), `body` is a
    /// declaration-shape field the engine always expects to find (final
    /// review F1).
    pub body: Option<String>,
    /// Subquest reference (2026-08-31 subquest design, IR §3): the
    /// child-quest id this objective's completion is bound to (§2.1 —
    /// `done.raw` is the synthesized `quest.<child>.state == 'complete'`).
    /// Serialized ONLY for subquest objectives (`skip_serializing_if`):
    /// artifacts without the feature are BYTE-IDENTICAL to pre-subquest
    /// output, and the field is appended AFTER `body` so field-declaration
    /// order (the byte-stability contract, this file's header) is
    /// preserved for every existing objective. The journal tree is
    /// derivable from this field alone across artifacts — no new edge
    /// table, no new command kind (design doc §3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest: Option<String>,
    /// Owner-declared `<reward/>` entries (dsl 0.16.0 §2/§3) in declaration
    /// order — objective grants fire once at first `done` (spec D-D), before
    /// any quest-level grants. `outcome=` is never legal here (checker
    /// rejects it, compiler never emits it). Appended AFTER `quest` so every prior
    /// objective serializes byte-identically (`Vec::is_empty`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rewards: Vec<RewardEntry>,
    /// dsl 0.21.0 §7a.2: the occasion at which this objective is judged —
    /// the engine evaluates `done` only when it raises this occasion while
    /// the quest is `active`. Appended after `rewards` and skipped when
    /// unauthored, so every objective without it is byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// dsl 0.23.0 §2, 0.24.0 §2.1: `by=` — while the objective is not done,
    /// the first time this condition is true the objective FAILS (a
    /// required objective fails its quest). Judged at every lifecycle
    /// settle, `on=` or not — a deadline is a moment, not a place.
    /// Appended and skipped when unauthored (byte-stability).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<CelPair>,
    /// dsl 0.23.0 §2: with `on`, the objective is judged only when the
    /// occasion is raised for this target (the beat target rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// dsl 0.24.0 §2.1: `until=` — the place-bound deadline: judged only
    /// when the objective's `on` occasion (and `target`) is raised, after
    /// its `done`; the first time it is true there the objective FAILS.
    /// Appended and skipped when unauthored (byte-stability).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<CelPair>,
}

/// `<on>` event-condition-action record (dsl 0.2.0 §4, §6.6, IR addendum
/// §3.3): an independent event rule (NOT part of the quest's declaration
/// table, unlike an objective), so it is its own standalone record.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnCmd {
    pub position: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub body: String,
    /// dsl 0.24.0 §2: with a `target`, the handler fires only when the
    /// same-named occasion is raised for that target. Appended after `body`
    /// and omitted when unauthored (byte-stable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// `<entry>` declaration head (dsl 0.19.0 §7, `docs/runtime/lore-entries.md`).
/// Like [`OnCmd`], its `body` addresses the entry's body segment, which
/// follows this record and is addressed/terminated exactly as an `<on>` body
/// (an empty body resolves to the entry unit's one-past-end). Field
/// declaration order is serialized order (byte-stability contract).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryCmd {
    pub position: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_line_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub body: String,
    /// dsl 0.21.0 §3.2: the occasion this entry answers, making it a beat.
    /// Appended after `body` and skipped when unauthored, so an entry that
    /// is not a beat is byte-identical to its 0.20 record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// dsl 0.21.0 §3.2: the authored beat priority; unauthored → omitted
    /// (the engine reads an absent priority as `0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// dsl 0.22.0 §7: an entry beat's repetition policy — `"run"` (not
    /// eligible while `entry.<id>.read`), `"user"` (not eligible once
    /// `entry.<id>.everRead`). Omitted = repeatable, as in 0.21.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<BeatOnce>,
    /// dsl 0.25.0 §2: the shared-spend key, as [`BeatIr::share`]. Omitted
    /// when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// dsl 0.26.0 §5: as [`BeatIr::target_kind`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<TargetKind>,
    /// dsl 0.27.0 §3 (T2-10): `for="kind:<kind>"` — presented once per
    /// member. Omitted when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_kind: Option<ForKind>,
    /// dsl 0.27.0 §5: as [`BeatIr::spent_by`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spent_by: Option<CelPair>,
    /// dsl 0.31.0 §1: clock movement performed when the beat is presented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advances: Option<AdvanceSpec>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// A bundle beat's declaration head (dsl 0.23.0 §4): a scene-like beat
/// written in a lore document. Like [`EntryCmd`] it is the FIRST record of
/// its own addressing unit and its `body` addresses the body segment that
/// follows it (to the next `entry`/`beat` record or the artifact end); the
/// body is lowered exactly like a scene shot. `id` is the canonical
/// `<document id>.<beat id>` — the `ProjectIndex.beats` row id, the
/// `visited()` key, and the identity prefix of its `lineId`s. The beat
/// fields mirror a scene's [`BeatIr`] (resolved `priority`, `once`
/// defaulting to `run`, `also` only when true). Field declaration order is
/// serialized order (byte-stability contract).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatCmd {
    pub position: String,
    pub id: String,
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_line_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub priority: i64,
    pub once: BeatOnce,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub also: bool,
    /// dsl 0.25.0 §2: the shared-spend key, as [`BeatIr::share`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// dsl 0.25.0 §3: the raw `after=` prerequisite (the restricted
    /// `visited` / `completed` / `active` grammar, a scene `after:`'s) — an
    /// eligibility conjunct; the artifact's `prereqEdges` carries the same
    /// text as the beat's scenario edge. Omitted when not authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// dsl 0.26.0 §5: as [`BeatIr::target_kind`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<TargetKind>,
    /// dsl 0.27.0 §3 (T2-10): as [`EntryCmd::for_kind`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_kind: Option<ForKind>,
    /// dsl 0.27.0 §5: as [`BeatIr::spent_by`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spent_by: Option<CelPair>,
    /// dsl 0.31.0 §1: clock movement performed when the beat is presented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advances: Option<AdvanceSpec>,
    pub body: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}


/// Compiler-side constructors of a [`CelPair`]: they lower CEL.
pub trait CelPairExt {
    /// Build a pair from a checked, possibly expanded syntax slot.
    fn from_slot(slot: &lute_syntax::ast::CelSlot) -> Self;
    /// Build a pair from a raw CEL fragment used by synthetic/test helpers.
    fn from_raw(raw: &str) -> Self;
    /// Build a pair for a compiler-synthesized condition.
    fn from_expr(expr: ExprNode, authored: Option<String>) -> Self;
}

impl CelPairExt for CelPair {
    fn from_slot(slot: &lute_syntax::ast::CelSlot) -> Self {
        let cel = slot.raw.clone();
        Self {
            raw: cel,
            expr: crate::expr::lower_expr(&slot.raw)
                .unwrap_or_else(|| panic!("invalid CEL slot reached compilation: {}", slot.raw)),
            authored: slot.authored.clone(),
        }
    }

    fn from_raw(raw: &str) -> Self {
        CelPair {
            raw: raw.to_string(),
            expr: crate::expr::lower_expr(raw)
                .unwrap_or_else(|| panic!("invalid CEL slot reached compilation: {raw}")),
            authored: None,
        }
    }

    fn from_expr(expr: ExprNode, authored: Option<String>) -> Self {
        let raw = expr_to_cel(&expr);
        Self {
            raw,
            expr,
            authored,
        }
    }
}

fn expr_to_cel(expr: &ExprNode) -> String {
    match expr {
        ExprNode::Lit { lit } => match lit {
            LitVal::Int(v) => v.to_string(),
            LitVal::Num(v) => {
                let text = v.to_string();
                if v.is_finite() && v.fract() == 0.0 && !text.contains('.') {
                    format!("{text}.0")
                } else {
                    text
                }
            }
            LitVal::Bool(v) => v.to_string(),
            LitVal::Str(v) => cel_quote(v),
        },
        ExprNode::Path { path } => path.clone(),
        ExprNode::Unary { op, l } => format!("({op}{})", expr_to_cel(l)),
        ExprNode::Binary { op, l, r } => {
            format!("({} {op} {})", expr_to_cel(l), expr_to_cel(r))
        }
        ExprNode::Cond {
            cond,
            then,
            otherwise,
        } => format!(
            "({} ? {} : {})",
            expr_to_cel(cond),
            expr_to_cel(then),
            expr_to_cel(otherwise)
        ),
        ExprNode::List { list } => format!(
            "[{}]",
            list.iter().map(expr_to_cel).collect::<Vec<_>>().join(", ")
        ),
        ExprNode::Index { index, key } => {
            format!("{}[{}]", expr_to_cel(index), expr_to_cel(key))
        }
        ExprNode::Call { call, args } => format!(
            "{call}({})",
            args.iter().map(expr_to_cel).collect::<Vec<_>>().join(", ")
        ),
        ExprNode::Has { has } => format!("has({has})"),
    }
}

fn cel_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}


/// One `<reward/>` entry inlined in `QuestCmd.rewards` / `ObjectiveEntry.rewards`
/// (dsl 0.16.0 §2/§3). Pure declaration data — the engine grants at the
/// spec's fresh transitions (§3 D-D); the compiler NEVER synthesizes
/// handler bodies or `Command::Set` records here (0.14.0 subquest inverse:
/// declarative rewards are lifted OUT of executable content). Field
/// declaration order is serialized order (byte-stability contract).
///
/// Wire (dsl 0.16.0 Global Constraints): exactly one of `amount` XOR
/// (`amountMin`+`amountMax`) is present after amount defaulting
/// (unauthored → `amount: 1`). `outcome` is only ever `Some("failed")`, and
/// only on a quest-level entry.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardEntry {
    /// dsl 0.37.0 D10: the authored `<reward id>`, unique within its quest.
    /// Omitted when unauthored (the declaration index is the fallback key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// dsl 0.23.0 §8: the state path the reward kind's `credits:` names —
    /// a grant adds its amount there. Stamped from the capability snapshot
    /// at compile; omitted when the kind credits nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,
}

impl RewardEntry {
    /// Lower an AST [`lute_syntax::ast::Reward`] into the wire record.
    /// `owner_is_quest = true` preserves `outcome="failed"` (spec §2 — only legal
    /// on a quest-level entry); every other `outcome` value is dropped (the
    /// checker rejects it upstream, but this stays defensive so a stray
    /// value never reaches the wire). Amount defaulting (§3 Global
    /// Constraints): unauthored → `amount: 1`; a scalar fills `amount`; a
    /// range fills `amountMin`/`amountMax` verbatim (never pre-rolled,
    /// spec D-C).
    pub fn from_ast(reward: &lute_syntax::ast::Reward, owner_is_quest: bool) -> Self {
        use lute_syntax::ast::RewardAmount;
        let (amount, amount_min, amount_max) = match reward.amount {
            None => (Some(1), None, None),
            Some(RewardAmount::Scalar(n)) => (Some(n), None, None),
            Some(RewardAmount::Range(lo, hi)) => (None, Some(lo), Some(hi)),
        };
        let outcome = if owner_is_quest {
            reward.outcome.as_deref().and_then(|v| {
                if v == "failed" {
                    Some("failed".to_string())
                } else {
                    None
                }
            })
        } else {
            None
        };
        RewardEntry {
            id: reward.id.as_ref().map(|(id, _)| id.clone()),
            kind: reward.kind.clone(),
            target: reward.target.clone(),
            amount,
            amount_min,
            amount_max,
            when: reward.when.as_ref().map(CelPair::from_slot),
            outcome,
            credits: None,
        }
    }
}

impl Command {
    /// The serialized `kind` — the name of the authored directive or tag that
    /// produces the record (dsl 0.37.0 §5.3).
    pub fn kind(&self) -> &'static str {
        match self {
            Command::Line(_) => "line",
            Command::Bg(_) => "bg",
            Command::Music(_) => "music",
            Command::Sfx(_) => "sfx",
            Command::Vfx(_) => "vfx",
            Command::Actor(_) => "actor",
            Command::Camera(_) => "camera",
            Command::Cg(_) => "cg",
            Command::Video(_) => "video",
            Command::Sequence(_) => "sequence",
            Command::Set(_) => "set",
            Command::Assert(_) => "assert",
            Command::Retract(_) => "retract",
            Command::Choice(_) => "choice",
            Command::Match(_) => "match",
            Command::Hub(_) => "hub",
            Command::Jump(_) => "jump",
            Command::End(_) => "end",
            Command::Barrier(_) => "barrier",
            Command::Quest(_) => "quest",
            Command::On(_) => "on",
            Command::Plugin(_) => "plugin",
            Command::Entry(_) => "entry",
            Command::Accept(_) => "accept",
            Command::Beat(_) => "beat",
        }
    }

    /// The normative `family` of the record's kind (dsl 0.37.0 §5.1).
    pub fn family(&self) -> Family {
        match self {
            Command::Line(_) => Family::Content,
            Command::Bg(_)
            | Command::Music(_)
            | Command::Sfx(_)
            | Command::Vfx(_)
            | Command::Actor(_)
            | Command::Camera(_)
            | Command::Cg(_)
            | Command::Video(_)
            | Command::Sequence(_) => Family::Staging,
            Command::Set(_) | Command::Assert(_) | Command::Retract(_) => Family::State,
            Command::Choice(_)
            | Command::Match(_)
            | Command::Hub(_)
            | Command::Jump(_)
            | Command::End(_)
            | Command::Barrier(_) => Family::Control,
            Command::Quest(_)
            | Command::On(_)
            | Command::Entry(_)
            | Command::Accept(_)
            | Command::Beat(_) => Family::Declaration,
            Command::Plugin(_) => Family::Plugin,
        }
    }

    /// The record's `position` slot (filled by the addressing pass).
    pub fn position_mut(&mut self) -> &mut String {
        match self {
            Command::Line(c) => &mut c.position,
            Command::Bg(c) => &mut c.position,
            Command::Music(c) => &mut c.position,
            Command::Sfx(c) => &mut c.position,
            Command::Vfx(c) => &mut c.position,
            Command::Actor(c) => &mut c.position,
            Command::Camera(c) => &mut c.position,
            Command::Cg(c) => &mut c.position,
            Command::Video(c) => &mut c.position,
            Command::Sequence(c) => &mut c.position,
            Command::Set(c) => &mut c.position,
            Command::Assert(c) => &mut c.position,
            Command::Retract(c) => &mut c.position,
            Command::Choice(c) => &mut c.position,
            Command::Match(c) => &mut c.position,
            Command::Hub(c) => &mut c.position,
            Command::Jump(c) => &mut c.position,
            Command::Barrier(c) => &mut c.position,
            Command::End(c) => &mut c.position,
            Command::Plugin(c) => &mut c.position,
            Command::Quest(c) => &mut c.position,
            Command::On(c) => &mut c.position,
            Command::Entry(c) => &mut c.position,
            Command::Accept(c) => &mut c.position,
            Command::Beat(c) => &mut c.position,
        }
    }

    /// Visit every control-flow target field (option/arm `target`s,
    /// `otherwise`, `converge`, jump `target`) — the addressing pass rewrites
    /// symbolic labels to concrete positions through this single seam.
    pub fn for_each_target(&mut self, f: &mut impl FnMut(&mut String)) {
        match self {
            Command::Jump(j) => f(&mut j.target),
            Command::Choice(c) => {
                for o in &mut c.options {
                    f(&mut o.target);
                }
                f(&mut c.converge);
            }
            Command::Match(m) => {
                for a in &mut m.arms {
                    f(&mut a.target);
                }
                if let Some(o) = &mut m.otherwise {
                    f(o);
                }
                f(&mut m.converge);
            }
            Command::Hub(c) => {
                for o in &mut c.options {
                    f(&mut o.target);
                }
                if let Some(r) = &mut c.on_return {
                    f(r);
                }
                f(&mut c.converge);
            }
            // Quest/On carry symbolic `body`/objective-`body` targets that
            // MUST be rewritten to concrete addrs (IR addendum §5) — kept
            // EXPLICIT rather than folding into the `_` wildcard below.
            Command::Quest(q) => {
                for o in &mut q.objectives {
                    if let Some(b) = &mut o.body {
                        f(b);
                    }
                }
            }
            Command::On(o) => f(&mut o.body),
            Command::Entry(e) => f(&mut e.body),
            Command::Beat(b) => f(&mut b.body),
            Command::Line(_)
            | Command::Bg(_)
            | Command::Music(_)
            | Command::Sfx(_)
            | Command::Vfx(_)
            | Command::Actor(_)
            | Command::Camera(_)
            | Command::Cg(_)
            | Command::Video(_)
            | Command::Sequence(_)
            | Command::Set(_)
            | Command::Assert(_)
            | Command::Retract(_)
            | Command::Barrier(_)
            | Command::End(_)
            | Command::Plugin(_)
            | Command::Accept(_) => {}
        }
    }

    /// The record's stamp, when it has one (`jump`/`barrier` do not).
    pub fn stamp_mut(&mut self) -> Option<&mut Stamp> {
        match self {
            Command::Line(c) => Some(&mut c.stamp),
            Command::Bg(c) => Some(&mut c.stamp),
            Command::Music(c) => Some(&mut c.stamp),
            Command::Sfx(c) => Some(&mut c.stamp),
            Command::Vfx(c) => Some(&mut c.stamp),
            Command::Actor(c) => Some(&mut c.stamp),
            Command::Camera(c) => Some(&mut c.stamp),
            Command::Cg(c) => Some(&mut c.stamp),
            Command::Video(c) => Some(&mut c.stamp),
            Command::Sequence(c) => Some(&mut c.stamp),
            Command::Set(c) => Some(&mut c.stamp),
            Command::Assert(c) => Some(&mut c.stamp),
            Command::Retract(c) => Some(&mut c.stamp),
            Command::Choice(c) => Some(&mut c.stamp),
            Command::Match(c) => Some(&mut c.stamp),
            Command::Hub(c) => Some(&mut c.stamp),
            Command::Plugin(c) => Some(&mut c.stamp),
            Command::Quest(c) => Some(&mut c.stamp),
            Command::On(c) => Some(&mut c.stamp),
            Command::Entry(c) => Some(&mut c.stamp),
            Command::Accept(c) => Some(&mut c.stamp),
            Command::Beat(c) => Some(&mut c.stamp),
            Command::End(c) => Some(&mut c.stamp),
            Command::Jump(_) | Command::Barrier(_) => None,
        }
    }

    /// `(position, authored directive)` of a record a directive lowered to —
    /// staging, `::end`, a plugin passthrough ([`Stamp::authored`]) — and
    /// of the match a guarded `::use` compiled to (dsl 0.26.0 §4).
    pub fn authored(&self) -> Option<(&str, &str)> {
        let (position, stamp) = match self {
            Command::Bg(c) => (&c.position, &c.stamp),
            Command::Music(c) => (&c.position, &c.stamp),
            Command::Sfx(c) => (&c.position, &c.stamp),
            Command::Vfx(c) => (&c.position, &c.stamp),
            Command::Actor(c) => (&c.position, &c.stamp),
            Command::Camera(c) => (&c.position, &c.stamp),
            Command::Cg(c) => (&c.position, &c.stamp),
            Command::Video(c) => (&c.position, &c.stamp),
            Command::Sequence(c) => (&c.position, &c.stamp),
            Command::End(c) => (&c.position, &c.stamp),
            Command::Plugin(c) => (&c.position, &c.stamp),
            Command::Match(c) => (&c.position, &c.stamp),
            _ => return None,
        };
        Some((position, stamp.authored.as_deref()?))
    }
}
