//! `lute context` additions (dsl 0.22.0 §13): the surface an author (or an AI
//! writing Lute) needs beyond the capability snapshot — the document's defs
//! with their types, parameters and bodies, the language's built-in
//! directives, beat keys and quest attributes, the project's clock,
//! `terminal:`, seasons and `chapters:`, and every scene /
//! quest / entry id the project declares.
//!
//! The capability surface itself (directives, state schema, relations, …)
//! is assembled by `authoring_surface` in `main.rs`; this module adds the
//! keys that do not come from a snapshot, and renders them in the outline.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// Render a `lute.core` directive's call shape from its snapshot
/// declaration: required attrs bare, optional ones in brackets, and the
/// `when=` guard every directive takes.
fn core_syntax(name: &str, snap: &lute_manifest::snapshot::CapabilitySnapshot) -> String {
    let mut parts = Vec::new();
    let mut has_when = false;
    if let Some(decl) = snap.directive(name) {
        for a in &decl.attrs {
            let ty = if a.name == "when" {
                has_when = true;
                "<condition>".to_string()
            } else {
                format!("<{}>", crate::attr_type_str(&a.ty).0)
            };
            let one = format!("{}=\"{ty}\"", a.name);
            parts.push(if a.required { one } else { format!("[{one}]") });
        }
    }
    if !has_when {
        parts.push("[when=\"<condition>\"]".to_string());
    }
    format!("::{name}{{{}}}", parts.join(" "))
}

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
        "spent once the condition has held; `once` sets how long it stays spent (`run` unless written)",
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
    (
        "advances",
        "slot | day | <whole number ≥ 1>",
        "moves the clock after this beat presents",
    ),
];

/// Descriptions of `<quest>` / `<objective>` / `<reward>` attributes, keyed
/// by name. The keys listed are the checker's own ([`QUEST_ATTRS`],
/// [`OBJECTIVE_ATTRS`], [`REWARD_ATTRS`]); this table only describes them.
/// `(key, value syntax, meaning)`.
///
/// [`QUEST_ATTRS`]: lute_check::logic_attrs::QUEST_ATTRS
/// [`OBJECTIVE_ATTRS`]: lute_check::logic_attrs::OBJECTIVE_ATTRS
/// [`REWARD_ATTRS`]: lute_check::logic_attrs::REWARD_ATTRS
const QUEST_KEY_DOCS: &[(&str, &str, &str)] = &[
    ("id", "<questId>", "read as quest.<id>.state"),
    ("title", "<text>", "the quest's name"),
    (
        "start",
        "<condition>",
        "activates the quest when it holds; without it the quest is accept-driven",
    ),
    ("fail", "<condition>", "fails the active quest when it holds"),
    (
        "follows",
        "<prerequisite>",
        "the quest's place in the scene graph; never gates activation (to wait, write start=\"visited('…')\")",
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

const OBJECTIVE_KEY_DOCS: &[(&str, &str, &str)] = &[
    (
        "id",
        "<objectiveId>",
        "read as quest.<quest>.objectives.<id>.done / .failed",
    ),
    ("done", "<condition>", "the objective is done once it holds"),
    (
        "quest",
        "<questId>",
        "a subquest objective: done when that quest completes",
    ),
    (
        "visibleWhen",
        "<condition>",
        "hides the objective while false; never gates `done`",
    ),
    ("title", "<text>", "the objective's name"),
    ("optional", "true", "not required for the quest to complete"),
    ("on", "<occasion>", "judged when that occasion is raised"),
    (
        "by",
        "<condition>",
        "a deadline: the first time it holds while not done, the objective fails",
    ),
    (
        "target",
        "<prefix>.<member>",
        "with `on`: judged only for a raise for that target",
    ),
    (
        "until",
        "<condition>",
        "with `on`: a deadline judged only when the objective's occasion is raised, after `done`",
    ),
];

const REWARD_KEY_DOCS: &[(&str, &str, &str)] = &[
    ("kind", "<rewardKind>", "what the engine pays"),
    ("target", "<id>", "what the reward is for, per its kind"),
    ("amount", "<integer> | <N>..<M>", "how much"),
    ("when", "<condition>", "granted only while it holds"),
    (
        "outcome",
        "failed",
        "grant when the quest fails; without it the reward grants on complete",
    ),
];

/// The checker's `keys`, in attribute spelling (`key="<value>"`), each with
/// its description from `docs`.
fn attr_rows(keys: &[&str], docs: &[(&str, &str, &str)]) -> Value {
    keys.iter()
        .map(|key| {
            let (syntax, meaning) = docs
                .iter()
                .find(|(k, _, _)| k == key)
                .map_or(("<value>", ""), |(_, s, m)| (*s, *m));
            json!({ "key": key, "syntax": format!("{key}=\"{syntax}\""), "meaning": meaning })
        })
        .collect::<Vec<_>>()
        .into()
}

/// Add the non-snapshot keys to the authoring `surface`:
///
/// - `defs`: every def the document can `@ref` (plugin < imported < inline,
///   the checker's own precedence), name-sorted, with its (declared or
///   inferred) result type, ordered params, and CEL body;
/// - `builtinDirectives`: the checker's [`LANGUAGE_DIRECTIVES`]
///   (`lute.core` ones with their declared attrs); `beatKeys`; `questKeys` /
///   `objectiveKeys` / `rewardKeys`: the checker's attribute lists in
///   attribute spelling;
/// - `reservedQuestPaths`: every reserved path of every quest the project
///   declares (and any this document reads), typed by the checker's
///   [`RESERVED_PATHS`]; `enginePaths`: every engine-owned path shape;
/// - dsl 0.27.0: the project's `clock` as declared (a finite one with its
///   `last` / `days`), its `terminal` condition, its `seasons` (`name`,
///   `live`) and the manifest's `chapters` — each only when declared;
///
/// [`LANGUAGE_DIRECTIVES`]: lute_check::directives::LANGUAGE_DIRECTIVES
/// [`RESERVED_PATHS`]: lute_check::cel_paths::RESERVED_PATHS
/// - `ids`: the scene, quest and entry ids declared across the project
///   (`--project <dir>`), or in the document alone without one.
pub(crate) fn extend_surface(
    surface: &mut Value,
    folded: &lute_check::FoldedEnv,
    input: &lute_check::CheckInput,
    referenced: &std::collections::BTreeSet<String>,
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

    let builtins: Vec<Value> = lute_check::directives::LANGUAGE_DIRECTIVES
        .iter()
        .map(|d| {
            let syntax = d
                .syntax
                .map_or_else(|| core_syntax(d.name, &input.snapshot), str::to_string);
            json!({ "name": d.name, "syntax": syntax, "meaning": d.meaning })
        })
        .collect();
    root.insert("builtinDirectives".into(), builtins.into());
    // Cross-cutting attributes the checker admits on every directive.
    let mut every: Vec<Value> = vec![json!({
        "name": "when", "type": "condition", "on": "every directive"
    })];
    for (name, ty) in lute_check::directives::UNIVERSAL_TIMING_ATTRS {
        every.push(json!({
            "name": name, "type": crate::attr_type_str(ty).0, "on": "every directive but ::clear"
        }));
    }
    every.push(
        json!({ "name": "at", "type": "time", "on": "a directive inside a <track> clip only" }),
    );
    for (name, a) in &input.snapshot.stamp_attrs {
        every.push(json!({
            "name": name, "type": crate::attr_type_str(&a.ty).0, "on": "every directive (stampAttrs)"
        }));
    }
    root.insert("directiveAttrs".into(), every.into());
    let keys = |table: &[(&str, &str, &str)]| -> Value {
        table
            .iter()
            .map(|(key, syntax, meaning)| json!({ "key": key, "syntax": syntax, "meaning": meaning }))
            .collect::<Vec<_>>()
            .into()
    };
    root.insert("beatKeys".into(), keys(BEAT_KEYS));
    use lute_check::logic_attrs::{OBJECTIVE_ATTRS, QUEST_ATTRS, REWARD_ATTRS};
    root.insert("questKeys".into(), attr_rows(QUEST_ATTRS, QUEST_KEY_DOCS));
    root.insert(
        "objectiveKeys".into(),
        attr_rows(OBJECTIVE_ATTRS, OBJECTIVE_KEY_DOCS),
    );
    root.insert(
        "rewardKeys".into(),
        attr_rows(REWARD_ATTRS, REWARD_KEY_DOCS),
    );
    root.insert("enginePaths".into(), engine_paths());

    if let Some(clock) = &env.clock {
        root.insert("clock".into(), json!(clock));
    }
    if let Some(terminal) = &env.terminal {
        root.insert("terminal".into(), terminal.clone().into());
        if env.terminal_persists {
            root.insert("terminalPersists".into(), true.into());
        }
    }
    if !env.seasons.is_empty() {
        let seasons: Vec<Value> = env
            .seasons
            .iter()
            .map(|(name, decl)| json!({ "name": name, "live": decl.live }))
            .collect();
        root.insert("seasons".into(), seasons.into());
    }
    let chapters: Vec<Value> = input
        .defaults
        .chapters()
        .iter()
        .map(|c| {
            json!({
                "on": c.on,
                "scenes": c.scenes,
                "applied": c.applied && !c.retired,
                "chained": lute_check::chapters::chained(&c.on, &input.snapshot.occasions),
            })
        })
        .collect();
    if !chapters.is_empty() {
        root.insert("chapters".into(), chapters.into());
    }

    let docs = project_docs(file, project);
    root.insert(
        "reservedQuestPaths".into(),
        reserved_quest_paths(&docs, referenced),
    );
    root.insert("ids".into(), project_ids(&docs));
}

/// A reserved / engine path's type label and member domain.
fn engine_type(
    ty: lute_check::cel_paths::EnginePathType,
) -> (&'static str, Option<Vec<&'static str>>) {
    use lute_check::cel_paths::EnginePathType as T;
    match ty {
        T::Enum(members) => ("enum", Some(members.to_vec())),
        T::Bool => ("bool", None),
        T::NarrativeTime => ("narrativeTime", None),
        T::Folded => ("folded", None),
    }
}

/// Every engine-owned path shape the checker knows ([`RESERVED_PATHS`] then
/// [`FOLDED_ENGINE_PATHS`]).
///
/// [`RESERVED_PATHS`]: lute_check::cel_paths::RESERVED_PATHS
/// [`FOLDED_ENGINE_PATHS`]: lute_check::cel_paths::FOLDED_ENGINE_PATHS
fn engine_paths() -> Value {
    use lute_check::cel_paths::{FOLDED_ENGINE_PATHS, RESERVED_PATHS};
    RESERVED_PATHS
        .iter()
        .chain(FOLDED_ENGINE_PATHS)
        .map(|p| {
            let (ty, domain) = engine_type(p.ty);
            let mut o =
                json!({ "shape": p.shape, "type": ty, "tier": p.tier, "meaning": p.meaning });
            if let Some(d) = domain {
                o["domain"] = json!(d);
            }
            o
        })
        .collect::<Vec<_>>()
        .into()
}

/// Every reserved quest path of every quest in `docs` (per objective for
/// the objective shapes), plus the `referenced` ones, path-sorted, each
/// typed by its [`RESERVED_PATHS`] row.
///
/// [`RESERVED_PATHS`]: lute_check::cel_paths::RESERVED_PATHS
fn reserved_quest_paths(
    docs: &[(PathBuf, lute_syntax::ast::Document)],
    referenced: &std::collections::BTreeSet<String>,
) -> Value {
    use lute_check::cel_paths::{reserved_path, RESERVED_PATHS};
    let mut paths = referenced.clone();
    for q in docs.iter().flat_map(|(_, d)| &d.quests) {
        if q.id.is_empty() {
            continue;
        }
        let objectives: Vec<&str> = q
            .body
            .iter()
            .filter_map(|n| match n {
                lute_syntax::ast::Node::Objective(o) if !o.id.is_empty() => Some(o.id.as_str()),
                _ => None,
            })
            .collect();
        for shape in RESERVED_PATHS.iter().map(|p| p.shape) {
            let Some(rest) = shape.strip_prefix("quest.<quest>.") else {
                continue;
            };
            if rest.contains("<objective>") {
                for o in &objectives {
                    paths.insert(format!("quest.{}.{}", q.id, rest.replace("<objective>", o)));
                }
            } else {
                paths.insert(format!("quest.{}.{rest}", q.id));
            }
        }
    }
    paths
        .iter()
        .filter_map(|path| {
            let (ty, domain) = engine_type(reserved_path(path)?.ty);
            let mut o = json!({ "path": path, "type": ty, "namespace": "quest" });
            if let Some(d) = domain {
                o["domain"] = json!(d);
            }
            Some(o)
        })
        .collect::<Vec<_>>()
        .into()
}

/// The documents of `project` (the same `.lute` walk `check-project` does),
/// else `file` alone, parsed.
fn project_docs(file: &Path, project: Option<&Path>) -> Vec<(PathBuf, lute_syntax::ast::Document)> {
    let files: Vec<PathBuf> = match project {
        Some(dir) => crate::find_lute_files(dir).unwrap_or_default(),
        None => vec![file.to_path_buf()],
    };
    files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, lute_syntax::parse(&text).0))
        })
        .collect()
}

/// The ids a document may name in `visited(…)`, `after:`, `completed(…)`,
/// `::accept`, `quest.<id>.state` and `entry.<id>.read`: every scene key
/// (authored `id:` or the derived `{character}.sNNepNN`), quest id, lore
/// entry id, and lore bundle beat canonical id (`<document id>.<beat id>`,
/// dsl 0.23.0 §4; the `beats` key is present only when some document bundles
/// beats) — under `project` when given (the same `.lute` walk
/// `check-project` does), else in `file` alone. Parse-only: ids are
/// syntactic, and a document that does not check still declares them.
fn project_ids(docs: &[(PathBuf, lute_syntax::ast::Document)]) -> Value {
    let scenes: Vec<String> = lute_check::connectivity::scene_key_set(docs)
        .into_keys()
        .collect();
    let quests: Vec<String> = lute_check::connectivity::quest_id_set(docs)
        .into_iter()
        .collect();
    let entries: std::collections::BTreeSet<String> = docs
        .iter()
        .flat_map(|(_, doc)| doc.entries.iter())
        .filter(|e| !e.id.is_empty())
        .map(|e| e.id.clone())
        .collect();
    let mut ids = json!({ "scenes": scenes, "quests": quests, "entries": entries });
    let beats: Vec<String> = lute_check::connectivity::bundle_beat_key_set(docs)
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
    if let Some(attrs) = surface["directiveAttrs"].as_array() {
        let _ = writeln!(
            out,
            "directiveAttrs ({}; beyond each directive's own):",
            attrs.len()
        );
        for a in attrs {
            let _ = writeln!(
                out,
                "  {}: {} — {}",
                a["name"].as_str().unwrap_or(""),
                a["type"].as_str().unwrap_or(""),
                a["on"].as_str().unwrap_or("")
            );
        }
    }
    for (key, note) in [
        (
            "beatKeys",
            "scene frontmatter `key: value`; <entry> / <beat> attributes `key=\"value\"`",
        ),
        ("questKeys", "<quest> attributes"),
        ("objectiveKeys", "<objective> attributes"),
        ("rewardKeys", "<reward> attributes"),
    ] {
        if let Some(rows) = surface[key].as_array() {
            let _ = writeln!(out, "{key} ({}; {note}):", rows.len());
            for k in rows {
                let syntax = k["syntax"].as_str().unwrap_or("");
                let meaning = k["meaning"].as_str().unwrap_or("");
                if key == "beatKeys" {
                    let name = k["key"].as_str().unwrap_or("");
                    let _ = writeln!(out, "  {name}: {syntax} — {meaning}");
                } else {
                    let _ = writeln!(out, "  {syntax} — {meaning}");
                }
            }
        }
    }
    if let Some(paths) = surface["enginePaths"].as_array() {
        let _ = writeln!(
            out,
            "enginePaths ({}; the engine writes these — read them, never declare or ::set them):",
            paths.len()
        );
        for p in paths {
            let ty = match p["type"].as_str() {
                Some("folded") | None => String::new(),
                Some(t) => {
                    let dom = p["domain"]
                        .as_array()
                        .map(|d| format!(" [{}]", strs(&Value::Array(d.clone())).join(", ")))
                        .unwrap_or_default();
                    format!(": {t}{dom}")
                }
            };
            let _ = writeln!(
                out,
                "  {}{ty} [{}] — {}",
                p["shape"].as_str().unwrap_or(""),
                p["tier"].as_str().unwrap_or(""),
                p["meaning"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(clock) = surface.get("clock") {
        let _ = writeln!(out, "clock: {}", clock_line(clock));
    }
    if let Some(t) = surface["terminal"].as_str() {
        let persists = if surface["terminalPersists"] == true {
            "; it persists — a new run does not reopen the game"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "terminal: {t} (no occasion is raised once it holds{persists})"
        );
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
    if let Some(chains) = surface["chapters"].as_array() {
        let _ = writeln!(out, "chapters ({}):", chains.len());
        for c in chains {
            let how = if !c["applied"].as_bool().unwrap_or(false) {
                "not applied (its E-CHAPTERS error is at lute.project.yaml)"
            } else if c["chained"].as_bool().unwrap_or(true) {
                "each scene gets `on:`, `after: visited(\"<previous>\")` and a descending \
                 `priority:` unless it writes its own"
            } else {
                "the list orders one raise: each scene gets `on:` and a descending `priority:`, \
                 no `after:`"
            };
            let _ = writeln!(
                out,
                "  on {}: {} — {how}",
                c["on"].as_str().unwrap_or(""),
                strs(&c["scenes"]).join(" → ")
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
