//! Typed JSON IR (spec §4): tagged records with camelCase fields; only
//! relevant fields present (D3). Field DECLARATION ORDER is the serialized
//! order — part of the byte-stability contract; never reorder.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::expr::ExprNode;

/// Envelope (§4.1 + A9): language-version pin + IR schema version + capability
/// snapshot stamp + meta + folded state schema + flat command array. Field
/// DECLARATION ORDER is the serialized order (byte-stability contract).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    /// Document kind discriminator (dsl 0.2.0 §2/§3.1) — FIRST field, the
    /// byte-stability contract (IR addendum §1): most fundamental
    /// discriminator, read before anything else to know `meta`'s shape.
    pub kind: DocKind,
    /// Language-version pin (DSL 0.1.0), serialized as `lute`.
    pub lute: String,
    /// IR schema version (A9), independent of `lute`; engines gate parsing on it.
    pub ir_version: String,
    /// Plugin-system §13 capability snapshot stamp (A9): `snapshot.version`.
    pub capability_version: String,
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
    /// Authored shot headings (dsl 0.8.0 §6), 1-based `shot` number → the
    /// verbatim `## ` heading text. 0.6.0 made shot headings free text and
    /// dropped `Shot.number` from the AST; before 0.8.0 the heading was
    /// discarded entirely at lowering, so a compile → decompile round trip
    /// lost the author's section titles (the only authored structure with no
    /// other IR carrier). Emitted only for shots whose heading is non-empty;
    /// omitted entirely when no shot carries one. APPENDED LAST — after
    /// `prereqEdges`, the prior last field (byte-stability contract).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shots: Vec<ShotEntry>,
}

/// One authored shot heading (dsl 0.8.0 §6): the 1-based document-position
/// shot number and its verbatim `## ` text. Purely descriptive — control flow
/// never references it; `addr`'s shot segment is the join.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotEntry {
    pub shot: i64,
    pub heading: String,
}

/// One advisory prerequisite edge (connectivity spec §2.6, T13): a single
/// node's raw declared `after` formula text, verbatim and unvalidated.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrereqEdgeEntry {
    /// The contributing node's canonical key: a scene's own
    /// `{character}.{episodeId}` ([`lute_check::meta::canonical_episode_key`])
    /// or a quest's `<quest id>`.
    pub node: String,
    /// The RAW declared `after` formula text — unresolved, unvalidated CEL.
    pub after: String,
}

/// One merged entity kind (dsl 0.3.0 §3.1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityKindEntry {
    pub name: String,
    /// `None` for `open: engine` kinds (§3.1) — the engine mints members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
    pub open: bool,
}

/// One merged `enums:` entry (dsl 0.3.0 §3).
#[derive(Clone, Debug, Serialize)]
pub struct EnumEntry {
    pub name: String,
    pub members: Vec<String>,
}

/// One merged relation declaration (dsl 0.3.0 §4).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationEntry {
    pub name: String,
    pub args: Vec<String>,
    /// Effective tier for base relations (default `run` applied); ABSENT
    /// for `derive: true` (§4 — a derived relation has no write tier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    pub derive: bool,
    pub reserved: bool,
    /// 0-based functional-key arg indices (§4); empty when undeclared.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub key: Vec<usize>,
}

/// One seed `facts:` ground tuple (dsl 0.3.0 §4).
#[derive(Clone, Debug, Serialize)]
pub struct SeedFactEntry {
    pub relation: String,
    /// Ground literals as strings; bools serialize as `"true"`/`"false"`
    /// (§4 — seed facts are ground, never `_`).
    pub args: Vec<String>,
}

/// One Datalog rule (dsl 0.3.0 §7.1), emitted as STRUCTURED data — head +
/// body — for the engine's least-fixpoint evaluator. Lute performs NO
/// evaluation (D1); this is the declared rule set, verbatim.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEntry {
    pub head: AtomEntry,
    pub body: Vec<BodyEntry>,
    /// The rule's original source text (`rules:` entry), for engine
    /// diagnostics/tooling.
    pub raw: String,
}

/// One rule atom: a relation name applied to terms (dsl 0.3.0 §7.1).
#[derive(Clone, Debug, Serialize)]
pub struct AtomEntry {
    pub relation: String,
    pub terms: Vec<TermEntry>,
}

/// One rule term: a variable (leading-uppercase ident) or a ground constant
/// (dsl 0.3.0 §7.1). Bools lower to `Const` with a `"true"`/`"false"` value.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TermEntry {
    Var { name: String },
    Const { value: String },
}

/// One rule body literal (dsl 0.3.0 §7.1): a positive/negated atom, a CEL
/// guard, or a term comparison.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BodyEntry {
    Atom {
        atom: AtomEntry,
        negated: bool,
    },
    Guard {
        cel: String,
    },
    Cmp {
        lhs: TermEntry,
        rhs: TermEntry,
        negated: bool,
    },
}

/// Kind-polymorphic envelope `meta` (dsl 0.2.0, IR addendum §1; dsl 0.15.0
/// §2): untagged so the wire shape is exactly `SceneMeta`'s or `QuestMeta`'s
/// own fields — the consumer reads `Artifact.kind` to know which. Since IR
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
}

/// Scene-kind envelope meta (dsl 0.15.0 §2/§3). Field DECLARATION ORDER is
/// the serialized order (byte-stability contract):
///
///   `id` -> `character` -> `season` -> `episode` -> `episodeId` -> `title`
///   -> `meta` -> `plugin`
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
}

/// Quest-kind envelope meta (dsl 0.2.0 §6.1, IR addendum §1; dsl 0.15.0 §3
/// adds the descriptive `extra:` block): MAY serialize as `{}` when none of
/// title/contentLang/extra/plugin are authored.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestMeta {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Cross-cutting optional stamps (§4.3), flattened into every stamped record:
/// resolved blocking, timing, timeline clip placement, injection provenance,
/// component source.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<lute_check::Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Plugin-declared CROSS-CUTTING attrs (plugin 0.0.2 §14.1 `stampAttrs:`),
    /// flattened alongside the reserved timing keys above. An engine reads
    /// these exactly like a directive `fields` entry — typed by the declaring
    /// plugin's `AttrDecl`, absent when unauthored. Assembly REJECTS a
    /// `stampAttrs` name colliding with any reserved key above
    /// (`E-PLUGIN-RESERVED-STAMP-ATTR`), so this map can never shadow them.
    /// Serialized LAST within the stamp so existing field order is untouched.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `source { component }` on component-expanded records (§4.3, D8).
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub component: String,
}

/// `:line` role (§4.4, foundation D7). Voiced roles carry a `voiceKey`
/// (§4.2): `Dialogue`/`Voiceover`/`Offscreen` are heard (an off-screen line
/// is still spoken audio, just with no on-screen sprite this line);
/// `Monologue` (inner voice, not spoken aloud) and `Narration` are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Dialogue,
    Narration,
    Monologue,
    Voiceover,
    Offscreen,
}

impl Role {
    pub fn voiced(self) -> bool {
        matches!(self, Role::Dialogue | Role::Voiceover | Role::Offscreen)
    }
}

/// Document kind (dsl 0.2.0 §2/§3.1): `"scene"` | `"quest"`, mirrors
/// `lute_check::meta::DocKind` — kept as a SEPARATE compile-local serde enum
/// so `Serialize` never leaks onto lute-check's public type (serialization
/// concerns stay in the crate that owns the wire format). Mapped once, here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Scene,
    Quest,
}

impl From<lute_check::DocKind> for DocKind {
    fn from(k: lute_check::DocKind) -> Self {
        match k {
            lute_check::DocKind::Scene => DocKind::Scene,
            lute_check::DocKind::Quest => DocKind::Quest,
        }
    }
}

/// One record (§4.4). Internally tagged on `kind`; the `Other` variant is the
/// plugin-directive passthrough (plan spec-gap note 1) and serializes as
/// `kind: "plugin"`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Command {
    Line(LineCmd),
    Background(BackgroundCmd),
    Music(MusicCmd),
    Sfx(SfxCmd),
    Vfx(VfxCmd),
    Sprite(SpriteCmd),
    Camera(CameraCmd),
    Cut(CutCmd),
    Video(VideoCmd),
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
    #[serde(rename = "plugin")]
    Other(OtherCmd),
}

/// One `{{…}}` interpolation placeholder (IR A3): the runtime substitutes it
/// against live state, while `text`/`label` keep the verbatim `{{…}}` marker.
/// Kind-keyed referent — `{"kind":"path","path":…}`, `{"kind":"ref","ref":…}`,
/// `{"kind":"reserved","token":…}` — matching the A3 example and the C1
/// `ExprNode` kind-keyed convention. Entries appear in left-to-right order.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Placeholder {
    /// A state-path read (`{{run.coins}}` → `{"kind":"path","path":"run.coins"}`).
    Path { path: String },
    /// A `@def` / `@fn(args)` reference; the referent includes the leading `@`.
    Ref {
        #[serde(rename = "ref")]
        reference: String,
    },
    /// A reserved token (only `userName` in 0.1).
    Reserved { token: String },
}

/// Map one syntactic [`Interp`](lute_syntax::ast::Interp) to its typed IR
/// [`Placeholder`]. Shared by the content-line lowering ([`crate::lower`]) and
/// the option-label lowering ([`crate::stage`]) — the single kind→referent
/// match, never duplicated. The referent is the interp's verbatim trimmed `raw`.
pub(crate) fn placeholder_from_interp(i: &lute_syntax::ast::Interp) -> Placeholder {
    use lute_syntax::ast::InterpKind;
    match i.kind {
        InterpKind::Path => Placeholder::Path {
            path: i.raw.clone(),
        },
        InterpKind::Ref => Placeholder::Ref {
            reference: i.raw.clone(),
        },
        InterpKind::Reserved => Placeholder::Reserved {
            token: i.raw.clone(),
        },
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCmd {
    pub addr: String,
    pub role: Role,
    pub speaker: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_key: Option<String>,
    /// IR A3: `{{…}}` interpolations found in `text`, in left-to-right order.
    /// Absent when the line has no interpolation (byte-stability: skip-if-empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Merged locale texts (dsl 0.8.0 §7), locale tag → translated text, keyed
    /// on this record's `lineId`. Populated only when `compile --locales` was
    /// given a locale bundle; `text` always remains the SOURCE-language string
    /// (`contentLang`), so a 0.7 consumer is unaffected. Absent when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    /// Authored (or back-filled) per-speaker `code` — feeds `lineId`/`voiceKey`
    /// in the addressing pass, NEVER serialized (3-id model, §4.2).
    #[serde(skip)]
    pub code: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCmd {
    pub addr: String,
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
    pub addr: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SfxCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VfxCmd {
    pub addr: String,
    pub vfx_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// Authored `::auto` OR an injected sprite command (§7.4) — injected records
/// are SEPARATE records with `provenance` in their stamp.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteCmd {
    pub addr: String,
    pub character: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    /// A1 (schema-only): resolved costume id from the character-cast plugin;
    /// always `None` until cast ships, so it never serializes (skip-if-none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub costume: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shake: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCmd {
    pub addr: String,
    pub path: String,
    pub op: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One asserted delta (dsl 0.3.0 §5): the engine applies it as a positive
/// write to the relation's fact set. Emitted as DATA only (D1) — Lute
/// performs no evaluation, no fact store, no timestamps.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertCmd {
    pub addr: String,
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
    pub addr: String,
    pub relation: String,
    /// Ground literals or "_" wildcards (§5 RetractPattern).
    pub args: Vec<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceCmd {
    pub addr: String,
    pub branch_id: String,
    pub record_key: String,
    pub options: Vec<ChoiceOption>,
    pub converge: String,
    /// dsl 0.11.0: the choice-situation sentence for the UI. Absent unless
    /// authored (skip-if-empty, matching `ChoiceOption::when`/`expr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// dsl 0.11.0: the countdown, in whole seconds, matching the wire's
    /// `timeoutSec` name. Absent unless authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_sec: Option<u32>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    pub target: String,
    /// IR A3: `{{…}}` interpolations in `label`, in left-to-right order. Absent
    /// when the label has none (skip-if-empty). Label text stays verbatim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Merged locale labels (dsl 0.8.0 §7), locale tag → translated label,
    /// keyed on this option's `lineId`. See `LineCmd::texts`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

/// `<hub>` (§7.3.2, IR A2): structurally a `choice` plus revisit flags. The
/// hub record is the loop head; re-presentation is a RUNTIME property of the
/// `hub` kind, so no backward jump is emitted (D2/§3.2).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCmd {
    pub addr: String,
    pub id: String,
    pub record_key: String,
    pub options: Vec<HubOption>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One `<hub>` option: a `<choice>` option plus always-present `once`/`exit`
/// revisit flags. `when`/`expr` appear only when the choice is guarded.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubOption {
    pub id: String,
    pub label: String,
    pub line_id: String,
    pub once: bool,
    pub exit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    pub target: String,
    /// IR A3: `{{…}}` interpolations in `label`, in left-to-right order. Absent
    /// when the label has none (skip-if-empty). Label text stays verbatim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Merged locale labels (dsl 0.8.0 §7), locale tag → translated label,
    /// keyed on this option's `lineId`. See `LineCmd::texts`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCmd {
    pub addr: String,
    pub subject: String,
    pub arms: Vec<MatchArm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<String>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct MatchArm {
    pub test: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JumpCmd {
    pub addr: String,
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
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct BarrierCmd {
    pub addr: String,
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

/// Plugin-directive passthrough (plan spec-gap note 1): `kind: "plugin"`,
/// the authored tag, and its attrs typed via the manifest `AttrDecl`s.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherCmd {
    pub addr: String,
    pub tag: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    /// IR A12: resolved plugin state-write bindings from the manifest directive's
    /// `effects.writes`. Absent when the directive declares none (skip-if-empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
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
    pub addr: String,
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
    #[serde(flatten)]
    pub stamp: Stamp,
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
    pub when: Option<CelPair>,
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
    /// any quest-level grants. `on=` is never legal here (checker rejects it,
    /// compiler never emits it). Appended AFTER `quest` so every prior
    /// objective serializes byte-identically (`Vec::is_empty`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rewards: Vec<RewardEntry>,
}

/// `<on>` event-condition-action record (dsl 0.2.0 §4, §6.6, IR addendum
/// §3.3): an independent event rule (NOT part of the quest's declaration
/// table, unlike an objective), so it is its own standalone record.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnCmd {
    pub addr: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<CelPair>,
    pub body: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// A CEL slot's raw text + its portable lowered form (IR A7 `ExprNode`
/// shape), reused for every 0.2.0 quest-kind CEL attr (`start`/`fail`/
/// `done`/`when`/`on.when`) — the `{raw, expr}` dual-field shape
/// (`HubOption.when`/`.expr`, flattened for a choice option, nested here).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CelPair {
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
}

impl CelPair {
    /// Lower a raw CEL fragment via [`crate::expr::lower_expr`] into the
    /// `{raw, expr}` pair — `expr` is `None` for empty/out-of-profile CEL.
    pub fn from_raw(raw: &str) -> Self {
        CelPair {
            raw: raw.to_string(),
            expr: crate::expr::lower_expr(raw),
        }
    }
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
/// (unauthored → `amount: 1`). `on` is only ever `Some("failed")`, and
/// only on a quest-level entry.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardEntry {
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
    pub on: Option<String>,
}

impl RewardEntry {
    /// Lower an AST [`lute_syntax::ast::Reward`] into the wire record.
    /// `owner_is_quest = true` preserves `on="failed"` (spec §2 — only legal
    /// on a quest-level entry); every other `on` value is dropped (the
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
        let on = if owner_is_quest {
            reward.on.as_deref().and_then(|v| {
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
            kind: reward.kind.clone(),
            target: reward.target.clone(),
            amount,
            amount_min,
            amount_max,
            when: reward.when.as_ref().map(|w| CelPair::from_raw(&w.raw)),
            on,
        }
    }
}

impl Command {
    /// The record's `addr` slot (filled by the addressing pass, Task 11).
    pub fn addr_mut(&mut self) -> &mut String {
        match self {
            Command::Line(c) => &mut c.addr,
            Command::Background(c) => &mut c.addr,
            Command::Music(c) => &mut c.addr,
            Command::Sfx(c) => &mut c.addr,
            Command::Vfx(c) => &mut c.addr,
            Command::Sprite(c) => &mut c.addr,
            Command::Camera(c) => &mut c.addr,
            Command::Cut(c) => &mut c.addr,
            Command::Video(c) => &mut c.addr,
            Command::Set(c) => &mut c.addr,
            Command::Assert(c) => &mut c.addr,
            Command::Retract(c) => &mut c.addr,
            Command::Choice(c) => &mut c.addr,
            Command::Match(c) => &mut c.addr,
            Command::Hub(c) => &mut c.addr,
            Command::Jump(c) => &mut c.addr,
            Command::Barrier(c) => &mut c.addr,
            Command::End(c) => &mut c.addr,
            Command::Other(c) => &mut c.addr,
            Command::Quest(c) => &mut c.addr,
            Command::On(c) => &mut c.addr,
        }
    }

    /// Visit every control-flow target field (option/arm `target`s,
    /// `otherwise`, `converge`, jump `target`) — the addressing pass rewrites
    /// symbolic labels to concrete `addr`s through this single seam.
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
            Command::Line(_)
            | Command::Background(_)
            | Command::Music(_)
            | Command::Sfx(_)
            | Command::Vfx(_)
            | Command::Sprite(_)
            | Command::Camera(_)
            | Command::Cut(_)
            | Command::Video(_)
            | Command::Set(_)
            | Command::Assert(_)
            | Command::Retract(_)
            | Command::Barrier(_)
            | Command::End(_)
            | Command::Other(_) => {}
        }
    }

    /// The record's stamp, when it has one (`jump`/`barrier` do not).
    pub fn stamp_mut(&mut self) -> Option<&mut Stamp> {
        match self {
            Command::Line(c) => Some(&mut c.stamp),
            Command::Background(c) => Some(&mut c.stamp),
            Command::Music(c) => Some(&mut c.stamp),
            Command::Sfx(c) => Some(&mut c.stamp),
            Command::Vfx(c) => Some(&mut c.stamp),
            Command::Sprite(c) => Some(&mut c.stamp),
            Command::Camera(c) => Some(&mut c.stamp),
            Command::Cut(c) => Some(&mut c.stamp),
            Command::Video(c) => Some(&mut c.stamp),
            Command::Set(c) => Some(&mut c.stamp),
            Command::Assert(c) => Some(&mut c.stamp),
            Command::Retract(c) => Some(&mut c.stamp),
            Command::Choice(c) => Some(&mut c.stamp),
            Command::Match(c) => Some(&mut c.stamp),
            Command::Hub(c) => Some(&mut c.stamp),
            Command::Other(c) => Some(&mut c.stamp),
            Command::Quest(c) => Some(&mut c.stamp),
            Command::On(c) => Some(&mut c.stamp),
            Command::End(c) => Some(&mut c.stamp),
            Command::Jump(_) | Command::Barrier(_) => None,
        }
    }
}
