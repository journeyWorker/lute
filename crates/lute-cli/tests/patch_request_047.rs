//! dsl 0.47.0 §5.2 (#124): `lute schema patch` describes the `PATCH` request,
//! and a request built only from that description goes through the patch
//! request parser — every op, every preserve key, every node-key kind — so the
//! description cannot drift from the parser.

use std::process::Command;

use lute_model::PatchRequest;
use serde_json::{json, Map, Value};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn patch_schema() -> Value {
    let out = Command::new(BIN)
        .args(["schema", "patch"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["schemaVersion"], "0.48.0.schema");
    value
}

fn request_input(schema: &Value) -> Value {
    let inputs = schema["inputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 1, "{inputs:?}");
    inputs[0].clone()
}

/// A value of a described field type.
fn sample(ty: &str, input: &Value) -> Value {
    if let Some(item) = ty.strip_suffix("[]") {
        return match item {
            "edit" => Value::Array(
                input["ops"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|op| {
                        let mut edit = Map::new();
                        edit.insert("op".into(), op["op"].clone());
                        for field in op["fields"].as_array().unwrap() {
                            let name = field["name"].as_str().unwrap();
                            edit.insert(name.into(), sample(field["type"].as_str().unwrap(), input));
                        }
                        Value::Object(edit)
                    })
                    .collect(),
            ),
            "preserve" => Value::Array(
                input["preserve"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|key| {
                        json!({ key["key"].as_str().unwrap(): sample(key["type"].as_str().unwrap(), input) })
                    })
                    .collect(),
            ),
            _ => json!([sample(item, input)]),
        };
    }
    match ty {
        "string" => json!("x"),
        "true" => json!(true),
        "path" => json!("scenes/fair.lute"),
        "sha256" => json!(format!("sha256:{}", "0".repeat(64))),
        "nodeKey" => json!("line:fair.narrator_0020"),
        "map<path,sha256>" => json!({ "scenes/fair.lute": sample("sha256", input) }),
        "span" => {
            json!({"byte_start": 0, "byte_end": 0, "line": 1, "column": 1, "utf16_range": [0, 0]})
        }
        other => panic!("undocumented field type `{other}`"),
    }
}

fn object(fields: &Value, input: &Value) -> Value {
    let mut out = Map::new();
    for field in fields.as_array().unwrap() {
        let name = field["name"].as_str().unwrap();
        let value = match field.get("fields") {
            Some(nested) => object(nested, input),
            None => sample(field["type"].as_str().unwrap(), input),
        };
        out.insert(name.into(), value);
    }
    Value::Object(out)
}

#[test]
fn the_described_request_round_trips_through_the_parser() {
    let schema = patch_schema();
    let input = request_input(&schema);
    assert_eq!(input["kind"], "request");
    assert_eq!(input["format"], "json");
    assert!(
        schema["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == input["argument"]),
        "{} is not an argument of `lute patch`",
        input["argument"]
    );
    let request = object(&input["fields"], &input);
    let parsed: PatchRequest = serde_json::from_value(request.clone())
        .unwrap_or_else(|error| panic!("described request does not parse: {error}\n{request:#}"));

    let ops: Vec<&str> = input["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["op"].as_str().unwrap())
        .collect();
    assert_eq!(
        ops,
        [
            "replaceNode",
            "insertBefore",
            "insertAfter",
            "replaceAttr",
            "removeNode",
            "replaceText",
            "createFile",
            "moveFile"
        ]
    );
    let back = serde_json::to_value(&parsed).unwrap();
    let parsed_ops: Vec<&str> = back["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| edit["op"].as_str().unwrap())
        .collect();
    assert_eq!(parsed_ops, ops);
    let keys: Vec<&str> = input["preserve"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["key"].as_str().unwrap())
        .collect();
    let parsed_keys: Vec<String> = back["preserve"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_object().unwrap().keys().next().unwrap().clone())
        .collect();
    assert_eq!(parsed_keys, keys);
    assert_eq!(keys.len(), 9, "{keys:?}");
}

#[test]
fn every_described_field_is_exactly_what_the_parser_requires() {
    let input = request_input(&patch_schema());
    let request = object(&input["fields"], &input);
    // Each edit: dropping a required field or adding an undescribed one fails.
    for (index, edit) in request["edits"].as_array().unwrap().iter().enumerate() {
        let op = &input["ops"][index];
        for field in op["fields"].as_array().unwrap() {
            let name = field["name"].as_str().unwrap();
            let mut broken = request.clone();
            broken["edits"] = json!([edit]);
            broken["edits"][0].as_object_mut().unwrap().remove(name);
            let result = serde_json::from_value::<PatchRequest>(broken);
            assert_eq!(
                result.is_err(),
                field["required"] == true,
                "{} without {name}",
                op["op"]
            );
        }
        let mut extra = request.clone();
        extra["edits"] = json!([edit]);
        extra["edits"][0]["undescribed"] = json!("x");
        assert!(
            serde_json::from_value::<PatchRequest>(extra).is_err(),
            "{} accepts an undescribed field",
            op["op"]
        );
    }
    // Top level and base: required fields are required, optional ones are not.
    for (path, fields) in [
        (None, &input["fields"]),
        (Some("base"), &input["fields"][0]["fields"]),
    ] {
        for field in fields.as_array().unwrap() {
            let name = field["name"].as_str().unwrap();
            let mut broken = request.clone();
            let target = match path {
                Some(key) => &mut broken[key],
                None => &mut broken,
            };
            target.as_object_mut().unwrap().remove(name);
            assert_eq!(
                serde_json::from_value::<PatchRequest>(broken).is_err(),
                field["required"] == true,
                "request without {path:?}.{name}"
            );
        }
    }
    // Every described node-key kind parses as a target.
    let kinds = input["nodeKinds"].as_array().unwrap();
    assert_eq!(kinds.len(), 20);
    let mut targets = request.clone();
    targets["targets"] = Value::Array(
        kinds
            .iter()
            .map(|kind| json!(format!("{}:k", kind["kind"].as_str().unwrap())))
            .collect(),
    );
    serde_json::from_value::<PatchRequest>(targets).unwrap();
    let untargetable: Vec<&str> = kinds
        .iter()
        .filter(|kind| kind["targetable"] == false)
        .map(|kind| kind["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        untargetable,
        ["project", "occasion", "relation", "state", "expanded", "fact", "engine"]
    );
}

/// A target of a kind the description marks untargetable is refused.
#[test]
fn an_untargetable_kind_is_refused_as_a_target() {
    let root = lute_scratch::ScratchDir::named("lute-patch-request-047");
    std::fs::write(
        root.join("scene.lute"),
        "---\nkind: scene\nid: fair\n---\n\n## Opening\n\n@narrator: Hello\n",
    )
    .unwrap();
    let model =
        lute_model::ProjectModel::build_single_root(&root, &lute_model::ModelOptions::default())
            .unwrap();
    let project = model
        .graph()
        .nodes
        .keys()
        .find(|key| key.kind == lute_semantic::NodeKind::Project)
        .cloned()
        .unwrap();
    let request: PatchRequest = serde_json::from_value(json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": [project.canonical()],
        "edits": [],
    }))
    .unwrap();
    let refusal =
        lute_model::apply_patch_to(&model, request, true, &lute_model::ModelOptions::default())
            .unwrap_err();
    assert_eq!(refusal.code(), "E-PATCH-TARGET");
}
