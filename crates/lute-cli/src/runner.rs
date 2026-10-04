//! `lute run` — the reference headless runner over a COMPILED execution IR
//! (the executable counterpart of `docs/runtime/` +
//! `schemas/lute-ir-0.36.schema.json`).
//!
//! `lute run` is the *engine* side of the runtime contract. It loads a compiled
//! execution IR (`lute compile` output), gates on `irVersion` by exact minor
//! while the IR major is 0 and by MAJOR only from 1.0 onward
//! `commands` stream headlessly against a `--mock` playthrough — the same mock
//! surfaces `lute trace --mock` reads (`state:`/`facts:`/`choose:`/`events:`/
//! `accepts:`). Distinct from `lute trace`, which previews the SOURCE document
//! under three-valued logic and refuses to run the engine machinery: `run`
//! consumes the EXECUTION IR an engine would and actually does the engine's job.
//!
//! The walk itself is [`lute_trace::exec::Machine`] — the one walker `lute
//! play` runs too (its module doc lists what it implements: the dispatcher,
//! CEL guards, the Datalog fixpoint, `choice`/`hub`/`match`, the quest
//! lifecycle, `visited(…)`, lore entries and bundle beats). This module is
//! `lute run`'s I/O around it and its policy, [`RunDriver`]: decisions come
//! from the mock's ordered `choose:` (a branch's list consumed one decision
//! per presentation, a hub's as its visit sequence), bridge answers from the
//! mock's `bridges:`, a repeat force of a spent `once` hub option is skipped,
//! a force of a guard-closed option refused, and no unknown halts the walk.
//! `--entry <id>` / `--beat <id>` present one lore entry / bundle beat; a
//! lore execution IR without exactly one of them (or either flag on another
//! kind) is a usage error.
//!
//! Output: a human transcript by default; `--json` emits a stable machine
//! transcript `{ kind, irVersion, exit, commands, state, facts, quests }`.
//!
//! Exit codes: `0` a complete walk, `2` an I/O / usage failure (unreadable
//! execution IR/mock, malformed execution IR, an `irVersion` outside the
//! pre-1.0 exact-minor or post-1.0 MAJOR line, or an unknown command `kind`),
//! `3` an incomplete walk (`choice`/`hub` reached with no mock decision —
//! mirroring `lute trace`'s §4.5 incomplete convention).
//!
//! ## Deliberately NOT implemented (out of the reference runner's scope)
//! These are host/engine policy the runtime contract leaves unspecified; the
//! runner records them honestly rather than faking them (see also
//! `conformance/README.md`):
//! - **No real timeline clock.** `<timeline>` clips are already flattened and
//!   pre-scheduled by the compiler (timeline-semantics.md); the runner replays
//!   the stamped records in stream order and treats a `barrier` as a transcript
//!   note — it honors no `at`/`duration`/`delay` wall-clock timing and
//!   simulates no frame pacing or track concurrency.
//! - **No real bridges.** A `plugin` command (bridge-protocol.md) is recorded
//!   as an external call; its `op`/literal effects ARE applied. A
//!   `bridgeResult` effect reads the mock's `bridges:` answer for the call
//!   (dsl 0.24.0 §5, one per call of the tag, in order); with none it is
//!   recorded unresolved and the walk goes on — `lute play` instead halts at
//!   the call. The runner invokes no host service and ignores `wait`.
//! - **No narrative-time history.** `now()` / `validAt(...)` have no mock
//!   surface and read unknown; the fact store is valid-now (`holds`/`count`
//!   over the current least-fixpoint).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use lute_trace::exec::{
    render_fact, value_to_json, value_to_string, BridgeCall, BridgeQueues, BridgeReply, Driver,
    Forced, Machine, Menu, OnUnknown, Pick, ScriptedChoices, Seed, UnknownSite, Verdict,
    LINE_DELIVERY_KEYS, MENU_MARK_KEYS,
};
use serde_json::{json, Value as Json};

/// The IR major.minor line this reference runner implements, derived from
/// [`lute_compile::LUTE_IR_VERSION`] so it follows the compiler's IR
/// version forever. Pre-1.0 parsing gates on the exact major.minor line
/// (execution-model.md, Version negotiation); from 1.0 onward it gates on
/// MAJOR only. The minor is still carried because the `--json` transcript
/// reports the full implemented line.
fn impl_ir_line() -> (u64, u64) {
    parse_ir_version(lute_compile::LUTE_IR_VERSION)
        .map(|(major, minor, _)| (major, minor))
        .expect("LUTE_IR_VERSION must be MAJOR.MINOR.PATCH")
}

/// Execute a compiled execution IR against a mock playthrough. See [`crate::Command::Run`].
/// `entry` / `beat` select the one `entry` record (dsl 0.19.0 §8) or bundle
/// `beat` record (dsl 0.23.0 §4) a lore execution IR presents: exactly one is
/// required for `kind: "lore"`, either is refused for any other kind.
/// `occasions` are the `--occasion` flags, raised after the mock's own
/// `occasions:` (dsl 0.21.0 §7a.2).
pub fn run_artifact(
    artifact: &Path,
    engine: Option<&Path>,
    mock: Option<&Path>,
    occasions: Vec<String>,
    json_out: bool,
    entry: Option<&str>,
    beat: Option<&str>,
    dump_conditions: Option<&Path>,
) -> ExitCode {
    let text = match std::fs::read_to_string(artifact) {
        Ok(t) => t,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute run: cannot read {}: {e}", artifact.display());
            return ExitCode::from(2);
        }
    };
    let art: Json = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("lute run: {} is not valid JSON: {e}", artifact.display());
            return ExitCode::from(2);
        }
    };
    let matrix = match crate::EngineMatrix::load(engine) {
        Ok(m) => m,
        Err(e) => { eprintln!("lute run: {e}"); return ExitCode::from(2); }
    };
    if let Err(e) = matrix.negotiate(&art) {
        eprintln!("lute run: {e}");
        return ExitCode::from(2);
    }

    // ── Version negotiation (execution-model.md): pre-1.0 exact minor,
    // post-1.0 MAJOR only. ──
    let (impl_major, impl_minor) = impl_ir_line();
    let ir_version = art.get("irVersion").and_then(Json::as_str).unwrap_or("");
    match parse_ir_version(ir_version) {
        Some((maj, min, _))
            if maj == impl_major && (impl_major != 0 || min == impl_minor) => {}
        _ => {
            let rule = if impl_major == 0 {
                format!("pre-1.0 engines require exact major.minor {impl_major}.{impl_minor}")
            } else {
                format!("this runner implements MAJOR {impl_major}")
            };
            eprintln!(
                "lute run: unsupported irVersion {ir_version:?}: {rule} (got {ir_version:?})"
            );
            return ExitCode::from(2);
        }
    }

    if let Some(reason) = owned_write_refusal(&art) {
        eprintln!("lute run: {reason}");
        return ExitCode::from(1);
    }

    if !art.get("commands").map(Json::is_array).unwrap_or(false) {
        eprintln!("lute run: execution IR has no `commands` array");
        return ExitCode::from(2);
    }
    // ── dsl 0.19.0 §8: a lore execution IR is looked up, never played. ──
    let is_lore = art.get("kind").and_then(Json::as_str) == Some("lore");
    let presented = match (entry, beat) {
        (Some(_), Some(_)) => {
            eprintln!("lute run: pass `--entry` or `--beat`, not both");
            return ExitCode::from(2);
        }
        (Some(id), None) => Some(("entry", id)),
        (None, Some(id)) => Some(("beat", id)),
        (None, None) => None,
    };
    match (is_lore, presented) {
        (true, None) => {
            eprintln!(
                 "lute run: {} is a lore execution IR — there is no sequence to play; pass \
                 `--entry <id>` to present one entry or `--beat <id>` to present one bundle beat",
                artifact.display()
            );
            return ExitCode::from(2);
        }
        (false, Some((flag, id))) => {
            eprintln!(
                "lute run: `--{flag} {id}` needs a lore execution IR; {} is kind {:?}",
                artifact.display(),
                art.get("kind").and_then(Json::as_str).unwrap_or("scene")
            );
            return ExitCode::from(2);
        }
        _ => {}
    }

    // ── Mock playthrough (same surfaces as `lute trace --mock`). ──
    let mut mock_set = match mock {
        None => lute_trace::MockSet::default(),
        Some(path) => match std::fs::read_to_string(path) {
            Ok(t) => match lute_trace::parse_mock_yaml(&t) {
                Ok(m) => m,
                Err(d) => {
                    eprintln!("lute run: invalid mock {}: {}", path.display(), d.text());
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                let e = lute_manifest::io_reason(&e);
                eprintln!("lute run: cannot read mock {}: {e}", path.display());
                return ExitCode::from(2);
            }
        },
    };

    mock_set.occasions.extend(occasions);
    let mut m = run_machine(&art, &mock_set, entry, beat);
    if let Some(path) = dump_conditions {
        let file = match std::fs::File::create(path) {
            Ok(file) => std::rc::Rc::new(std::cell::RefCell::new(file)),
            Err(e) => {
                eprintln!("lute run: cannot create condition dump {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let mut env = art.get("celEnv").cloned().unwrap_or_else(|| json!({}));
        if let Json::Object(map) = &mut env {
            let vars = map.entry("variables").or_insert_with(|| json!([]));
            if let Json::Array(vars) = vars {
                for root in ["prev", "occasion", "clock", "entry"] {
                    if !vars.iter().any(|v| v.get("name").and_then(Json::as_str) == Some(root)) {
                        vars.push(json!({"name": root, "type": "map(string, dyn)"}));
                    }
                }
            }
        }
        let exprs = ExprIndex::new([&art]);
        {
            let header = json!({"kind": "env", "env": env});
            if let Ok(mut f) = file.try_borrow_mut() {
                let _ = serde_json::to_writer(&mut *f, &header);
                use std::io::Write;
                let _ = writeln!(f);
            }
        }
        m = m.with_eval_observer(move |raw, value, _atoms, snapshot| {
            let condition = exprs.get(raw);
            let expr = condition.map_or(Json::Null, |condition| condition.expr.clone());
            let no_relations = BTreeSet::new();
            let (relations, needs_visited) = condition
                .map_or((&no_relations, false), |condition| (&condition.relations, condition.needs_visited));
            let paths: BTreeSet<String> = snapshot.reads.iter().map(|(p, _)| p.clone()).collect();
            let mut dump_state = snapshot.state.clone();
            for (path, read) in snapshot.reads {
                if let lute_trace::Read::Value(value) = read {
                    dump_state.insert(path.clone(), value.clone());
                }
            }
            for (id, status) in snapshot.quest_status {
                dump_state.insert(
                    format!("quest.{id}.state"),
                    lute_trace::Value::Str(status.clone()),
                );
            }
            if let Some(target) = snapshot.occasion_target {
                dump_state.insert(
                    "occasion.target".to_string(),
                    lute_trace::Value::Str(target.to_string()),
                );
            }
            let mut line = json!({
                "cel": raw,
                "expr": expr,
                "activation": activation_json_paths(&dump_state, snapshot.state_types, &paths),
                "facts": condition_facts(&snapshot.facts, &relations),
                "result": typed_value(value),
            });
            if needs_visited {
                line["visited"] = snapshot.visited.iter().cloned().collect::<Vec<_>>().into();
            }
            // One write per line: the dump file is unbuffered, and serializing
            // straight into it issues a syscall per JSON token.
            if let (Ok(mut f), Ok(mut bytes)) = (file.try_borrow_mut(), serde_json::to_vec(&line)) {
                use std::io::Write;
                bytes.push(b'\n');
                let _ = f.write_all(&bytes);
            }
        });
    }
    let direct_lore = is_lore && presented.is_some();
    let result = m.run();
    if direct_lore {
        m.bind_occasion_target(None);
    }
    match result {
        Err(msg) => {
            eprintln!("lute run: {}", lute_core_span::plain_message(&msg));
            ExitCode::from(2)
        }
        Ok(()) => {
            if json_out {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output_value(&m, &art)).unwrap_or_default()
                );
            } else {
                print_human(&m, &art, artifact);
            }
            ExitCode::from(if m.incomplete() { 3 } else { 0 })
        }
    }
}
pub(crate) fn typed_value(value: &lute_trace::Value) -> Json {
    match value {
        lute_trace::Value::Bool(v) => json!({"bool": v}),
        lute_trace::Value::Int(v) => json!({"int": v}),
        lute_trace::Value::Double(v) => json!({"double": if v.is_nan() { json!("nan") } else if v.is_infinite() { json!(if *v > 0.0 { "inf" } else { "-inf" }) } else { json!(v) }}),
        lute_trace::Value::Str(v) => json!({"string": v}),
        lute_trace::Value::Unknown => json!({"error": "unknown"}),
        lute_trace::Value::Error(v) => json!({"error": v}),
    }
}

#[allow(dead_code)]
pub(crate) fn activation_json(
    state: &std::collections::BTreeMap<String, lute_trace::Value>,
    types: &std::collections::BTreeMap<String, String>,
) -> Json {
    activation_json_scoped(state, types, &BTreeSet::new())
}

pub(crate) fn activation_json_scoped(
    state: &std::collections::BTreeMap<String, lute_trace::Value>,
    types: &std::collections::BTreeMap<String, String>,
    selected_roots: &BTreeSet<String>,
) -> Json {
    activation_json_scoped_with_defaults(state, types, selected_roots, &BTreeMap::new())
}

pub(crate) fn activation_json_scoped_with_defaults(
    state: &std::collections::BTreeMap<String, lute_trace::Value>,
    types: &std::collections::BTreeMap<String, String>,
    selected_roots: &BTreeSet<String>,
    defaults: &BTreeMap<String, lute_trace::Value>,
) -> Json {
    fn insert(node: &mut Json, parts: &[&str], value: Json) {
        if parts.is_empty() {
            return;
        }
        if !node.get("map").is_some_and(Json::is_object) {
            *node = json!({"map": {}});
        }
        let map = node.get_mut("map").and_then(Json::as_object_mut).unwrap();
        if parts.len() == 1 {
            map.insert(parts[0].to_string(), value);
            return;
        }
        let child = map.entry(parts[0].to_string()).or_insert_with(|| json!({"map": {}}));
        insert(child, &parts[1..], value);
    }
    fn ensure_maps(node: &mut Json, parts: &[&str]) {
        if parts.len() < 2 {
            return;
        }
        if !node.get("map").is_some_and(Json::is_object) {
            *node = json!({"map": {}});
        }
        let map = node.get_mut("map").and_then(Json::as_object_mut).unwrap();
        let child = map.entry(parts[0].to_string()).or_insert_with(|| json!({"map": {}}));
        ensure_maps(child, &parts[1..]);
    }
    let all = selected_roots.is_empty();
    let allowed = |path: &str| all || selected_roots.contains(path.split('.').next().unwrap_or(path));
    let mut roots = json!({"map": {}});
    for (path, value) in state {
        if allowed(path) {
            let parts: Vec<_> = path.split('.').collect();
            insert(&mut roots, &parts, typed_value(value));
        }
    }
    for (path, value) in defaults {
        if !state.contains_key(path) && allowed(path) {
            let parts: Vec<_> = path.split('.').collect();
            insert(&mut roots, &parts, typed_value(value));
        }
    }
    for (path, _ty) in types {
        let root = path.split('.').next().unwrap_or(path);
        if !all && !selected_roots.contains(root) && !(selected_roots.contains("prev")
            && (path.starts_with("run.") || path.starts_with("season.")))
        {
            continue;
        }
        let parts: Vec<_> = path.split('.').collect();
        if allowed(path) {
            ensure_maps(&mut roots, &parts);
        }
        if path.starts_with("run.") || path.starts_with("season.") {
            if all || selected_roots.contains("prev") {
                let prev = format!("prev.{path}");
                let prev_parts: Vec<_> = prev.split('.').collect();
                ensure_maps(&mut roots, &prev_parts);
            }
        }
        if state.contains_key(path) || defaults.contains_key(path) {
            continue;
        }
        let fallback = match _ty.as_str() {
            "bool" | "boolean" => Some(json!({"bool": false})),
            "int" | "integer" => Some(json!({"int": 0})),
            "double" | "number" => Some(json!({"double": 0.0})),
            _ => None,
        };
        if let Some(value) = fallback.filter(|_| allowed(path)) {
            insert(&mut roots, &parts, value);
            continue;
        }
        let Some(value) = (path.starts_with("quest.") && path.ends_with(".state"))
            .then(|| json!({"string": "unset"}))
        else {
            continue;
        };
        if allowed(path) {
            insert(&mut roots, &parts, value);
        }
    }
    for root in selected_roots {
        let root_parts = [root.as_str()];
        insert(&mut roots, &root_parts, json!({"map": {}}));
    }
    roots.get("map").cloned().unwrap_or_else(|| json!({}))
}

pub(crate) fn condition_scope(expr: &Json) -> (BTreeSet<String>, BTreeSet<String>, bool) {
    fn visit(node: &Json, roots: &mut BTreeSet<String>, relations: &mut BTreeSet<String>, visited: &mut bool) {
        let Json::Object(map) = node else { return };
        if let Some(path) = map.get("path").and_then(Json::as_str).or_else(|| map.get("has").and_then(Json::as_str)) {
            if let Some(root) = path.split('.').next() { roots.insert(root.to_string()); }
        }
        if let Some(call) = map.get("call").and_then(Json::as_str) {
            if call == "visited" { *visited = true; }
            if matches!(call, "holds" | "count" | "countDistinct" | "validAt") {
                if let Some(rel) = map.get("args").and_then(Json::as_array).and_then(|a| a.first()).and_then(|v| v.get("string")).and_then(Json::as_str) {
                    relations.insert(rel.to_string());
                }
            }
        }
        for child in map.values() { visit(child, roots, relations, visited); }
    }
    let mut roots = BTreeSet::new();
    let mut relations = BTreeSet::new();
    let mut visited = false;
    visit(expr, &mut roots, &mut relations, &mut visited);
    (roots, relations, visited)
}


pub(crate) fn activation_json_paths(
    state: &BTreeMap<String, lute_trace::Value>,
    types: &BTreeMap<String, String>,
    selected_paths: &BTreeSet<String>,
) -> Json {
    fn insert(node: &mut Json, parts: &[&str], value: Option<Json>) {
        if parts.is_empty() { return; }
        if !node.get("map").is_some_and(Json::is_object) { *node = json!({"map": {}}); }
        let map = node.get_mut("map").and_then(Json::as_object_mut).unwrap();
        if parts.len() == 1 {
            if let Some(value) = value { map.insert(parts[0].to_string(), value); }
            else { map.entry(parts[0].to_string()).or_insert_with(|| json!({"map": {}})); }
            return;
        }
        let child = map.entry(parts[0].to_string()).or_insert_with(|| json!({"map": {}}));
        insert(child, &parts[1..], value);
    }
    let wanted = |candidate: &str| path_wanted(selected_paths, candidate);
    let mut out = json!({"map": {}});
    for selected in selected_paths {
        let root = selected.split('.').next().unwrap_or(selected);
        insert(&mut out, &[root], None);
    }
    for (path, value) in state {
        if wanted(path) { insert(&mut out, &path.split('.').collect::<Vec<_>>(), Some(typed_value(value))); }
    }
    for path in types.keys() {
        if !wanted(path) || state.contains_key(path) { continue; }
        let parts: Vec<_> = path.split('.').collect();
        if parts.len() > 1 {
            insert(&mut out, &parts[..parts.len() - 1], None);
        }
    }
    for selected in selected_paths {
        if selected.starts_with("prev.") {
            let parts: Vec<_> = selected.split('.').collect();
            if parts.len() > 2 {
                insert(&mut out, &parts[..parts.len() - 1], None);
            }
        }
    }
    out.get("map").cloned().unwrap_or_else(|| json!({}))
}

/// Whether `candidate` is a selected path, an ancestor of one, or below one —
/// the only state entries a condition-dump activation can contain.
pub(crate) fn path_wanted(selected_paths: &BTreeSet<String>, candidate: &str) -> bool {
    fn below(path: &str, parent: &str) -> bool {
        path.len() > parent.len() && path.starts_with(parent) && path.as_bytes()[parent.len()] == b'.'
    }
    selected_paths
        .iter()
        .any(|selected| selected == candidate || below(selected, candidate) || below(candidate, selected))
}

pub(crate) fn condition_facts(
    facts: &std::collections::BTreeSet<lute_trace::datalog::Fact>,
    relations: &BTreeSet<String>,
) -> Json {
    facts.iter().filter(|(rel, _)| relations.contains(rel)).map(|(rel, args)| json!({
        "rel": rel,
        "args": args.iter().map(|a| json!({"string": a})).collect::<Vec<_>>()
    })).collect()
}

#[cfg(test)]
mod dump_tests {
    use super::*;

    #[test]
    fn typed_encoder_preserves_numeric_kinds_and_errors() {
        assert_eq!(typed_value(&lute_trace::Value::Int(3)), json!({"int": 3}));
        assert_eq!(typed_value(&lute_trace::Value::Double(3.5)), json!({"double": 3.5}));
        assert_eq!(typed_value(&lute_trace::Value::Error("division by zero".into())), json!({"error": "division by zero"}));
    }

    #[test]
    fn activation_omits_unset_slots_but_exposes_quest_state() {
        let state = std::collections::BTreeMap::from([(
            "run.visits".to_string(),
            lute_trace::Value::Bool(true),
        )]);
        let types = std::collections::BTreeMap::from([
            ("run.tip".to_string(), "string".to_string()),
            ("run.visits".to_string(), "bool".to_string()),
            ("quest.demo.state".to_string(), "enum".to_string()),
        ]);
        let activation = activation_json(&state, &types);
        assert_eq!(activation["run"]["map"]["visits"], json!({"bool": true}));
        assert!(activation["run"]["map"].get("tip").is_none());
        assert_eq!(activation["quest"]["map"]["demo"]["map"]["state"], json!({"string": "unset"}));
    }

    #[test]
    fn path_activation_omits_unset_leaf_but_keeps_read_list() {
        let state = BTreeMap::from([
            ("run.visits.0".to_string(), lute_trace::Value::Str("zero".into())),
            ("run.visits.1".to_string(), lute_trace::Value::Str("one".into())),
        ]);
        let types = BTreeMap::new();
        let paths = BTreeSet::from(["run.lantern.wish".to_string(), "run.visits".to_string()]);
        let activation = activation_json_paths(&state, &types, &paths);
        assert!(activation["run"]["map"].get("lantern").is_none());
        assert!(activation["run"]["map"].get("visits").is_some());
    }
    #[test]
    fn direct_lore_run_binds_raised_target_member() {
        let art = json!({
            "kind": "lore",
            "commands": [
                {
                    "kind": "beat",
                    "addr": "001",
                    "id": "catch.land",
                    "on": "landed",
                    "target": "kind:fish",
                    "targetKind": {
                        "kind": "fish",
                        "prefix": "fish",
                        "members": ["cod"]
                    },
                    "body": "002"
                },
                {
                    "kind": "line",
                    "addr": "002",
                    "speaker": "narrator",
                    "text": "You land a {{occasion.target}}.",
                    "placeholders": [{"kind": "occasionTarget"}]
                }
            ]
        });
        let mock = lute_trace::MockSet {
            occasions: vec!["landed@fish.cod".into()],
            ..Default::default()
        };
        let mut machine = run_machine(&art, &mock, None, Some("catch.land"));
        machine.run().unwrap();
        assert!(machine
            .driver()
            .transcript
            .iter()
            .any(|record| record["text"] == "You land a cod."));
        let mut unraised = run_machine(&art, &Default::default(), None, Some("catch.land"));
        unraised.run().unwrap();
        assert!(unraised
            .driver()
            .transcript
            .iter()
            .any(|record| record["text"] == "You land a kind:fish."));
    }

}

/// One condition's emitted `expr` tree and the scope the dump reads from it.
pub(crate) struct DumpCondition {
    pub expr: Json,
    pub relations: BTreeSet<String>,
    pub needs_visited: bool,
}

/// Emitted conditions keyed by raw CEL text, built once per condition dump so
/// each evaluation is a lookup instead of a walk over every artifact. Within an
/// artifact the first node carrying a `cel` decides (depth-first, node before
/// children); across artifacts the first one whose deciding node has an
/// `expr` wins.
pub(crate) struct ExprIndex(std::collections::HashMap<String, DumpCondition>);

impl ExprIndex {
    pub(crate) fn new<'a>(artifacts: impl IntoIterator<Item = &'a Json>) -> Self {
        fn collect<'a>(value: &'a Json, first: &mut std::collections::HashMap<&'a str, Option<&'a Json>>) {
            match value {
                Json::Object(map) => {
                    if let Some(cel) = map.get("cel").and_then(Json::as_str) {
                        first.entry(cel).or_insert(map.get("expr"));
                    }
                    map.values().for_each(|v| collect(v, first));
                }
                Json::Array(items) => items.iter().for_each(|v| collect(v, first)),
                _ => {}
            }
        }
        let mut index = std::collections::HashMap::new();
        for artifact in artifacts {
            let mut first = std::collections::HashMap::new();
            collect(artifact, &mut first);
            for (cel, expr) in first {
                let Some(expr) = expr else { continue };
                index.entry(cel.to_owned()).or_insert_with(|| {
                    let (_roots, relations, needs_visited) = condition_scope(expr);
                    DumpCondition { expr: expr.clone(), relations, needs_visited }
                });
            }
        }
        Self(index)
    }

    pub(crate) fn get(&self, cel: &str) -> Option<&DumpCondition> {
        self.0.get(cel)
    }
}


/// Refuse hand-built or tampered execution IR that asks content to mutate
/// state or facts owned by the engine. Compiled IR should already exclude
/// these writes; this guard runs before mocks are loaded or a Machine starts.
fn owned_write_refusal(art: &Json) -> Option<String> {
    let owned: std::collections::BTreeSet<&str> = art
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("owner").and_then(Json::as_str) == Some("engine"))
        .filter_map(|entry| entry.get("path").and_then(Json::as_str))
        .collect();
    let reserved: std::collections::BTreeSet<&str> = art
        .get("relations")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("reserved").and_then(Json::as_bool) == Some(true))
        .filter_map(|entry| entry.get("name").and_then(Json::as_str))
        .collect();
    let owns = |path: &str| {
        owned.contains(path)
            || lute_check::target_writes::indexed_family(path).is_some_and(|family| {
                owned.contains(family)
                    || owned.iter().any(|p| {
                        p.strip_prefix(family).is_some_and(|rest| rest.starts_with('.'))
                    })
            })
    };
    for command in art
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let kind = command.get("kind").and_then(Json::as_str).unwrap_or("");
        if kind == "set" {
            if let Some(path) = command.get("path").and_then(Json::as_str) {
                if owns(path) {
                    return Some(format!(
                        "E-RUN-OWNED-WRITE: command writes engine-owned state path `{path}`"
                    ));
                }
            }
        }
        if matches!(kind, "assert" | "retract") {
            if let Some(relation) = command.get("relation").and_then(Json::as_str) {
                if reserved.contains(relation) {
                    return Some(format!(
                        "E-RUN-OWNED-WRITE: command {}s reserved relation `{relation}`",
                        kind
                    ));
                }
            }
        }
        for (key, label) in [("asserts", "assert"), ("retracts", "retract")] {
            if let Some(relation) = command
                .get(key)
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .find_map(|fact| {
                    fact.get("relation")
                        .and_then(Json::as_str)
                        .filter(|relation| reserved.contains(relation))
                })
            {
                return Some(format!(
                    "E-RUN-OWNED-WRITE: plugin {label}s reserved relation `{relation}`"
                ));
            }
        }
        if kind == "plugin" {
            if let Some(path) = command
                .get("effects")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|effect| effect.get("path").and_then(Json::as_str))
                .find(|path| owns(path))
            {
                return Some(format!(
                    "E-RUN-OWNED-WRITE: plugin writes engine-owned state path `{path}`"
                ));
            }
        }
    }
    None
}

/// The member a direct lore presentation binds as `occasion.target`.
///
/// Direct `lute run --entry/--beat` walks do not execute the quest occasion
/// loop, so the selected lore command must apply the mock raise itself. The
/// compiled target metadata carries the same member set trace uses: a
/// `targetKind` carries its occasion prefix, while `forKind` carries members
/// and accepts either a bare member or a prefixed target.
fn direct_occasion_member(
    art: &Json,
    mock: &lute_trace::MockSet,
    entry: Option<&str>,
    beat: Option<&str>,
) -> Option<String> {
    let (kind, id) = match (entry, beat) {
        (Some(id), None) => ("entry", id),
        (None, Some(id)) => ("beat", id),
        _ => return None,
    };
    let commands = art.get("commands")?.as_array()?;
    let command = commands.iter().find(|command| {
        command.get("kind").and_then(Json::as_str) == Some(kind)
            && command.get("id").and_then(Json::as_str).is_some_and(|declared| {
                declared == id
                    || (kind == "beat" && declared.strip_suffix(&format!(".{id}")).is_some())
            })
    })?;
    let occasion = command.get("on").and_then(Json::as_str)?;
    let target = mock.occasions.iter().find_map(|raise| {
        let (name, target) = lute_trace::split_occasion(raise);
        (name == occasion).then_some(target).flatten()
    });
    let target_kind = command
        .get("targetKind")
        .or_else(|| command.get("forKind"))?;
    let members = target_kind
        .get("members")?
        .as_array()?
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>();
    let candidate = match target {
        Some(target) => {
            if let Some(prefix) = command
                .get("targetKind")
                .and_then(|target_kind| target_kind.get("prefix"))
                .and_then(Json::as_str)
            {
                target
                    .strip_prefix(prefix)
                    .and_then(|target| target.strip_prefix('.'))
                    .unwrap_or(target)
            } else {
                target.rsplit_once('.').map(|(_, member)| member).unwrap_or(target)
            }
        }
        None => {
            return target_kind
                .get("kind")
                .and_then(Json::as_str)
                .map(|k| format!("kind:{k}"))
        }
    };
    members
        .iter()
        .any(|member| *member == candidate)
        .then(|| candidate.to_string())
}

/// Bind a direct lore presentation's occasion target, unless the mock seeded
/// `occasion.target` explicitly (the trace path gives that seed precedence).
pub(crate) fn bind_direct_occasion_target<D: Driver>(
    m: &mut Machine<D>,
    art: &Json,
    mock: &lute_trace::MockSet,
    entry: Option<&str>,
    beat: Option<&str>,
) {
    if !mock
        .state
        .iter()
        .any(|(path, _, _)| path == lute_check::beats::OCCASION_TARGET)
    {
        if let Some(member) = direct_occasion_member(art, mock, entry, beat) {
            m.bind_occasion_target(Some(&member));
        }
    }
}

/// `lute run`'s Machine over `art`: a fresh walk seeded by `mock`,
/// presenting `entry` / `beat` of a lore artifact when given.
pub(crate) fn run_machine(
    art: &Json,
    mock: &lute_trace::MockSet,
    entry: Option<&str>,
    beat: Option<&str>,
) -> Machine<RunDriver> {
    let mut m = Machine::new(art, Seed::from(mock), RunDriver::from_mock(mock));
    if let Some(id) = entry {
        m = m.with_entry(id);
    }
    if let Some(id) = beat {
        m = m.with_bundle_beat(id);
    }
    bind_direct_occasion_target(&mut m, art, mock, entry, beat);
    m
}

/// Parse a full numeric `MAJOR.MINOR.PATCH` IR version.
fn parse_ir_version(v: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = v.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

/// `lute run`'s [`Driver`]: the mock's `choose:` and `bridges:`, a spent
/// `once` force skipped and a closed one refused, no unknown halting. Its
/// transcript is the conformance contract: the play-only record fields
/// ([`LINE_DELIVERY_KEYS`], [`MENU_MARK_KEYS`]) are dropped as records
/// arrive.
pub(crate) struct RunDriver {
    choices: ScriptedChoices,
    bridges: BridgeQueues,
    pub(crate) transcript: Vec<Json>,
}

impl RunDriver {
    pub(crate) fn from_mock(mock: &lute_trace::MockSet) -> Self {
        RunDriver {
            choices: ScriptedChoices::new(mock.choose.clone(), Default::default()),
            bridges: BridgeQueues {
                top: BridgeQueues::queue(&mock.bridges),
                ..BridgeQueues::default()
            },
            transcript: Vec::new(),
        }
    }
}

impl Driver for RunDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        self.choices.pick(menu)
    }

    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, verdict: &Verdict) -> Forced {
        match verdict {
            Verdict::Spent => Forced::Skip,
            Verdict::Closed(_) => Forced::Refuse,
            Verdict::Open | Verdict::Unknown(_) => Forced::Take,
        }
    }

    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        match self.bridges.next(call.tag) {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        OnUnknown::Continue
    }

    fn emit(&mut self, mut rec: Json) {
        let drop: &[&str] = match rec.get("kind").and_then(Json::as_str) {
            Some("line") => &LINE_DELIVERY_KEYS,
            Some("choice" | "hub") => &MENU_MARK_KEYS,
            _ => &[],
        };
        if let Some(map) = rec.as_object_mut() {
            for key in drop {
                map.remove(*key);
            }
        }
        self.transcript.push(rec);
    }
}

/// The state `lute run` reports: every path the Machine holds except an
/// `entry.<id>.everRead` the artifact does not declare — the engine's
/// user-tier read flag the walk now writes (D11), which is not part of the
/// `lute run` transcript contract (conformance `expected.json`).
fn reported_state<'m>(
    m: &'m Machine<RunDriver>,
    art: &Json,
) -> impl Iterator<Item = (&'m String, &'m lute_trace::Value)> {
    let declared: std::collections::BTreeSet<String> = art
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.get("path").and_then(Json::as_str).map(str::to_string))
        .collect();
    m.state().iter().filter(move |(k, _)| {
        !(k.starts_with("entry.") && k.ends_with(".everRead")) || declared.contains(k.as_str())
    })
}

/// The `--json` transcript `{ kind, irVersion, exit, commands, state,
/// facts, quests }`.
fn output_value(m: &Machine<RunDriver>, art: &Json) -> Json {
    let state: serde_json::Map<String, Json> = reported_state(m, art)
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();
    let facts: Vec<Json> = m
        .all_facts()
        .iter()
        .map(|(r, a)| Json::String(render_fact(r, a)))
        .collect();
    let quests: serde_json::Map<String, Json> = m
        .quest_status()
        .iter()
        .map(|(k, v)| (k.clone(), Json::String(v.clone())))
        .collect();
    let (ir_major, ir_minor) = impl_ir_line();
    json!({
        "kind": m.kind(),
        "irVersion": format!("{ir_major}.{ir_minor}"),
        "exit": if m.incomplete() { "incomplete" } else { "complete" },
        "commands": m.driver().transcript,
        "state": state,
        "facts": facts,
        "quests": quests,
    })
}

fn print_human(m: &Machine<RunDriver>, art: &Json, artifact: &Path) {
    println!("run {} execution IR {}", m.kind(), artifact.display());
    for e in &m.driver().transcript {
        let k = e.get("kind").and_then(Json::as_str).unwrap_or("");
        let a = e.get("addr").and_then(Json::as_str).unwrap_or("");
        let line = match k {
            "line" => format!(
                "  {a}  {}: {}",
                e.get("speaker").and_then(Json::as_str).unwrap_or(""),
                e.get("text").and_then(Json::as_str).unwrap_or("")
            ),
            "set" => format!(
                "  {a}  set    {} = {}",
                e.get("path").and_then(Json::as_str).unwrap_or(""),
                json_scalar_str(e.get("value"))
            ),
            "assert" => format!(
                "  {a}  assert {}",
                e.get("fact").and_then(Json::as_str).unwrap_or("")
            ),
            "retract" => format!(
                "  {a}  retract {}",
                e.get("pattern").and_then(Json::as_str).unwrap_or("")
            ),
            "choice" => format!(
                "  {a}  choice [{}] -> {}",
                e.get("branch").and_then(Json::as_str).unwrap_or(""),
                e.get("chose").and_then(Json::as_str).unwrap_or("(none)")
            ),
            "hub" => format!(
                "  {a}  hub    [{}]{} -> {}",
                e.get("hub").and_then(Json::as_str).unwrap_or(""),
                e.get("prompt")
                    .and_then(Json::as_str)
                    .map(|p| format!(" \"{p}\""))
                    .unwrap_or_default(),
                e.get("chose").and_then(Json::as_str).unwrap_or("(none)")
            ),
            "hubReturn" => format!(
                "  {a}  return [{}]",
                e.get("hub").and_then(Json::as_str).unwrap_or("")
            ),
            "match" => format!(
                "  {a}  match  -> {}",
                e.get("result").and_then(Json::as_str).unwrap_or("")
            ),
            "barrier" => format!("  {a}  barrier (no real clock)"),
            "entry" => {
                let read = if e.get("firstRead").and_then(Json::as_bool) == Some(true) {
                    "first read"
                } else {
                    "re-read: effects skipped"
                };
                let gate = match e.get("eligible").and_then(Json::as_bool) {
                    Some(true) => "",
                    Some(false) => ", not eligible (`when` is false)",
                    None => ", eligibility unknown",
                };
                format!(
                    "  {a}  entry  {} ({read}{gate})",
                    e.get("id").and_then(Json::as_str).unwrap_or("")
                )
            }
            "beat" => {
                let gate = match e.get("eligible").and_then(Json::as_bool) {
                    Some(true) => "",
                    Some(false) => " (not eligible: `when` is false)",
                    None => " (eligibility unknown)",
                };
                format!(
                    "  {a}  beat   {}{gate}",
                    e.get("id").and_then(Json::as_str).unwrap_or("")
                )
            }
            "skipped" => {
                let what = ["path", "fact", "pattern"]
                    .iter()
                    .find_map(|k| e.get(*k).and_then(Json::as_str))
                    .unwrap_or("");
                format!(
                    "  {a}  {} {what} (skipped: re-read)",
                    e.get("effect").and_then(Json::as_str).unwrap_or("")
                )
            }
            "exclusive" => format!(
                "  ✗ exclusive: {}",
                e.get("text").and_then(Json::as_str).unwrap_or("")
            ),
            "end" => match e.get("reason").and_then(Json::as_str) {
                Some(r) => format!("  {a}  end    reason={r}"),
                None => format!("  {a}  end"),
            },
            "plugin" => {
                let note = plugin_call_note(e);
                let tag = e.get("tag").and_then(Json::as_str).unwrap_or("");
                if note.is_empty() {
                    format!("  {a}  plugin {tag}")
                } else {
                    format!("  {a}  plugin {tag} {note}")
                }
            }
            "accept" => {
                let ignored = e
                    .get("ignored")
                    .and_then(Json::as_str)
                    .map(|s| format!(" ({s} — ignored)"))
                    .unwrap_or_default();
                format!(
                    "  {a}  quest {} accepted{ignored}",
                    e.get("quest").and_then(Json::as_str).unwrap_or("")
                )
            }
            "occasion" => {
                let target = e
                    .get("target")
                    .and_then(Json::as_str)
                    .map_or_else(String::new, |t| format!(" → {t}"));
                format!(
                    "  occasion {}{target}",
                    e.get("occasion").and_then(Json::as_str).unwrap_or("")
                )
            }
            "objective" => {
                let quest = e.get("quest").and_then(Json::as_str).unwrap_or("");
                let objective = e.get("objective").and_then(Json::as_str).unwrap_or("");
                // dsl 0.23.0 §2 / 0.24.0 §2.1: a `by` (or `until`)
                // deadline passed first.
                match e.get("failedBy").and_then(Json::as_str) {
                    Some(by) if e.get("failed").and_then(Json::as_bool) == Some(true) => {
                        format!("  {quest}.{objective} failed ({by})")
                    }
                    _ => format!("  {quest}.{objective} done"),
                }
            }
            // dsl 0.24.0 §2: a failure names its reason (`failedBy`).
            "quest" => {
                let reason = e
                    .get("failedBy")
                    .and_then(Json::as_str)
                    .map(|by| format!(" ({by})"))
                    .unwrap_or_default();
                format!(
                    "  quest {} -> {}{reason}",
                    e.get("quest").and_then(Json::as_str).unwrap_or(""),
                    e.get("state").and_then(Json::as_str).unwrap_or("")
                )
            }
            "grant" => {
                let quest = e.get("quest").and_then(Json::as_str).unwrap_or("");
                let owner = match e.get("objective").and_then(Json::as_str) {
                    Some(oid) => format!("{quest}.{oid}"),
                    None => quest.to_string(),
                };
                let reward = e.get("reward").cloned().unwrap_or(Json::Null);
                let kind = reward.get("kind").and_then(Json::as_str).unwrap_or("");
                let amount = if let Some(n) = reward.get("amount").and_then(Json::as_i64) {
                    n.to_string()
                } else {
                    let lo = reward.get("amountMin").and_then(Json::as_i64);
                    let hi = reward.get("amountMax").and_then(Json::as_i64);
                    match (lo, hi) {
                        (Some(l), Some(h)) => format!("{l}..{h}"),
                        _ => "?".to_string(),
                    }
                };
                let target = reward
                    .get("target")
                    .and_then(Json::as_str)
                    .map(|t| format!(" -> {t}"))
                    .unwrap_or_default();
                let annot = if e.get("onFailed").and_then(Json::as_bool) == Some(true) {
                    " (outcome=\"failed\")"
                } else {
                    ""
                };
                let instance = e.get("instance").and_then(Json::as_u64).unwrap_or(0);
                let index = e.get("index").and_then(Json::as_u64).unwrap_or(0);
                format!("  grant[#{instance} i{index}] {owner}  {kind} {amount}{target}{annot}")
            }
            _ => format!("  {a}  {k}"),
        };
        println!("{line}");
    }
    println!("-- final state --");
    for (k, v) in reported_state(m, art) {
        println!("  {k} = {}", value_to_string(v));
    }
    if !m.all_facts().is_empty() {
        println!("-- facts --");
        for (r, a) in m.all_facts() {
            println!("  {}", render_fact(r, a));
        }
    }
    if !m.quest_status().is_empty() {
        println!("-- quests --");
        for (k, v) in m.quest_status() {
            println!("  {k}: {v}");
        }
    }
    println!(
        "run {}",
        if m.incomplete() {
            "incomplete"
        } else {
            "complete"
        }
    );
}

/// The human annotation of a `plugin` transcript record (dsl 0.24.0 §5):
/// `(bridge answered: passed=true, margin=3)` when a `bridges:` answer
/// decided it, `(bridge unanswered: passed, margin)` when `lute play`
/// halted at it, `(external call, not invoked)` when its bridge results
/// went unresolved, and nothing for a call with only declared effects
/// (T1-3: each effect is its own `set` record, `effectOf` the tag).
pub(crate) fn plugin_call_note(rec: &Json) -> String {
    if let Some(Json::Array(fields)) = rec.get("answered") {
        let parts: Vec<String> = fields
            .iter()
            .map(|a| {
                format!(
                    "{}={}",
                    a.get("field").and_then(Json::as_str).unwrap_or(""),
                    a.get("value").unwrap_or(&Json::Null)
                )
            })
            .collect();
        return format!("(bridge answered: {})", parts.join(", "));
    }
    if let Some(Json::Array(fields)) = rec.get("unanswered") {
        let parts: Vec<&str> = fields.iter().filter_map(Json::as_str).collect();
        return format!("(bridge unanswered: {})", parts.join(", "));
    }
    if rec.get("note").is_some() {
        return "(external call, not invoked)".to_string();
    }
    String::new()
}
fn json_scalar_str(j: Option<&Json>) -> String {
    match j {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::Null) | None => "unset".to_string(),
        Some(other) => other.to_string(),
    }
}
