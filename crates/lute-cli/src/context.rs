//! `lute context` additions (dsl 0.22.0 §13): the surface an author (or an AI
//! writing Lute) needs beyond the capability snapshot — the document's defs
//! with their types, parameters and bodies, the language's built-in
//! directives, beat keys and quest attributes, the project's clock,
//! `terminal:`, seasons and `sequence:` (dsl 0.27.0), and every scene /
//! quest / entry id the project declares.
//!
//! The capability surface itself (directives, state schema, relations, …)
//! is assembled by `authoring_surface` in `main.rs`; this module adds the
//! keys that do not come from a snapshot, and renders them in the outline.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// The language's built-in directives: recognized by tag in every document,
/// never looked up in a capability snapshot, so the snapshot-derived
/// `directives` list cannot show them. `(name, syntax, meaning)`. Every one
/// takes an optional trailing `when="<condition>"` guard (dsl 0.24.0 §1 for
/// `::set`, dsl 0.26.0 §4 for the rest), shown in brackets.
const BUILTIN_DIRECTIVES: &[(&str, &str, &str)] = &[
    (
        "set",
        "::set{ <path> = <expr> [when=\"<condition>\"] }  (also += / -=)",
        "write a declared state path; `owner: engine` paths are the engine's (E-ENGINE-OWNED-WRITE)",
    ),
    (
        "assert",
        "::assert{ <relation>(<arg>, …) [when=\"<condition>\"] }",
        "assert a ground fact of a declared, non-derived, non-reserved relation",
    ),
    (
        "retract",
        "::retract{ <relation>(<arg | _>, …) [when=\"<condition>\"] }",
        "retract the matching facts of a declared, non-derived, non-reserved relation",
    ),
    (
        "accept",
        "::accept{quest=\"<questId>\" [when=\"<condition>\"]}",
        "accept a quest that has no `start` condition",
    ),
    (
        "use",
        "::use{component=\"<name>\" <param>=<value> … [when=\"<condition>\"]}",
        "expand an imported component with named arguments; a param with a default may be omitted",
    ),
];

/// The beat keys (dsl 0.21.0 §3, 0.27.0 §3/§5/§6): a scene's frontmatter
/// (`key: value`), an `<entry on=…>`'s and a bundle `<beat>`'s attributes
/// (`key="value"`). Language, not capability, like the built-in directives.
/// `(key, value syntax, meaning)`.
const BEAT_KEYS: &[(&str, &str, &str)] = &[
    ("on", "<occasion>", "the occasion the beat answers"),
    (
        "target",
        "<prefix>.<member> | kind:<kind>",
        "the one target it answers, or every member of a kind (read as occasion.target)",
    ),
    (
        "for",
        "kind:<kind>",
        "on an untargeted `select: sequence` occasion: presented once per member whose `when` holds, binding occasion.target",
    ),
    ("when", "<condition>", "eligible only while it holds"),
    ("priority", "<integer>", "the higher eligible beat wins"),
    (
        "once",
        "run | user | false | day | slot | week | season:<name>",
        "presented at most once per run, ever, without limit, per clock day / slot / week, or per window of a season",
    ),
    (
        "spentBy",
        "<condition>",
        "instead of `once`: repeatable until the condition holds",
    ),
    (
        "also",
        "true",
        "scene and bundle beats, on a `select: first` occasion: presented after the winner too",
    ),
    ("share", "<key>", "beats with one `share` key spend one `once` together"),
    (
        "after",
        "<prerequisite>",
        "scene and bundle beats: eligible once it holds, e.g. visited(\"<id>\")",
    ),
    (
        "use",
        "<component>",
        "bundle `<beat>`: its header from the component's `beat:` template, the component's params as attributes",
    ),
];

/// `<quest>`'s attributes (dsl 0.2.0 §6.3 … 0.27.0 §5), as [`BEAT_KEYS`].
const QUEST_KEYS: &[(&str, &str, &str)] = &[
    (
        "start",
        "<condition>",
        "activates the quest when it holds; without it the quest is accept-driven",
    ),
    (
        "fail",
        "<condition>",
        "fails the active quest when it holds",
    ),
    (
        "after",
        "<prerequisite>",
        "its place in the scene graph; does not gate activation",
    ),
    (
        "tier",
        "user | run | season:<name>",
        "when it returns to unset: never, at each new run, or each time the season opens",
    ),
    (
        "rearm",
        "<condition>",
        "returns the quest to unset (objectives cleared) each time the condition goes false→true",
    ),
    (
        "complete",
        "all | any",
        "completes when every / any one required objective is done",
    ),
    (
        "activate",
        "accept",
        "a child that waits for an ::accept instead of activating with its parent",
    ),
    (
        "accept",
        "external",
        "the engine accepts the quest outside any document",
    ),
];

/// Add the non-snapshot keys to the authoring `surface`:
///
/// - `defs`: every def the document can `@ref` (plugin < imported < inline,
///   the checker's own precedence), name-sorted, with its (declared or
///   inferred) result type, ordered params, and CEL body;
/// - `builtinDirectives`: [`BUILTIN_DIRECTIVES`]; `beatKeys` / `questKeys`:
///   [`BEAT_KEYS`] / [`QUEST_KEYS`];
/// - dsl 0.27.0: the project's `clock` as declared (a finite one with its
///   `last` / `days`), its `terminal` condition, its `seasons` (`name`,
///   `live`) and the manifest's `sequence` (`occasion`, `scenes`) — each
///   only when declared;
/// - `ids`: the scene, quest and entry ids declared across the project
///   (`--project <dir>`), or in the document alone without one.
pub(crate) fn extend_surface(
    surface: &mut Value,
    folded: &lute_check::FoldedEnv,
    sequence: Option<&lute_manifest::project::Sequence>,
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
        .map(
            |(name, syntax, meaning)| json!({ "name": name, "syntax": syntax, "meaning": meaning }),
        )
        .collect();
    root.insert("builtinDirectives".into(), builtins.into());
    let keys = |table: &[(&str, &str, &str)]| -> Value {
        table
            .iter()
            .map(|(key, syntax, meaning)| json!({ "key": key, "syntax": syntax, "meaning": meaning }))
            .collect::<Vec<_>>()
            .into()
    };
    root.insert("beatKeys".into(), keys(BEAT_KEYS));
    root.insert("questKeys".into(), keys(QUEST_KEYS));

    if let Some(clock) = &env.clock {
        root.insert("clock".into(), json!(clock));
    }
    if let Some(terminal) = &env.terminal {
        root.insert("terminal".into(), terminal.clone().into());
    }
    if !env.seasons.is_empty() {
        let seasons: Vec<Value> = env
            .seasons
            .iter()
            .map(|(name, decl)| json!({ "name": name, "live": decl.live }))
            .collect();
        root.insert("seasons".into(), seasons.into());
    }
    if let Some(s) = sequence {
        root.insert(
            "sequence".into(),
            json!({ "occasion": s.occasion, "scenes": s.scenes }),
        );
    }

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
    for (key, note) in [
        ("beatKeys", "scene frontmatter; <entry> / <beat> attributes"),
        ("questKeys", "<quest> attributes"),
    ] {
        if let Some(rows) = surface[key].as_array() {
            let _ = writeln!(out, "{key} ({}; {note}):", rows.len());
            for k in rows {
                let _ = writeln!(
                    out,
                    "  {}: {} — {}",
                    k["key"].as_str().unwrap_or(""),
                    k["syntax"].as_str().unwrap_or(""),
                    k["meaning"].as_str().unwrap_or("")
                );
            }
        }
    }
    if let Some(clock) = surface.get("clock") {
        let _ = writeln!(out, "clock: {}", clock_line(clock));
    }
    if let Some(t) = surface["terminal"].as_str() {
        let _ = writeln!(out, "terminal: {t} (no occasion is raised once it holds)");
    }
    if let Some(seasons) = surface["seasons"].as_array() {
        let _ = writeln!(out, "seasons ({}):", seasons.len());
        for s in seasons {
            let _ = writeln!(
                out,
                "  {} — live: {}",
                s["name"].as_str().unwrap_or(""),
                s["live"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(s) = surface.get("sequence") {
        let _ = writeln!(
            out,
            "sequence (occasion: {}): {}",
            s["occasion"].as_str().unwrap_or(""),
            strs(&s["scenes"]).join(" → ")
        );
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

/// A declared clock (its JSON form) on one line: `day run.day, slot
/// run.slot [morning, night], week 7 [Mon, …], raise dayEnd: dayEnd, days 5`.
fn clock_line(c: &Value) -> String {
    let mut parts = vec![format!("day {}", c["day"].as_str().unwrap_or(""))];
    if let Some(slot) = c["slot"].as_str() {
        parts.push(format!("slot {slot} [{}]", strs(&c["slots"]).join(", ")));
    }
    if let Some(week) = c.get("week") {
        let labels = strs(&week["labels"]);
        let mut w = format!("week {}", week["length"]);
        if week["first"].as_u64().unwrap_or(0) != 0 {
            let _ = write!(w, " (first {})", week["first"]);
        }
        if !labels.is_empty() {
            let _ = write!(w, " [{}]", labels.join(", "));
        }
        parts.push(w);
    }
    match &c["raise"] {
        Value::String(o) => parts.push(format!("raise {o}")),
        Value::Object(m) => parts.push(format!(
            "raise {}",
            m.iter()
                .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => {}
    }
    // dsl 0.27.0 §4: a finite clock's end.
    if let Some(last) = c.get("last") {
        let slot = last["slot"]
            .as_str()
            .map(|s| format!(" {s}"))
            .unwrap_or_default();
        parts.push(format!("last day {}{slot}", last["day"]));
    }
    if let Some(days) = c.get("days") {
        parts.push(format!("days {days}"));
    }
    parts.join(", ")
}
