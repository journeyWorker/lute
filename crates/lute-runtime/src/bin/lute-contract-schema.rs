use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lute_runtime::runtime::{Await, Event, Input, Output, Rejected, Seed, Snapshot, StreamLine};
use lute_runtime::session::{Candidate, ClockView, Premise, Verdict, WorldView};
use lute_runtime::GuardRead;
use lute_manifest::clock::ClockAt;
use lute_manifest::semantics::prereq::Atom;
use lute_ir::BeatOnce;
use schemars::schema_for;
use serde_json::{json, Value};
fn schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("schema serializes")
}

fn collect<T: schemars::JsonSchema>(name: &str, defs: &mut BTreeMap<String, Value>) {
    let mut root = schema::<T>();
    if let Some(found) = root.get_mut("$defs").and_then(Value::as_object_mut) {
        for (key, value) in std::mem::take(found) {
            defs.insert(key, value);
        }
    }
    root.as_object_mut().unwrap().remove("$schema");
    root.as_object_mut().unwrap().remove("$defs");
    defs.insert(name.to_string(), root);
}

fn main() {
    let mut defs = BTreeMap::new();
    collect::<Input>("Input", &mut defs);
    collect::<Seed>("Seed", &mut defs);
    collect::<Output>("Output", &mut defs);
    collect::<Event>("Event", &mut defs);
    collect::<Await>("Await", &mut defs);
    collect::<Rejected>("Rejected", &mut defs);
    collect::<StreamLine>("StreamLine", &mut defs);
    collect::<Candidate>("Candidate", &mut defs);
    collect::<Verdict>("Verdict", &mut defs);
    collect::<Premise>("Premise", &mut defs);
    collect::<WorldView>("WorldView", &mut defs);
    collect::<ClockView>("ClockView", &mut defs);
    collect::<GuardRead>("GuardRead", &mut defs);
    collect::<Atom>("Atom", &mut defs);
    collect::<BeatOnce>("BeatOnce", &mut defs);
    collect::<ClockAt>("ClockAt", &mut defs);
    let event = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://lute-lang.vercel.app/schemas/lute-events-0.39.schema.json",
        "title": "Lute runtime events (eventVersion 0.39.x)",
        "oneOf": [
            {"$ref":"#/$defs/Input"}, {"$ref":"#/$defs/Seed"},
            {"$ref":"#/$defs/Output"}, {"$ref":"#/$defs/Rejected"},
            {"$ref":"#/$defs/StreamLine"}
        ],
        "$defs": defs,
    });
    let mut snap = schema::<Snapshot>();
    let mut snap_defs = BTreeMap::new();
    if let Some(found) = snap.get_mut("$defs").and_then(Value::as_object_mut) {
        for (key, value) in std::mem::take(found) { snap_defs.insert(key, value); }
    }
    snap.as_object_mut().unwrap().insert("$schema".into(), json!("https://json-schema.org/draft/2020-12/schema"));
    snap.as_object_mut().unwrap().insert("$id".into(), json!("https://lute-lang.vercel.app/schemas/lute-snapshot-0.39.schema.json"));
    snap.as_object_mut().unwrap().insert("$defs".into(), serde_json::to_value(snap_defs).unwrap());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    fs::create_dir_all(&root).unwrap();
    write(root.join("lute-events-0.39.schema.json"), event);
    write(root.join("lute-snapshot-0.39.schema.json"), snap);
}

fn write(path: impl AsRef<Path>, mut value: Value) {
    strip_numeric_formats(&mut value);
    let mut bytes = serde_json::to_vec_pretty(&value).expect("schema JSON");
    bytes.push(b'\n');
    fs::write(path, bytes).expect("write schema");
}

// Rust's numeric formats are not registered JSON Schema formats. The
// generated type and minimum constraints already express their wire shape.
fn strip_numeric_formats(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("format").and_then(Value::as_str).is_some_and(|format| {
                matches!(format, "uint" | "uint32" | "uint64" | "int32" | "int64" | "float" | "double")
            }) {
                object.remove("format");
            }
            for child in object.values_mut() {
                strip_numeric_formats(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                strip_numeric_formats(child);
            }
        }
        _ => {}
    }
}
