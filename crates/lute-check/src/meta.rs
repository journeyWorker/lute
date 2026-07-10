use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::DefParam;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::Meta;

use crate::cel_paths::{state_path_has_hyphen, E_PATH_IDENT};

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

/// Typed frontmatter (dsl §6.1). Built-in core keys are lifted into fields;
/// `plugins`/`defs` are retained structurally for downstream tasks.
#[derive(Clone, Debug, Default)]
pub struct TypedMeta {
    pub character: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub pov: Option<String>,
    pub profile: Option<String>,
    pub plugins: BTreeMap<String, serde_yaml::Value>,
    pub uses: Vec<String>,
    pub extends: Vec<String>,
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
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
    "components",
];

/// Frontmatter keys valid ONLY in a `MetaKind::Scene` document (dsl 0.1.0 §6.1,
/// dsl 0.2.0 §3.1/§6.1): the scene identity triad plus the scene-only extras.
/// A Quest document declaring any of these is `E-META-UNKNOWN-KEY`.
const SCENE_KEYS: &[&str] = &["character", "season", "episode", "episodeId", "pov"];

/// Frontmatter keys that are valid ONLY in a component file (dsl §13): the
/// component's own name (`component:`) and its parameter signature (`params:`).
/// In a scene or schema doc these are unknown top-level keys.
const COMPONENT_ONLY_KEYS: &[&str] = &["component", "params"];

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
            let kind_str = v.as_str().map(str::to_string).unwrap_or_else(|| format!("{v:?}"));
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
    };

    let value: serde_yaml::Value = match serde_yaml::from_str(&meta.raw_yaml) {
        Ok(v) => v,
        Err(e) => {
            diags.push(err(
                "E-META-PARSE",
                format!("invalid meta frontmatter YAML: {e}"),
            ));
            return (TypedMeta::default(), diags);
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

    let mut typed = TypedMeta::default();

    // Required-key check (dsl §6.1); only scenes carry required core keys.
    // Schema docs imported via `uses:` (§9.2) are not scenes.
    if kind == MetaKind::Scene {
        for missing in REQUIRED_KEYS
            .iter()
            .filter(|k| !map.contains_key(yaml_key(k)))
        {
            diags.push(err(
                "E-META-MISSING",
                format!("required meta key `{missing}` is missing"),
            ));
        }
    }
    // Unknown-key check over the top-level keys (dsl §6.1); applies to every kind.
    // `component:`/`params:` (dsl §13) are allowed ONLY in a component file, so a
    // scene or schema doc that declares them hits the unknown-key diagnostic.
    let component_key_allowed = kind == MetaKind::Component;
    for (k, _) in map.iter() {
        let Some(key) = k.as_str() else {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                "meta keys must be strings".to_string(),
            ));
            continue;
        };
        let known = UNIVERSAL_KEYS.contains(&key)
            || (key == "kind" && matches!(kind, MetaKind::Scene | MetaKind::Quest))
            || (kind == MetaKind::Scene && SCENE_KEYS.contains(&key))
            || (component_key_allowed && COMPONENT_ONLY_KEYS.contains(&key))
            || snapshot.frontmatter.contains_key(key);
        if !known {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                format!("unknown top-level meta key `{key}` (not a core key and not owned by an active plugin)"),
            ));
        }
    }

    // Lift the built-in scalar/collection keys.
    typed.character = get_str(map, "character");
    typed.season = get_i64(map, "season");
    typed.episode = get_i64(map, "episode");
    typed.pov = get_str(map, "pov");
    typed.profile = get_str(map, "profile");
    typed.uses = get_ref_list(map, "uses");
    typed.extends = get_ref_list(map, "extends");
    typed.plugins = get_sub_map(map, "plugins");
    typed.defs = get_sub_map(map, "defs");
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
                        Err(e) => diags.push(err(
                            "E-STATE-DECL",
                            format!("invalid state declaration for `{path}`: {e}"),
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

/// Best-effort narrow document span for a meta-side diagnostic pointing at the
/// offending identifier (a hyphenated `defs` name / def param / state path).
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
fn meta_key_span(meta: &Meta, needle: &str) -> Span {
    // `raw_yaml` is the frontmatter interior sliced verbatim after the 4-byte
    // `"---\n"` opener (itself included in `meta.span`); a `raw_yaml` offset maps
    // to the document by adding `meta.span.byte_start + 4`.
    const OPENER_LEN: usize = 4; // "---\n"
    let base = meta.span.byte_start + OPENER_LEN;
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
}
