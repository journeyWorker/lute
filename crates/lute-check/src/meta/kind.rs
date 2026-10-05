use super::*;
/// Frontmatter keys valid in EVERY root document kind (dsl 0.2.0 §6.1): the
/// kind-agnostic core keys. `kind:` itself is handled separately (valid only
/// for a root kind — Scene/Quest — never Schema/Component, dsl 0.2.0 §3.1), so
/// it is NOT in this list; the unknown-key loop below tests it independently.
/// `components:` (the import list, dsl §13) is valid everywhere; `component:`/
/// `params:` are NOT — see [`COMPONENT_ONLY_KEYS`].
pub(super) const UNIVERSAL_KEYS: &[&str] = &[
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
/// dsl 0.2.0 §3.1/§6.1, dsl 0.15.0 §2, dsl 0.21.0 §3.1): the scene identity
/// triad plus the scene-only extras plus the authored canonical key `id:`
/// plus the beat keys ([`crate::beats::BEAT_KEYS`]). A Quest document
/// declaring any of these but `id:` is `E-META-UNKNOWN-KEY`.
pub const SCENE_KEYS: &[&str] = &[
    "id",
    "character",
    "season",
    "episode",
    "episodeId",
    "pov",
    "after",
    "on",
    "target",
    "when",
    "priority",
    "once",
    "also",
    "share",
    "spentBy",
    "for",
    "advances",
];

/// Frontmatter keys valid ONLY in a `MetaKind::Quest` document: the optional
/// document id (dsl 0.19.0 §2.1, D-J).
const QUEST_KEYS: &[&str] = &["id"];

/// Frontmatter keys valid ONLY in a `MetaKind::Lore` document: the optional
/// document id and the document-level `series:` (dsl 0.19.0 §2.1, D-J/D-K),
/// and (dsl 0.28.0, T3-66) `pov:` — a lore document's `<beat>` bundles are
/// scene bodies, so it names their point of view as a scene's does.
const LORE_KEYS: &[&str] = &["id", "series", "pov"];

/// Frontmatter keys valid ONLY in a `MetaKind::Schema` document: the
/// declared cast (dsl 0.23.0 §7).
const SCHEMA_KEYS: &[&str] = &["cast", "clock", "terminal", "seasons"];

/// The kind-specific core keys of `kind` beyond [`UNIVERSAL_KEYS`] and the
/// root-wide `kind:`/`extra:` — empty for the import-role kinds.
pub(super) fn kind_keys(kind: MetaKind) -> &'static [&'static str] {
    match kind {
        MetaKind::Scene => SCENE_KEYS,
        MetaKind::Quest => QUEST_KEYS,
        MetaKind::Lore => LORE_KEYS,
        MetaKind::Schema => SCHEMA_KEYS,
        MetaKind::Component => &[],
    }
}

/// Frontmatter keys that are valid ONLY in a component file (dsl §13): the
/// component's own name (`component:`), its parameter signature (`params:`),
/// and (dsl 0.24.0 §4) whether its body writes state (`effects:`).
/// In a scene or schema doc these are unknown top-level keys.
pub(super) const COMPONENT_ONLY_KEYS: &[&str] = &["component", "params", "effects", "beat"];

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
/// it either (dsl 0.19.0 D-J: the same holds for a quest/lore document id).
/// The other quest/lore kind key, lore `series:` (D-K), is per-document too,
/// so only the SCENE kind keys are ever defaultable. dsl 0.21.0 §3.1: the
/// beat keys are one scene's own declaration — never defaultable either.
/// `extra:` is legal on every ROOT kind — Scene, Quest, and Lore all carry
/// it (§3).
pub fn default_key_legal_on(key: &str, kind: MetaKind) -> bool {
    if key == "id" || crate::beats::BEAT_KEYS.contains(&key) {
        return false;
    }
    if key == "extra" {
        return kind.is_root();
    }
    UNIVERSAL_KEYS.contains(&key)
        || (key == "kind" && kind.is_root())
        || (kind == MetaKind::Scene && SCENE_KEYS.contains(&key))
}

/// dsl 0.26.0 §2.4: `defaults.questTier` is the `tier=` of every `<quest>`
/// that writes none (anchored at its id). An authored `tier=` wins. Applied
/// right after parsing, so every pass and the compiled artifact see one tier.
pub fn apply_quest_tier_default(
    doc: &mut lute_syntax::ast::Document,
    defaults: &lute_manifest::project::MetaDefaults,
) {
    let Some(tier) = defaults.get("questTier").and_then(|v| v.as_str()) else {
        return;
    };
    for q in doc.quests.iter_mut().filter(|q| q.tier.is_none()) {
        q.tier = Some((tier.to_string(), q.id_span));
    }
}

/// The advisory tail on `E-META-UNKNOWN-KEY` (dsl 0.5.0 §2.2's "did you mean",
/// generalised): empty when nothing is close, because a wrong suggestion is
/// worse than none.
///
/// `after` / `follows` is the one special case (0.10.0 backlog #11, T9.1).
/// `after` is a CORE key on the sibling kind and `follows` a legal ATTRIBUTE
/// in this one, so the generic edit-distance suggestion has no candidate to
/// land on and the author is told only that the key is unknown — while
/// `follows=` is legal two lines below in the same file. Name the attribute
/// form instead.
///
/// The candidate set mirrors the unknown-key loop's own `core_key` predicate
/// exactly, so the suggestion can never name a key that would itself be
/// rejected on this kind. Slice order is source order, which makes
/// [`lute_manifest::suggest::nearest`]'s first-wins tie-break deterministic.
pub(super) fn unknown_key_hint(key: &str, kind: MetaKind, component_key_allowed: bool) -> String {
    if matches!(key, "after" | "follows") && kind == MetaKind::Quest {
        return " — a quest's graph edge is the `follows=` ATTRIBUTE on its `<quest>` element, \
                not a frontmatter key; it does not gate the quest (to wait, write \
                `start=\"visited('<scene id>')\"`)"
            .to_string();
    }
    if crate::beats::BEAT_KEYS.contains(&key) && matches!(kind, MetaKind::Quest | MetaKind::Lore) {
        return if kind == MetaKind::Lore && matches!(key, "on" | "priority" | "target" | "when") {
            format!(
                " — only a scene's frontmatter declares a beat; a lore entry answers an occasion \
                 with its own `<entry {key}=…>` attribute"
            )
        } else {
            " — only a scene's frontmatter declares a beat".to_string()
        };
    }
    if let Some(owner) = owning_layer(key, kind) {
        return owner;
    }
    let is_root = kind.is_root();
    let component_keys: &[&str] = if component_key_allowed {
        COMPONENT_ONLY_KEYS
    } else {
        &[]
    };
    let root_extras: &[&str] = if is_root { &["kind", "extra"] } else { &[] };
    let candidates = UNIVERSAL_KEYS
        .iter()
        .copied()
        .chain(root_extras.iter().copied())
        .chain(kind_keys(kind).iter().copied())
        .chain(component_keys.iter().copied());
    match lute_manifest::suggest::nearest(key, candidates, 2) {
        Some(sugg) => format!(" — did you mean `{sugg}`?"),
        None => String::new(),
    }
}

/// T3-9/T3-10: the layer that owns a key written in the wrong one, as the
/// tail of `E-META-UNKNOWN-KEY` — the key is real, just not here. `None`
/// for a key no other layer declares either (the did-you-mean then runs).
fn owning_layer(key: &str, kind: MetaKind) -> Option<String> {
    let beat_doc = matches!(kind, MetaKind::Scene);
    Some(match key {
        // T3-9: an occasion is named `on` in a document, `occasion` in the
        // manifest-free surfaces (play steps, the CLI).
        "occasion" | "event" if beat_doc => {
            " — a scene names the occasion it answers with `on:`, e.g. `on: <occasion>` \
             (`occasion:` is a play step's key)"
                .to_string()
        }
        "occasion" | "event" if kind == MetaKind::Lore => {
            " — a lore entry answers an occasion with its own `<entry on=…>` attribute".to_string()
        }
        "tier" | "start" | "fail" | "rearm" | "repeatable" if kind == MetaKind::Quest => format!(
            " — `{key}` is an attribute of the `<quest>` element (`<quest id=\"…\" {key}=\"…\">`), \
             not a frontmatter key{}",
            if key == "tier" {
                "; a project-wide default is `defaults: { questTier: … }` in lute.project.yaml"
            } else {
                ""
            }
        ),
        "rearm" | "tier" => format!(
            " — `{key}=` is an attribute of a `<quest>`; a beat comes back with `once:` (how long \
             it stays spent) or `spentBy:`"
        ),
        "raisedWhen" | "select" | "outsideRun" | "judge" => format!(
            " — `{key}:` belongs to an occasion's declaration (the `occasions:` of a plugin), not \
             to a document"
        ),
        "sequence" | "chapters" => " — chapters are declared once for the project, in \
             lute.project.yaml: `chapters: [{ on: <occasion>, scenes: [<scene id>, …] }]`"
            .to_string(),
        "questTier" => " — `questTier` is a project default, `defaults: { questTier: … }` in \
             lute.project.yaml; one quest sets `<quest tier=\"…\">`"
            .to_string(),
        "terminal" | "clock" | "seasons" | "cast" if kind != MetaKind::Schema => format!(
            " — `{key}:` belongs in a schema (a `.schema.yaml` the documents import through \
             `uses:`)"
        ),
        "use" => " — a schema is imported with `uses:`; a template is applied by a bundle \
                   beat's `<beat use=\"…\">` attribute"
            .to_string(),
        "tags" | "tag" => {
            " — keep free-form data under `extra:`, e.g. `extra: { tags: [...] }`".to_string()
        }
        _ => return None,
    })
}

pub(super) const REQUIRED_KEYS: &[&str] = &["character", "season", "episode"];

/// Which document kind's frontmatter is being parsed. A `Schema` doc (imported
/// via `uses:`, dsl §9.2) and a `Component` doc (imported via `components:`,
/// dsl §13) are NOT scenes — neither carries the required character/season/
/// episode keys. `Quest` (dsl 0.2.0 §3.1, §6.1) is a second ROOT kind: like
/// Scene it carries `kind:`, but (like Schema/Component) requires no keys and
/// additionally rejects the scene-only [`SCENE_KEYS`]. `Lore` (dsl 0.19.0 §2)
/// is the third ROOT kind and accepts exactly the quest-document keys.
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
    /// The lore kind (dsl 0.19.0 §2): a third ROOT document kind whose
    /// frontmatter keys are the quest document's — no required keys, rejects
    /// [`SCENE_KEYS`].
    Lore,
}

impl MetaKind {
    /// `true` for a ROOT document kind — one that carries `kind:` and the
    /// `extra:` block (Scene, Quest, Lore) — never an import-role fragment.
    pub fn is_root(self) -> bool {
        matches!(self, MetaKind::Scene | MetaKind::Quest | MetaKind::Lore)
    }
}

/// A ROOT document's domain kind (dsl 0.2.0 §3.1, dsl 0.19.0 §2): the
/// frontmatter `kind:` discriminator. Import-role docs
/// (`MetaKind::Schema`/`MetaKind::Component`) never carry `kind:` and are never
/// a `DocKind` — only a Scene, Quest, or Lore root document resolves one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    Scene,
    Quest,
    /// `kind: lore` (dsl 0.19.0 §2): a document whose top level is one or more
    /// `<entry>` declarations — content the engine looks up, not plays.
    Lore,
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
/// The document kind an AUTHORED frontmatter declares — [`resolve_doc_kind`]'s
/// verdict without its diagnostics, over an already-parsed frontmatter (`None`
/// when it did not parse, is not a mapping, or names no known kind).
pub fn authored_doc_kind(yaml: Option<&serde_yaml::Value>) -> Option<DocKind> {
    match yaml?.as_mapping()?.get(yaml_key("kind"))?.as_str()? {
        "scene" => Some(DocKind::Scene),
        "quest" => Some(DocKind::Quest),
        "lore" => Some(DocKind::Lore),
        _ => None,
    }
}

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
        evidence: None,
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
                 declare `kind: scene`, `kind: quest`, or `kind: lore` (dsl 0.2.0 §3.1, \
                 dsl 0.19.0 §2)"
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
                "lore" => (Some(DocKind::Lore), Vec::new()),
                other => (
                    None,
                    vec![err(
                        E_UNKNOWN_KIND,
                        format!(
                            "unknown document kind `{other}`{}; expected `scene`, `quest`, or \
                             `lore`",
                            lute_manifest::suggest::did_you_mean(other, ["scene", "quest", "lore"])
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
        Some("lore") => (Some(DocKind::Lore), Vec::new()),
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
