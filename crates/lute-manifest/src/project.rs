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

use serde::Deserialize;

use crate::core::load_core_snapshot;
use crate::loader::load_plugins_dir;
use crate::resolve::{
    resolve_activation, validate_activation_options, ActivationMap, Profile, ProfileGraph,
};
use crate::snapshot::CapabilitySnapshot;
use crate::types::Literal;

/// A loaded `lute.project.yaml`: the resolved profile graph plus the absolute
/// plugins directory the registry loads from.
#[derive(Clone, Debug)]
pub struct ProjectConfig {
    pub graph: ProfileGraph,
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
    /// reproduces 0.7.0's hardcoded shapes byte-for-byte.
    pub identity: IdentityTemplates,
    /// `E-IDENTITY-TEMPLATE` diagnostics raised while resolving `identity:`.
    /// Held on the config rather than failing the load, so a bad template
    /// degrades to the default shape instead of collapsing the whole project
    /// to core-only; [`resolve_document_snapshot`] replays them so BOTH the
    /// CLI and the LSP report them (the no-divergence invariant).
    pub identity_diags: Vec<ResolveDiag>,
    /// The manifest's resolved `defaults:` block (0.10.0 §6). Empty when the
    /// manifest supplies none, which is every manifest written before this
    /// release.
    pub defaults: MetaDefaults,
    /// `E-DEFAULTS-KEY` diagnostics raised while resolving `defaults:`. Held
    /// on the config rather than failing the load — same treatment as
    /// `identity_diags`, for the same reason: a bad `defaults:` must not
    /// collapse the project to core-only. Reported ONCE per manifest by
    /// `lute-cli`'s manifest pass (0.10.0 §7), never once per inheriting
    /// document (D-Z).
    pub defaults_diags: Vec<ResolveDiag>,
}

/// A resolution diagnostic surfaced to the caller (folded into the check
/// result). `code` is the stable, machine-readable `E-*` code of the underlying
/// `LoadError`/`ResolveError`/`AssembleError` (so a consumer can key on it); the
/// message is the `Debug` form for human display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveDiag {
    pub code: String,
    pub message: String,
}

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
}

#[derive(Debug, Deserialize)]
struct RawProfile {
    #[serde(default)]
    extends: Option<String>,
    /// Each entry activates a plugin: `true` (presence-only) or a mapping of
    /// option values. Kept as raw YAML so `true` and a map coexist under one key.
    #[serde(default)]
    plugins: BTreeMap<String, serde_yaml::Value>,
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

/// 0.7.0's hardcoded `voiceKey` shape (v1: the voice bank IS the speaker,
/// dsl §11).
pub const DEFAULT_VOICE_KEY_TEMPLATE: &str = "{speaker}-{code}";

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
pub const DEFAULTABLE_KEYS: [&str; 10] = [
    "character", "components", "contentLang", "episode", "extends", "kind", "luteVersion",
    "pov", "season", "uses",
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
}

/// Raw `identity:` block — both keys optional, each defaulting to its 0.7.0
/// shape independently (a project may retemplate `lineId` alone).
#[derive(Debug, Default, Deserialize)]
struct RawIdentity {
    #[serde(rename = "lineId", default)]
    line_id: Option<String>,
    #[serde(rename = "voiceKey", default)]
    voice_key: Option<String>,
}

/// The resolved `identity:` block (0.8.0 §9, adoption G4): the `lineId` and
/// `voiceKey` join shapes the compiler stamps onto every line.
///
/// Pre-0.8.0 both were hardcoded, which blocked adopters whose existing assets
/// already key voice/translation tables on a different convention (OSHiZ's
/// 6,640 rows use `npc_koyuki_ep05.koyuki-0010`, i.e. a `-` join). Templating
/// them costs nothing when unused: [`Default`] IS the 0.7.0 behavior.
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
pub fn render_identity_template(
    template: &str,
    prefix: &str,
    speaker: &str,
    code: &str,
) -> String {
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
            code: E_IDENTITY_TEMPLATE.to_string(),
            message: format!("identity template `{field}` resolves to an empty string"),
        });
    }
    ok
}

/// Resolve the raw `identity:` block: each key defaults to its 0.7.0 shape
/// independently, and a REJECTED key falls back to that same default (fail
/// closed — a malformed template must never reach the artifact). Returns the
/// resolved pair plus its `E-IDENTITY-TEMPLATE` diagnostics.
fn resolve_identity(raw: Option<RawIdentity>) -> (IdentityTemplates, Vec<ResolveDiag>) {
    let raw = raw.unwrap_or_default();
    let mut diags = Vec::new();
    let mut resolved = IdentityTemplates::default();
    if let Some(t) = raw.line_id {
        if validate_template(&t, "lineId", &mut diags) {
            resolved.line_id = t;
        }
    }
    if let Some(t) = raw.voice_key {
        if validate_template(&t, "voiceKey", &mut diags) {
            resolved.voice_key = t;
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
        _ => Ok(()),
    }
}

/// Resolve `defaults:` (0.10.0 §6.1): reject every key outside the closed
/// set with a did-you-mean, reject every value whose shape cannot inhabit
/// its key, and keep the rest. A rejected entry is NOT applied — a default
/// nobody can act on must not silently reach a document.
fn resolve_defaults(raw: Option<serde_yaml::Mapping>) -> (MetaDefaults, Vec<ResolveDiag>) {
    let mut out = MetaDefaults::default();
    let mut diags = Vec::new();
    let Some(raw) = raw else { return (out, diags) };
    for (k, v) in raw {
        let Some(key) = k.as_str() else {
            diags.push(ResolveDiag {
                code: E_DEFAULTS_KEY.to_string(),
                message: "every `defaults:` key must be a string (0.10.0 §6.1)".to_string(),
            });
            continue;
        };
        if !DEFAULTABLE_KEYS.contains(&key) {
            let hint = match crate::suggest::nearest(key, DEFAULTABLE_KEYS, 3) {
                Some(near) => format!(" — did you mean `{near}`?"),
                None => String::new(),
            };
            diags.push(ResolveDiag {
                code: E_DEFAULTS_KEY.to_string(),
                message: format!(
                    "`defaults.{key}` is not a defaultable frontmatter key{hint} \
                     The defaultable set is closed: {} (0.10.0 §6.1)",
                    DEFAULTABLE_KEYS.join(", ")
                ),
            });
            continue;
        }
        if let Err(want) = defaults_shape_ok(key, &v) {
            diags.push(ResolveDiag {
                code: E_DEFAULTS_KEY.to_string(),
                message: format!("`defaults.{key}` must be {want} (0.10.0 §6.1)"),
            });
            continue;
        }
        out.entries.insert(key.to_string(), v);
    }
    (out, diags)
}

/// Read `<project_dir>/lute.project.yaml` into a [`ProjectConfig`].
///
/// Distinguishes an absent config from a broken one (plugin §11): a missing
/// file → `Ok(None)` (the document legitimately resolves core-only); a read
/// error other than not-found or a YAML parse/deserialize error → `Err(msg)`
/// so the caller can surface it instead of silently mis-validating; a valid
/// file → `Ok(Some(cfg))`.
///
/// A malformed `identity:` template is NOT a load failure: the offending key
/// falls back to its 0.7.0 default and the `E-IDENTITY-TEMPLATE` rides along
/// in [`ProjectConfig::identity_diags`], so the project still resolves its
/// plugins and both surfaces report the same diagnostic.
pub fn load_project(project_dir: &Path) -> Result<Option<ProjectConfig>, String> {
    let path = project_dir.join("lute.project.yaml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let raw: RawProject =
        serde_yaml::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?;

    let mut profiles = BTreeMap::new();
    for (name, rp) in raw.profiles {
        let plugins: ActivationMap = rp
            .plugins
            .iter()
            .map(|(id, value)| (id.clone(), plugin_options(value)))
            .collect();
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
    let (identity, identity_diags) = resolve_identity(raw.identity);
    let (defaults, defaults_diags) = resolve_defaults(raw.defaults);

    Ok(Some(ProjectConfig {
        graph,
        plugins_dir,
        catalog_dir,
        identity,
        identity_diags,
        defaults,
        defaults_diags,
    }))
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

    // `identity:` was validated at load; replay its diagnostics here so the ONE
    // shared resolver remains the single reporting seam for both surfaces.
    let mut diags = project.identity_diags.clone();

    // 1. Load every installed plugin package; surface load errors.
    let (registry, load_errs) = load_plugins_dir(&project.plugins_dir);
    diags.extend(load_errs.into_iter().map(|e| ResolveDiag {
        code: e.code().into(),
        message: format!("{e:?}"),
    }));

    // 2. Pick the profile: scene override, else the graph's default.
    let selected = scene_profile.unwrap_or(project.graph.default_profile.as_str());

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
                code: e.code().into(),
                message: format!("{e:?}"),
            });
            // No conforming activation → fall back to the core-only baseline so
            // the caller still gets a usable snapshot.
            return (load_core_snapshot(), diags);
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
                code: e.code().into(),
                message: e.message(),
            }),
    );

    // 5. Assemble the merged snapshot; surface assembly errors.
    let (snapshot, assemble_errs) = crate::assemble::assemble_snapshot(&active, &registry);
    diags.extend(assemble_errs.into_iter().map(|e| ResolveDiag {
        code: e.code().into(),
        message: e.to_string(),
    }));

    (snapshot, diags)
}
