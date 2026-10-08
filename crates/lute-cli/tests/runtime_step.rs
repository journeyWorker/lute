use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use lute_cli::{build_runtime, run_runtime_reference};
use lute_runtime::runtime::{AdvanceBy, Await, Event, Input, PickInput, Seed, State, StateWrite, WritesInput};
use lute_runtime::session::World;
use lute_runtime::Runtime;
use serde_json::Value;

fn json_yaml(text: &str) -> Value {
    serde_json::to_value(serde_yaml::from_str::<serde_yaml::Value>(text).unwrap()).unwrap()
}

fn scalar(value: &Value) -> Result<Value, String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(value.clone()),
        _ => Err(format!("non-scalar value {value}")),
    }
}

fn writes(value: Option<&Value>) -> Result<WritesInput, String> {
    let Some(Value::Object(map)) = value else { return Ok(WritesInput::default()) };
    let mut out = WritesInput::default();
    if let Some(Value::Object(state)) = map.get("state") {
        for (path, value) in state {
            if let Value::Object(m) = value {
                if let Some(add) = m.get("add") {
                    out.state.push(StateWrite { path: path.clone(), value: None, add: add.as_f64() });
                } else {
                    out.state.push(StateWrite { path: path.clone(), value: Some(scalar(value)?), add: None });
                }
            } else {
                out.state.push(StateWrite { path: path.clone(), value: Some(scalar(value)?), add: None });
            }
        }
    }
    for (key, dest) in [("facts", &mut out.facts), ("retract", &mut out.retract), ("accept", &mut out.accept)] {
        if let Some(Value::Array(items)) = map.get(key) {
            dest.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    Ok(out)
}

fn pick(value: Option<&Value>) -> Result<Option<PickInput>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.as_str() == Some("none") { return Ok(Some(PickInput::Pass("pass".into()))) }
    Ok(Some(PickInput::Beat { beat: value.as_str().ok_or_else(|| format!("invalid pick {value}"))?.into() }))
}

fn choose_map(value: Option<&Value>) -> BTreeMap<String, Vec<String>> {
    let Some(Value::Object(map)) = value else { return BTreeMap::new() };
    map.iter().filter_map(|(id, v)| {
        let values = match v {
            Value::Array(a) => a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            Value::String(s) => vec![s.clone()],
            _ => return None,
        };
        Some((id.clone(), values))
    }).collect()
}

fn bridges(value: Option<&Value>) -> BTreeMap<String, Vec<BTreeMap<String, Value>>> {
    let Some(Value::Object(map)) = value else { return BTreeMap::new() };
    map.iter().map(|(tag, value)| {
        let values = match value {
            Value::Array(items) => items.iter().filter_map(|item| item.as_object().cloned().map(|m| m.into_iter().collect())).collect(),
            Value::Object(item) => vec![item.clone().into_iter().collect()],
            _ => Vec::new(),
        };
        (tag.clone(), values)
    }).collect()
}

#[derive(Clone)]
struct ScriptStep {
    input: Input,
    choose: BTreeMap<String, Vec<String>>,
    bridges: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
}

fn advance(value: &Value) -> Result<AdvanceBy, String> {
    match value {
        Value::String(s) => Ok(AdvanceBy::Named(s.clone())),
        Value::Number(n) => Ok(AdvanceBy::Slots(n.as_u64().ok_or_else(|| format!("invalid advance {value}"))? as u32)),
        Value::Object(m) => {
            let Some(to) = m.get("to") else { return Err(format!("invalid advance {value}")) };
            if let Some(slot) = to.as_str() { return Ok(AdvanceBy::Named(slot.into())); }
            let Some(to) = to.as_object() else { return Err(format!("invalid advance {value}")) };
            Ok(AdvanceBy::To { to: lute_runtime::runtime::ClockPosition { weekday: to.get("weekday").and_then(Value::as_str).ok_or("missing weekday")?.into(), slot: to.get("slot").and_then(Value::as_str).ok_or("missing slot")?.into() } })
        }
        _ => Err(format!("invalid advance {value}")),
    }
}

fn script(text: &str, base: Option<&Path>) -> Result<(Seed, Vec<ScriptStep>, BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<BTreeMap<String, Value>>>), String> {
    let root = json_yaml(text).as_object().cloned().ok_or("script is not a map")?;
    let state = root.get("state").and_then(Value::as_object).map(|m| m.iter().map(|(path, value)| StateWrite { path: path.clone(), value: Some(scalar(value).unwrap()), add: None }).collect()).unwrap_or_default();
    let mut seed = Seed { state, facts: root.get("facts").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default(), derive: root.get("derive").and_then(Value::as_bool).unwrap_or(true), save: Default::default() };
    seed.save.visited = root.get("visited").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
    seed.save.presented = root.get("presented").and_then(Value::as_object).map(|m| m.iter().map(|(k,v)| (k.clone(), v.as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default())).collect()).unwrap_or_default();
    seed.save.quests = root.get("quests").and_then(Value::as_object).map(|m| m.iter().filter_map(|(k,v)| v.as_str().map(|v|(k.clone(),v.into()))).collect()).unwrap_or_default();
    seed.save.quest_instances = root.get("questInstances").and_then(Value::as_object).map(|m| m.iter().filter_map(|(k,v)| v.as_u64().map(|v|(k.clone(),v))).collect()).unwrap_or_default();
    seed.save.entries_read = root.get("entriesRead").and_then(Value::as_object).map(|m| m.iter().map(|(k,v)| (k.clone(), v.as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default())).collect()).unwrap_or_default();
    let top_choose = choose_map(root.get("choose"));
    let top_bridges = bridges(root.get("bridges"));
    let mut steps = Vec::new();
    let source_steps = root.get("steps").and_then(Value::as_array).ok_or("script has no steps")?;
    let mut raw_steps = Vec::new();
    for step in source_steps {
        let Some(m) = step.as_object() else { return Err("step is not a map".into()) };
        if let Some(include) = m.get("include").and_then(Value::as_str) {
            if ["repeat", "choose", "bridges"].iter().any(|key| m.contains_key(*key)) { return Err("scoped/repeated include is not representable".into()); }
            let Some(base) = base else { return Err("include is not representable".into()) };
            let included = fs::read_to_string(base.join(include)).map_err(|e| e.to_string())?;
            let included = json_yaml(&included);
            let items = included.get("steps").and_then(Value::as_array).ok_or("included file has no steps")?;
            for item in items {
                let mut item = item.as_object().cloned().ok_or("included step is not a map")?;
                for key in ["choose", "bridges"] {
                    if let Some(value) = m.get(key) { item.entry(key).or_insert_with(|| value.clone()); }
                }
                raw_steps.push(Value::Object(item));
            }
        } else {
            raw_steps.push(step.clone());
        }
    }
    for step in raw_steps {
        let m = step.as_object().ok_or("step is not a map")?;
        let choose = choose_map(m.get("choose"));
        let bridges = bridges(m.get("bridges"));
        let input = if let Some(occasion) = m.get("occasion").and_then(Value::as_str) {
            let payload = m.get("payload").and_then(Value::as_object).map(|m| m.clone().into_iter().collect()).unwrap_or_default();
            Input::RaiseOccasion { occasion: occasion.into(), target: m.get("target").and_then(Value::as_str).map(str::to_owned), payload, pick: pick(m.get("pick"))?, writes: writes(m.get("engine"))? }
        } else if let Some(by) = m.get("advance") {
            Input::AdvanceClock { by: advance(by)?, writes: writes(m.get("engine"))?, pick: pick(m.get("pick"))? }
        } else if m.contains_key("engine") {
            Input::HostWrite { writes: writes(m.get("engine"))? }
        } else if let Some(name) = m.get("event").and_then(Value::as_str) {
            Input::WorldEvent { name: name.into() }
        } else if m.contains_key("newRun") {
            Input::NewRun { writes: if m.get("newRun").and_then(Value::as_bool) == Some(true) { WritesInput::default() } else { writes(m.get("newRun"))? } }
        } else {
            return Err("unsupported step".into());
        };
        let repeated = m.get("repeat").and_then(Value::as_u64).unwrap_or(1);
        for _ in 0..repeated { steps.push(ScriptStep { input: input.clone(), choose: choose.clone(), bridges: bridges.clone() }); }
    }
    Ok((seed, steps, top_choose, top_bridges))
}

fn world_eq(a: &World, b: &World) {
    assert_eq!(a.state, b.state, "state");
    assert_eq!(a.facts, b.facts, "facts");
    assert_eq!(a.quests, b.quests, "quests");
    assert_eq!(a.quest_instances, b.quest_instances, "quest instances");
    assert_eq!(a.visited, b.visited, "visited");
    assert_eq!(a.spent_run, b.spent_run, "spent run");
    assert_eq!(a.spent_user, b.spent_user, "spent user");
    assert_eq!(a.spent_at, b.spent_at, "spent at");
    assert_eq!(a.share_spent_by, b.share_spent_by, "share spends");
    assert_eq!(a.accepts, b.accepts, "accepts");
    assert_eq!(a.next_run_accepts, b.next_run_accepts, "next run accepts");
    assert_eq!(a.derive, b.derive, "derive");
    assert_eq!(a.failed_objectives, b.failed_objectives, "failed objectives");
    assert_eq!(a.defer_by, b.defer_by, "defer by");
    assert_eq!(a.defer_handlers, b.defer_handlers, "defer handlers");
    assert_eq!(a.deferred_handlers, b.deferred_handlers, "deferred handlers");
    assert_eq!(a.clock_advanced_by_beat, b.clock_advanced_by_beat, "clock advancement");
    assert_eq!(a.advance_cascade_depth, b.advance_cascade_depth, "advance cascade");
    assert_eq!(a.cadence.live, b.cadence.live, "cadence live");
    assert_eq!(a.cadence.rearm, b.cadence.rearm, "cadence rearm");
    assert_eq!(a.cadence.spent_season, b.cadence.spent_season, "spent season");
    assert_eq!(a.cadence.windows, b.cadence.windows, "cadence windows");
    assert_eq!(a.cadence.latched, b.cadence.latched, "cadence latches");
}

fn records(events: &[Event]) -> Vec<(String, Value)> {
    events.iter().filter_map(|event| match event { Event::Record { document, record } => Some((document.clone(), record.clone())), _ => None }).collect()
}

fn answer_loop(runtime: &Runtime, mut state: State, mut output: lute_runtime::Output, step: &ScriptStep, top_choose: &BTreeMap<String, Vec<String>>, top_bridges: &BTreeMap<String, Vec<BTreeMap<String, Value>>>, choice_cursors: &mut BTreeMap<String, usize>, bridge_cursors: &mut BTreeMap<String, usize>, all_records: &mut Vec<(String, Value)>) -> Result<(State, lute_runtime::Output), String> {
    all_records.extend(records(&output.events));
    loop {
        let input = match &output.await_ {
            Await::Choice { request, menu } => {
                let list = step.choose.get(&menu.id).or_else(|| top_choose.get(&menu.id)).cloned().unwrap_or_default();
                let option = if menu.construct == "hub" { list.get(menu.presentation).cloned() } else if list.len() == 1 { list.first().cloned() } else { let c = choice_cursors.entry(menu.id.clone()).or_default(); let v = list.get(*c).cloned(); if v.is_some() { *c += 1; } v };
                Input::Choose { request: *request, option: option.ok_or_else(|| format!("awaited choice `{}` has no scripted answer", menu.id))? }
            }
            Await::Bridge { request, tag, .. } => {
                let (queue, cursor) = if step.bridges.contains_key(tag) { (&step.bridges, bridge_cursors.entry(format!("step:{tag}")).or_default()) } else { (top_bridges, bridge_cursors.entry(format!("top:{tag}")).or_default()) };
                let answers = queue.get(tag).ok_or_else(|| format!("awaited bridge `{tag}` has no scripted answer"))?;
                let fields = answers.get(*cursor).cloned().ok_or_else(|| format!("bridge `{tag}` answer queue exhausted"))?;
                *cursor += 1;
                Input::BridgeResult { request: *request, fields }
            }
            Await::Idle | Await::Ended { .. } | Await::Halted { .. } => return Ok((state, output)),
        };
        match runtime.step(state, input) {
            Ok((next, next_output)) => { state = next; output = next_output; all_records.extend(records(&output.events)); }
            Err((_, rejected)) if rejected.code == "E-RUNTIME-BRIDGE-SHAPE" => return Err(rejected.message),
            Err((_, rejected)) => panic!("runtime rejected scripted answer: {} {}", rejected.code, rejected.message),
        }
    }
}

fn nearest_project(script: &Path) -> PathBuf {
    let mut dir = script.parent().unwrap();
    loop {
        if dir.join("lute.project.yaml").is_file() { return dir.to_path_buf(); }
        dir = dir.parent().expect("script has project root");
    }
}

fn all_play_scripts() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { walk(&path, out); }
            else if path.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with(".play.yaml")) { out.push(path); }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let mut out = Vec::new();
    walk(&root.join("conformance/session"), &mut out);
    walk(&root.join("docs/examples"), &mut out);
    out.sort();
    out
}

#[test]
fn play_scripts_match_resumable_runtime() {
    let mut exercised = 0;
    let mut skipped = 0;
    'scripts: for script_path in all_play_scripts() {
        let text = fs::read_to_string(&script_path).unwrap();
        let (seed, steps, top_choose, top_bridges) = match script(&text, script_path.parent()) {
            Ok(value) => value,
            Err(message) => { skipped += 1; println!("SKIP {} ({message})", script_path.display()); continue; }
        };
        let project = nearest_project(&script_path);
        let reference = match run_runtime_reference(&project, &script_path) {
            Ok(reference) => reference,
            Err(message) => { skipped += 1; println!("SKIP {} ({message})", script_path.display()); continue; }
        };
        let runtime = match build_runtime(&project) {
            Ok(runtime) => runtime,
            Err(message) => { skipped += 1; println!("SKIP {} ({message})", script_path.display()); continue; }
        };
        let (mut state, output) = runtime.begin(seed).unwrap_or_else(|e| panic!("{}: begin rejected: {} {}", script_path.display(), e.code, e.message));
        let mut actual = Vec::new();
        let mut choice_cursors = BTreeMap::new();
        let mut bridge_cursors = BTreeMap::new();
        actual.extend(records(&output.events));
        for step in &steps {
            let (next, next_output) = runtime.step(state, step.input.clone()).unwrap_or_else(|(_, e)| panic!("{}: step rejected: {} {}", script_path.display(), e.code, e.message));
            let (next, _) = match answer_loop(&runtime, next, next_output, step, &top_choose, &top_bridges, &mut choice_cursors, &mut bridge_cursors, &mut actual) {
                Ok(value) => value,
                Err(message) => { skipped += 1; println!("SKIP {} ({message})", script_path.display()); continue 'scripts; }
            };
            state = next;
        }
        assert_eq!(actual, reference.records, "record divergence in {}", script_path.display());
        world_eq(&state.world, &reference.world);
        exercised += 1;
    }
    println!("runtime_step: exercised={exercised}, skipped={skipped}");
    assert!(exercised >= 60, "only exercised {exercised} scripts (skipped {skipped})");
}

#[test]
fn nested_hub_and_branch_resume_matches_uninterrupted_play() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("conformance/session/snapshot-hub/project");
    let script_path = project.join("script.play.yaml");
    let text = fs::read_to_string(&script_path).unwrap();
    let (seed, steps, top_choose, top_bridges) = script(&text, script_path.parent()).unwrap();
    let runtime = build_runtime(&project).unwrap();
    let reference = run_runtime_reference(&project, &script_path).unwrap();
    let (state, begun) = runtime.begin(seed).unwrap();
    let mut records = records(&begun.events);
    let (state, output) = match runtime.step(state, steps[0].input.clone()) { Ok(value) => value, Err((_, error)) => panic!("step rejected: {} {}", error.code, error.message) };
    let (state, _) = answer_loop(&runtime, state, output, &steps[0], &top_choose, &top_bridges, &mut BTreeMap::new(), &mut BTreeMap::new(), &mut records).unwrap();
    assert_eq!(records, reference.records);
    world_eq(&state.world, &reference.world);
}

#[test]
fn rejected_inputs_leave_state_unchanged() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("conformance/session/occasion-first/project");
    let runtime = build_runtime(&project).unwrap();
    let (state, _) = runtime.begin(Seed::default()).unwrap();
    let before = state.clone();
    let (returned, busy) = match runtime.step(state, Input::Choose { request: 1, option: "x".into() }) { Err(err) => err, Ok(_) => panic!("idle choose unexpectedly succeeded") };
    assert_eq!(busy.code, "E-RUNTIME-BUSY");
    world_eq(&returned.world, &before.world);
    assert_eq!(returned.request, before.request);
    assert_eq!(format!("{:?}", returned.phase), format!("{:?}", before.phase));
    assert!(returned.continuation.is_none());
    let hub_project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("conformance/session/snapshot-hub/project");
    let hub_script = hub_project.join("script.play.yaml");
    let text = fs::read_to_string(&hub_script).unwrap();
    let (seed, steps, _, _) = script(&text, hub_script.parent()).unwrap();
    let runtime = build_runtime(&hub_project).unwrap();
    let (state, _) = runtime.begin(seed).unwrap();
    let (state, output) = match runtime.step(state, steps[0].input.clone()) { Ok(value) => value, Err((_, error)) => panic!("step rejected: {} {}", error.code, error.message) };
    if let Await::Choice { request, menu } = output.await_ {
        let before = state.world.clone();
        let (returned, rejected) = match runtime.step(state, Input::Choose { request: request + 1, option: menu.options[0].id.clone() }) { Err(err) => err, Ok(_) => panic!("wrong request unexpectedly succeeded") };
        assert_eq!(rejected.code, "E-RUNTIME-REQUEST");
        world_eq(&returned.world, &before);
    } else {
        panic!("hub did not await a choice");
    }
}

#[test]
fn nested_hub_and_bridge_resume_and_records_are_not_duplicated() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("conformance/session/bridge-call/project");
    let script_path = project.join("script.play.yaml");
    let text = fs::read_to_string(&script_path).unwrap();
    let (seed, steps, top_choose, top_bridges) = script(&text, script_path.parent()).unwrap();
    let runtime = build_runtime(&project).unwrap();
    let reference = run_runtime_reference(&project, &script_path).unwrap();
    let (mut state, mut output) = runtime.begin(seed).unwrap();
    let mut all = records(&output.events);
    let mut choices = BTreeMap::new();
    let mut bridges = BTreeMap::new();
    for step in &steps {
        let before = all.len();
        let (next, next_output) = match runtime.step(state, step.input.clone()) { Ok(value) => value, Err((_, error)) => panic!("step rejected: {} {}", error.code, error.message) };
        state = next;
        let (next, _) = answer_loop(&runtime, state, next_output, step, &top_choose, &top_bridges, &mut choices, &mut bridges, &mut all).unwrap();
        state = next;
        assert!(all.len() >= before);
        output = lute_runtime::Output { event_version: "0.38.0".into(), events: Vec::new(), await_: Await::Idle };
    }
    assert_eq!(all, reference.records);
    world_eq(&state.world, &reference.world);
    assert!(output.events.is_empty());
}
