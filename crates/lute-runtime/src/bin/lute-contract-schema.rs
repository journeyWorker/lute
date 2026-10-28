use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lute_ir::{BeatOnce, ProjectIndex};
use lute_manifest::bindings::Bindings;
use lute_manifest::clock::ClockAt;
use lute_manifest::semantics::prereq::Atom;
use lute_runtime::runtime::{
    Await, Evaluation, Event, Gate, Input, Output, Rejected, Seed, Snapshot, StreamLine,
};
use lute_runtime::session::{Candidate, ClockView, Premise, Verdict, WorldView};
use lute_runtime::GuardRead;
use schemars::generate::SchemaSettings;
use schemars::schema_for;
use serde_json::{json, Map, Value};
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

/// The release line (`major.minor`) the schema files are named for, from the
/// workspace version: `0.53.0` writes `lute-*-0.53.schema.json`.
fn release_line() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts = version.splitn(3, '.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        _ => panic!("CARGO_PKG_VERSION {version:?} is not major.minor.patch"),
    }
}

fn main() {
    let line = release_line();
    let id =
        |name: &str| format!("https://lute-lang.vercel.app/schemas/lute-{name}-{line}.schema.json");
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
    collect::<Evaluation>("Evaluation", &mut defs);
    collect::<Gate>("Gate", &mut defs);
    collect::<GuardRead>("GuardRead", &mut defs);
    collect::<Atom>("Atom", &mut defs);
    collect::<BeatOnce>("BeatOnce", &mut defs);
    collect::<ClockAt>("ClockAt", &mut defs);
    let mut bindings = schema::<Bindings>();
    bindings.as_object_mut().unwrap().insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    bindings
        .as_object_mut()
        .unwrap()
        .insert("$id".into(), json!(id("player-bindings")));
    let event = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": id("events"),
        "title": format!("Lute runtime events (eventVersion {line}.x)"),
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
        for (key, value) in std::mem::take(found) {
            snap_defs.insert(key, value);
        }
    }
    snap.as_object_mut().unwrap().insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    snap.as_object_mut()
        .unwrap()
        .insert("$id".into(), json!(id("snapshot")));
    snap.as_object_mut()
        .unwrap()
        .insert("$defs".into(), serde_json::to_value(snap_defs).unwrap());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    fs::create_dir_all(&root).unwrap();
    write(root.join(format!("lute-events-{line}.schema.json")), event);
    write(root.join(format!("lute-snapshot-{line}.schema.json")), snap);
    write(
        root.join(format!("lute-player-bindings-{line}.schema.json")),
        bindings,
    );
    let ir = root.join(format!("lute-ir-{line}.schema.json"));
    let text = fs::read_to_string(&ir).expect("read the IR schema");
    fs::write(&ir, splice_ir_defs(&text, &index_defs())).expect("write the IR schema");
}

/// 0.41.0 §4.4: the `$defs` of the IR schema generated from the Rust index
/// types. Every other key of that file is hand-written.
const INDEX_DEFS: [&str; 8] = [
    "projectIndex",
    "indexDocument",
    "indexEntry",
    "indexBeat",
    "indexOccasion",
    "indexStateDomain",
    "indexDef",
    "fieldType",
];

/// The index types' schemas, keyed as [`INDEX_DEFS`]. They describe the
/// index as the compiler writes it (the serialize contract): a key the
/// writer omits when empty is optional, and is never `null`. A type with no
/// key of its own (an occasion's `select`, a beat's `once`, …) is inlined;
/// a shape the hand-written part defines is a `$ref` to it.
fn index_defs() -> BTreeMap<String, Value> {
    let root = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<ProjectIndex>();
    let Value::Object(mut root) = serde_json::to_value(root).expect("schema serializes") else {
        unreachable!("a struct's schema is an object")
    };
    let Some(Value::Object(found)) = root.remove("$defs") else {
        panic!("the index schema has no $defs");
    };
    root.remove("$schema");
    root.remove("title");
    let mut defs: BTreeMap<String, Value> = found.into_iter().collect();
    defs.insert("projectIndex".into(), Value::Object(root));
    #[expect(
        clippy::incompatible_msrv,
        reason = "dev-only schema generator behind `json-schema`, built with the pinned CI toolchain"
    )]
    let helpers: BTreeMap<String, Value> = defs
        .extract_if(.., |name, _| !INDEX_DEFS.contains(&name.as_str()))
        .collect();
    for value in defs.values_mut() {
        inline(value, &helpers);
        drop_null_options(value);
        strip_numeric_formats(value);
    }
    assert_eq!(
        defs.keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        INDEX_DEFS.into_iter().collect(),
        "the index types define exactly the generator-owned $defs"
    );
    defs
}

/// Replace every `$ref` to a `helpers` schema with the schema itself, the
/// referring object's own keys (its `description`) over it.
fn inline(value: &mut Value, helpers: &BTreeMap<String, Value>) {
    match value {
        Value::Object(object) => {
            let helper = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
                .and_then(|name| helpers.get(name));
            if let Some(helper) = helper {
                let mut merged = helper.as_object().cloned().unwrap_or_default();
                object.remove("$ref");
                merged.extend(std::mem::take(object));
                *object = merged;
            }
            for child in object.values_mut() {
                inline(child, helpers);
            }
        }
        Value::Array(items) => {
            for item in items {
                inline(item, helpers);
            }
        }
        _ => {}
    }
}

/// An optional property of an object schema (one not `required`) is
/// omitted when absent, never written `null`: drop the `null` alternative
/// schemars adds for an `Option`.
fn drop_null_options(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let required: Vec<String> = object
                .get("required")
                .and_then(Value::as_array)
                .map(|r| {
                    r.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(Value::Object(properties)) = object.get_mut("properties") {
                for (name, property) in properties.iter_mut() {
                    if !required.contains(name) {
                        drop_null(property);
                    }
                }
            }
            for child in object.values_mut() {
                drop_null_options(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                drop_null_options(item);
            }
        }
        _ => {}
    }
}

fn drop_null(property: &mut Value) {
    let Some(object) = property.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(types)) = object.get_mut("type") {
        types.retain(|t| t != "null");
        if let [only] = types.as_slice() {
            let only = only.clone();
            object.insert("type".into(), only);
        }
        return;
    }
    let Some(Value::Array(choices)) = object.get("anyOf") else {
        return;
    };
    let kept: Vec<Value> = choices
        .iter()
        .filter(|c| c.get("type").and_then(Value::as_str) != Some("null"))
        .cloned()
        .collect();
    if let [only] = kept.as_slice() {
        let mut merged = only.as_object().cloned().unwrap_or_default();
        object.remove("anyOf");
        merged.extend(std::mem::take(object));
        *object = merged;
    }
}

/// `text` (the IR schema) with each generator-owned `$defs` entry replaced
/// by its generated schema in place, the ones it lacks appended in
/// [`INDEX_DEFS`] order; every other line is kept byte for byte.
fn splice_ir_defs(text: &str, defs: &BTreeMap<String, Value>) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let open = lines
        .iter()
        .position(|l| *l == "  \"$defs\": {\n")
        .expect("the IR schema has a top-level $defs");
    let close = open
        + lines[open..]
            .iter()
            .position(|l| l.trim_end() == "  }" || l.trim_end() == "  },")
            .expect("the IR schema's $defs closes");
    let entry_key = |line: &str| {
        let rest = line.strip_prefix("    \"")?;
        Some(rest[..rest.find('"')?].to_string())
    };
    let mut starts: Vec<(usize, String)> = (open + 1..close)
        .filter_map(|i| entry_key(lines[i]).map(|key| (i, key)))
        .collect();
    starts.push((close, String::new()));
    let render = |key: &str, value: &Value| {
        let pretty = serde_json::to_string_pretty(value).expect("schema JSON");
        format!("    \"{key}\": {}", pretty.replace('\n', "\n    "))
    };
    let mut entries: Vec<String> = Vec::new();
    let mut present = Vec::new();
    for pair in starts.windows(2) {
        let ((from, key), (to, _)) = (&pair[0], &pair[1]);
        entries.push(match defs.get(key) {
            Some(value) => render(key, value),
            None => {
                let body = lines[*from..*to].concat();
                let body = body.trim_end_matches('\n');
                body.strip_suffix(',').unwrap_or(body).to_string()
            }
        });
        present.push(key.clone());
    }
    for key in INDEX_DEFS {
        if !present.iter().any(|k| k == key) {
            entries.push(render(key, &defs[key]));
        }
    }
    let mut out = lines[..=open].concat();
    out.push_str(&entries.join(",\n"));
    out.push('\n');
    out.push_str(&lines[close..].concat());
    let parsed: Value = serde_json::from_str(&out).expect("the spliced IR schema is JSON");
    check_refs(&parsed, parsed["$defs"].as_object().expect("$defs"));
    out
}

/// Every `#/$defs/<name>` reference names a definition of the file.
fn check_refs(value: &Value, defs: &Map<String, Value>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
                .and_then(|r| r.split('/').next())
            {
                assert!(defs.contains_key(name), "`$ref` to undefined `{name}`");
            }
            for child in object.values() {
                check_refs(child, defs);
            }
        }
        Value::Array(items) => {
            for item in items {
                check_refs(item, defs);
            }
        }
        _ => {}
    }
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
            if object
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(|format| {
                    matches!(
                        format,
                        "uint" | "uint32" | "uint64" | "int32" | "int64" | "float" | "double"
                    )
                })
            {
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
