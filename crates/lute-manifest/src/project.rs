//! `lute.project.yaml` loader + the single shared document resolver (plugin §11).
//!
//! This module is the one place both the CLI and the LSP resolve a scene's
//! capability surface, so they build byte-identical snapshots — the
//! no-divergence linchpin (plugin §11). `load_project` reads a project's
//! `profiles` graph + `defaultProfile` + optional `pluginsDir` into a
//! [`ProfileGraph`] plus a resolved plugins directory; `resolve_document_snapshot`
//! composes the already-built pieces (`load_plugins_dir` → `resolve_activation`
//! → `validate_activation_options` → `assemble_snapshot`) into a deterministic
//! snapshot, folding every `LoadError`/`ResolveError`/`OptionError`/
//! `AssembleError` into a [`ResolveDiag`]. It never panics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::load_core_snapshot;
use crate::loader::load_plugins_dir;
use crate::permissions::{PermissionSet, Permissions};
use crate::resolve::{
    resolve_activation, validate_activation_options, ActivationMap, Profile, ProfileGraph,
    ResolveError,
};
use crate::snapshot::CapabilitySnapshot;
use crate::types::Literal;
use crate::constraints::{parse_constraints, ConstraintDecl};

/// A resolved canonical identity migration entry. The manifest loader keeps
/// source spans separately; this wire DTO is intentionally only the two
/// canonical keys that engines consume.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentityRename {
    pub from: String,
    pub to: String,
}

/// One authored ledger entry together with its manifest declaration location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityRenameDecl {
    pub rename: IdentityRename,
    pub span: Option<lute_core_span::Span>,
}

/// A loaded `lute.project.yaml`: the resolved profile graph plus the absolute
/// plugins directory the registry loads from.
#[derive(Clone, Debug)]
pub struct ProjectConfig {
    pub graph: ProfileGraph,
    /// Project-wide permission ceiling, applied before every profile layer.
    pub permissions: PermissionSet,
    /// Permission ceiling authored on each profile. Kept separate from
    /// [`Profile`] so the existing public profile API remains compatible.
    pub profile_permissions: BTreeMap<String, PermissionSet>,
    /// Resolved plugins dir (`project_dir.join(pluginsDir)`; defaults to
    /// `project_dir/plugins/`).
    pub plugins_dir: PathBuf,
    /// Resolved pinned provider catalog dir (`project_dir.join(catalogDir)`;
    /// defaults to `project_dir/catalog/`). Both the CLI (when `--providers`
    /// is absent) and the LSP resolve provider ids against this via
    /// [`project_providers`], so the two surfaces resolve the same ids for the
    /// same project (plugin §10).
    pub catalog_dir: PathBuf,
    /// Resolved `lineId`/`voiceKey` templates (0.8.0 §9, adoption G4). Absent
    /// or malformed entries fall back to [`IdentityTemplates::default`], which
    /// is the default pair ([`DEFAULT_LINE_ID_TEMPLATE`], [`DEFAULT_VOICE_KEY_TEMPLATE`]).
    pub identity: IdentityTemplates,
    /// `E-IDENTITY-TEMPLATE` diagnostics raised while resolving `identity:`.
    /// Held on the config rather than failing the load, so a bad template
    /// degrades to the default instead of collapsing the whole project
    /// to core-only; [`resolve_document_snapshot`] replays them so BOTH the
    /// CLI and the LSP report them (the no-divergence invariant).
    pub identity_diags: Vec<ResolveDiag>,
    /// Whether the project opts into stable-identity diagnostics for untagged
    /// lines and component uses. Defaults to false for the transition period.
    pub identity_require_stable: bool,
    /// Authored identity migration entries, retained with their source spans
    /// until project graph resolution can validate their endpoints.
    pub identity_renames: Vec<IdentityRenameDecl>,
    /// Malformed ledger declarations that can be reported before graph
    /// resolution. Endpoint existence and graph-dependent rules are validated
    /// by `lute-model`.
    pub identity_rename_diags: Vec<ResolveDiag>,
    /// The manifest's resolved `defaults:` block (0.10.0 §6). Empty when the
    /// manifest supplies none, which is every manifest written before this
    /// release.
    pub defaults: MetaDefaults,
    /// Source locations retained from the manifest's `chapters:` YAML.
    pub chapter_origins: Vec<ChapterOrigin>,
    /// `E-DEFAULTS-KEY` diagnostics raised while resolving `defaults:`. Held
    /// on the config rather than failing the load — same treatment as
    /// `identity_diags`, for the same reason: a bad `defaults:` must not
    /// collapse the project to core-only. Reported ONCE per manifest by
    /// `lute-cli`'s manifest pass (0.10.0 §7), never once per inheriting
    /// document (D-Z).
    pub defaults_diags: Vec<ResolveDiag>,
    /// dsl 0.28.0 §4: `E-CHAPTERS` diagnostics of a malformed `chapters:`
    /// (its shape, or the retired `sequence:` key; the ids are checked
    /// project-wide against the scenes). Reported once per manifest,
    /// located: by `check-project` beside the documents (never fatal to
    /// their check), by the commands that build a project (`compile --all`,
    /// `play`) as a gate.
    pub chapter_diags: Vec<ChapterDiag>,
    /// dsl 0.28.0 §1 (T1-1): `E-MANIFEST-KEY` — every key of the manifest
    /// outside the ones it defines (top level, a profile, `identity:`),
    /// located. The key is dropped, so the rest of the manifest still loads;
    /// the manifest is invalid.
    pub key_diags: Vec<ResolveDiag>,
    /// Typed project-level constraints and declaration diagnostics.
    pub constraints: Vec<ConstraintDecl>,
    pub constraint_diags: Vec<ResolveDiag>,
}

impl ProjectConfig {
    /// Whether stable identity warnings are enabled for this project.
    pub fn identity_require_stable(&self) -> bool { self.identity_require_stable }
}

/// Source locations for one manifest `chapters:` chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterOrigin {
    /// The manifest file containing this chain.
    pub file: PathBuf,
    /// The chain's `on:` value, when it has one.
    pub on: Option<lute_core_span::Span>,
    /// Each accepted scene id and its YAML span.
    pub scenes: BTreeMap<String, lute_core_span::Span>,
}

/// A resolution diagnostic surfaced to the caller (folded into the check
/// result). `code` is the stable, machine-readable `E-*` code of the underlying
/// `LoadError`/`ResolveError`/`OptionError`/`AssembleError` (so a consumer can
/// key on it); the message is that error's rendered [`std::fmt::Display`] prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveDiag {
    pub code: String,
    pub message: String,
    /// The byte range in `lute.project.yaml` the diagnostic concerns — set
    /// only for a manifest-scoped diagnostic (`E-MANIFEST-KEY`,
    /// `E-DEFAULTS-KEY`, `E-IDENTITY-TEMPLATE`), `None` for the rest.
    pub span: Option<std::ops::Range<usize>>,
}

impl ResolveDiag {
    /// Whether this error leaves no snapshot worth checking documents
    /// against. A name declared twice — within one plugin package, across
    /// two plugins, or a domain two plugins declare — is resolved by keeping
    /// the first declaration, so documents still check against what their
    /// plugins declare; the error still fails the run.
    pub fn stops_checking(&self) -> bool {
        self.code.starts_with("E-")
            && !matches!(
                self.code.as_str(),
                "E-PLUGIN-DUP-ID" | "E-PLUGIN-DUP-ACROSS" | "E-DOMAIN-DUP"
            )
    }
}

/// An unknown key of `lute.project.yaml` (dsl 0.28.0 §1): a top-level key,
/// a profile's key or an `identity:` key outside the ones the manifest
/// defines. The key's value never applied.
pub const E_MANIFEST_KEY: &str = "E-MANIFEST-KEY";

/// `lute.project.yaml` that cannot be read as a manifest at all: it does not
/// parse, is not a mapping, lacks `defaultProfile:`, or a value has the wrong
/// shape. The load fails.
pub const E_MANIFEST: &str = "E-MANIFEST";

/// The top-level keys of `lute.project.yaml`. `sequence` is retired
/// (`chapters:`) and refused by [`resolve_chapters`], not here.
pub const MANIFEST_KEYS: [&str; 10] = [
    "defaultProfile",
    "profiles",
    "pluginsDir",
    "catalogDir",
    "identity",
    "defaults",
    "chapters",
    "permissions",
    "sequence",
    "constraints",
];

/// The keys of one `profiles:` entry.
const PROFILE_KEYS: [&str; 3] = ["extends", "plugins", "permissions"];

/// Keys accepted below `identity:`.
const IDENTITY_KEYS: [&str; 4] = ["lineId", "voiceKey", "requireStable", "renames"];

/// Keys a schema declares, which a manifest cannot (dsl 0.28.0 §1: "an
/// engine-only key written in the wrong layer names the layer that owns
/// it").
const SCHEMA_ONLY_KEYS: [&str; 12] = [
    "terminal",
    "seasons",
    "clock",
    "cast",
    "state",
    "relations",
    "rules",
    "entities",
    "enums",
    "facts",
    "defs",
    "occasions",
];

/// Raw `lute.project.yaml` shape (plugin §11). `profiles` is a map of name →
/// `{ extends?, plugins: map<id, true|options-map> }`.
#[derive(Debug, Deserialize)]
struct RawProject {
    #[serde(rename = "pluginsDir")]
    plugins_dir: Option<String>,
    #[serde(rename = "catalogDir", default)]
    catalog_dir: Option<String>,
    #[serde(rename = "defaultProfile")]
    default_profile: String,
    #[serde(default)]
    profiles: BTreeMap<String, RawProfile>,
    #[serde(default)]
    identity: Option<RawIdentity>,
    #[serde(default)]
    defaults: Option<serde_yaml::Mapping>,
    #[serde(default)]
    chapters: Option<serde_yaml::Value>,
    /// Retired in dsl 0.28.0 for `chapters:`; read only to refuse it with
    /// the new spelling.
    #[serde(default)]
    sequence: Option<serde_yaml::Value>,
    #[serde(default)]
    constraints: Option<serde_yaml::Value>,
    #[serde(default)]
    permissions: PermissionSet,
}

#[derive(Debug, Deserialize)]
struct RawProfile {
    #[serde(default)]
    extends: Option<String>,
    /// Each entry activates a plugin: `true` (presence-only) or a mapping of
    /// option values. Kept as raw YAML so `true` and a map coexist under one key.
    #[serde(default)]
    plugins: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    permissions: PermissionSet,
}

/// Normalize a single `profiles[..].plugins` entry value into an option map:
/// `true` (or any non-mapping scalar) → empty map (plugin §11: presence
/// activates); a mapping → `Literal::from_yaml` per value.
fn plugin_options(value: &serde_yaml::Value) -> BTreeMap<String, Literal> {
    match Literal::from_yaml(value) {
        Some(Literal::Map(m)) => m,
        _ => BTreeMap::new(),
    }
}

/// Diagnostic code for a malformed `identity:` template (0.8.0 §9).
pub const E_IDENTITY_TEMPLATE: &str = "E-IDENTITY-TEMPLATE";

/// 0.7.0's hardcoded `lineId` shape. The default, so a project that omits
/// `identity:` compiles byte-identically to 0.7.0.
pub const DEFAULT_LINE_ID_TEMPLATE: &str = "{prefix}.{speaker}_{code}";

/// The default `voiceKey` shape (v1: the voice bank IS the speaker, dsl §11).
/// `{prefix}` since dsl 0.22.0 §11: the 0.7.0 default `{speaker}-{code}`
/// repeated across documents, so every scene's `@ann{code="0010"}` landed on
/// one voice asset. A project with audio recorded against the old keys pins
/// `identity.voiceKey: "{speaker}-{code}"`.
pub const DEFAULT_VOICE_KEY_TEMPLATE: &str = "{prefix}.{speaker}-{code}";

/// The COMPLETE identity-template token set. Any other `{token}` is
/// [`E_IDENTITY_TEMPLATE`] at project load.
pub const IDENTITY_TOKENS: [&str; 3] = ["prefix", "speaker", "code"];

/// 0.10.0 §6.1 (D-P): the CLOSED set of frontmatter keys `defaults:` may
/// supply. Sorted, so `E-DEFAULTS-KEY`'s did-you-mean and its "one of …"
/// listing are both deterministic.
///
/// Everything else is excluded with a reason. `episodeId` is derived
/// identity — defaulting it gives every scene under the root the same
/// `{prefix}` and a silent `lineId` collision. `mode` is a legal core key
/// that nothing reads (the analysis mode comes from the check invocation),
/// and a defaultable key that changes nothing is a trap. `title`/`after` are
/// per-document content and routing. `enums`/`state`/`entities`/`relations`/
/// `facts`/`rules`/`defs` already have a composition mechanism — hoist them
/// into a schema and default `uses:`. `profile`/`plugins` are already
/// project-level. `component`/`params` are per-file identity.
pub const DEFAULTABLE_KEYS: [&str; 12] = [
    "character",
    "components",
    "contentLang",
    "episode",
    "extends",
    "extra",
    "kind",
    "luteVersion",
    "pov",
    "questTier",
    "season",
    "uses",
];

/// The three defaultable keys holding PATHS. A path in a manifest is not a
/// path in a document (D-Y): these resolve against the MANIFEST's directory,
/// and Task 4 canonicalises them here at load time (D-Z).
pub const DEFAULTABLE_PATH_KEYS: [&str; 3] = ["components", "extends", "uses"];

/// A `defaults:` key outside [`DEFAULTABLE_KEYS`], or a value whose shape
/// cannot inhabit that key's core type (0.10.0 §6.1). Spanless and anchored
/// at the manifest: `ResolveDiag` has no span and spanned YAML parsing is not
/// in scope for one diagnostic (D-Z).
pub const E_DEFAULTS_KEY: &str = "E-DEFAULTS-KEY";

/// A manifest's resolved `defaults:` block (0.10.0 §6): frontmatter every
/// document under the root would otherwise retype.
///
/// **Not `Serialize`, deliberately.** After Task 4 the `uses`/`extends`/
/// `components` entries hold CANONICAL absolute paths, which are
/// machine-specific. An absolute path in a stamp means two developers on one
/// commit compute different versions and the drift guard fires on nothing.
/// Pre-resolution is for resolution only; anything that renders or hashes
/// keeps the authored string (D-Z).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaDefaults {
    entries: std::collections::BTreeMap<String, serde_yaml::Value>,
    /// dsl 0.28.0 §4: the manifest's `chapters:` — carried with the
    /// defaults because it is frontmatter the listed scenes did not have to
    /// write (`on:` / `after:` / `priority:`), applied by the same parse-time
    /// pass that applies `questTier` (`lute_check::chapters::apply_chapters`).
    chapters: Vec<Chain>,
    /// Where `defaults.questTier` is written: the manifest file and the
    /// key's span in it — where a problem with the default itself is
    /// reported once, not at every quest it applies to.
    quest_tier_home: Option<(PathBuf, lute_core_span::Span)>,
}

impl MetaDefaults {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// The default for `key`, or `None` when the manifest supplies none.
    /// A key present with an empty or null value IS present and returns
    /// `Some` — §6.2's "present-but-empty counts as present".
    pub fn get(&self, key: &str) -> Option<&serde_yaml::Value> {
        self.entries.get(key)
    }
    /// Every supplied key, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|k| k.as_str())
    }
    /// Every chain the manifest's `chapters:` declares, in order — the ones
    /// a malformed block keeps unapplied ([`Chain::applied`]) included.
    pub fn chapters(&self) -> &[Chain] {
        &self.chapters
    }
    /// The chain that lists scene `id`, applied or not.
    pub fn chain_of(&self, id: &str) -> Option<&Chain> {
        self.chapters
            .iter()
            .find(|c| c.scenes.iter().any(|s| s == id))
    }
    /// These defaults with `chapters` attached — the manifest load's own
    /// step, and a unit test's way to stage one without a file.
    pub fn with_chapters(mut self, chapters: Vec<Chain>) -> Self {
        self.chapters = chapters;
        self
    }
    /// Where `defaults.questTier` is written, when loaded from a manifest.
    pub fn quest_tier_home(&self) -> Option<&(PathBuf, lute_core_span::Span)> {
        self.quest_tier_home.as_ref()
    }
}

/// dsl 0.28.0 §4: one chain of `chapters: [{ on: chapter, scenes: [a, b, c] }]`
/// — scenes answering one occasion, in play order. Each listed scene without
/// its own key gets `on: <occasion>`, `after: visited("<previous>")` (every
/// scene but the first) and a descending `priority:` (`10 × (n − i)`). On a
/// `select: sequence` occasion, which presents every eligible beat in one
/// raise, the chain is the order within that raise: no `after:` (it would be
/// judged before the raise that plays the previous scene).
/// `scenes` holds each id once across all chains, in order (a repeat is
/// [`E_CHAPTERS`] at load and dropped).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chain {
    /// The occasion every listed scene answers; empty when the chain names
    /// none it could read.
    pub on: String,
    pub scenes: Vec<String>,
    /// Whether the chain derives its scenes' keys. A malformed chain is
    /// reported and NOT applied — a half-understood chain must not silently
    /// reorder scenes — but its ids are kept, so a listed scene is never told
    /// to list itself, and they are still checked against the scenes.
    pub applied: bool,
    /// Read from the retired `sequence:` key: never applied, and its ids are
    /// not checked (the rename error is the one report).
    pub retired: bool,
}

impl Chain {
    /// The keys scene `id` derives, in `(key, YAML value)` form, or `None`
    /// when `id` is not listed or the chain is not applied: `on`, then
    /// `after` (not for the first scene, and only when `chained` — the
    /// occasion is not `select: sequence`), then `priority`.
    pub fn derived(
        &self,
        id: &str,
        chained: bool,
    ) -> Option<Vec<(&'static str, serde_yaml::Value)>> {
        if !self.applied {
            return None;
        }
        let i = self.scenes.iter().position(|s| s == id)?;
        let mut out = vec![("on", serde_yaml::Value::String(self.on.clone()))];
        if let Some(prev) = i
            .checked_sub(1)
            .map(|p| &self.scenes[p])
            .filter(|_| chained)
        {
            out.push((
                "after",
                serde_yaml::Value::String(format!("visited(\"{prev}\")")),
            ));
        }
        let priority = 10 * (self.scenes.len() - i) as i64;
        out.push(("priority", serde_yaml::Value::Number(priority.into())));
        Some(out)
    }
}

/// Where an `E-CHAPTERS` shape diagnostic points in the manifest (a
/// [`ResolveDiag`] has no span; `lute_check::chapters` locates it in the
/// text). Chain indexes count the `chapters:` list from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChapterAnchor {
    /// A top-level key: `chapters:` itself, or the retired `sequence:`.
    Top(String),
    /// The `n`th chain's list item.
    Chain(usize),
    /// The `<key>:` inside the `n`th chain.
    Key(usize, String),
    /// The `nth` (0-based) occurrence of a `scenes:` entry's text inside
    /// the `n`th chain.
    Entry(usize, String, usize),
}

/// A malformed `chapters:` (dsl 0.28.0 §4), with where it points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterDiag {
    pub message: String,
    pub anchor: ChapterAnchor,
}

/// A malformed `chapters:` (dsl 0.28.0 §4) or the retired `sequence:`: not a
/// list of `{ on, scenes }` chains, an occasion or entry that is no id, a
/// scene listed twice, two chains on one occasion, or (project-wide, `lute
/// check-project`) a listed id no scene declares, a listed scene whose own
/// `on:` names another occasion, or a scene of a targeted chain with no
/// `target:`.
pub const E_CHAPTERS: &str = "E-CHAPTERS";

/// The label a chain goes by in a message: its occasion when it names one,
/// else its 1-based place in the list.
pub fn chain_label(index: usize, on: &str) -> String {
    if on.is_empty() {
        format!("chain {} of `chapters:`", index + 1)
    } else {
        format!("the chain on `{on}`")
    }
}

/// Whether a `scenes:` entry can name a scene: one bare word (no spaces,
/// quotes, parentheses or commas). Which scenes exist is the project-wide
/// check's question.
fn is_scene_word(s: &str) -> bool {
    !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || "\"'(),[]{}:".contains(c))
}

/// The scene id inside an entry written as a condition (`visited("x")`).
fn entry_hint(text: &str) -> Option<&str> {
    let inner = text.split_once('(')?.1.strip_suffix(')')?;
    let inner = inner.trim().trim_matches(['"', '\'']);
    is_scene_word(inner).then_some(inner)
}

/// Resolve the `chapters:` value (and refuse the retired `sequence:`). Each
/// chain resolves on its own: a malformed one is reported and kept
/// unapplied, the others still apply; a repeated id alone is reported and
/// its later occurrence dropped. Each diagnostic carries its
/// [`ChapterAnchor`]; `lute check-project` reports them located, beside its
/// checks of the documents.
fn resolve_chapters(
    raw: Option<serde_yaml::Value>,
    retired: Option<serde_yaml::Value>,
) -> (Vec<Chain>, Vec<ChapterDiag>) {
    let mut diags = Vec::new();
    let mut chains: Vec<Chain> = Vec::new();
    if let Some(old) = retired {
        let on = ["on", "occasion"]
            .iter()
            .find_map(|k| old.get(k)?.as_str())
            .unwrap_or_default()
            .to_string();
        let scenes: Vec<String> = ["scenes", "scene"]
            .iter()
            .find_map(|k| old.get(k)?.as_sequence())
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().filter(|s| is_scene_word(s)).map(str::to_string))
            .collect();
        // TH28-8: the rewrite built from the author's own occasion and
        // scenes, with a placeholder only for what the old block lacks.
        let line = format!(
            "chapters: [{{ on: {}, scenes: [{}] }}]",
            if on.is_empty() { "<occasion>" } else { &on },
            if scenes.is_empty() {
                "<scene id>, …".to_string()
            } else {
                scenes.join(", ")
            }
        );
        diags.push(ChapterDiag {
            message: format!(
                "`sequence:` is now `chapters:`, a list of chains, and its `occasion:` is `on:` \
                 — write `{line}`"
            ),
            anchor: ChapterAnchor::Top("sequence".to_string()),
        });
        // Keep the old block's ids (unapplied, unchecked) so the scenes it
        // lists hear about the rename, not that they are listed nowhere.
        if raw.is_none() {
            chains.push(Chain {
                on,
                scenes,
                applied: false,
                retired: true,
            });
        }
    }
    let Some(raw) = raw else {
        return (chains, diags);
    };
    let items: Vec<serde_yaml::Value> = match raw {
        serde_yaml::Value::Sequence(items) if !items.is_empty() => items,
        serde_yaml::Value::Mapping(map) => {
            diags.push(ChapterDiag {
                message: "`chapters:` is a list of chains — write this chain as a list item, \
                          `- on: …` in block style or `chapters: [{ on: <occasion>, scenes: \
                          [<scene id>, …] }]`"
                    .to_string(),
                anchor: ChapterAnchor::Top("chapters".to_string()),
            });
            let mut chain = resolve_chain(0, &serde_yaml::Value::Mapping(map), &[], &mut diags);
            chain.applied = false;
            chains.push(chain);
            return (chains, diags);
        }
        _ => {
            diags.push(ChapterDiag {
                message: "`chapters:` must be a non-empty list of chains `[{ on: <occasion>, \
                          scenes: [<scene id>, …] }]`"
                    .to_string(),
                anchor: ChapterAnchor::Top("chapters".to_string()),
            });
            return (chains, diags);
        }
    };
    for (i, item) in items.iter().enumerate() {
        let chain = resolve_chain(i, item, &chains, &mut diags);
        chains.push(chain);
    }
    (chains, diags)
}

/// One `chapters:` item, against the chains before it (a repeated occasion
/// or scene).
fn resolve_chain(
    index: usize,
    item: &serde_yaml::Value,
    earlier: &[Chain],
    diags: &mut Vec<ChapterDiag>,
) -> Chain {
    const KEYS: [&str; 2] = ["on", "scenes"];
    let mut err = |message: String, anchor: ChapterAnchor| {
        diags.push(ChapterDiag { message, anchor });
    };
    let mut chain = Chain::default();
    let serde_yaml::Value::Mapping(map) = item else {
        err(
            format!(
                "{} must be a mapping `{{ on: <occasion>, scenes: [<scene id>, …] }}`",
                chain_label(index, "")
            ),
            ChapterAnchor::Chain(index),
        );
        return chain;
    };
    let mut ok = true;
    // A misspelt key stands for the key it is close to: that key's own
    // "missing" error would only repeat it.
    let mut meant: Vec<&str> = Vec::new();
    let mut scenes_key = "scenes";
    let mut written_on: Option<&serde_yaml::Value> = map.get("on");
    for key in map.keys() {
        let name = key.as_str().unwrap_or("");
        if KEYS.contains(&name) {
            continue;
        }
        ok = false;
        let near = crate::suggest::nearest(name, KEYS.iter().copied(), 2)
            .filter(|n| !map.contains_key(*n));
        meant.extend(near);
        if near == Some("scenes") {
            scenes_key = name;
        }
        let message = if near == Some("on") {
            written_on = written_on.or_else(|| map.get(name));
            let on = map.get(name).and_then(|v| v.as_str()).unwrap_or("");
            format!(
                "{} names its occasion with `{name}:`, but a chain says `on:`, as a scene's \
                 frontmatter does — write `on: {}`",
                chain_label(index, on),
                if on.is_empty() { "<occasion>" } else { on }
            )
        } else {
            let why = near.map_or_else(
                || " — a chain declares `on:` and `scenes:`".to_string(),
                |n| format!(" — did you mean `{n}`?"),
            );
            format!("`{name}` is not a chain key{why}")
        };
        err(message, ChapterAnchor::Key(index, name.to_string()));
    }
    match written_on.map(|v| v.as_str()) {
        Some(Some(o)) if is_scene_word(o) && !o.contains(['.', '-']) => {
            chain.on = o.to_string();
        }
        Some(_) => {
            ok = false;
            err(
                format!(
                    "`on:` of {} must name an occasion, e.g. `on: chapter`",
                    chain_label(index, "")
                ),
                ChapterAnchor::Key(index, "on".to_string()),
            );
        }
        None => {
            ok = false;
            if !meant.contains(&"on") {
                err(
                    format!(
                        "{} needs `on:` — the occasion every listed scene answers, e.g. `on: \
                         chapter`",
                        chain_label(index, "")
                    ),
                    ChapterAnchor::Chain(index),
                );
            }
        }
    }
    let label = chain_label(index, &chain.on);
    if let Some(j) = earlier
        .iter()
        .position(|c| !c.retired && !chain.on.is_empty() && c.on == chain.on)
    {
        ok = false;
        err(
            format!(
                "{label} is the second chain on `{on}` (chain {} is the first) — one chain per \
                 occasion; merge the two `scenes:` lists (this one is not applied)",
                j + 1,
                on = chain.on
            ),
            ChapterAnchor::Key(index, "on".to_string()),
        );
    }
    match map.get(scenes_key) {
        Some(serde_yaml::Value::Sequence(items)) if !items.is_empty() => {
            for item in items {
                match item.as_str().map(str::trim) {
                    Some(id) if is_scene_word(id) => {
                        let elsewhere = earlier
                            .iter()
                            .enumerate()
                            .find(|(_, c)| !c.retired && c.scenes.iter().any(|s| s == id));
                        if chain.scenes.iter().any(|s| s == id) {
                            err(
                                format!(
                                    "{label} lists `{id}` twice — a scene has one place in the \
                                     chapters; the later entry is ignored, remove it"
                                ),
                                ChapterAnchor::Entry(index, id.to_string(), 1),
                            );
                        } else if let Some((j, other)) = elsewhere {
                            err(
                                format!(
                                    "{label} lists `{id}`, which {} already lists — a scene has \
                                     one place in the chapters; the later entry is ignored, \
                                     remove it",
                                    chain_label(j, &other.on)
                                ),
                                ChapterAnchor::Entry(index, id.to_string(), 0),
                            );
                        } else {
                            chain.scenes.push(id.to_string());
                        }
                    }
                    _ => {
                        ok = false;
                        let rendered = serde_yaml::to_string(item).unwrap_or_default();
                        let text = item.as_str().unwrap_or(rendered.trim());
                        let hint = entry_hint(text).map_or_else(
                            || "e.g. `prologue`".to_string(),
                            |id| format!("here `{id}`"),
                        );
                        err(
                            format!(
                                "{label} lists `{text}`, which is not a scene id — list each \
                                 scene's `id:`, {hint}"
                            ),
                            ChapterAnchor::Entry(index, text.to_string(), 0),
                        );
                    }
                }
            }
        }
        None if meant.contains(&"scenes") => ok = false,
        found => {
            ok = false;
            err(
                format!(
                    "`scenes:` of {label} must be a non-empty list of scene ids, in play order, \
                     e.g. `scenes: [prologue, counter, kitchen]`"
                ),
                if found.is_some() {
                    ChapterAnchor::Key(index, "scenes".to_string())
                } else {
                    ChapterAnchor::Chain(index)
                },
            );
        }
    }
    chain.applied = ok;
    chain
}

/// Build a defaults set directly from `(key, YAML value)` pairs, without
/// going through a `lute.project.yaml` on disk. Used by downstream crates'
/// unit tests (`lute-check`) to exercise the defaults-merge path in
/// `parse_meta_kind_with_defaults` without staging a temp manifest.
impl FromIterator<(String, serde_yaml::Value)> for MetaDefaults {
    fn from_iter<I: IntoIterator<Item = (String, serde_yaml::Value)>>(iter: I) -> Self {
        Self {
            entries: iter.into_iter().collect(),
            chapters: Vec::new(),
            quest_tier_home: None,
        }
    }
}

/// Raw `identity:` block — both template keys optional, with an optional
/// canonical-key rename mapping retained as YAML so malformed entries can
/// receive the ledger diagnostic rather than a generic manifest-shape error.
#[derive(Debug, Default, Deserialize, Clone)]
struct RawIdentity {
    #[serde(rename = "lineId", default)]
    line_id: Option<String>,
    #[serde(rename = "voiceKey", default)]
    voice_key: Option<String>,
    #[serde(rename = "requireStable", default)]
    require_stable: bool,
    #[serde(default)]
    renames: Option<serde_yaml::Value>,
}

/// Resolve the YAML mapping while retaining one source location per entry.
/// Graph-dependent endpoint checks belong to the project model.
fn resolve_identity_renames(
    raw: Option<serde_yaml::Value>,
    text: &str,
    locate: &dyn Fn(&[&str]) -> Option<std::ops::Range<usize>>,
) -> (Vec<IdentityRenameDecl>, Vec<ResolveDiag>) {
    let text_index = lute_core_span::TextIndex::new(text);
    let mut entries = Vec::new();
    let mut diags = Vec::new();
    let Some(raw) = raw else { return (entries, diags) };
    let serde_yaml::Value::Mapping(map) = raw else {
        diags.push(ResolveDiag {
            span: locate(&["identity", "renames"]),
            code: "E-RENAME-LEDGER".into(),
            message: "`identity.renames` must be a mapping from canonical NodeKey to canonical NodeKey".into(),
        });
        return (entries, diags);
    };
    for (from, to) in map {
        let Some(from) = from.as_str() else {
            diags.push(ResolveDiag {
                span: locate(&["identity", "renames"]),
                code: "E-RENAME-LEDGER".into(),
                message: "rename source keys must be canonical NodeKey strings".into(),
            });
            continue;
        };
        let span = locate(&["identity", "renames", from]);
        let Some(to) = to.as_str() else {
            diags.push(ResolveDiag {
                span,
                code: "E-RENAME-LEDGER".into(),
                message: format!("rename destination for `{from}` must be a canonical NodeKey string"),
            });
            continue;
        };
        entries.push(IdentityRenameDecl {
            rename: IdentityRename { from: from.to_string(), to: to.to_string() },
            span: span.map(|r| lute_core_span::Span::from_bytes(&text_index, r.start, r.end)),
        });
    }
    (entries, diags)
}

/// The resolved `identity:` block (0.8.0 §9, adoption G4): the `lineId` and
/// `voiceKey` join shapes the compiler stamps onto every line.
///
/// Pre-0.8.0 both were hardcoded, which blocked adopters whose existing assets
/// already key voice/translation tables on a different convention (Stage's
/// 6,640 rows use `npc_koyuki_ep05.koyuki-0010`, i.e. a `-` join). Templating
/// them costs nothing when unused: [`Default`] is the documented default pair.
///
/// Only the LINE identity is templated. A choice/hub option's
/// `{prefix}.{branchOrHubId}.{optionId}` is structural, not a content join,
/// and stays fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityTemplates {
    pub line_id: String,
    pub voice_key: String,
}

impl Default for IdentityTemplates {
    fn default() -> Self {
        Self {
            line_id: DEFAULT_LINE_ID_TEMPLATE.to_string(),
            voice_key: DEFAULT_VOICE_KEY_TEMPLATE.to_string(),
        }
    }
}

impl IdentityTemplates {
    /// Render `line_id` for one line. Never panics.
    pub fn render_line_id(&self, prefix: &str, speaker: &str, code: &str) -> String {
        render_identity_template(&self.line_id, prefix, speaker, code)
    }

    /// Render `voice_key` for one voiced line. Never panics.
    pub fn render_voice_key(&self, prefix: &str, speaker: &str, code: &str) -> String {
        render_identity_template(&self.voice_key, prefix, speaker, code)
    }

    /// Every [`E_IDENTITY_TEMPLATE`] this pair raises, in `lineId`-then-
    /// `voiceKey` order. Empty for a conforming pair (in particular for
    /// [`Default`]). Pure — `load_project` uses the same check to decide which
    /// field to reset.
    pub fn validate(&self) -> Vec<ResolveDiag> {
        let mut diags = Vec::new();
        validate_template(&self.line_id, "lineId", &mut diags);
        validate_template(&self.voice_key, "voiceKey", &mut diags);
        diags
    }
}

/// Walk `template` once, handing each run of literal text plus the `{token}`
/// that terminates it (bare name, braces stripped) to `piece`; the trailing
/// literal arrives with `None`. An unterminated `{` is literal text. The scan
/// never fails, so validation and rendering can never disagree about a
/// template's decomposition.
fn scan_template(template: &str, mut piece: impl FnMut(&str, Option<&str>)) {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        // `tail` starts AT the `{`; `close` is the `}` offset within its name.
        let (lit, tail) = rest.split_at(open);
        let Some(close) = tail[1..].find('}') else {
            piece(rest, None);
            return;
        };
        piece(lit, Some(&tail[1..1 + close]));
        rest = &tail[close + 2..];
    }
    piece(rest, None);
}

/// Substitute `{prefix}`/`{speaker}`/`{code}` in `template`. Never panics and
/// never allocates twice: the output is sized up front.
///
/// An unknown token is rejected at project load, and the offending template is
/// reset to its default there — so this arm is reachable only for a hand-built
/// [`IdentityTemplates`], where the token is emitted verbatim rather than
/// silently dropped.
pub fn render_identity_template(template: &str, prefix: &str, speaker: &str, code: &str) -> String {
    let mut out = String::with_capacity(template.len() + prefix.len() + speaker.len() + code.len());
    scan_template(template, |lit, token| {
        out.push_str(lit);
        match token {
            Some("prefix") => out.push_str(prefix),
            Some("speaker") => out.push_str(speaker),
            Some("code") => out.push_str(code),
            Some(other) => {
                out.push('{');
                out.push_str(other);
                out.push('}');
            }
            None => {}
        }
    });
    out
}

/// Push every [`E_IDENTITY_TEMPLATE`] `template` raises onto `diags`; `true`
/// when it conforms. `field` is the authored key (`lineId`/`voiceKey`) so the
/// message points at the YAML the author wrote.
///
/// Two rejections: an unknown `{token}` (a typo would otherwise be emitted
/// literally into every id), and a template that renders empty — checked by
/// rendering against non-empty probes, so only a literally empty template
/// trips it.
fn validate_template(template: &str, field: &str, diags: &mut Vec<ResolveDiag>) -> bool {
    let mut ok = true;
    scan_template(template, |_lit, token| {
        if let Some(name) = token {
            if !IDENTITY_TOKENS.contains(&name) {
                ok = false;
                diags.push(ResolveDiag {
                    span: None,
                    code: E_IDENTITY_TEMPLATE.to_string(),
                    message: format!(
                        "unknown token `{{{name}}}` in identity template `{field}`; \
                         valid tokens are {{prefix}}, {{speaker}}, {{code}}"
                    ),
                });
            }
        }
    });
    if ok && render_identity_template(template, "x", "x", "x").is_empty() {
        ok = false;
        diags.push(ResolveDiag {
            span: None,
            code: E_IDENTITY_TEMPLATE.to_string(),
            message: format!("identity template `{field}` resolves to an empty string"),
        });
    }
    ok
}

/// Resolve the raw `identity:` block: each key defaults to its default shape
/// independently, and a REJECTED key falls back to that same default (fail
/// closed — a malformed template must never reach the artifact). Returns the
/// resolved pair plus its `E-IDENTITY-TEMPLATE` diagnostics.
fn resolve_identity(
    raw: Option<RawIdentity>,
    locate: &dyn Fn(&[&str]) -> Option<std::ops::Range<usize>>,
) -> (IdentityTemplates, Vec<ResolveDiag>) {
    let raw = raw.unwrap_or_default();
    let mut diags = Vec::new();
    let mut resolved = IdentityTemplates::default();
    for (field, t, into) in [
        ("lineId", raw.line_id, &mut resolved.line_id),
        ("voiceKey", raw.voice_key, &mut resolved.voice_key),
    ] {
        let Some(t) = t else { continue };
        let before = diags.len();
        if validate_template(&t, field, &mut diags) {
            *into = t;
        }
        for d in &mut diags[before..] {
            d.span = locate(&["identity", field]);
        }
    }
    (resolved, diags)
}

/// Which core type a defaultable key's value must be able to inhabit. This
/// is the subset of frontmatter typing that is decidable WITHOUT a document,
/// and it is the only shape check `defaults:` can make at the manifest.
///
/// The rest — per-kind legality, required keys, plugin-declared schemas —
/// happens where it already happens, at the document, because a defaulted
/// value flows through the identical `parse_meta_kind` path as an authored
/// one (§6.2, Task 5).
fn defaults_shape_ok(key: &str, v: &serde_yaml::Value) -> Result<(), &'static str> {
    match key {
        "season" | "episode" => v.as_i64().map(|_| ()).ok_or("an integer"),
        "kind" => match v.as_str() {
            Some("scene") | Some("quest") => Ok(()),
            _ => Err("`scene` or `quest`"),
        },
        "character" | "pov" | "luteVersion" | "contentLang" => {
            v.as_str().map(|_| ()).ok_or("a string")
        }
        // dsl 0.26.0 §2.4: the `tier=` a `<quest>` without one takes;
        // dsl 0.28.0 §5: any tier a `<quest tier=>` takes, `season:<name>`
        // included (the season itself is checked at each quest).
        "questTier" => match v.as_str() {
            Some("run") | Some("user") => Ok(()),
            Some(t) if crate::season::season_ref(t).is_some_and(crate::ident::is_name) => Ok(()),
            _ => Err("`run`, `user` or `season:<name>`"),
        },
        // `uses`/`extends`/`components` take one string or a sequence of
        // strings, exactly as authored frontmatter does. A null or empty
        // sequence is a PRESENT value meaning "none" (§6.2) and is legal.
        "uses" | "extends" | "components" => match v {
            serde_yaml::Value::Null | serde_yaml::Value::String(_) => Ok(()),
            serde_yaml::Value::Sequence(items) => {
                if items.iter().all(|i| i.is_string()) {
                    Ok(())
                } else {
                    Err("a string or a list of strings")
                }
            }
            _ => Err("a string or a list of strings"),
        },
        // dsl 0.15.0 §3: the manifest-supplied descriptive block. Only the
        // top-level shape is checked at the manifest — inner scalar/list
        // enforcement runs at the document (through the identical `extra:`
        // lift path that an authored block takes), same discipline as the
        // `uses`/`extends`/`components` string-list check above.
        "extra" => match v {
            serde_yaml::Value::Mapping(_) | serde_yaml::Value::Null => Ok(()),
            _ => Err("a mapping"),
        },
        _ => Ok(()),
    }
}

/// What a refused `defaults:` value most likely meant: a quest tier's
/// nearest legal spelling (`Run` → `run`), a bare season name for
/// `season:<name>`, and a season name in the legacy `season:` (the episode
/// number) the way a scene is tied to a season instead. Empty otherwise.
fn defaults_value_hint(key: &str, v: &serde_yaml::Value) -> String {
    let Some(raw) = v.as_str() else {
        return String::new();
    };
    let name = raw
        .strip_prefix(crate::season::SEASON_PREFIX)
        .or_else(|| raw.strip_prefix("season."))
        .unwrap_or(raw);
    match key {
        "questTier" => {
            let near = crate::suggest::did_you_mean(raw, ["run", "user"]);
            // `scene` / `app` / `quest` are state tiers, not season names.
            if !near.is_empty()
                || !crate::ident::is_name(name)
                || ["scene", "app", "quest"].contains(&name)
            {
                return near;
            }
            format!(" — if `{name}` is a declared season, write `season:{name}`")
        }
        // A quoted episode number (`season: "3"`) is not a season name.
        "season" if crate::ident::is_name(name) && !name.bytes().all(|b| b.is_ascii_digit()) => {
            format!(
                " — `season:` is the legacy episode number; to tie a scene to season `{name}`, \
                 write `once: season:{name}` on the scene and/or gate its `when:` on the \
                 season's `live:` condition"
            )
        }
        _ => String::new(),
    }
}

/// Canonicalise one `defaults:` path entry against the manifest's directory
/// (D-Y: a path resolves relative to the file that contains it; D-Z:
/// pre-resolved at load, so the import resolvers keep their single
/// `base_dir` and are not touched).
///
/// Documents under one root sit at different depths — `scenes/`, `quests/`,
/// a nested subdirectory — so a manifest default resolved document-relative
/// would be correct only while every consumer shared a depth, and would
/// break when a single file moved. That is worse than the boilerplate §6
/// exists to delete.
fn canonical_default_path(manifest_dir: &Path, rel: &str) -> Result<String, String> {
    match std::fs::canonicalize(manifest_dir.join(rel)) {
        Ok(p) => Ok(p.display().to_string()),
        Err(e) => {
            // `- a.lute, b.lute` is one item naming one (missing) path.
            let split = if rel.contains(',') {
                " — a list item holds one path; write each path as its own `- ` item"
            } else {
                ""
            };
            Err(format!(
                "`{rel}` does not resolve against {}: {}{split}",
                manifest_dir.display(),
                crate::io_reason(&e)
            ))
        }
    }
}

/// Canonicalise every path inside one `defaults:` value, preserving its
/// authored shape (a scalar stays a scalar, a sequence stays a sequence).
/// The FIRST failure is reported and the whole entry is dropped: a partly
/// resolved import list is worse than none, because it silently changes what
/// a document imports.
///
/// dsl 0.26.0 §2.4: with `globs`, an entry containing `*`, `?` or `**` is a
/// glob, expanded in path order in its place (the value becomes a sequence);
/// a file listed twice is imported once, at its first position.
fn canonicalise_entry(
    manifest_dir: &Path,
    v: &serde_yaml::Value,
    globs: bool,
) -> Result<serde_yaml::Value, String> {
    let items: Vec<&str> = match v {
        serde_yaml::Value::Null => return Ok(v.clone()),
        serde_yaml::Value::String(s) if !(globs && is_glob(s)) => {
            return Ok(serde_yaml::Value::String(canonical_default_path(
                manifest_dir,
                s,
            )?))
        }
        serde_yaml::Value::String(s) => vec![s.as_str()],
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .expect("shape already checked by defaults_shape_ok")
            })
            .collect(),
        _ => unreachable!("shape already checked by defaults_shape_ok"),
    };
    let mut out: Vec<serde_yaml::Value> = Vec::with_capacity(items.len());
    let mut push = |p: String| {
        let p = serde_yaml::Value::String(p);
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for s in items {
        if globs && is_glob(s) {
            for rel in expand_glob(manifest_dir, s) {
                push(canonical_default_path(manifest_dir, &rel)?);
            }
        } else {
            push(canonical_default_path(manifest_dir, s)?);
        }
    }
    Ok(serde_yaml::Value::Sequence(out))
}

fn is_glob(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// The files under `dir` matching `pattern` (`/`-separated; `*`/`?` within a
/// segment, `**` for any number of directories), as `dir`-relative paths in
/// path order. A glob matching nothing — an existing directory with no match
/// yet, or a directory not created yet (an area not written; git keeps no
/// empty directory) — expands to nothing (dsl 0.26.0 §2.4, prerelease N7).
/// Only a literal (glob-free) entry must exist.
fn expand_glob(dir: &Path, pattern: &str) -> Vec<String> {
    let segments: Vec<&str> = pattern
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    let fixed = segments.iter().take_while(|s| !is_glob(s)).count();
    let prefix: PathBuf = segments[..fixed].iter().collect();
    let mut out = Vec::new();
    walk_glob(dir, &prefix, &segments[fixed..], &mut out);
    out.sort();
    out.dedup();
    out
}

fn walk_glob(root: &Path, at: &Path, rest: &[&str], out: &mut Vec<String>) {
    let Some((seg, tail)) = rest.split_first() else {
        if root.join(at).is_file() {
            out.push(at.to_string_lossy().replace('\\', "/"));
        }
        return;
    };
    if *seg == "**" {
        walk_glob(root, at, tail, out);
    }
    let Ok(entries) = std::fs::read_dir(root.join(at)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let next = at.join(&name);
        if *seg == "**" {
            if root.join(&next).is_dir() {
                walk_glob(root, &next, rest, out);
            }
        } else if segment_matches(seg, &name) {
            walk_glob(root, &next, tail, out);
        }
    }
}

/// `*` (any run) and `?` (one char) within one path segment.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Resolve `defaults:` (0.10.0 §6.1): reject every key outside the closed
/// set with a did-you-mean, reject every value whose shape cannot inhabit
/// its key, and keep the rest. A rejected entry is NOT applied — a default
/// nobody can act on must not silently reach a document.
fn resolve_defaults(
    manifest_dir: &Path,
    raw: Option<serde_yaml::Mapping>,
    locate: &dyn Fn(&[&str]) -> Option<std::ops::Range<usize>>,
) -> (MetaDefaults, Vec<ResolveDiag>) {
    let mut out = MetaDefaults::default();
    let mut diags = Vec::new();
    let Some(raw) = raw else { return (out, diags) };
    for (k, v) in raw {
        let Some(key) = k.as_str() else {
            diags.push(ResolveDiag {
                span: locate(&["defaults"]),
                code: E_DEFAULTS_KEY.to_string(),
                message: "every `defaults:` key must be a string".to_string(),
            });
            continue;
        };
        let span = locate(&["defaults", key]);
        if !DEFAULTABLE_KEYS.contains(&key) {
            let hint = crate::suggest::did_you_mean(key, DEFAULTABLE_KEYS);
            diags.push(ResolveDiag {
                span,
                code: E_DEFAULTS_KEY.to_string(),
                message: format!(
                    "`defaults.{key}` is not a defaultable frontmatter key{hint}. \
                     The defaultable set is closed: {}",
                    DEFAULTABLE_KEYS.join(", ")
                ),
            });
            continue;
        }
        if let Err(want) = defaults_shape_ok(key, &v) {
            diags.push(ResolveDiag {
                span,
                code: E_DEFAULTS_KEY.to_string(),
                message: format!(
                    "`defaults.{key}` must be {want}{}",
                    defaults_value_hint(key, &v)
                ),
            });
            continue;
        }
        if DEFAULTABLE_PATH_KEYS.contains(&key) {
            match canonicalise_entry(manifest_dir, &v, key == "uses") {
                Ok(resolved) => {
                    out.entries.insert(key.to_string(), resolved);
                }
                Err(why) => diags.push(ResolveDiag {
                    span,
                    code: E_DEFAULTS_KEY.to_string(),
                    message: format!("`defaults.{key}`: {why}"),
                }),
            }
            continue;
        }
        out.entries.insert(key.to_string(), v);
    }
    (out, diags)
}

/// dsl 0.28.0 §1 (T1-1): the message for a top-level manifest key outside
/// [`MANIFEST_KEYS`] — the layer that owns it when another layer does (a
/// schema key, a document key that `defaults:` can supply, a profile key),
/// otherwise the nearest manifest key.
fn manifest_key_message(key: &str) -> String {
    if SCHEMA_ONLY_KEYS.contains(&key) {
        return format!(
            "`{key}:` belongs in a schema (a `*.schema.yaml` your documents list in `uses:`), \
             not in lute.project.yaml"
        );
    }
    if DEFAULTABLE_KEYS.contains(&key) {
        return format!(
            "`{key}:` is a document key — to give it to every document, write it under \
             `defaults:`"
        );
    }
    if crate::suggest::nearest(key, ["sequence", "sequences"], 2).is_some() {
        return format!(
            "`{key}:` is not a manifest key — every chain goes in the one `chapters:` list"
        );
    }
    if matches!(key, "plugins" | "extends") {
        return format!("`{key}:` belongs to a profile — `profiles: {{ <name>: {{ {key}: … }} }}`");
    }
    // `sequence` (retired, the last key) is never suggested.
    let current = &MANIFEST_KEYS[..MANIFEST_KEYS.len() - 1];
    let hint = crate::suggest::did_you_mean(key, current.iter().copied());
    format!(
        "unknown key `{key}` in lute.project.yaml{hint} (it takes {})",
        current
            .iter()
            .map(|k| format!("`{k}:`"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Remove every key of `map` outside `known` (so the rest still loads) and
/// report each as [`E_MANIFEST_KEY`] at `path` + the key.
fn drop_unknown_keys(
    map: &mut serde_yaml::Mapping,
    known: &[&str],
    path: &[&str],
    message: &dyn Fn(&str) -> String,
    locate: &dyn Fn(&[&str]) -> Option<std::ops::Range<usize>>,
    diags: &mut Vec<ResolveDiag>,
) {
    let unknown: Vec<serde_yaml::Value> = map
        .keys()
        .filter(|k| !k.as_str().is_some_and(|k| known.contains(&k)))
        .cloned()
        .collect();
    for k in unknown {
        map.remove(&k);
        let key = match &k {
            serde_yaml::Value::String(s) => s.clone(),
            other => serde_yaml::to_string(other)
                .unwrap_or_default()
                .trim()
                .to_string(),
        };
        let mut at: Vec<&str> = path.to_vec();
        at.push(&key);
        diags.push(ResolveDiag {
            span: locate(&at).or_else(|| locate(path)),
            code: E_MANIFEST_KEY.to_string(),
            message: message(&key),
        });
    }
}

fn chapter_origins(
    text: &str,
    raw: Option<&serde_yaml::Value>,
    path: &Path,
) -> Vec<ChapterOrigin> {
    let Some(serde_yaml::Value::Sequence(items)) = raw else {
        return Vec::new();
    };
    let file = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    use crate::yaml_text::{yaml_span, YamlStep::{Item, Key, Value}};
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let on = item.get("on").and_then(serde_yaml::Value::as_str)
                .and_then(|_| yaml_span(text, &[Key("chapters"), Item(i), Key("on"), Value]));
            let mut scenes = BTreeMap::new();
            if let Some(values) = item.get("scenes").and_then(serde_yaml::Value::as_sequence) {
                for (j, value) in values.iter().enumerate() {
                    let Some(value) = value.as_str() else { continue };
                    let Some(span) = yaml_span(text, &[Key("chapters"), Item(i), Key("scenes"), Item(j), Value]) else { continue };
                    scenes.entry(value.to_string()).or_insert(span);
                }
            }
            ChapterOrigin {
                file: file.clone(),
                on,
                scenes,
            }
        })
        .collect()
}


/// Read `<project_dir>/lute.project.yaml` into a [`ProjectConfig`].
///
/// Distinguishes an absent config from a broken one (plugin §11): a missing
/// file → `Ok(None)` (the document legitimately resolves core-only); a read
/// error, a file that does not parse, is not a mapping or lacks
/// `defaultProfile:` → `Err(msg)` so the caller can surface it instead of
/// silently mis-validating (dsl 0.28.0: `<path>:<line>:<col>: error
/// [E-MANIFEST] …`, in plain words); a valid file → `Ok(Some(cfg))`.
///
/// An unknown key, a malformed `identity:` template and a bad `defaults:`
/// entry are NOT load failures: each is dropped and reported (located) on
/// the config, so the project still resolves its plugins and both surfaces
/// report the same diagnostic.
pub fn load_project(project_dir: &Path) -> Result<Option<ProjectConfig>, String> {
    let path = project_dir.join("lute.project.yaml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot read {}: {}",
                path.display(),
                crate::io_reason(&e)
            ))
        }
    };
    let fail = |offset: usize, message: &str| {
        let (line, col) = crate::yaml_text::line_col(&text, offset);
        format!(
            "{}:{line}:{col}: error [{E_MANIFEST}] {message}",
            path.display()
        )
    };
    let value: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let fault = crate::yaml_text::yaml_fault(&text, &e);
            return Err(fail(fault.offset, &fault.message));
        }
    };
    let mut map = match value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => serde_yaml::Mapping::new(),
        _ => {
            return Err(fail(
                0,
                "lute.project.yaml must be a mapping of keys, starting with \
                 `defaultProfile:` and `profiles:`",
            ))
        }
    };
    let locate = |at: &[&str]| crate::yaml_text::key_span(&text, at);
    let mut key_diags = Vec::new();
    drop_unknown_keys(
        &mut map,
        &MANIFEST_KEYS,
        &[],
        &manifest_key_message,
        &locate,
        &mut key_diags,
    );
    if let Some(serde_yaml::Value::Mapping(profiles)) = map.get_mut("profiles") {
        for (name, profile) in profiles.iter_mut() {
            let (Some(name), serde_yaml::Value::Mapping(profile)) = (name.as_str(), profile) else {
                continue;
            };
            let message = |key: &str| {
                format!(
                    "profile `{name}`: unknown key `{key}`{} (a profile takes `extends:`, \
                     `plugins:` and `permissions:`)",
                    crate::suggest::did_you_mean(key, PROFILE_KEYS)
                )
            };
            drop_unknown_keys(
                profile,
                &PROFILE_KEYS,
                &["profiles", name],
                &message,
                &locate,
                &mut key_diags,
            );
        }
    }
    if let Some(serde_yaml::Value::Mapping(identity)) = map.get_mut("identity") {
        let message = |key: &str| {
            format!(
                "`identity:` has no key `{key}`{} (it takes `lineId:` and `voiceKey:`)",
                crate::suggest::did_you_mean(key, IDENTITY_KEYS)
            )
        };
        drop_unknown_keys(
            identity,
            &IDENTITY_KEYS,
            &["identity"],
            &message,
            &locate,
            &mut key_diags,
        );
    }
    if !map.contains_key("defaultProfile") {
        return Err(fail(
            0,
            "lute.project.yaml needs `defaultProfile:` naming the profile a document uses \
             when it names none — e.g. `defaultProfile: core` with `profiles: { core: { \
             plugins: {} } }`",
        ));
    }
    // Through text, not `from_value`: `Value`'s deserializer reads a `null`
    // as an empty mapping, which would accept `permissions: null`.
    let cleaned = serde_yaml::to_string(&serde_yaml::Value::Mapping(map)).unwrap_or_default();
    let raw: RawProject = serde_yaml::from_str(&cleaned).map_err(|e| {
        fail(
            0,
            &format!("a value has the wrong shape: {}", plain_serde(&e)),
        )
    })?;

    let mut profiles = BTreeMap::new();
    let mut profile_permissions = BTreeMap::new();
    for (name, rp) in raw.profiles {
        let plugins: ActivationMap = rp
            .plugins
            .iter()
            .map(|(id, value)| (id.clone(), plugin_options(value)))
            .collect();
        profile_permissions.insert(name.clone(), rp.permissions);
        profiles.insert(
            name,
            Profile {
                extends: rp.extends,
                plugins,
            },
        );
    }

    let graph = ProfileGraph {
        profiles,
        default_profile: raw.default_profile,
    };
    let plugins_dir = project_dir.join(raw.plugins_dir.as_deref().unwrap_or("plugins/"));
    let catalog_dir = project_dir.join(raw.catalog_dir.as_deref().unwrap_or("catalog/"));
    let identity_raw = raw.identity.clone();
    let identity_require_stable = identity_raw.as_ref().is_some_and(|i| i.require_stable);
    let (identity, identity_diags) = resolve_identity(identity_raw.clone(), &locate);
    let (identity_renames, identity_rename_diags) =
        resolve_identity_renames(identity_raw.and_then(|i| i.renames), &text, &locate);
    let chapter_origins = chapter_origins(&text, raw.chapters.as_ref(), &path);
    let (chapters, chapter_diags) = resolve_chapters(raw.chapters, raw.sequence);
    let (constraints, constraint_diags) = parse_constraints(raw.constraints.as_ref(), &text);
    let (defaults, defaults_diags) = resolve_defaults(project_dir, raw.defaults, &locate);
    let mut defaults = defaults.with_chapters(chapters);
    if defaults.get("questTier").is_some() {
        if let Some(r) = locate(&["defaults", "questTier"]) {
            let idx = lute_core_span::TextIndex::new(&text);
            let file = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            defaults.quest_tier_home =
                Some((file, lute_core_span::Span::from_bytes(&idx, r.start, r.end)));
        }
    }
    Ok(Some(ProjectConfig {
        graph,
        permissions: raw.permissions,
        profile_permissions,
        plugins_dir,
        catalog_dir,
        identity,
        identity_diags,
        identity_require_stable,
        identity_renames,
        identity_rename_diags,
        defaults,
        chapter_origins,
        defaults_diags,
        chapter_diags,
        key_diags,
        constraints,
        constraint_diags,
    }))
}

/// A `serde` deserialization error of a manifest value, without the
/// library's line/column marks (the manifest's own key is named instead)
/// and without a Rust type name.
fn plain_serde(e: &serde_yaml::Error) -> String {
    let s = e.to_string();
    let s = s.split(" at line ").next().unwrap_or(&s);
    s.replace("invalid type: ", "")
        .replace("a map", "a mapping")
        .replace("struct ", "")
}

/// The ONE catalog-loading path both surfaces use (plugin §10). Given a resolved
/// project, load its pinned provider catalog from `project.catalog_dir`; given
/// `None` (a loose scene, or no project discovered), an empty [`ProviderSet`].
///
/// The CLI calls this when `--providers` is absent and the LSP calls it in every
/// analyze pass, so the two resolve the same provider ids for the same project
/// — the no-divergence invariant extended to catalog resolution. Never panics:
/// [`ProviderSet::load`] already tolerates a missing/corrupt catalog dir.
pub fn project_providers(project: Option<&ProjectConfig>) -> crate::provider::ProviderSet {
    match project {
        Some(p) => crate::provider::ProviderSet::load(&p.catalog_dir),
        None => crate::provider::ProviderSet::default(),
    }
}

/// Resolve the project ceiling, global profile, ancestor profiles, and
/// selected profile into a retained conjunctive permission stack.
///
/// `global` is applied exactly once even when it appears in the selected
/// profile's inheritance chain. Missing profiles and inheritance cycles use
/// the same resolver errors as plugin activation.
pub fn resolve_permissions(
    project: &ProjectConfig,
    selected: &str,
) -> Result<Permissions, ResolveError> {
    let mut permissions = Permissions::default();
    permissions.push(project.permissions.clone());
    if let Some(global) = project.profile_permissions.get("global") {
        permissions.push(global.clone());
    }
    for name in project.graph.extends_chain(selected)? {
        if name == "global" {
            continue;
        }
        if let Some(layer) = project.profile_permissions.get(&name) {
            permissions.push(layer.clone());
        }
    }
    Ok(permissions)
}

/// The ONE resolution both CLI and LSP call (plugin §11). Given a project (or
/// `None` for core-only) and the scene's parsed frontmatter (profile + plugins),
/// resolve activation and assemble the snapshot deterministically. Returns the
/// snapshot plus any resolution diagnostics (load errors / unresolved depends /
/// cycles / assembly dup ids / `identity:` template errors). Never panics.
pub fn resolve_document_snapshot(
    project: Option<&ProjectConfig>,
    scene_profile: Option<&str>,
    scene_plugins: &BTreeMap<String, serde_yaml::Value>,
) -> (CapabilitySnapshot, Vec<ResolveDiag>) {
    let Some(project) = project else {
        return (load_core_snapshot(), Vec::new());
    };
    let mut diags = project.identity_diags.clone();
    diags.extend(project.identity_rename_diags.clone());
    // shared resolver remains the single reporting seam for both surfaces.

    // 1. Load every installed plugin package; surface load errors.
    let (registry, load_errs) = load_plugins_dir(&project.plugins_dir);
    diags.extend(load_errs.into_iter().map(|e| ResolveDiag {
        span: None,
        code: e.code().into(),
        message: format!("{e}"),
    }));

    // 2. Pick the profile: scene override, else the graph's default.
    let selected = scene_profile.unwrap_or(project.graph.default_profile.as_str());

    // Permission resolution follows the same graph as activation, but remains
    // independent of plugin activation and source-local plugin additions.
    let permissions = match resolve_permissions(project, selected) {
        Ok(permissions) => permissions,
        Err(e) => {
            diags.push(ResolveDiag {
                span: None,
                code: e.code().into(),
                message: e.to_string(),
            });
            return (load_core_snapshot(), diags);
        }
    };

    // 3. Convert scene-local `plugins:` frontmatter to an ActivationMap.
    let scene_local: ActivationMap = scene_plugins
        .iter()
        .map(|(id, value)| (id.clone(), plugin_options(value)))
        .collect();

    // 4. Resolve activation (§11.1 order + §11.2 merge).
    let active = match resolve_activation(&project.graph, selected, &scene_local, &registry) {
        Ok(active) => active,
        Err(e) => {
            diags.push(ResolveDiag {
                span: None,
                code: e.code().into(),
                message: format!("{e}"),
            });
            // No conforming activation → keep the resolved permission ceiling
            // on the core-only fallback so ignoring diagnostics cannot widen it.
            let mut snapshot = load_core_snapshot();
            snapshot.restrict_permissions(&permissions);
            return (snapshot, diags);
        }
    };

    // 4b. plugin Appendix C1: reject an unknown option name / a value that is
    //     not valid for its declared type. Reported through THIS channel — the
    //     one both the CLI and the LSP read — so neither surface can diverge on
    //     an option the other rejects. Non-fatal: the activation ORDER is still
    //     conforming, so the snapshot below still assembles and the author sees
    //     the document's real diagnostics instead of core-only fallback noise.
    diags.extend(
        validate_activation_options(&active, &registry)
            .into_iter()
            .map(|e| ResolveDiag {
                span: None,
                code: e.code().into(),
                message: e.message(),
            }),
    );

    // 5. Assemble the merged snapshot; surface assembly errors.
    let (mut snapshot, assemble_errs) = crate::assemble::assemble_snapshot(&active, &registry);
    diags.extend(assemble_errs.into_iter().map(|e| ResolveDiag {
        span: None,
        code: e.code().into(),
        message: e.to_string(),
    }));
    snapshot.restrict_permissions(&permissions);

    (snapshot, diags)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_origins_anchor_flow_style_values() {
        let text = "defaultProfile: core\nprofiles: {core: {plugins: {}}}\nchapters: [{on: open, scenes: [first, second]}]\n";
        let value: serde_yaml::Value = serde_yaml::from_str(text).unwrap();
        let origins = chapter_origins(
            text,
            value.get("chapters"),
            Path::new("lute.project.yaml"),
        );
        assert_eq!(origins.len(), 1);
        let on = origins[0].on.expect("flow `on` span");
        assert_eq!(&text[on.byte_start..on.byte_end], "open");
        assert_eq!(
            &text[origins[0].scenes["first"].byte_start..origins[0].scenes["first"].byte_end],
            "first"
        );
        assert_eq!(
            &text[origins[0].scenes["second"].byte_start..origins[0].scenes["second"].byte_end],
            "second"
        );
    }
    #[test]
    fn identity_rename_mapping_is_loaded_with_source_location() {
        let root = std::env::temp_dir().join(format!(
            "lute-identity-renames-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::create_dir_all(&root);
        let path = root.join("lute.project.yaml");
        std::fs::write(
            &path,
            "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\nidentity:\n  renames:\n    \"quest:old\": \"quest:new\"\n",
        )
        .unwrap();
        let config = load_project(&root).unwrap().unwrap();
        assert_eq!(config.identity_renames.len(), 1);
        assert_eq!(config.identity_renames[0].rename.from, "quest:old");
        assert!(config.identity_renames[0].span.is_some());
        let _ = std::fs::remove_dir_all(root);
    }
}
