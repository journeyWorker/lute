//! `lute context` additions (dsl 0.22.0 §13): the surface an author (or an AI
//! writing Lute) needs beyond the capability snapshot — the document's defs
//! with their types, parameters and bodies, the language's built-in
//! directives, and every scene / quest / entry id the project declares.
//!
//! The capability surface itself (directives, state schema, relations, …)
//! is assembled by `authoring_surface` in `main.rs`; this module adds the
//! keys that do not come from a snapshot, and renders them in the outline.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// The language's built-in directives: recognized by tag in every document,
/// never looked up in a capability snapshot, so the snapshot-derived
/// `directives` list cannot show them. `(name, syntax, meaning)`.
const BUILTIN_DIRECTIVES: &[(&str, &str, &str)] = &[
    (
        "set",
        "::set{ <path> = <expr> }  (also += / -=)",
        "write a declared state path; `owner: engine` paths are the engine's (E-ENGINE-OWNED-WRITE)",
    ),
    (
        "assert",
        "::assert{ <relation>(<arg>, …) }",
        "assert a ground fact of a declared, non-derived, non-reserved relation",
    ),
    (
        "retract",
        "::retract{ <relation>(<arg | _>, …) }",
        "retract the matching facts of a declared, non-derived, non-reserved relation",
    ),
    (
        "accept",
        "::accept{quest=\"<questId>\"}",
        "accept a quest that has no `start` condition",
    ),
    (
        "use",
        "::use{component=\"<name>\" <param>=<value> …}",
        "expand an imported component with named arguments",
    ),
];

/// Add the non-snapshot keys to the authoring `surface`:
///
/// - `defs`: every def the document can `@ref` (plugin < imported < inline,
///   the checker's own precedence), name-sorted, with its (declared or
///   inferred) result type, ordered params, and CEL body;
/// - `builtinDirectives`: [`BUILTIN_DIRECTIVES`];
/// - `ids`: the scene, quest and entry ids declared across the project
///   (`--project <dir>`), or in the document alone without one.
pub(crate) fn extend_surface(
    surface: &mut Value,
    folded: &lute_check::FoldedEnv,
    file: &Path,
    project: Option<&Path>,
) {
    let Some(root) = surface.as_object_mut() else {
        return;
    };
    let env = &folded.env;
    let defs: Vec<Value> = folded
        .def_bodies
        .iter()
        .map(|(name, body)| {
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            if let Some(ty) = env.def_types.get(name) {
                o.insert("type".into(), crate::attr_type_str(ty).0.into());
            }
            let params: Vec<Value> = env
                .def_params
                .get(name)
                .map(|ps| {
                    ps.iter()
                        .map(|(p, ty)| json!({ "name": p, "type": crate::attr_type_str(ty).0 }))
                        .collect()
                })
                .unwrap_or_default();
            o.insert("params".into(), params.into());
            o.insert("body".into(), body.clone().into());
            Value::Object(o)
        })
        .collect();
    root.insert("defs".into(), defs.into());

    let builtins: Vec<Value> = BUILTIN_DIRECTIVES
        .iter()
        .map(|(name, syntax, meaning)| json!({ "name": name, "syntax": syntax, "meaning": meaning }))
        .collect();
    root.insert("builtinDirectives".into(), builtins.into());

    root.insert("ids".into(), project_ids(file, project));
}

/// The ids a document may name in `visited(…)`, `after:`, `completed(…)`,
/// `::accept`, `quest.<id>.state` and `entry.<id>.read`: every scene key
/// (authored `id:` or the derived `{character}.sNNepNN`), quest id, lore
/// entry id, and lore bundle beat canonical id (`<document id>.<beat id>`,
/// dsl 0.23.0 §4; the `beats` key is present only when some document bundles
/// beats) — under `project` when given (the same `.lute` walk
/// `check-project` does), else in `file` alone. Parse-only: ids are
/// syntactic, and a document that does not check still declares them.
fn project_ids(file: &Path, project: Option<&Path>) -> Value {
    let files: Vec<PathBuf> = match project {
        Some(dir) => crate::find_lute_files(dir).unwrap_or_default(),
        None => vec![file.to_path_buf()],
    };
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, lute_syntax::parse(&text).0))
        })
        .collect();
    let scenes: Vec<String> = lute_check::connectivity::scene_key_set(&docs)
        .into_keys()
        .collect();
    let quests: Vec<String> = lute_check::connectivity::quest_id_set(&docs)
        .into_iter()
        .collect();
    let entries: std::collections::BTreeSet<String> = docs
        .iter()
        .flat_map(|(_, doc)| doc.entries.iter())
        .filter(|e| !e.id.is_empty())
        .map(|e| e.id.clone())
        .collect();
    let mut ids = json!({ "scenes": scenes, "quests": quests, "entries": entries });
    let beats: Vec<String> = lute_check::connectivity::bundle_beat_key_set(&docs)
        .into_keys()
        .collect();
    if !beats.is_empty() {
        ids["beats"] = json!(beats);
    }
    ids
}

fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Render the [`extend_surface`] keys in the human outline.
pub(crate) fn outline_extras(out: &mut String, surface: &Value) {
    if let Some(defs) = surface["defs"].as_array() {
        if !defs.is_empty() {
            let _ = writeln!(out, "defs ({}):", defs.len());
            for d in defs {
                let params: Vec<String> = d["params"]
                    .as_array()
                    .map(|ps| {
                        ps.iter()
                            .map(|p| {
                                format!(
                                    "{}: {}",
                                    p["name"].as_str().unwrap_or(""),
                                    p["type"].as_str().unwrap_or("")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let sig = if params.is_empty() {
                    String::new()
                } else {
                    format!("({})", params.join(", "))
                };
                let ty = d["type"]
                    .as_str()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "  @{}{sig}{ty} = {}",
                    d["name"].as_str().unwrap_or(""),
                    d["body"].as_str().unwrap_or("")
                );
            }
        }
    }
    if let Some(builtins) = surface["builtinDirectives"].as_array() {
        let _ = writeln!(out, "builtinDirectives ({}):", builtins.len());
        for b in builtins {
            let _ = writeln!(
                out,
                "  {} — {}",
                b["syntax"].as_str().unwrap_or(""),
                b["meaning"].as_str().unwrap_or("")
            );
        }
    }
    let ids = &surface["ids"];
    for (key, note) in [
        ("scenes", "read as visited(\"<id>\")"),
        ("quests", "read as quest.<id>.state; accepted by ::accept"),
        (
            "entries",
            "read as entry.<id>.read [run] / entry.<id>.everRead [user]",
        ),
        ("beats", "bundle beats; read as visited(\"<id>\")"),
    ] {
        let list = strs(&ids[key]);
        if !list.is_empty() {
            let _ = writeln!(out, "{key} ({}; {note}):", list.len());
            let _ = writeln!(out, "  {}", list.join(", "));
        }
    }
}
