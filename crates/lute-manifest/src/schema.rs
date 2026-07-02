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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, Type>,
    pub cel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
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
}
