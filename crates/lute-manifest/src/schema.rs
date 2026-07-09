use crate::types::{Field, Literal, PathSegment, Type};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct DirectivesFile {
    pub directives: Vec<DirectiveDecl>,
}
#[derive(Debug, Deserialize)]
pub struct ShapesFile {
    #[serde(rename = "stateShapes")]
    pub state_shapes: Vec<StateShape>,
}
#[derive(Debug, Deserialize)]
pub struct TemplatesFile {
    #[serde(rename = "stateTemplates")]
    pub state_templates: Vec<StateTemplate>,
}
#[derive(Debug, Deserialize)]
pub struct ProvidersFile {
    pub providers: Vec<ProviderDecl>,
}
#[derive(Debug, Deserialize)]
pub struct BridgeFile {
    #[serde(rename = "bridgeCapabilities")]
    pub bridge: Vec<BridgeCapability>,
}

#[derive(Debug, Deserialize)]
pub struct DefsFile {
    pub defs: Vec<DefDecl>,
}

#[derive(Debug, Deserialize)]
pub struct FrontmatterFile {
    pub frontmatter: Vec<FrontmatterDecl>,
}

#[derive(Debug, Deserialize)]
pub struct FrontmatterDecl {
    pub key: String,
    pub schema: Type,
}

#[derive(Debug, Deserialize)]
pub struct EnumsFile {
    pub enums: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct EventsFile {
    pub events: Vec<EventDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    pub attrs: Vec<AttrDecl>,
    #[serde(default)]
    pub semantics: Vec<String>, // closed vocabulary; validated in Task 1.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<DirectiveState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<DirectiveEffects>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<BridgeRef>,
    pub lower: Lowering,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttrDecl {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveState {
    pub declares: Vec<SlotDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlotDecl {
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub shape: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveEffects {
    pub writes: Vec<WriteDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteDecl {
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub value: WriteValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WriteValue {
    FromBridgeResult {
        #[serde(rename = "fromBridgeResult")]
        from_bridge_result: String,
    },
    Op {
        op: String,
        by: f64,
    },
    Literal(Literal),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeRef {
    pub service: String,
    pub operation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Lowering {
    Record {
        record: String,
        fields: serde_yaml::Value,
    },
    Builtin {
        kind: String,
        name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateShape {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateTemplate {
    pub name: String,
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub shape: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderDecl {
    pub name: String,
    #[serde(rename = "idShape", default, skip_serializing_if = "Option::is_none")]
    pub id_shape: Option<String>,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeCapability {
    pub service: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
    #[serde(default)]
    pub result: Vec<Field>,
}

/// A single declared def parameter (dsl §8.1). Order-preserving: the position of
/// a `DefParam` in [`DefDecl::params`] is the positional-binding order for
/// `@name(args)` calls.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, deserialize_with = "de_params")]
    pub params: Vec<DefParam>,
    pub cel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

/// A capability-declared world event (dsl 0.2.0 §4.5): a named event kind an
/// active plugin makes fireable via `<on event="…">`. Payload (if any) is
/// ordinary plugin `state`, written by the engine before the event fires — NOT
/// part of this declaration. Name is a `CelIdent`-shaped event kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDecl {
    pub name: String,
}

/// Deserialize `DefDecl.params` in SOURCE order (dsl §8.1). Accepts either the
/// §8.1 `params:` YAML MAPPING (`{ p: number }`) — read via `serde_yaml::Mapping`,
/// which is insertion-ordered in serde_yaml 0.9.34, so declaration order is
/// preserved for positional arg binding — OR a SEQUENCE of `{ name, type }`
/// entries (the plugin `defs.yaml` list spelling). A malformed mapping entry is
/// skipped, never a panic.
fn de_params<'de, D>(d: D) -> Result<Vec<DefParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Map(serde_yaml::Mapping),
        Seq(Vec<serde_yaml::Value>),
    }
    Ok(match Raw::deserialize(d)? {
        Raw::Seq(v) => v
            .into_iter()
            .filter_map(|v| serde_yaml::from_value::<DefParam>(v).ok())
            .collect(),
        Raw::Map(m) => m
            .into_iter()
            .filter_map(|(k, v)| {
                let name = k.as_str()?.to_string();
                let ty: Type = serde_yaml::from_value(v).ok()?;
                Some(DefParam { name, ty })
            })
            .collect(),
    })
}

/// plugin §5 manifest entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    #[serde(default)]
    pub depends: Vec<Depends>,
    pub exports: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub options: Vec<OptionDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Depends {
    pub id: String,
    pub range: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

/// plugin §6.9 asset-kind declaration (export file `assetkinds/*.yaml`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetKindDecl {
    pub kind: String,
    #[serde(default = "default_sep")]
    pub sep: String,
    #[serde(default)]
    pub resolve: AssetResolve,
    #[serde(default)]
    pub segments: Vec<AssetSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, rename = "match")]
    pub match_: Vec<AssetMatch>,
    #[serde(default)]
    pub aliases: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub fallback: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AssetResolve {
    #[default]
    Compose,
    Query,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSegment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#const: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMatch {
    pub attr: String,
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssetKindsFile {
    #[serde(rename = "assetKinds")]
    pub asset_kinds: Vec<AssetKindDecl>,
}

fn default_sep() -> String {
    ".".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIGAME_DIR: &str = r#"
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind, required: true, type: { enumFromOption: allowedKinds } }
      - { name: id, required: true, type: { providerRef: minigameId } }
      - { name: wait, type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    bridge: { service: minigame, operation: play }
    lower: { kind: builtin, name: bridgeMinigame }
"#;

    #[test]
    fn parses_directive_with_attrs_and_lower() {
        let file: DirectivesFile = serde_yaml::from_str(MINIGAME_DIR).unwrap();
        let d = &file.directives[0];
        assert_eq!(d.name, "minigame");
        assert_eq!(d.attrs.len(), 3);
        assert!(d.attrs[0].required);
        assert!(matches!(d.lower, Lowering::Builtin { .. }));
    }

    #[test]
    fn state_shape_field_defaults_are_typed() {
        let y = r#"
stateShapes:
  - name: minigameResult
    fields:
      - { name: rank, type: { enum: [fail, gold] }, default: fail }
"#;
        let f: ShapesFile = serde_yaml::from_str(y).unwrap();
        assert_eq!(f.state_shapes[0].fields[0].name, "rank");
    }
    #[test]
    fn write_value_untagged_variants_bind() {
        let y = r#"
writes:
  - { scope: scene, path: [minigame, rank], value: { fromBridgeResult: rank } }
  - { scope: scene, path: [minigame, attempts], value: { op: increment, by: 1 } }
  - { scope: scene, path: [flags, done], value: true }
"#;
        let e: DirectiveEffects = serde_yaml::from_str(y).unwrap();
        assert!(matches!(
            e.writes[0].value,
            WriteValue::FromBridgeResult { .. }
        ));
        assert!(matches!(e.writes[1].value, WriteValue::Op { .. }));
        assert!(matches!(e.writes[2].value, WriteValue::Literal(_)));
    }

    #[test]
    fn lowering_record_form_binds() {
        let y = "record: setBackground\nfields: {}";
        let l: Lowering = serde_yaml::from_str(y).unwrap();
        assert!(matches!(l, Lowering::Record { .. }));
    }

    #[test]
    fn plugin_manifest_parses_spec_entry() {
        let y = r#"
id: idola.minigame
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports: { directives: directives/, state: state/ }
options:
  - { name: resultScope, type: { enum: [scene, run] }, default: scene }
"#;
        let m: PluginManifest = serde_yaml::from_str(y).unwrap();
        assert_eq!(m.id, "idola.minigame");
        assert_eq!(m.kind, "capability");
        assert_eq!(m.depends.len(), 1);
        assert_eq!(m.options[0].name, "resultScope");
    }

    #[test]
    fn asset_kind_decl_parses_ch() {
        let y = r#"
assetKinds:
  - kind: CH
    sep: "."
    segments:
      - { name: prefix,      const: CH }
      - { name: characterId, type: { providerRef: character } }
      - { name: costume,     type: string }
      - { name: emotion,     type: { enum: [delighted, content, neutral] } }
      - { name: variant,     type: number }
    fallback: [emotionGroup, neutral, variant0]
    persistence: scene
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "CH");
        assert_eq!(d.sep, ".");
        assert_eq!(d.resolve, AssetResolve::Compose);
        assert_eq!(d.segments.len(), 5);
        assert_eq!(
            d.segments
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["prefix", "characterId", "costume", "emotion", "variant"]
        );
        assert_eq!(d.segments[0].r#const.as_deref(), Some("CH"));
        assert_eq!(d.segments[0].ty, None);
        assert_eq!(
            d.segments[1].ty,
            Some(Type::ProviderRef("character".into()))
        );
        assert_eq!(d.segments[2].ty, Some(Type::Str));
        assert_eq!(
            d.segments[3].ty,
            Some(Type::Enum(vec![
                "delighted".into(),
                "content".into(),
                "neutral".into(),
            ]))
        );
        assert_eq!(d.segments[4].ty, Some(Type::Number));
        assert_eq!(d.fallback, ["emotionGroup", "neutral", "variant0"]);
        assert_eq!(d.persistence.as_deref(), Some("scene"));
    }

    #[test]
    fn asset_kind_decl_parses_bg_query() {
        let y = r#"
assetKinds:
  - kind: BG
    resolve: query
    provider: backgrounds
    aliases: { location: locationAlias }
    match: [ { attr: location, field: spaceId, via: locationAlias },
             { attr: time, field: timeOfDay }, { attr: view, field: view },
             { attr: variation, field: variation } ]
    fallback: [ dropVariation, areaKind, preferAfternoon, anyView ]
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "BG");
        assert_eq!(d.resolve, AssetResolve::Query);
        assert_eq!(d.provider.as_deref(), Some("backgrounds"));
        assert_eq!(d.match_.len(), 4);
        assert_eq!(d.match_[0].attr, "location");
        assert_eq!(d.match_[0].field, "spaceId");
        assert_eq!(d.match_[0].via.as_deref(), Some("locationAlias"));
        assert_eq!(d.match_[1].via, None);
        assert_eq!(d.aliases["location"], "locationAlias");
        assert_eq!(
            d.fallback,
            ["dropVariation", "areaKind", "preferAfternoon", "anyView"]
        );
    }

    #[test]
    fn def_params_mapping_deserializes_in_source_order() {
        // §8.1 `params:` MAPPING spelling — order MUST be preserved for positional
        // arg binding (serde_yaml::Mapping is insertion-ordered).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params: { a: number, b: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        let d = &file.defs[0];
        assert_eq!(
            d.params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Number
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_spelling_deserializes() {
        // The plugin `defs.yaml` list spelling `[{ name, type }]` also works.
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: number }\n      - { name: b, type: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Number
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_skips_malformed_entry() {
        // §8.1 SEQUENCE spelling MUST be fail-soft: one malformed entry (here
        // missing `type`) is skipped, not fatal — the file still keeps its good
        // params rather than being rejected wholesale (mirrors the MAPPING path).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: number }\n      - { name: b }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![DefParam {
                name: "a".into(),
                ty: Type::Number
            }]
        );
    }

    #[test]
    fn def_params_absent_yields_empty() {
        let src = "defs:\n  - name: bare\n    type: bool\n    cel: \"true\"\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert!(file.defs[0].params.is_empty());
    }
}
