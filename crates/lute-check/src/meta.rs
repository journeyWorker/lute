use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::Meta;

/// State lifetime tier (dsl §9.1), keyed by the declared path's leading segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Scene,
    Run,
    User,
    App,
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
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
}

/// Core built-in top-level meta keys (dsl §6.1). Keys here are never "unknown";
/// keys outside this set and not owned by an active plugin's `frontmatter`
/// export are a static error (dsl §6.1).
const BUILTIN_KEYS: &[&str] = &[
    "character",
    "season",
    "episode",
    "mode",
    "pov",
    "title",
    "luteVersion",
    "contentLang",
    "profile",
    "plugins",
    "uses",
    "state",
    "defs",
];

const REQUIRED_KEYS: &[&str] = &["character", "season", "episode"];

/// Parse the peeled YAML frontmatter (dsl §6.1) into typed form plus the inline
/// `state:` schema (dsl §9.3). Never panics on malformed YAML: a parse failure
/// surfaces `E-META-PARSE` and yields a best-effort (empty) `TypedMeta`.
///
/// This performs the §6.1 required-key and unknown-key checks and records each
/// `state:` path's `Namespace` from its leading segment. App-write read-only
/// enforcement (§9.5, Task 4.5) and def-assignment (§8.1, Task 4.4) are NOT done
/// here.
pub fn parse_meta(meta: &Meta, snapshot: &CapabilitySnapshot) -> (TypedMeta, Vec<Diagnostic>) {
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

    // Required + unknown-key checks over the top-level keys (dsl §6.1).
    for missing in REQUIRED_KEYS
        .iter()
        .filter(|k| !map.contains_key(yaml_key(k)))
    {
        diags.push(err(
            "E-META-MISSING",
            format!("required meta key `{missing}` is missing"),
        ));
    }
    for (k, _) in map.iter() {
        let Some(key) = k.as_str() else {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                "meta keys must be strings".to_string(),
            ));
            continue;
        };
        let known = BUILTIN_KEYS.contains(&key) || snapshot.frontmatter.contains_key(key);
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
    typed.uses = get_uses(map);
    typed.plugins = get_sub_map(map, "plugins");
    typed.defs = get_sub_map(map, "defs");

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

/// `uses` (dsl §9.2) may be a single ref or a list of refs; normalize to a Vec.
fn get_uses(map: &serde_yaml::Mapping) -> Vec<String> {
    match map.get(yaml_key("uses")) {
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
}
