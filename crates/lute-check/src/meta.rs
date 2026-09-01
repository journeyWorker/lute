use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::DefParam;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::{lit_str, type_accepts, type_str, Literal, Type};
use lute_syntax::ast::Meta;

use crate::cel_paths::{is_reserved_quest_path, state_path_has_hyphen, E_PATH_IDENT};

/// State lifetime tier (dsl §9.1), keyed by the declared path's leading segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Scene,
    Run,
    User,
    App,
    /// `quest.<id>.*` (dsl 0.2.0 §5): a scratch tier scoped to one quest
    /// instance, MAY carry engine-reserved implicit sub-namespaces
    /// (`quest.<id>.state`, `quest.<id>.objectives.<oid>.done`, §5.2).
    Quest,
}

/// A single `state:` declaration (dsl §9.3): `type` + optional `default`, plus
/// the tier its path prefix maps to.
#[derive(Clone, Debug, PartialEq)]
pub struct StateDecl {
    pub ty: Type,
    pub default: Option<Literal>,
    pub namespace: Namespace,
}

/// The document's inline `state:` schema (dsl §9), path -> decl.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateSchema {
    pub decls: BTreeMap<String, StateDecl>,
}

/// A parsed seed fact from a `facts:` list (spec §4). Seeds are ground
/// (checked as for `::assert` — no `_` wildcard, decision D12).
#[derive(Clone, Debug)]
pub struct FactDecl {
    pub fact: lute_syntax::datalog::FactPattern,
    pub raw: String,
    pub span: Span,
}

/// A parsed rule from a `rules:` list (spec §7.1).
#[derive(Clone, Debug)]
pub struct RuleDecl {
    pub rule: lute_syntax::datalog::Rule,
    pub raw: String,
    pub span: Span,
}

/// Typed frontmatter (dsl §6.1). Built-in core keys are lifted into fields;
/// `plugins`/`defs` are retained structurally for downstream tasks.
#[derive(Clone, Debug, Default)]
pub struct TypedMeta {
    pub character: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// dsl §2.3: authored `episodeId:` frontmatter value (if any). A non-empty
    /// authored value overrides the derived `s{season:02}ep{episode:02}` default
    /// in [`canonical_episode_id`]; an absent or empty value falls back to the
    /// default. Lifted so [`canonical_scene_key`] (dsl 0.15.0 §2) can reproduce
    /// the derived scene key from `TypedMeta` alone without a second raw-YAML
    /// peek.
    pub episode_id: Option<String>,
    pub pov: Option<String>,
    /// The frontmatter `luteVersion:` stamp (dsl §6.1), lifted straight from
    /// the raw YAML mapping like `character`/`pov`. D13 stands: it is NEVER
    /// validated against capabilities — `check()` only compares it against
    /// the toolchain's [`crate::LUTE_LANG_VERSION`] for the warning-grade
    /// `W-LUTE-VERSION-STALE` freshness signal (dsl 0.6.1 §3).
    pub lute_version: Option<String>,
    /// The scene-level prerequisite `after:` frontmatter key (connectivity
    /// layer, T2): raw CEL text, lifted straight from the raw YAML mapping
    /// the same way `character`/`season`/`episode`/`pov` are — validated
    /// separately (grammar only, `crate::prereq::parse_prereq`) by `check()`,
    /// never here.
    pub after: Option<String>,
    /// dsl 0.15.0 §2: authored canonical scene key. When present, this string
    /// is the canonical scene identity everywhere the derived
    /// `{character}.{episodeId}` join is consumed today (lineId prefix,
    /// connectivity `visited(K)` targets, `prereqEdges[].node`,
    /// `project.index.json` document key, `lute play`'s visited-set). The
    /// value is validated on lift: non-empty and matching `[A-Za-z0-9_.-]+`
    /// — anything else stays `None` and draws `E-META-ID`. Absent → derived
    /// fallback via [`canonical_scene_key`], byte-identical to 0.14.0.
    pub id: Option<String>,
    /// dsl 0.15.0 §3: authored `meta:` descriptive block. Free open mapping
    /// with scalar or flat-scalar-list values, never consulted by any
    /// checker/compiler/runtime rule and never routed through CEL — the
    /// sanctioned home for team search metadata (`arc`, `location`, whatever)
    /// carried verbatim into the artifact (`SceneMeta.meta`/`QuestMeta.meta`,
    /// omitted when empty). A nested mapping or non-scalar list entry stays
    /// out of this map and draws `E-META-VALUE`.
    pub meta_block: BTreeMap<String, serde_json::Value>,
    pub profile: Option<String>,
    pub plugins: BTreeMap<String, serde_yaml::Value>,
    pub uses: Vec<String>,
    pub extends: Vec<String>,
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    /// Project-authored `enums:`/`entities:` declarations (dsl data-catalog
    /// foundation A3; 0.3.0 draft §3.1), parsed via
    /// `lute_manifest::entities::parse_enums` /
    /// `lute_manifest::relations::{parse_entity_kinds, kinds_to_domains}`
    /// into enum-style/open [`Domain`]s. Lifted into the checker's merged
    /// domain vocabulary the SAME way as `state`/`defs` (`crate::schema_import`,
    /// alongside `CapabilitySnapshot.domains`, A2). A same-doc `enums:`/
    /// `entities:` name collision is NOT diagnosed here (`entities:` simply
    /// wins) — cross-source collisions are `schema_import`'s job
    /// (`E-DOMAIN-DUP`).
    pub domains: BTreeMap<String, Domain>,
    /// Scene-level reusable-content component imports (dsl §13): each entry is a
    /// relative path to a component file resolved via `resolve_components`.
    pub components: Vec<String>,
    /// A component file's own declared name (dsl §13): `Some` only when this
    /// document was parsed as a `MetaKind::Component` file that declared
    /// `component:`. A scene leaves this `None`.
    pub component: Option<String>,
    /// A component file's declared params (dsl §13), in source order (the
    /// named-arg binding namespace for `::use`). Empty for a scene.
    pub params: Vec<DefParam>,
    /// True when a `params:` key is PRESENT but malformed (not a mapping, a
    /// non-string key, or a value that is not a valid [`Type`]) — the resolver
    /// surfaces this as `E-COMPONENT-PARSE` rather than silently entering a
    /// shrunken signature (dsl §13). `false` when `params:` is absent or wholly
    /// valid.
    pub params_malformed: bool,
    /// Project-authored `entities:` entity-kind decls (0.3.0 draft §3.1, T4),
    /// parsed via `lute_manifest::relations::parse_entity_kinds`. Distinct
    /// from [`Self::domains`] (the 0.2.2 attr-layer projection): this is the
    /// full decl shape the relational checker (Tasks 6/7) needs.
    pub rel_kinds: lute_manifest::relations::ParsedKinds,
    /// Project-authored `relations:` decls (0.3.0 draft §4, T4), parsed via
    /// `lute_manifest::relations::parse_relations`.
    pub rel_relations: lute_manifest::relations::ParsedRelations,
    /// Project-authored `facts:` seeds (0.3.0 draft §4), each string parsed
    /// via `lute_syntax::datalog::parse_fact`. A malformed entry is diagnosed
    /// here at lift (`E-DATALOG-PARSE`/`E-DATALOG-FUNCTION`) and simply
    /// omitted from this list.
    pub rel_facts: Vec<FactDecl>,
    /// Project-authored `rules:` (0.3.0 draft §7.1), each string parsed via
    /// `lute_syntax::datalog::parse_rule`. Same omit-on-error discipline as
    /// [`Self::rel_facts`].
    pub rel_rules: Vec<RuleDecl>,
}

/// Frontmatter keys valid in EVERY root document kind (dsl 0.2.0 §6.1): the
/// kind-agnostic core keys. `kind:` itself is handled separately (valid only
/// for a root kind — Scene/Quest — never Schema/Component, dsl 0.2.0 §3.1), so
/// it is NOT in this list; the unknown-key loop below tests it independently.
/// `components:` (the import list, dsl §13) is valid everywhere; `component:`/
/// `params:` are NOT — see [`COMPONENT_ONLY_KEYS`].
const UNIVERSAL_KEYS: &[&str] = &[
    "mode",
    "title",
    "luteVersion",
    "contentLang",
    "profile",
    "plugins",
    "uses",
    "extends",
    "state",
    "defs",
    "enums",
    "entities",
    "relations",
    "facts",
    "rules",
    "components",
    "codesLocked",
];

/// Frontmatter keys valid ONLY in a `MetaKind::Scene` document (dsl 0.1.0 §6.1,
/// dsl 0.2.0 §3.1/§6.1, dsl 0.15.0 §2): the scene identity triad plus the
/// scene-only extras plus the authored canonical key `id:`. A Quest document
/// declaring any of these is `E-META-UNKNOWN-KEY`.
const SCENE_KEYS: &[&str] = &[
    "id",
    "character",
    "season",
    "episode",
    "episodeId",
    "pov",
    "after",
];

/// Frontmatter keys that are valid ONLY in a component file (dsl §13): the
/// component's own name (`component:`) and its parameter signature (`params:`).
/// In a scene or schema doc these are unknown top-level keys.
const COMPONENT_ONLY_KEYS: &[&str] = &["component", "params"];

/// 0.10.0 §6.3: is a `defaults:` key legal on `kind`? A default whose key is
/// not legal on a document's resolved kind is NOT applied to that document,
/// and that is not an error — `character:` is scene-only, and a quest under a
/// root defaulting it simply does not receive it.
///
/// The predicate is the same one the unknown-key loop uses below, minus the
/// component-only and plugin-owned arms:
/// [`lute_manifest::project::DEFAULTABLE_KEYS`] holds neither.
///
/// dsl 0.15.0 D-D: `id:` is per-document unique — it is deliberately NOT in
/// `DEFAULTABLE_KEYS`, so it can never reach this predicate at runtime, but
/// filter it explicitly so a hand-built `MetaDefaults` cannot silently smuggle
/// it either. `meta:` is legal on Scene AND Quest — both artifact meta kinds
/// carry it (§3).
pub fn default_key_legal_on(key: &str, kind: MetaKind) -> bool {
    if key == "id" {
        return false;
    }
    if key == "meta" {
        return matches!(kind, MetaKind::Scene | MetaKind::Quest);
    }
    UNIVERSAL_KEYS.contains(&key)
        || (key == "kind" && matches!(kind, MetaKind::Scene | MetaKind::Quest))
        || (kind == MetaKind::Scene && SCENE_KEYS.contains(&key))
}

/// The advisory tail on `E-META-UNKNOWN-KEY` (dsl 0.5.0 §2.2's "did you mean",
/// generalised): empty when nothing is close, because a wrong suggestion is
/// worse than none.
///
/// `after` is the one special case (0.10.0 backlog #11, T9.1). It is a CORE key
/// on the sibling kind and a legal ATTRIBUTE in this one, so the generic
/// edit-distance suggestion has no candidate to land on and the author is told
/// only that the key is unknown — while `after=` is legal two lines below in
/// the same file. Name the attribute form instead.
///
/// The candidate set mirrors the unknown-key loop's own `core_key` predicate
/// exactly, so the suggestion can never name a key that would itself be
/// rejected on this kind. Slice order is source order, which makes
/// [`lute_manifest::suggest::nearest`]'s first-wins tie-break deterministic.
fn unknown_key_hint(key: &str, kind: MetaKind, component_key_allowed: bool) -> String {
    if key == "after" && kind == MetaKind::Quest {
        return " — a quest's prerequisite is the `after=` ATTRIBUTE on its `<quest>` element, \
                not a frontmatter key (dsl §4.1)"
            .to_string();
    }
    let is_root = matches!(kind, MetaKind::Scene | MetaKind::Quest);
    let scene_keys: &[&str] = if kind == MetaKind::Scene {
        SCENE_KEYS
    } else {
        &[]
    };
    let component_keys: &[&str] = if component_key_allowed {
        COMPONENT_ONLY_KEYS
    } else {
        &[]
    };
    let root_extras: &[&str] = if is_root { &["kind", "meta"] } else { &[] };
    let candidates = UNIVERSAL_KEYS
        .iter()
        .copied()
        .chain(root_extras.iter().copied())
        .chain(scene_keys.iter().copied())
        .chain(component_keys.iter().copied());
    match lute_manifest::suggest::nearest(key, candidates, 2) {
        Some(sugg) => format!(" — did you mean `{sugg}`?"),
        None => String::new(),
    }
}

const REQUIRED_KEYS: &[&str] = &["character", "season", "episode"];

/// Which document kind's frontmatter is being parsed. A `Schema` doc (imported
/// via `uses:`, dsl §9.2) and a `Component` doc (imported via `components:`,
/// dsl §13) are NOT scenes — neither carries the required character/season/
/// episode keys. `Quest` (dsl 0.2.0 §3.1, §6.1) is a second ROOT kind: like
/// Scene it carries `kind:`, but (like Schema/Component) requires no keys and
/// additionally rejects the scene-only [`SCENE_KEYS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaKind {
    Scene,
    Schema,
    /// A reusable-content component file (dsl §13): lifts `component:`+`params:`,
    /// skips the scene-required keys exactly like `Schema`.
    Component,
    /// The quest kind (dsl 0.2.0 §3.1, §6.1): a second ROOT document kind. No
    /// required keys; rejects [`SCENE_KEYS`].
    Quest,
}

/// A ROOT document's domain kind (dsl 0.2.0 §3.1): the frontmatter `kind:`
/// discriminator. Import-role docs (`MetaKind::Schema`/`MetaKind::Component`)
/// never carry `kind:` and are never a `DocKind` — only a Scene or Quest root
/// document resolves one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    Scene,
    Quest,
}

/// `<quest>`/`kind:`/etc. diagnostic codes owned by [`resolve_doc_kind`] (dsl
/// 0.2.0 Appendix B).
pub const E_KIND_MISSING: &str = "E-KIND-MISSING";
pub const E_UNKNOWN_KIND: &str = "E-UNKNOWN-KIND";

/// dsl 0.8.0 §4: an author `state:` declaration whose `type` is `list`,
/// `record`, or `map`. Author state is scalar (`number|bool|string|enum`);
/// collection shapes reach the artifact's state table only through a plugin
/// `state_shapes` expansion. Like `E-TEMPORAL-ARG` (dsl 0.3.0 D11) the decl is
/// NOT installed, so a later read of the path is plain `E-UNDECLARED`.
pub const E_STATE_COLLECTION: &str = "E-STATE-COLLECTION";

/// Resolve the frontmatter `kind:` scalar (dsl 0.2.0 §3.1) — a cheap peek of
/// `meta.raw_yaml` run BEFORE the full [`parse_meta_kind`] pass (kind gates
/// which per-kind keys that pass allows). An absent `kind` is `E-KIND-MISSING`;
/// a present but unrecognized value is `E-UNKNOWN-KIND`; either way this
/// returns `None` so the caller can degrade to a safe default. On a YAML parse
/// failure this returns `(None, [])` — the separate `E-META-PARSE` diagnostic
/// surfaces from `parse_meta_kind`, never duplicated here.
pub fn resolve_doc_kind(meta: &Meta) -> (Option<DocKind>, Vec<Diagnostic>) {
    let value: serde_yaml::Value = match serde_yaml::from_str(&meta.raw_yaml) {
        Ok(v) => v,
        Err(_) => return (None, Vec::new()),
    };
    let empty = serde_yaml::Mapping::new();
    let map = match &value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => &empty,
        _ => return (None, Vec::new()),
    };
    let span = meta.span;
    let err = |code: &str, message: String| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    match map.get(yaml_key("kind")) {
        None => (
            None,
            vec![err(
                E_KIND_MISSING,
                "required frontmatter key `kind` is missing; every root document must \
                 declare `kind: scene` or `kind: quest` (dsl 0.2.0 §3.1)"
                    .to_string(),
            )],
        ),
        Some(v) => {
            let kind_str = v
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{v:?}"));
            match kind_str.as_str() {
                "scene" => (Some(DocKind::Scene), Vec::new()),
                "quest" => (Some(DocKind::Quest), Vec::new()),
                other => (
                    None,
                    vec![err(
                        E_UNKNOWN_KIND,
                        format!(
                            "unknown document kind `{other}`; expected `scene` or `quest` \
                             (dsl 0.2.0 §3.1)"
                        ),
                    )],
                ),
            }
        }
    }
}

/// [`resolve_doc_kind`] with the manifest's `defaults:` consulted when the
/// document declares no `kind:` (0.10.0 §6.3). `E-KIND-MISSING` now means
/// "neither the document nor the manifest says".
pub fn resolve_doc_kind_with_defaults(
    meta: &Meta,
    defaults: &lute_manifest::project::MetaDefaults,
) -> (Option<DocKind>, Vec<Diagnostic>) {
    let (kind, diags) = resolve_doc_kind(meta);
    if kind.is_some() || defaults.is_empty() {
        return (kind, diags);
    }
    match defaults.get("kind").and_then(|v| v.as_str()) {
        // `defaults_shape_ok` already rejected anything but these two, so a
        // manifest can never route a document to an unknown kind here.
        Some("scene") => (Some(DocKind::Scene), Vec::new()),
        Some("quest") => (Some(DocKind::Quest), Vec::new()),
        _ => (kind, diags),
    }
}

/// The `episodeId` component of the canonical scene identity key (dsl §2.3,
/// connectivity layer T3): `episode_id` is the raw authored `episodeId:`
/// frontmatter value (if any); a non-empty authored value is used verbatim,
/// otherwise the lowercase default `s{season:02}ep{episode:02}` (dsl §4.1/
/// A4/A9) is derived from `season`/`episode`. This is the SAME component
/// [`canonical_episode_key`] joins onto `character`, and the same string
/// `lute-compile`'s `artifact_meta`/address-pass lineId prefix join computes
/// (`lute-compile/src/lib.rs`) — kept as one shared implementation so both
/// crates agree byte-for-byte.
pub fn canonical_episode_id(season: i64, episode: i64, episode_id: Option<&str>) -> String {
    match episode_id.filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => format!("s{season:02}ep{episode:02}"),
    }
}

/// Canonical scene identity key (dsl §2.3, connectivity layer T3):
/// `{character}.{episodeId}`, where the `episodeId` component is
/// [`canonical_episode_id`]. This is the SAME string `lute-compile`'s
/// `artifact_meta`/address-pass lineId prefix join computes
/// (`{character}.{episode_id}`, `lute-compile/src/lib.rs`) — kept as one
/// shared implementation so both crates (and this crate's
/// [`crate::connectivity::scene_key_set`] project-wide grouping key) agree
/// byte-for-byte.
pub fn canonical_episode_key(
    character: &str,
    season: i64,
    episode: i64,
    episode_id: Option<&str>,
) -> String {
    format!(
        "{character}.{}",
        canonical_episode_id(season, episode, episode_id)
    )
}

/// dsl 0.15.0 §2: the canonical scene identity key for a parsed frontmatter.
/// Authored `id:` wins entire — its lift already validates the value against
/// the `[A-Za-z0-9_.-]+` charset, so a `Some` here is the exact string the
/// author wrote. Otherwise the derived legacy join
/// `{character}.{episodeId}` via [`canonical_episode_key`] is reconstructed —
/// requires all of `character`+`season`+`episode` (with `episodeId` optional,
/// defaulting to `s{season:02}ep{episode:02}` per [`canonical_episode_id`]).
/// `None` when neither branch yields a key (a doc still missing the legacy
/// triad and authoring no `id:` — a `E-META-MISSING` case; this project-wide
/// helper must never fabricate `.s00ep00` for it).
///
/// One shared resolution point: `lute-compile`'s prefix/`prereqEdges`,
/// `connectivity::scene_key_set`, `project.index.json`'s `document_key`, and
/// `lute play`'s canonical-key lookup all route through this — so authored
/// and derived keys land byte-for-byte identical downstream.
pub fn canonical_scene_key(meta: &TypedMeta) -> Option<String> {
    if let Some(id) = meta.id.as_deref() {
        return Some(id.to_string());
    }
    let character = meta.character.as_deref()?;
    if character.is_empty() {
        return None;
    }
    let season = meta.season?;
    let episode = meta.episode?;
    Some(canonical_episode_key(
        character,
        season,
        episode,
        meta.episode_id.as_deref(),
    ))
}

/// dsl 0.15.0 §2: the authored `id:` charset gate — non-empty and matching
/// `[A-Za-z0-9_.-]+`. The set is a superset of every string
/// [`canonical_episode_id`]'s derived fallback can produce (`.` admits
/// namespacing like `anseo.s01ep01`), so a document migrated from the
/// derived join keeps the same canonical key when it moves to authored `id:`.
pub(crate) fn is_valid_scene_id_raw(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// dsl 0.15.0 §3: lift one authored `meta:` YAML value into the JSON-ready
/// `meta_block` on `TypedMeta`. The value MUST be a mapping with string keys
/// whose values are either scalars (string/int/float/bool) or FLAT sequences
/// of scalars — anything else (nested mapping, mixed list, non-string key,
/// non-mapping top-level) stays out of the block and draws one `E-META-VALUE`
/// anchored at the offending inner key (or at `meta:` itself for the
/// non-mapping case).
///
/// Never panics; a malformed entry is simply skipped so a partly-valid block
/// still contributes its clean keys downstream.
fn lift_meta_block(
    meta: &Meta,
    value: &serde_yaml::Value,
    into: &mut BTreeMap<String, serde_json::Value>,
    diags: &mut Vec<Diagnostic>,
) {
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => return,
        _ => {
            diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: "`meta:` must be a YAML mapping of descriptive keys; each value \
                          is a scalar (string/int/float/bool) or a flat list of scalars \
                          (dsl 0.15.0 §3)"
                    .to_string(),
                span: meta_key_span(meta, "meta"),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            return;
        }
    };
    for (k, v) in map {
        let Some(key) = k.as_str() else {
            diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: "`meta:` mapping keys must be strings (dsl 0.15.0 §3)".to_string(),
                span: meta_key_span(meta, "meta"),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            continue;
        };
        match scalar_or_flat_seq_to_json(v) {
            Some(json) => {
                into.insert(key.to_string(), json);
            }
            None => diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: format!(
                    "`meta.{key}` must be a scalar (string/int/float/bool) or a flat list \
                     of scalars; nested mappings and non-scalar list entries are not allowed \
                     (dsl 0.15.0 §3)"
                ),
                span: meta_key_span(meta, key),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }),
        }
    }
}

/// dsl 0.15.0 §3: convert a `meta:` value to `serde_json::Value` if and only
/// if it is a scalar or a flat sequence of scalars. `None` for a nested
/// mapping, a sequence containing a non-scalar, a `!tag`, or a mapping with
/// non-string keys. `null` is a scalar (it round-trips to JSON `null`).
fn scalar_or_flat_seq_to_json(value: &serde_yaml::Value) -> Option<serde_json::Value> {
    fn scalar_to_json(value: &serde_yaml::Value) -> Option<serde_json::Value> {
        match value {
            serde_yaml::Value::Null => Some(serde_json::Value::Null),
            serde_yaml::Value::Bool(b) => Some(serde_json::Value::Bool(*b)),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(serde_json::Value::Number(i.into()))
                } else if let Some(u) = n.as_u64() {
                    Some(serde_json::Value::Number(u.into()))
                } else if let Some(f) = n.as_f64() {
                    serde_json::Number::from_f64(f).map(serde_json::Value::Number)
                } else {
                    None
                }
            }
            serde_yaml::Value::String(s) => Some(serde_json::Value::String(s.clone())),
            _ => None,
        }
    }
    match value {
        serde_yaml::Value::Sequence(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(scalar_to_json(item)?);
            }
            Some(serde_json::Value::Array(out))
        }
        other => scalar_to_json(other),
    }
}

/// Parse the peeled YAML frontmatter (dsl §6.1) into typed form plus the inline
/// `state:` schema (dsl §9.3). Never panics on malformed YAML: a parse failure
/// surfaces `E-META-PARSE` and yields a best-effort (empty) `TypedMeta`.
///
/// This performs the §6.1 required-key and unknown-key checks and records each
/// `state:` path's `Namespace` from its leading segment. App-write read-only
/// enforcement (§9.5, Task 4.5) and def-assignment (§8.1, Task 4.4) are NOT done
/// here.
pub fn parse_meta(meta: &Meta, snapshot: &CapabilitySnapshot) -> (TypedMeta, Vec<Diagnostic>) {
    parse_meta_kind(meta, snapshot, MetaKind::Scene)
}

/// Kind-aware variant of [`parse_meta`]. Schema docs (`MetaKind::Schema`) skip
/// the §6.1 required-key check (they carry no character/season/episode); every
/// other check (unknown-key, `state:`, `defs:`, `uses:` lift) is identical.
pub fn parse_meta_kind(
    meta: &Meta,
    snapshot: &CapabilitySnapshot,
    kind: MetaKind,
) -> (TypedMeta, Vec<Diagnostic>) {
    parse_meta_kind_with_defaults(
        meta,
        snapshot,
        kind,
        &lute_manifest::project::MetaDefaults::default(),
    )
}

/// [`parse_meta_kind`] with the governing manifest's `defaults:` applied
/// (0.10.0 §6.2).
///
/// The merge happens on the frontmatter MAPPING, before any frontmatter rule
/// runs, which is what makes §6.2's last two bullets free: required-key
/// checking, unknown-key checking, type checking and every lift below all see
/// the resolved map, and a defaulted value draws exactly the diagnostic an
/// authored one would because it IS an authored one by the time they run.
///
/// Per key, whole-value (D-P): a key the document contains AT ALL wins
/// entire. `Mapping::contains_key` is exactly that test, so a present null or
/// an empty sequence is a present key — §6.2's "present-but-empty counts as
/// present", with no special case.
pub fn parse_meta_kind_with_defaults(
    meta: &Meta,
    snapshot: &CapabilitySnapshot,
    kind: MetaKind,
    defaults: &lute_manifest::project::MetaDefaults,
) -> (TypedMeta, Vec<Diagnostic>) {
    let span = meta.span;
    let mut diags = Vec::new();
    let err = |code: &str, message: String| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    // Same Content-layer error, but at a caller-supplied narrow span (used for
    // E-PATH-IDENT, which points at the offending key — not the whole block).
    let err_at = |code: &str, message: String, sp: Span| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: sp,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };

    let value: serde_yaml::Value = match serde_yaml::from_str(&meta.raw_yaml) {
        Ok(v) => v,
        Err(e) => {
            // A literal duplicate key inside `entities:`/`relations:`
            // (same-block dup, dsl 0.3.0 T5/T7) makes `serde_yaml::from_str`
            // reject the ENTIRE frontmatter — it does NOT silently collapse
            // the repeat, despite an earlier assumption in this codebase.
            // Retry against a text-sanitized copy (every occurrence after the
            // first of a same-indent-level `entities:`/`relations:` child key
            // commented out) so every OTHER frontmatter field still lifts;
            // the authoritative dup NAME still comes from
            // `scan_block_dup_names` against the UNMODIFIED `meta.raw_yaml`
            // below, never this sanitized copy.
            match serde_yaml::from_str(&sanitize_dup_block_keys(&meta.raw_yaml)) {
                Ok(v) => v,
                Err(_) => {
                    diags.push(err(
                        "E-META-PARSE",
                        format!("invalid meta frontmatter YAML: {e}"),
                    ));
                    return (TypedMeta::default(), diags);
                }
            }
        }
    };

    // An empty frontmatter deserializes to Null; treat as an empty mapping so the
    // required-key checks still fire.
    let empty = serde_yaml::Mapping::new();
    let map = match &value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => &empty,
        _ => {
            diags.push(err(
                "E-META-PARSE",
                "meta frontmatter must be a YAML mapping".to_string(),
            ));
            return (TypedMeta::default(), diags);
        }
    };

    // dsl 0.15.0 §4 (D-C): `W-META-LEGACY` reads the AUTHORED frontmatter,
    // never the defaults-merged view — a manifest-inherited legacy key is not
    // the document author's to move. Snapshot the authored `id:` presence and
    // authored legacy keys BEFORE the defaults overlay below; the required-
    // and unknown-key checks continue to run over the merged map (0.10.0 §6.2).
    let authored_has_id = map.contains_key(yaml_key("id"));
    let authored_legacy: Vec<&'static str> = ["character", "season", "episode", "episodeId"]
        .iter()
        .copied()
        .filter(|k| map.contains_key(yaml_key(k)))
        .collect();

    // 0.10.0 §6.2/§6.3: overlay the governing manifest's `defaults:`, filtered
    // to keys legal on THIS kind. Borrowed when there is nothing to overlay,
    // which is every project written before 0.10.0 — no allocation on the
    // path every existing document takes.
    let map: std::borrow::Cow<'_, serde_yaml::Mapping> = if defaults.is_empty() {
        std::borrow::Cow::Borrowed(map)
    } else {
        let mut merged = map.clone();
        for key in defaults.keys() {
            if !default_key_legal_on(key, kind) {
                continue;
            }
            if merged.contains_key(yaml_key(key)) {
                continue;
            }
            if let Some(v) = defaults.get(key) {
                merged.insert(yaml_key(key), v.clone());
            }
        }
        std::borrow::Cow::Owned(merged)
    };
    let map = map.as_ref();

    let mut typed = TypedMeta::default();

    // Required-key check (dsl §6.1). Only scenes carry the legacy required
    // core keys; Schema/Component/Quest do not. dsl 0.15.0 §4: authored (or
    // defaults-merged) `id:` IS the canonical scene key — the required-key
    // rule no longer fires on a document that supplies one.
    if kind == MetaKind::Scene && !map.contains_key(yaml_key("id")) {
        for missing in REQUIRED_KEYS
            .iter()
            .filter(|k| !map.contains_key(yaml_key(k)))
        {
            diags.push(err(
                "E-META-MISSING",
                format!(
                    "required meta key `{missing}` is missing (authored `id:` also \
                     satisfies scene identity, dsl 0.15.0 §2/§4)"
                ),
            ));
        }
    }
    // Unknown-key check over the top-level keys (dsl §6.1); applies to every kind.
    // `component:`/`params:` (dsl §13) are allowed ONLY in a component file, so a
    // scene or schema doc that declares them hits the unknown-key diagnostic.
    //
    // A key the CORE owns keeps its bespoke handling below (each is lifted and
    // typed by its own `get_*`/parse step); only a PLUGIN-owned key routes
    // through the declared-schema check (plugin Appendix C2).
    let component_key_allowed = kind == MetaKind::Component;
    for (k, v) in map.iter() {
        let Some(key) = k.as_str() else {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                "meta keys must be strings".to_string(),
            ));
            continue;
        };
        let core_key = UNIVERSAL_KEYS.contains(&key)
            || (key == "kind" && matches!(kind, MetaKind::Scene | MetaKind::Quest))
            || (kind == MetaKind::Scene && SCENE_KEYS.contains(&key))
            || (matches!(kind, MetaKind::Scene | MetaKind::Quest) && key == "meta")
            || (component_key_allowed && COMPONENT_ONLY_KEYS.contains(&key));
        if core_key {
            continue;
        }
        let Some(ty) = snapshot.frontmatter.get(key) else {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                format!(
                    "unknown top-level meta key `{key}` (not a core key and not owned by an active plugin){}",
                    unknown_key_hint(key, kind, component_key_allowed)
                ),
            ));
            continue;
        };
        // plugin Appendix C2: "The checker MUST validate a scene's frontmatter
        // block against each active plugin's declared key schema; a value that
        // violates the schema is a static error." A value with no `Literal`
        // representation at all (null, a `!tag`, a non-string mapping key)
        // is the SAME error, never a panic.
        let lit = Literal::from_yaml(v);
        if !lit.as_ref().is_some_and(|l| type_accepts(ty, l)) {
            let found = match &lit {
                Some(l) => lit_str(l),
                None => unrepresentable_yaml(v).to_string(),
            };
            diags.push(err_at(
                "E-FRONTMATTER-SCHEMA",
                format!(
                    "frontmatter key `{key}` expects {} (declared by an active plugin), got {found}",
                    type_str(ty)
                ),
                meta_key_span(meta, key),
            ));
        }
    }

    // Lift the built-in scalar/collection keys.
    typed.character = get_str(map, "character");
    typed.season = get_i64(map, "season");
    typed.episode = get_i64(map, "episode");
    typed.episode_id = get_str(map, "episodeId");
    typed.pov = get_str(map, "pov");
    typed.lute_version = get_str(map, "luteVersion");
    typed.after = get_str(map, "after");

    // dsl 0.15.0 §2: authored canonical scene key. Non-empty and matching
    // `[A-Za-z0-9_.-]+` — anything else is `E-META-ID` and the value stays
    // unlifted (a rejected id must never fall through as a valid canonical
    // key). Scene-only; a `Quest`/`Schema`/`Component` id: was already
    // rejected as `E-META-UNKNOWN-KEY` above.
    if let Some(raw) = get_str(map, "id") {
        if is_valid_scene_id_raw(&raw) {
            typed.id = Some(raw);
        } else {
            diags.push(err_at(
                "E-META-ID",
                format!(
                    "scene `id:` `{raw}` is not a valid canonical scene key; the value must \
                     be non-empty and match `[A-Za-z0-9_.-]+` (dsl 0.15.0 §2)"
                ),
                meta_key_span(meta, "id"),
            ));
        }
    }

    // dsl 0.15.0 §3: authored `meta:` descriptive block. Legal on Scene and
    // Quest roots (both artifact meta kinds carry it); on Schema/Component
    // documents the top-level unknown-key loop above already rejected it, so
    // the lift silently drops it there.
    if matches!(kind, MetaKind::Scene | MetaKind::Quest) {
        if let Some(meta_value) = map.get(yaml_key("meta")) {
            lift_meta_block(meta, meta_value, &mut typed.meta_block, &mut diags);
        }
    }

    // dsl 0.15.0 §4/D-C: per-key `W-META-LEGACY` for each authored legacy
    // identity key coexisting with an authored `id:`. Reads the AUTHORED
    // snapshot from BEFORE the defaults merge — a manifest-inherited legacy
    // key on an `id:`-carrying document is silent.
    if authored_has_id {
        for key in &authored_legacy {
            diags.push(Diagnostic {
                code: "W-META-LEGACY".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "`{key}` no longer carries scene identity (superseded by `id:`); move it \
                     under `meta:` to keep it searchable (dsl 0.15.0 §4)"
                ),
                span: meta_key_span(meta, key),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }

    typed.profile = get_str(map, "profile");
    typed.uses = get_ref_list(map, "uses");
    typed.extends = get_ref_list(map, "extends");
    typed.plugins = get_sub_map(map, "plugins");
    typed.defs = get_sub_map(map, "defs");
    // Project-authored `enums:`/`entities:`/`relations:` (dsl data-catalog
    // foundation A3; 0.3.0 draft §3.1 kinds, §4 relations, 0.3.0 T4/T5): same
    // YAML-lift discipline as `defs` above, but delegated to
    // `lute_manifest::entities::parse_enums` /
    // `lute_manifest::relations::{parse_entity_kinds, parse_relations,
    // kinds_to_domains}` (the latter owns the `entities:`/`relations:` decl
    // shapes, A2's `Domain` type). `entities:` is folded into `domains` AFTER
    // `enums:` so a same-doc name collision resolves to the `entities:` entry
    // (last-write-wins; not diagnosed here — see `TypedMeta::domains`'s doc
    // comment).
    let project_enums = lute_manifest::entities::parse_enums(
        map.get(yaml_key("enums"))
            .unwrap_or(&serde_yaml::Value::Null),
    );
    typed.domains = project_enums.clone();
    typed.rel_kinds = lute_manifest::relations::parse_entity_kinds(
        map.get(yaml_key("entities"))
            .unwrap_or(&serde_yaml::Value::Null),
    );
    typed.rel_relations = lute_manifest::relations::parse_relations(
        map.get(yaml_key("relations"))
            .unwrap_or(&serde_yaml::Value::Null),
    );
    // Domain projection for the 0.2.2 attr layer (entities win over enums, as before).
    typed
        .domains
        .extend(lute_manifest::relations::kinds_to_domains(
            &typed.rel_kinds.kinds,
        ));

    // `facts:`/`rules:` (dsl 0.3.0 §4/§7.1, T5): each entry is a QUOTED
    // STRING (§4 Quoting — an unquoted `head :- body` misparses as a YAML
    // mapping, not a scalar). A non-sequence, non-null block value is one
    // E-DATALOG-PARSE at the whole meta span; per entry, a non-string value
    // is E-DATALOG-PARSE (same span); a string is parsed via
    // `parse_fact`/`parse_rule` — `Malformed` → E-DATALOG-PARSE, `FunctionTerm`
    // → E-DATALOG-FUNCTION, both at the entry's textual span. A failing entry
    // is simply omitted from `rel_facts`/`rel_rules` (never a partial decl,
    // mirroring D13's downstream-skip discipline).
    match map.get(yaml_key("facts")) {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Sequence(seq)) => {
            for entry in seq {
                let Some(raw) = entry.as_str() else {
                    diags.push(err_at(
                        "E-DATALOG-PARSE",
                        "`facts:` entries must be quoted strings (dsl 0.3.0 §4 Quoting)"
                            .to_string(),
                        span,
                    ));
                    continue;
                };
                match lute_syntax::datalog::parse_fact(raw) {
                    Ok(fact) => typed.rel_facts.push(FactDecl {
                        fact,
                        raw: raw.to_string(),
                        span: meta_key_span(meta, raw),
                    }),
                    Err(lute_syntax::datalog::DatalogError::Malformed { msg, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-PARSE",
                            format!("malformed fact `{raw}`: {msg}"),
                            meta_key_span(meta, raw),
                        ));
                    }
                    Err(lute_syntax::datalog::DatalogError::FunctionTerm { name, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-FUNCTION",
                            format!(
                                "fact `{raw}` uses a function/compound term `{name}(...)`; \
                                 facts admit only ground identifiers/booleans (dsl §7.1)"
                            ),
                            meta_key_span(meta, raw),
                        ));
                    }
                }
            }
        }
        Some(_) => diags.push(err_at(
            "E-DATALOG-PARSE",
            "`facts:` must be a list of quoted fact strings (dsl 0.3.0 §4)".to_string(),
            span,
        )),
    }
    match map.get(yaml_key("rules")) {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Sequence(seq)) => {
            for entry in seq {
                let Some(raw) = entry.as_str() else {
                    diags.push(err_at(
                        "E-DATALOG-PARSE",
                        "`rules:` entries must be quoted strings (dsl 0.3.0 §4 Quoting)"
                            .to_string(),
                        span,
                    ));
                    continue;
                };
                match lute_syntax::datalog::parse_rule(raw) {
                    Ok(rule) => typed.rel_rules.push(RuleDecl {
                        rule,
                        raw: raw.to_string(),
                        span: meta_key_span(meta, raw),
                    }),
                    Err(lute_syntax::datalog::DatalogError::Malformed { msg, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-PARSE",
                            format!("malformed rule `{raw}`: {msg}"),
                            meta_key_span(meta, raw),
                        ));
                    }
                    Err(lute_syntax::datalog::DatalogError::FunctionTerm { name, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-FUNCTION",
                            format!(
                                "rule `{raw}` uses a function/compound term `{name}(...)`; \
                                 rule terms admit only Var/Const/bool (dsl §7.1)"
                            ),
                            meta_key_span(meta, raw),
                        ));
                    }
                }
            }
        }
        Some(_) => diags.push(err_at(
            "E-DATALOG-PARSE",
            "`rules:` must be a list of quoted rule strings (dsl 0.3.0 §4)".to_string(),
            span,
        )),
    }

    // §8.4 identifier alignment: relation names, entity-kind names, `enums:`
    // names, and declared member ids are CEL-facing identifiers — no `-`
    // (E-PATH-IDENT). Directive/attr/asset ids are `Ident` and keep
    // permitting `-`; only these relational-vocabulary positions are
    // CEL-facing (T5).
    let path_ident_diag = |name: &str| -> Option<Diagnostic> {
        if name.contains('-') {
            Some(err_at(
                E_PATH_IDENT,
                format!(
                    "`{name}` has a `-`; relation/entity-kind/enum names and entity ids \
                     are CEL-facing (dsl §8.4)"
                ),
                meta_key_span(meta, name),
            ))
        } else {
            None
        }
    };
    for (name, decl) in &typed.rel_kinds.kinds {
        if let Some(d) = path_ident_diag(name) {
            diags.push(d);
        }
        if let lute_manifest::relations::KindShape::Members(members) = &decl.shape {
            for member in members {
                if let Some(d) = path_ident_diag(member) {
                    diags.push(d);
                }
            }
        }
    }
    for name in typed.rel_relations.relations.keys() {
        if let Some(d) = path_ident_diag(name) {
            diags.push(d);
        }
    }
    for name in project_enums.keys() {
        if let Some(d) = path_ident_diag(name) {
            diags.push(d);
        }
    }

    // Authoritative same-block duplicate detection (0.3.0 T4/T5):
    // `serde_yaml::Mapping` collapses a repeated YAML key before
    // `parse_entity_kinds`/`parse_relations` ever see it, so their own
    // `dups` field is best-effort. This raw-text scan is the authoritative
    // source consumed by the checker (Task 7).
    typed.rel_relations.dups = scan_block_dup_names(&meta.raw_yaml, "relations");
    typed.rel_kinds.dups = scan_block_dup_names(&meta.raw_yaml, "entities");
    // §8.4 identifier alignment: a `defs` name and each of its parameter names
    // are CEL-facing identifiers — no `-` (E-PATH-IDENT). Directive/attr/asset
    // ids are `Ident` and keep permitting `-`; only these def positions are
    // CEL-facing. Imported-schema defs are checked when their own doc is parsed
    // (`MetaKind::Schema`), so both inline and imported defs are covered.
    for (name, def) in &typed.defs {
        if name.contains('-') {
            diags.push(err_at(
                E_PATH_IDENT,
                format!("def name `{name}` has a `-`; CEL-facing names forbid `-` (dsl §8.4)"),
                meta_key_span(meta, name),
            ));
        }
        if let Some(params) = def.get("params").and_then(|p| p.as_mapping()) {
            for pname in params.keys().filter_map(|k| k.as_str()) {
                if pname.contains('-') {
                    diags.push(err_at(
                        E_PATH_IDENT,
                        format!(
                            "def `{name}` parameter `{pname}` has a `-`; CEL-facing names \
                             forbid `-` (dsl §8.4)"
                        ),
                        meta_key_span(meta, pname),
                    ));
                }
            }
        }
    }
    typed.components = get_ref_list(map, "components");
    typed.component = get_str(map, "component");
    let (params, params_malformed) = get_params(map, "params");
    typed.params = params;
    typed.params_malformed = params_malformed;

    // Parse the inline `state:` schema (dsl §9.3).
    if let Some(state_val) = map.get(yaml_key("state")) {
        match state_val {
            serde_yaml::Value::Null => {}
            serde_yaml::Value::Mapping(state_map) => {
                for (path_key, decl_val) in state_map.iter() {
                    let Some(path) = path_key.as_str() else {
                        diags.push(err(
                            "E-STATE-DECL",
                            "state path keys must be strings".to_string(),
                        ));
                        continue;
                    };
                    let Some(namespace) = namespace_of(path) else {
                        diags.push(err(
                            "E-STATE-NAMESPACE",
                            format!("state path `{path}` must begin with scene./run./user./app."),
                        ));
                        continue;
                    };
                    // dsl 0.2.0 §5.2/§9.3: `quest.<id>.state` / `quest.<id>.objectives.<oid>.done`
                    // are RESERVED — implicitly declared and MUST NOT be author-declared,
                    // regardless of whether THIS document owns a matching `<quest id>` (a
                    // shape check, not a doc-scope one; mirrors `state_path_has_hyphen`/
                    // `narrativeTime` below). Skip the decl install entirely so a later read
                    // of `path` resolves via the reserved-path fallback, never a phantom
                    // author-typed decl. The `check.rs:410-427` collision guard (an imported/
                    // sibling-document quest's schema, folded later) still catches the
                    // fold-order case this shape check cannot see.
                    if is_reserved_quest_path(path) {
                        diags.push(err_at(
                            "E-QUEST-RESERVED-DECL",
                            format!(
                                "state path `{path}` collides with an implicitly-declared \
                                 reserved quest field (dsl 0.2.0 §5.2); it must not be \
                                 author-declared in `state:`"
                            ),
                            meta_key_span(meta, path),
                        ));
                        continue;
                    }
                    // §8.4: each state-path segment after the tier is a
                    // `CelIdent`; a `-` there is E-PATH-IDENT. Still record the
                    // decl below so downstream reads don't cascade to E-UNDECLARED.
                    if state_path_has_hyphen(path) {
                        diags.push(err_at(
                            E_PATH_IDENT,
                            format!(
                                "state path `{path}` has a `-` in a segment; CEL-facing \
                                 names forbid `-` (dsl §8.4)"
                            ),
                            meta_key_span(meta, path),
                        ));
                    }
                    match serde_yaml::from_value::<StateDeclRaw>(decl_val.clone()) {
                        Ok(raw) if matches!(raw.ty, Type::NarrativeTime) => {
                            // D11: `narrativeTime` is engine-surfaced only (a
                            // plugin capability's `state_shapes` anchor path,
                            // dsl 0.3.0 §6) — never author-declarable. Skip
                            // the decl entirely so a later read of `path`
                            // falls back to plain `E-UNDECLARED`, never a
                            // phantom narrative-time-typed path.
                            diags.push(err_at(
                                crate::temporal::E_TEMPORAL_ARG,
                                format!(
                                    "state path `{path}` cannot declare `type: narrativeTime`; \
                                     narrative-time paths are engine-surfaced (plugin capability \
                                     state shapes) only — author state is number|bool|string|enum \
                                     (dsl 0.3.0 §6, D11)"
                                ),
                                meta_key_span(meta, path),
                            ));
                        }
                        Ok(raw)
                            if matches!(
                                raw.ty,
                                Type::List(_) | Type::Record(_) | Type::Map { .. }
                            ) =>
                        {
                            // dsl 0.8.0 §4: author `state:` is SCALAR
                            // (`number|bool|string|enum`). `StateDeclRaw`
                            // deserializes the full `Type` union because the
                            // SAME shape carries plugin `state_shapes`
                            // expansions (where a `record`/`map` slot is
                            // legitimate) — so the scalar-only rule the
                            // normative text has always stated is enforced
                            // HERE, at the author-decl site, not in the
                            // deserializer. Mirrors the `narrativeTime` arm
                            // above: emit and SKIP the install, so a later
                            // read falls back to plain `E-UNDECLARED` rather
                            // than resolving through a phantom
                            // collection-typed slot (and, via
                            // `set_op::descend`, inventing field types for
                            // every path under it).
                            diags.push(err_at(
                                E_STATE_COLLECTION,
                                format!(
                                    "state path `{path}` cannot declare a collection type \
                                     (`list`/`record`/`map`); author state is scalar \
                                     (number|bool|string|enum) — model collections as \
                                     `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot"
                                ),
                                meta_key_span(meta, path),
                            ));
                        }
                        Ok(raw) => {
                            typed.state.decls.insert(
                                path.to_string(),
                                StateDecl {
                                    ty: raw.ty,
                                    default: raw.default,
                                    namespace,
                                },
                            );
                        }
                        // dsl 0.5.0 §2.2 / #21 T10.2: this arm forwarded
                        // `serde_yaml`'s own error — "invalid type: unit
                        // variant, expected newtype variant", or "expected
                        // struct StateDeclRaw". Neither "unit variant" nor
                        // "newtype variant" nor `StateDeclRaw` occurs
                        // anywhere in the docs, the language or YAML, and
                        // none of them names the wrong key, the missing key,
                        // the legal shape, or the one nesting level at issue.
                        // The neighbour `E_STATE_COLLECTION` above already
                        // writes its message for the author; so does this
                        // one, through the same `err_at`/`meta_key_span`
                        // pair, so it anchors at the offending key rather
                        // than at the whole meta block.
                        Err(_) => diags.push(err_at(
                            "E-STATE-DECL",
                            state_decl_message(path, decl_val),
                            meta_key_span(meta, path),
                        )),
                    }
                }
            }
            _ => diags.push(err(
                "E-STATE-DECL",
                "`state` must be a mapping of path to declaration".to_string(),
            )),
        }
    }

    (typed, diags)
}

/// `E-STATE-DECL`'s whole message (#21, T10.2): what is wrong with this
/// declaration and what the legal shape is, written from the YAML the author
/// typed rather than from `serde_yaml`'s report of how it failed to
/// deserialize into `StateDeclRaw`.
///
/// The library error cannot be forwarded and cannot be replaced by one fixed
/// sentence either. It says "invalid type: unit variant, expected newtype
/// variant" for BOTH the nesting mistake (`{ type: enum, values: [...] }`) and
/// a bare `type: enum`; "missing field `type`" for a declaration without one;
/// "unknown variant `nonsense`" followed by the entire internal `Type` union
/// for a bad name; and "expected struct StateDeclRaw" — a Rust type name —
/// for a non-mapping. One sentence blaming `values:` would be FALSE for four
/// of those five, which is worse than the serde text it replaces. So the
/// shape is classified here, from the value the author wrote.
///
/// Each arm returns the message WHOLE, as a single `format!` literal, because
/// `scripts/check-doc-snippets.py` pins every quoted diagnostic against the
/// scraped literals in `crates/*/src` and measures that they still reproduce
/// real output. A message assembled from shared `const` fragments resolves to
/// no literal and drops that floor.
fn state_decl_message(path: &str, decl: &serde_yaml::Value) -> String {
    let Some(map) = decl.as_mapping() else {
        return format!(
            "invalid state declaration for `{path}`: a declaration is a mapping with a \
             `type:` key — `{{ type: number, default: 0 }}` — but this is {} \
             (dsl 0.8.0 §4)",
            yaml_shape(decl)
        );
    };
    let Some(ty) = map.get(yaml_key("type")) else {
        return format!(
            "invalid state declaration for `{path}`: the declaration has no `type:` key; \
             author state is scalar, as `{{ type: number, default: 0 }}`, and an `enum` \
             NESTS its members inside `type:`, as `{{ type: {{ enum: [...] }} }}` \
             (dsl 0.8.0 §4)"
        );
    };
    let Some(name) = ty.as_str() else {
        return format!(
            "invalid state declaration for `{path}`: the `type:` value is malformed; author \
             state is scalar, as `{{ type: number, default: 0 }}`, and an `enum` NESTS its \
             members inside `type:`, as `{{ type: {{ enum: [...] }} }}` (dsl 0.8.0 §4)"
        );
    };
    // `type: enum` is the one scalar type that is INCOMPLETE as a bare name —
    // its members nest one level inside `type:`. Hoisting them to a sibling
    // key is the four-word mistake copied straight out of `state-model.md`,
    // and that one nesting level is the whole of what this diagnostic exists
    // to say.
    if name == "enum" {
        return match ["values", "members", "options"]
            .into_iter()
            .find(|k| map.contains_key(yaml_key(k)))
        {
            Some(k) => format!(
                "invalid state declaration for `{path}`: `type: enum` is not complete on its \
                 own, and a top-level `{k}:` is not a declaration key — an `enum` NESTS its \
                 members inside `type:`, as `{{ type: {{ enum: [...] }} }}` (dsl 0.8.0 §4)"
            ),
            None => format!(
                "invalid state declaration for `{path}`: `type: enum` declares no members — \
                 an `enum` NESTS them inside `type:`, as \
                 `{{ type: {{ enum: [teen, adult] }}, default: teen }}` (dsl 0.8.0 §4)"
            ),
        };
    }
    // `list`/`record`/`map` name real types, so "unknown type" would be a lie.
    // They are incomplete as bare names AND barred from author state anyway;
    // the well-formed spelling lands on `E-STATE-COLLECTION`, whose remedy
    // this arm repeats verbatim so the two read as one rule.
    if matches!(name, "list" | "record" | "map") {
        return format!(
            "invalid state declaration for `{path}`: `type: {name}` is an incomplete \
             collection type, and author state cannot declare a collection type in any \
             case; it is scalar (number|bool|string|enum) — model collections as \
             `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot"
        );
    }
    if matches!(name, "bool" | "number" | "string") {
        return format!(
            "invalid state declaration for `{path}`: `type: {name}` is a valid type, so the \
             rest of the declaration is what is malformed; a declaration is \
             `{{ type: {name}, default: ... }}` and nothing else (dsl 0.8.0 §4)"
        );
    }
    format!(
        "invalid state declaration for `{path}`: unknown type `{name}`; author state is \
         scalar — `number`, `bool`, `string`, or `enum`, and an `enum` NESTS its members \
         inside `type:`, as `{{ type: {{ enum: [...] }} }}` (dsl 0.8.0 §4)"
    )
}

/// The YAML shape of `v`, for the "but this is …" half of
/// [`state_decl_message`].
fn yaml_shape(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "an empty value",
        serde_yaml::Value::Bool(_) => "a boolean",
        serde_yaml::Value::Number(_) => "a number",
        serde_yaml::Value::String(_) => "a bare string",
        serde_yaml::Value::Sequence(_) => "a list",
        serde_yaml::Value::Tagged(_) => "a tagged value",
        serde_yaml::Value::Mapping(_) => "a mapping",
    }
}

/// Infer the import-role kind of a KIND-LESS root document from its frontmatter
/// shape (dsl §9.2/§13): a document opened standalone that is actually a Schema
/// or Component fragment. Returns None for a genuine scene missing `kind:`.
pub fn infer_meta_kind_from_shape(meta: &Meta, has_body: bool) -> Option<MetaKind> {
    let value: serde_yaml::Value = serde_yaml::from_str(&meta.raw_yaml).ok()?;
    let map = value.as_mapping()?;
    let has = |k: &str| map.contains_key(yaml_key(k));
    // Only genuinely kind-LESS docs are shape-inferred. A `kind:` that is
    // present but unrecognized must keep resolving through `resolve_doc_kind`
    // so its E-UNKNOWN-KIND diagnostic is never swallowed here.
    if has("kind") {
        return None;
    }
    if COMPONENT_ONLY_KEYS.iter().any(|k| has(k)) {
        return Some(MetaKind::Component);
    }
    if !has_body && (has("state") || has("defs")) {
        return Some(MetaKind::Schema);
    }
    None
}

/// Describe a frontmatter value that has NO [`Literal`] representation, for the
/// "got …" half of an `E-FRONTMATTER-SCHEMA` message. `Literal::from_yaml`
/// bails on `null`, a `!tag`, a non-string mapping key, or any nesting that
/// contains one — none of which any declared plugin key type inhabits, so each
/// is a schema violation rather than a checker failure.
fn unrepresentable_yaml(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Tagged(_) => "a tagged value",
        serde_yaml::Value::Sequence(_) => "a sequence with an unrepresentable element",
        serde_yaml::Value::Mapping(_) => {
            "a mapping with a non-string key or an unrepresentable value"
        }
        // Scalars always convert; defensive.
        _ => "an unrepresentable value",
    }
}

/// Best-effort narrow document span for a meta-side diagnostic pointing at the
/// offending identifier (a hyphenated `defs` name / def param / state path).
///
/// serde gives no per-key spans, so the key is located textually in `raw_yaml`.
///
/// **Key-aware:** the needle is matched only where it is a YAML *mapping key* —
/// at a line start (after optional indent), followed by optional whitespace then
/// `:` — so an identifier that also appears in a comment or scalar value does not
/// steal the span. Mirrors `lute_lsp`'s `find_yaml_key_span` (kept in sync; the
/// LSP copy is private and lute-lsp depends on this crate, not the reverse).
/// Falls back to a naive first occurrence, then the whole-frontmatter span, only
/// when no key line matches. `line`/`column`/`utf16` are left zeroed —
/// [`crate::check`]'s `normalize_spans` recomputes them from the byte offsets.
pub(crate) fn meta_key_span(meta: &Meta, needle: &str) -> Span {
    // `raw_yaml` is usually the frontmatter interior sliced verbatim after the
    // 4-byte `"---\n"` opener (itself included in `meta.span`), so a `raw_yaml`
    // offset maps to the document by adding `meta.span.byte_start + 4`.
    //
    // A bare `.yaml`/`.yml` state schema (data-catalog foundation B2) has NO
    // envelope: `schema_import::read_and_parse` and `lute check <schema.yaml>`
    // both wrap the whole file in a synthetic `Meta` whose `raw_yaml` IS the
    // span. Adding a delimiter that is not there shifted every key span four
    // bytes right — a wrong column, and on a short first line a wrong LINE —
    // so "anchored at the offending key" was only ever true for `.lute` (#21,
    // T10.2). The two cases are told apart exactly: a real frontmatter span
    // covers at least `"---\n"` + `"---"` more bytes than its interior.
    const OPENER_LEN: usize = 4; // "---\n"
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
    let base = meta.span.byte_start + if enveloped { OPENER_LEN } else { 0 };
    let at = |start: usize| Span {
        byte_start: start,
        byte_end: start + needle.len(),
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    // Key-aware scan: the needle at a line start (after indent), then `:`.
    let mut line_start = 0usize;
    for line in meta.raw_yaml.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = line.trim_start().strip_prefix(needle) {
            if rest.trim_start().starts_with(':') {
                return at(base + line_start + indent);
            }
        }
        line_start += line.len();
    }
    // Fallbacks: naive first occurrence, then the whole frontmatter block.
    match meta.raw_yaml.find(needle) {
        Some(idx) => at(base + idx),
        None => meta.span,
    }
}

/// Authoritative same-block duplicate-key scan for `relations:`/`entities:`
/// (dsl 0.3.0 T5): `serde_yaml::Mapping` silently collapses a repeated
/// mapping key before [`lute_manifest::relations::parse_relations`]/
/// `parse_entity_kinds` ever see it, so their own `dups` field is
/// best-effort. This is a dumb, total line scan over the RAW frontmatter
/// text (never a YAML re-parse): find the top-level `<block_key>:` line,
/// then collect every direct child key at the FIRST indent level seen under
/// it (a deeper-nested key, e.g. `members:`/`args:` inside a block-style
/// entry, is ignored — only entries at the entry-list's own indent count).
/// A name repeated at that level is recorded once, at its second
/// occurrence.
fn scan_block_dup_names(raw_yaml: &str, block_key: &str) -> Vec<String> {
    let prefix = format!("{block_key}:");
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    let mut dups = Vec::new();
    let mut in_block = false;
    let mut entry_indent: Option<usize> = None;
    for line in raw_yaml.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_block = trimmed.starts_with(&prefix);
            entry_indent = None;
            continue;
        }
        if !in_block || trimmed.is_empty() {
            continue;
        }
        let want_indent = *entry_indent.get_or_insert(indent);
        if indent != want_indent {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let name = trimmed[..colon].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let count = seen.entry(name.to_string()).or_insert(0);
        *count += 1;
        if *count == 2 {
            dups.push(name.to_string());
        }
    }
    dups
}

/// Recovery helper for [`parse_meta_kind`]'s initial whole-document YAML
/// parse (dsl 0.3.0 T5/T7): `serde_yaml` REJECTS a literal duplicate key
/// anywhere in the document (it does not silently collapse a repeat, contra
/// [`scan_block_dup_names`]'s original assumption), which would otherwise
/// take down the ENTIRE frontmatter lift over a same-block
/// `entities:`/`relations:` dup that this crate has a dedicated diagnostic
/// for (`E-KIND-NAME-CLASH`/`E-RELATION-DUP`). Returns a copy of `raw_yaml`
/// with every occurrence AFTER THE FIRST of a same-indent-level child key
/// under `entities:`/`relations:` commented out (indent preserved, a `#`
/// inserted) — just enough for the retry parse to succeed and lift every
/// OTHER field; mirrors `scan_block_dup_names`'s exact block/indent-tracking
/// so both agree on which key is "the" duplicate.
fn sanitize_dup_block_keys(raw_yaml: &str) -> String {
    const BLOCKS: [&str; 2] = ["entities", "relations"];
    let mut seen: BTreeMap<(&str, String), u32> = BTreeMap::new();
    let mut in_block: Option<&str> = None;
    let mut entry_indent: Option<usize> = None;
    let mut out = String::with_capacity(raw_yaml.len());
    for line in raw_yaml.split_inclusive('\n') {
        let body_len = line.trim_end_matches(['\n', '\r']).len();
        let (body, nl) = line.split_at(body_len);
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if indent == 0 {
            in_block = BLOCKS
                .iter()
                .copied()
                .find(|k| trimmed.starts_with(&format!("{k}:")));
            entry_indent = None;
            out.push_str(line);
            continue;
        }
        let Some(block) = in_block else {
            out.push_str(line);
            continue;
        };
        if trimmed.is_empty() {
            out.push_str(line);
            continue;
        }
        let want_indent = *entry_indent.get_or_insert(indent);
        if indent != want_indent {
            out.push_str(line);
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            out.push_str(line);
            continue;
        };
        let name = trimmed[..colon].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            out.push_str(line);
            continue;
        }
        let count = seen.entry((block, name.to_string())).or_insert(0);
        *count += 1;
        if *count >= 2 {
            out.push_str(&body[..indent]);
            out.push('#');
            out.push_str(trimmed);
            out.push_str(nl);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Raw `state:` entry (dsl §9.3): `{ type, default? }`. `Type` reuses the
/// manifest's manual serde (inline `{ enum: [...] }` etc. work).
#[derive(serde::Deserialize)]
struct StateDeclRaw {
    #[serde(rename = "type")]
    ty: Type,
    #[serde(default)]
    default: Option<Literal>,
}

fn yaml_key(k: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(k.to_string())
}

fn map_prefix(path: &str) -> &str {
    path.split_once('.').map_or(path, |(head, _)| head)
}

pub(crate) fn namespace_of(path: &str) -> Option<Namespace> {
    match map_prefix(path) {
        "scene" => Some(Namespace::Scene),
        "run" => Some(Namespace::Run),
        "user" => Some(Namespace::User),
        "app" => Some(Namespace::App),
        "quest" => Some(Namespace::Quest),
        _ => None,
    }
}

fn get_str(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(yaml_key(key))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn get_i64(map: &serde_yaml::Mapping, key: &str) -> Option<i64> {
    map.get(yaml_key(key)).and_then(|v| v.as_i64())
}

/// `uses`/`extends` (dsl §9.2) may each be a single ref or a list of refs;
/// normalize to a Vec.
fn get_ref_list(map: &serde_yaml::Mapping, key: &str) -> Vec<String> {
    match map.get(yaml_key(key)) {
        Some(serde_yaml::Value::String(s)) => vec![s.clone()],
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn get_sub_map(map: &serde_yaml::Mapping, key: &str) -> BTreeMap<String, serde_yaml::Value> {
    match map.get(yaml_key(key)) {
        Some(serde_yaml::Value::Mapping(m)) => m
            .iter()
            .filter_map(|(k, v)| k.as_str().map(|k| (k.to_string(), v.clone())))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// A component file's `params:` (dsl §13) is a YAML MAPPING (`{ who: <type> }`)
/// read in SOURCE order (`serde_yaml::Mapping` is insertion-ordered), the same
/// spelling as a def's `params:` (dsl §8.1). Each value deserializes to a
/// manifest [`Type`] via the same serde path `Type` uses.
///
/// dsl 0.10.0 §12.4: the LONG form `{ type: X }` is accepted as a synonym for
/// `X`, for every spelling `X` the shared deserializer admits — that is how
/// `state:` and `defs:` entries are written, and `components-and-extends.md`
/// says a component param is typed exactly like a def param. The wrapper takes
/// no other key: `{ type: string, default: … }` stays malformed, because a
/// param default would need a rule for how it interacts with
/// `E-COMPONENT-ARG`, which the issue explicitly does not ask for.
///
/// Returns the valid `(name, type)` pairs plus a `malformed` flag that is `true`
/// when `params:` is PRESENT but any part of it is invalid — not a mapping, a
/// non-string key, or a value that fails `Type` deserialization. The caller
/// (component resolver) turns a set flag into `E-COMPONENT-PARSE` so a malformed
/// signature is never silently shrunk. Absent `params:` ⇒ `(empty, false)`.
/// Never panics.
fn get_params(map: &serde_yaml::Mapping, key: &str) -> (Vec<DefParam>, bool) {
    let Some(raw) = map.get(yaml_key(key)) else {
        return (Vec::new(), false); // absent — fine (no params)
    };
    let Some(pm) = raw.as_mapping() else {
        return (Vec::new(), true); // present but not a mapping — malformed
    };
    let mut params = Vec::new();
    let mut malformed = false;
    for (k, tv) in pm.iter() {
        let Some(name) = k.as_str() else {
            malformed = true; // non-string key
            continue;
        };
        let tv = unwrap_long_form(tv).unwrap_or(tv);
        match serde_yaml::from_value::<Type>(tv.clone()) {
            Ok(ty) => params.push(DefParam {
                name: name.to_string(),
                ty,
            }),
            Err(_) => malformed = true, // value is not a valid Type
        }
    }
    (params, malformed)
}

/// dsl 0.10.0 §12.4: unwrap the long form `{ type: X }` to `X`. `None` for
/// anything else, including a `type:` wrapper carrying a second key — the
/// caller then hands the ORIGINAL value to the `Type` deserializer, which
/// rejects it, so an unwrap miss is never a silent acceptance.
///
/// There is no ambiguity to resolve: `Type` has no `type` variant
/// (`lute-manifest/src/types.rs`), so `{ type: … }` is not already a legal
/// spelling and this cannot shadow one.
fn unwrap_long_form(tv: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
    let m = tv.as_mapping()?;
    if m.len() != 1 {
        return None;
    }
    m.get(yaml_key("type"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_meta_str(yaml: &str) -> (TypedMeta, Vec<Diagnostic>) {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta(&meta, &CapabilitySnapshot::default())
    }

    // ── 0.15.0 §2/§3/§4: authored scene `id:`, `meta:` block, legacy demotion.

    #[test]
    fn authored_id_relieves_required_triad() {
        let (m, diags) = parse_meta_str("id: anseo.s01ep01\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "authored `id:` must satisfy the required-key rule (§4): {diags:?}"
        );
        assert_eq!(m.id.as_deref(), Some("anseo.s01ep01"));
        assert_eq!(canonical_scene_key(&m).as_deref(), Some("anseo.s01ep01"));
    }

    #[test]
    fn no_id_keeps_required_triad() {
        let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-MISSING"),
            "without `id:` the legacy required-key rule still fires (§4): {diags:?}"
        );
    }

    #[test]
    fn malformed_id_is_e_meta_id() {
        let (_m, diags) = parse_meta_str("id: \"has space\"\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-ID"),
            "an `id:` outside `[A-Za-z0-9_.-]+` must draw E-META-ID (§2): {diags:?}"
        );
    }

    #[test]
    fn empty_id_is_e_meta_id() {
        // §2: non-empty is part of the charset gate.
        let (m, diags) = parse_meta_str("id: \"\"\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-ID"),
            "an empty `id:` must draw E-META-ID (§2): {diags:?}"
        );
        assert!(m.id.is_none(), "a rejected `id:` value is not lifted (§2)");
    }

    #[test]
    fn meta_block_lifts_scalars_and_lists() {
        let (m, diags) =
            parse_meta_str("id: x\nmeta:\n  arc: main\n  season: 1\n  tags: [harbor, night]\n");
        assert!(
            diags.is_empty(),
            "clean scalars/lists must not diagnose: {diags:?}"
        );
        assert_eq!(m.meta_block.get("arc"), Some(&serde_json::json!("main")));
        assert_eq!(m.meta_block.get("season"), Some(&serde_json::json!(1)));
        assert_eq!(
            m.meta_block.get("tags"),
            Some(&serde_json::json!(["harbor", "night"]))
        );
    }

    #[test]
    fn nested_meta_value_is_e_meta_value() {
        let (_m, diags) = parse_meta_str("id: x\nmeta:\n  nested: { a: 1 }\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-VALUE"),
            "a nested mapping under meta: must draw E-META-VALUE (§3): {diags:?}"
        );
    }

    #[test]
    fn non_mapping_meta_is_e_meta_value() {
        let (m, diags) = parse_meta_str("id: x\nmeta: not-a-mapping\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-VALUE"),
            "a non-mapping `meta:` must draw one E-META-VALUE (§3): {diags:?}"
        );
        assert!(m.meta_block.is_empty());
    }

    #[test]
    fn authored_legacy_beside_id_warns_per_key() {
        let (_m, diags) = parse_meta_str("id: x\ncharacter: bianca\nseason: 1\nepisode: 1\n");
        let n = diags.iter().filter(|d| d.code == "W-META-LEGACY").count();
        assert_eq!(
            n, 3,
            "one W-META-LEGACY per authored legacy key (§4): {diags:?}"
        );
        assert!(diags
            .iter()
            .all(|d| d.code != "W-META-LEGACY" || d.severity == Severity::Warning));
    }

    #[test]
    fn authored_episode_id_beside_id_warns_too() {
        let (_m, diags) = parse_meta_str("id: x\nepisodeId: ep03\n");
        let n = diags.iter().filter(|d| d.code == "W-META-LEGACY").count();
        assert_eq!(
            n, 1,
            "authored `episodeId:` alongside `id:` warns (§2/§4): {diags:?}"
        );
    }

    #[test]
    fn defaults_inherited_legacy_beside_id_is_silent() {
        // §4/D-C: the pass reads the AUTHORED map. A manifest-inherited
        // `character:`/`season:`/`episode:` on an `id:`-carrying document
        // does not draw W-META-LEGACY (the document author wrote nothing to
        // move); required-key/unknown-key enforcement still runs over the
        // merged view (unchanged from 0.10.0 §6.2).
        let defaults: lute_manifest::project::MetaDefaults = [
            (
                "character".to_string(),
                serde_yaml::Value::String("bianca".into()),
            ),
            ("season".to_string(), serde_yaml::Value::Number(1.into())),
            ("episode".to_string(), serde_yaml::Value::Number(2.into())),
        ]
        .into_iter()
        .collect();
        let yaml = "id: x\n";
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        let (_m, diags) = parse_meta_kind_with_defaults(
            &meta,
            &CapabilitySnapshot::default(),
            MetaKind::Scene,
            &defaults,
        );
        assert!(
            !diags.iter().any(|d| d.code == "W-META-LEGACY"),
            "defaults-inherited legacy keys must be silent (§4 D-C): {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "authored `id:` satisfies the required-key rule via the merged map: {diags:?}"
        );
    }

    #[test]
    fn derived_fallback_unchanged() {
        let (m, diags) = parse_meta_str("character: x\nseason: 1\nepisode: 2\n");
        assert!(
            diags.is_empty(),
            "clean legacy scene must not diagnose: {diags:?}"
        );
        assert_eq!(canonical_scene_key(&m).as_deref(), Some("x.s01ep02"));
    }

    #[test]
    fn meta_key_is_legal_on_a_quest() {
        let (m, diags) = parse_kind_str("kind: quest\nmeta:\n  arc: main\n", MetaKind::Quest);
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`meta:` is legal on quest roots too (§3): {diags:?}"
        );
        assert_eq!(m.meta_block.get("arc"), Some(&serde_json::json!("main")));
    }

    /// 0.10.0 §12.4: `{ type: X }` is a synonym for `X`, for every spelling the
    /// shared `Type` deserializer admits — the same long form `state:`, `defs:`
    /// and a domain declaration already use.
    #[test]
    fn component_params_accept_the_long_form() {
        let (meta, _d) = parse_meta_str(
            "component: greet\nparams:\n  a: { type: string }\n  \
             b: { type: { enum: [steady, rising] } }\n  \
             c: { type: { providerRef: cast } }\n  d: number\n",
        );
        assert!(!meta.params_malformed, "the long form is legal (§12.4)");
        assert_eq!(
            meta.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"],
            "source order is preserved and no param is silently dropped"
        );
        assert_eq!(meta.params[0].ty, Type::Str);
        assert_eq!(
            meta.params[1].ty,
            Type::Enum(vec!["steady".to_string(), "rising".to_string()])
        );
        assert_eq!(meta.params[2].ty, Type::ProviderRef("cast".to_string()));
        assert_eq!(meta.params[3].ty, Type::Number);
    }

    /// §12.4: the wrapper takes NO other key. A `default:` beside `type:` stays
    /// malformed — param defaults are not added, because a default would need a
    /// rule for how it interacts with `E-COMPONENT-ARG`, which is a separate
    /// design the issue does not ask for.
    #[test]
    fn component_params_reject_a_long_form_with_extra_keys() {
        let (meta, _d) = parse_meta_str(
            "component: greet\nparams:\n  a: { type: string, default: \"steady\" }\n",
        );
        assert!(
            meta.params_malformed,
            "`{{ type: X, default: … }}` is still E-COMPONENT-PARSE (§12.4)"
        );
    }

    /// §12.4 relaxes `type`, not "any wrapper".
    #[test]
    fn component_params_reject_an_unknown_wrapper_key() {
        let (meta, _d) = parse_meta_str("component: greet\nparams:\n  a: { kind: string }\n");
        assert!(
            meta.params_malformed,
            "`{{ kind: X }}` is not a Type spelling"
        );
    }

    #[test]
    fn canonical_episode_id_uses_authored_value_verbatim() {
        assert_eq!(canonical_episode_id(1, 2, Some("custom-ep")), "custom-ep");
    }

    #[test]
    fn canonical_episode_id_empty_string_falls_back_to_default() {
        assert_eq!(canonical_episode_id(1, 2, Some("")), "s01ep02");
    }

    #[test]
    fn canonical_episode_id_absent_falls_back_to_default() {
        assert_eq!(canonical_episode_id(1, 2, None), "s01ep02");
    }

    #[test]
    fn parses_state_decls_with_namespace() {
        let yaml = "character: bianca\nseason: 1\nepisode: 2\npov: fixer\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n";
        let (meta, diags) = parse_meta_str(yaml);
        assert!(diags.is_empty(), "{diags:?}");
        let d = meta.state.decls.get("scene.affect.bianca").unwrap();
        assert_eq!(d.namespace, Namespace::Scene);
    }

    #[test]
    fn missing_required_meta_key_errors() {
        let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n"); // no character
        assert!(diags.iter().any(|d| d.code == "E-META-MISSING"));
    }

    #[test]
    fn codes_locked_is_a_known_universal_key() {
        // dsl §12 publish guard (`lute tag --force`): `codesLocked:` is core
        // frontmatter on every document kind — never E-META-UNKNOWN-KEY.
        let (_m, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\ncodesLocked: true\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "{diags:?}"
        );
    }

    #[test]
    fn app_write_is_flagged_readonly_at_schema_level() {
        // scene may declare; app.* declared read-only downstream (checked in Task 4.5)
        let (meta, _d) = parse_meta_str(
            "character: x\nseason: 1\nepisode: 2\nstate:\n  app.lang: { type: string }\n",
        );
        assert_eq!(
            meta.state.decls.get("app.lang").unwrap().namespace,
            Namespace::App
        );
    }

    #[test]
    fn parses_extends_scalar() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\nextends: base.lute\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`extends` must be a known core key; got {diags:?}"
        );
        assert_eq!(meta.extends, vec!["base.lute".to_string()]);
    }

    #[test]
    fn parses_extends_list() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\nextends: [a.lute, b.lute]\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`extends` list must parse; got {diags:?}"
        );
        assert_eq!(
            meta.extends,
            vec!["a.lute".to_string(), "b.lute".to_string()]
        );
    }

    fn parse_kind_str(yaml: &str, kind: MetaKind) -> (TypedMeta, Vec<Diagnostic>) {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta_kind(&meta, &CapabilitySnapshot::default(), kind)
    }

    #[test]
    fn scene_components_list_parses_as_known_key() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\ncomponents: [greet.lute]\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`components` must be a known core key; got {diags:?}"
        );
        assert_eq!(meta.components, vec!["greet.lute".to_string()]);
    }

    #[test]
    fn component_mode_lifts_component_and_params() {
        let (meta, diags) = parse_kind_str(
            "component: greet\nparams:\n  who: { providerRef: cast }\n",
            MetaKind::Component,
        );
        assert!(
            diags.is_empty(),
            "component frontmatter must be clean; got {diags:?}"
        );
        assert_eq!(meta.component.as_deref(), Some("greet"));
        assert_eq!(meta.params.len(), 1);
        assert_eq!(meta.params[0].name, "who");
        assert_eq!(meta.params[0].ty, Type::ProviderRef("cast".to_string()));
    }

    #[test]
    fn component_mode_skips_scene_required_keys() {
        // A component file carries no character/season/episode; Component mode
        // must not flag E-META-MISSING (like Schema).
        let (_m, diags) = parse_kind_str("component: greet\n", MetaKind::Component);
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "Component mode must skip scene-required keys; got {diags:?}"
        );
    }

    #[test]
    fn component_params_preserve_source_order() {
        let (meta, _d) = parse_kind_str(
            "component: c\nparams:\n  first: string\n  second: number\n  third: bool\n",
            MetaKind::Component,
        );
        let names: Vec<&str> = meta.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn scene_component_and_params_keys_are_unknown() {
        // dsl §13: `component:`/`params:` are COMPONENT-FILE-ONLY frontmatter keys.
        // A SCENE (MetaKind::Scene) declaring them must hit the unknown-key
        // diagnostic, not be silently accepted.
        let (_m, diags) = parse_meta_str(
            "character: x\nseason: 1\nepisode: 1\ncomponent: greet\nparams:\n  who: string\n",
        );
        let unknown: Vec<&str> = diags
            .iter()
            .filter(|d| d.code == "E-META-UNKNOWN-KEY")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            unknown.iter().any(|m| m.contains("`component`")),
            "scene `component:` must be an unknown top-level key; got {diags:?}"
        );
        assert!(
            unknown.iter().any(|m| m.contains("`params`")),
            "scene `params:` must be an unknown top-level key; got {diags:?}"
        );

        // No regression: Component mode still accepts both cleanly.
        let (cm, cdiags) = parse_kind_str(
            "component: greet\nparams:\n  who: string\n",
            MetaKind::Component,
        );
        assert!(
            !cdiags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "Component mode must accept component/params; got {cdiags:?}"
        );
        assert_eq!(cm.component.as_deref(), Some("greet"));

        // Schema mode (imported via `uses:`) likewise rejects them.
        let (_sm, sdiags) = parse_kind_str("component: greet\n", MetaKind::Schema);
        assert!(
            sdiags
                .iter()
                .any(|d| d.code == "E-META-UNKNOWN-KEY" && d.message.contains("`component`")),
            "Schema mode must reject `component:`; got {sdiags:?}"
        );
    }

    /// A snapshot whose active plugins declare three typed frontmatter keys
    /// (plugin Appendix C2).
    fn snap_with_frontmatter() -> CapabilitySnapshot {
        let mut snap = CapabilitySnapshot::default();
        snap.frontmatter
            .insert("questTier".into(), Type::Enum(vec!["a".into(), "b".into()]));
        snap.frontmatter.insert("difficulty".into(), Type::Number);
        snap.frontmatter
            .insert("tags".into(), Type::List(Box::new(crate::meta::Type::Str)));
        snap
    }

    fn parse_with_snapshot(yaml: &str) -> Vec<Diagnostic> {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta(&meta, &snap_with_frontmatter()).1
    }

    const SCENE_HEAD: &str = "character: bianca\nseason: 1\nepisode: 2\n";

    #[test]
    fn plugin_frontmatter_key_with_wrong_typed_value_is_an_error() {
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}difficulty: hard\n"));
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .collect();
        assert_eq!(hits.len(), 1, "{diags:?}");
        assert_eq!(
            hits[0].message,
            "frontmatter key `difficulty` expects number (declared by an active plugin), got \"hard\""
        );
        assert_eq!(hits[0].severity, Severity::Error);
        // The span points at the offending KEY, not the whole frontmatter.
        // `parse_with_snapshot` wraps the YAML in a synthetic whole-file
        // `Meta` with no `---` envelope, so the offset is direct: until #21
        // T10.2 `meta_key_span` added the four bytes of an opener that is not
        // there, and this assertion subtracted them back out.
        assert_eq!(
            &format!("{SCENE_HEAD}difficulty: hard\n")
                [hits[0].span.byte_start..hits[0].span.byte_end],
            "difficulty"
        );
        // A schema violation is NOT also an unknown key.
        assert!(!diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"));
    }

    #[test]
    fn plugin_frontmatter_key_with_a_conforming_value_passes() {
        let diags = parse_with_snapshot(&format!(
            "{SCENE_HEAD}difficulty: 3\nquestTier: b\ntags: [rhythm, timing]\n"
        ));
        assert!(
            !diags
                .iter()
                .any(|d| d.code == "E-FRONTMATTER-SCHEMA" || d.code == "E-META-UNKNOWN-KEY"),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_and_list_frontmatter_violations_report_the_declared_type() {
        let diags =
            parse_with_snapshot(&format!("{SCENE_HEAD}questTier: zzz\ntags: [rhythm, 4]\n"));
        let msgs: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            msgs,
            vec![
                "frontmatter key `questTier` expects enum(a|b) (declared by an active plugin), got \"zzz\"",
                "frontmatter key `tags` expects list<string> (declared by an active plugin), got [\"rhythm\", 4]",
            ],
            "{diags:?}"
        );
    }

    #[test]
    fn unrepresentable_frontmatter_value_is_the_same_error_not_a_panic() {
        // `null` has no `Literal` form at all — a schema violation, never a panic.
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}difficulty:\n"));
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .collect();
        assert_eq!(hits.len(), 1, "{diags:?}");
        assert_eq!(
            hits[0].message,
            "frontmatter key `difficulty` expects number (declared by an active plugin), got null"
        );
    }

    #[test]
    fn core_keys_are_not_routed_through_the_plugin_schema_check() {
        // `title`/`season` are core keys with bespoke handling; a plugin schema
        // check must never fire on them, and a genuinely unknown key still hits
        // `E-META-UNKNOWN-KEY` rather than the schema code.
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}title: A Night Out\nnope: 1\n"));
        assert!(
            !diags.iter().any(|d| d.code == "E-FRONTMATTER-SCHEMA"),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "E-META-UNKNOWN-KEY" && d.message.contains("`nope`")),
            "{diags:?}"
        );
    }
}
