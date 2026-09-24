//! `lute play <PROJECT_DIR> --script <FILE>` — the occasion-driven reference
//! playthrough (dsl 0.21.0 §6, `docs/proposals/scenario-dsl/0.21.0.md`).
//!
//! A play script raises a sequence of **occasions**. For each one this module
//! computes the candidate **beats** (`ProjectIndex.beats` rows answering the
//! occasion, restricted by target), decides each candidate's verdict against
//! the LIVE playthrough — `once` spending, `after:` over the real `visited` /
//! `completed` / `active` sets, `when` through the reference runner's CEL
//! evaluator — orders the eligible ones by priority then index order (§4),
//! presents the winner (or the script's `pick` for a `select: all` occasion)
//! and then advances every quest lifecycle, so the next step's `when` over
//! `quest.*` and `after: completed(…)` see real progress (D-H).
//!
//! ## Runs
//! One `lute play` invocation is one player profile. `once: run` is spent by
//! a presentation until the next `newRun` step; `once: user` stays spent for
//! the rest of the invocation (spending across separate invocations is not
//! modelled — put the runs in one script, separated by `newRun`). A
//! `newRun` resets `run.*` state and `entry.*` read flags (run-tier, dsl
//! 0.19.0 §5) to their declared defaults and run-tier facts to the project's
//! seed facts; `user.*`/`app.*`/`quest.*` state, other facts and the
//! `visited` history persist.
//!
//! ## What is reused, never re-implemented
//! - Whole-project compile + gate: [`crate::reconciled_project_results`] +
//!   [`crate::gate_for_doc`] + `lute_compile::compile_with_check`, the loop
//!   `compile --all` ([`crate::compile_all`]) runs, kept in memory.
//! - Beat table and declaration union: `lute_compile::index::build_index` —
//!   the SAME `beats` rows (and tiebreak order), rules, seed facts and
//!   relation tiers `compile --all` writes to `project.index.json`.
//! - Execution: [`crate::runner::Runner`] — `lute run`'s evaluator — runs
//!   every scene beat, every entry beat (its `--entry` path: first-read
//!   effects, `entry.<id>.read`), every quest-lifecycle advance
//!   ([`Runner::advance_quests`]) and every `when` ([`Runner::eval_guard`]).
//! - Script surfaces: `state:` / `facts:` / `choose:` are parsed by the
//!   trace-mock grammar ([`lute_trace::parse_mock_yaml`]); only `steps:` is
//!   this module's own.
//!
//! ## Honesty
//! Nothing the reference runner cannot decide is decided silently: a `when`
//! that evaluates unknown and could change the outcome, an unscripted
//! choice/hub, an undecidable required quest objective, `now()` /
//! `validAt(...)`, and an unresolved plugin `bridgeResult` all halt the walk
//! incomplete (exit 3), naming what could not be decided.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::PrereqFormula;
use lute_compile::index::{build_index, BeatKind, IndexBeat, IndexInput, ProjectIndex};
use lute_compile::{Artifact, BeatOnce};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_trace::{MockSet, UnresolvedAtom, Value};
use serde_json::{json, Value as Json};

use crate::runner::{Fact, Runner, RunnerOutcome};

// ===========================================================================
// Play script (`*.play.yaml`, dsl 0.21.0 §6).
// ===========================================================================

/// The complete legal top-level key set of a play script.
const SCRIPT_KEYS: &[&str] = &["choose", "facts", "state", "steps"];
/// The script keys that ARE trace-mock surfaces, parsed by the mock grammar.
const MOCK_SURFACES: &[&str] = &["choose", "facts", "state"];

/// One `steps:` entry.
enum ScriptStep {
    /// Raise `occasion` (for `target`); `pick` is the beat the player takes
    /// on a `select: all` occasion.
    Occasion {
        occasion: String,
        target: Option<String>,
        pick: Option<String>,
    },
    /// Start a new run: `run.*` state, run-tier facts and `once: run`
    /// spending reset.
    NewRun,
}

/// A parsed play script.
struct PlayScript {
    /// `state:` / `facts:` / `choose:`, exactly as a trace mock carries them.
    surfaces: MockSet,
    steps: Vec<ScriptStep>,
}

/// Parse a play script. Total: never panics; `Err` names what is wrong.
fn parse_script(text: &str) -> Result<PlayScript, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| format!("malformed YAML: {e}"))?;
    let serde_yaml::Value::Mapping(top) = value else {
        return Err("a play script must be a YAML mapping with a `steps:` list".to_string());
    };
    let mut surfaces = serde_yaml::Mapping::new();
    let mut steps = None;
    for (k, v) in &top {
        let Some(key) = k.as_str() else {
            return Err("a play script's top-level keys must be strings".to_string());
        };
        if key == "steps" {
            steps = Some(v);
        } else if MOCK_SURFACES.contains(&key) {
            surfaces.insert(k.clone(), v.clone());
        } else {
            return Err(format!(
                "unknown top-level key `{key}` (legal: {})",
                SCRIPT_KEYS.join(", ")
            ));
        }
    }
    let surfaces = if surfaces.is_empty() {
        MockSet::default()
    } else {
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(surfaces))
            .map_err(|e| format!("cannot re-read `state:`/`facts:`/`choose:`: {e}"))?;
        lute_trace::parse_mock_yaml(&text).map_err(|d| d.message)?
    };
    let Some(steps) = steps else {
        return Err("`steps:` is required — the occasions to raise, in order".to_string());
    };
    let serde_yaml::Value::Sequence(items) = steps else {
        return Err("`steps:` must be a list".to_string());
    };
    if items.is_empty() {
        return Err("`steps:` is empty — there is nothing to play".to_string());
    }
    let steps = items
        .iter()
        .enumerate()
        .map(|(i, item)| parse_step(i + 1, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlayScript { surfaces, steps })
}

/// One `steps:` entry: exactly `{occasion, target?, pick?}` or
/// `{newRun: true}`.
fn parse_step(n: usize, item: &serde_yaml::Value) -> Result<ScriptStep, String> {
    let shape = "`{occasion, target?, pick?}` or `{newRun: true}`";
    let serde_yaml::Value::Mapping(m) = item else {
        return Err(format!("step {n} must be a mapping — {shape}"));
    };
    if m.is_empty() {
        return Err(format!("step {n} is empty — {shape}"));
    }
    let (mut occasion, mut target, mut pick, mut new_run) = (None, None, None, false);
    for (k, v) in m {
        let Some(key) = k.as_str() else {
            return Err(format!("step {n}: keys must be strings"));
        };
        match key {
            "occasion" | "target" | "pick" => {
                let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
                    return Err(format!("step {n}: `{key}` must be a non-empty string"));
                };
                let slot = match key {
                    "occasion" => &mut occasion,
                    "target" => &mut target,
                    _ => &mut pick,
                };
                *slot = Some(s.to_string());
            }
            "newRun" => {
                if v.as_bool() != Some(true) {
                    return Err(format!("step {n}: `newRun` must be `true`"));
                }
                new_run = true;
            }
            other => {
                return Err(format!(
                    "step {n}: unknown key `{other}` (legal: occasion, target, pick, newRun)"
                ))
            }
        }
    }
    match (occasion, new_run) {
        (Some(occasion), false) => Ok(ScriptStep::Occasion {
            occasion,
            target,
            pick,
        }),
        (None, true) if target.is_none() && pick.is_none() => Ok(ScriptStep::NewRun),
        (None, true) => Err(format!("step {n}: `newRun` takes no other key")),
        (Some(_), true) => Err(format!(
            "step {n}: a step raises an `occasion` or starts a `newRun`, not both"
        )),
        (None, false) => Err(format!("step {n} names no `occasion` — {shape}")),
    }
}

// ===========================================================================
// Whole-project compile (the `compile --all` loop, in memory) + the index.
// ===========================================================================

/// `path` relative to `root`, forward-slash joined — the project-relative
/// artifact identity `compile_all.rs`'s private `rel_slash` uses.
fn project_rel(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for c in rel.components() {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&c.as_os_str().to_string_lossy());
    }
    (!out.is_empty()).then_some(out)
}

/// Everything the playthrough reads from the compiled project.
struct Project {
    /// project-relative path -> compiled artifact JSON.
    artifacts: BTreeMap<String, Json>,
    /// The `compile --all` project index: `beats` (selection tiebreak
    /// order), `relations` (tiers), `seedFacts`, `rules`.
    index: ProjectIndex,
    /// The occasions every resolved plugin declares, unioned across the
    /// project's documents (first declaration wins). Empty ⇒ shape-only.
    occasions: BTreeMap<String, OccasionDecl>,
    /// State-TABLE union (`build_index` does not cover it): path -> its
    /// `StateEntry` JSON, first declaration in path order wins.
    state_table: BTreeMap<String, Json>,
    /// `index.rules` as artifact JSON, handed to every runner.
    rules: Json,
    /// `index.seedFacts` as ground facts.
    seed_facts: BTreeSet<Fact>,
    /// Base relations of `tier: run` — the facts a `newRun` resets.
    run_relations: BTreeSet<String>,
    /// Quest documents, path order — advanced after every presentation.
    quest_docs: Vec<String>,
    /// A command-less artifact carrying the union rules + state table: the
    /// evaluator runner every `when` is decided by.
    eval_json: Json,
}

impl Project {
    fn select_of(&self, occasion: &str) -> OccasionSelect {
        self.occasions
            .get(occasion)
            .map(|d| d.select)
            .unwrap_or_default()
    }
}

/// Compile every non-component document under `project_dir` in memory with
/// the `compile --all` gate — refusing to play a project that does not
/// wholly compile — and build its index. `Err` carries the exit code after
/// the diagnostics are printed.
fn compile_project(project_dir: &Path) -> Result<Project, ExitCode> {
    match crate::manifests::validate_manifests_under(project_dir) {
        Ok(mut verdicts) => {
            crate::manifests::mark_inert_under(&mut verdicts, project_dir);
            if crate::manifests::report_and_gate(&verdicts) {
                return Err(ExitCode::from(1));
            }
        }
        Err(e) => {
            eprintln!(
                "lute play: cannot walk {} for manifests: {e}",
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        }
    }

    let reconciled = crate::reconciled_project_results(project_dir, None)?;
    let identity = lute_manifest::project::load_project(project_dir)
        .ok()
        .flatten()
        .map(|p| p.identity)
        .unwrap_or_default();

    let mut compiled: BTreeMap<String, Artifact> = BTreeMap::new();
    let mut occasions: BTreeMap<String, OccasionDecl> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let policy = crate::DenyPolicy::default();

    for (file, base) in &reconciled.per_doc {
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, project_dir) else {
            eprintln!(
                "lute play: {} is not under {}",
                file.display(),
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        };
        let Some(built) = crate::build_input(file, None, Some(project_dir), None) else {
            return Err(ExitCode::from(2));
        };
        built.report_project_diags();
        if built.resolve_error {
            return Err(ExitCode::from(1));
        }
        for (name, decl) in &built.input.snapshot.occasions {
            occasions
                .entry(name.clone())
                .or_insert_with(|| decl.clone());
        }
        let gate = crate::gate_for_doc(&reconciled, file, base);
        match lute_compile::compile_with_check(&built.input, gate, &identity) {
            Ok(artifact) => {
                compiled.insert(rel, artifact);
            }
            Err(diags) => {
                failures.insert(
                    file.clone(),
                    crate::render_diagnostics(file, &diags, &policy),
                );
            }
        }
    }

    if !failures.is_empty() {
        for rendered in failures.values() {
            print!("{rendered}");
        }
        eprintln!(
            "lute play: {} of {} document(s) failed to compile; refusing to play",
            failures.len(),
            failures.len() + compiled.len()
        );
        return Err(ExitCode::from(1));
    }

    let inputs: Vec<IndexInput> = compiled
        .iter()
        .map(|(rel, art)| IndexInput {
            path: rel.clone(),
            artifact_path: format!("{rel}.json"),
            artifact: art,
        })
        .collect();
    let index = match build_index(lute_compile::LUTE_IR_VERSION, &inputs) {
        Ok(index) => index,
        Err(errs) => {
            for e in &errs {
                eprintln!("lute play: {e}");
            }
            eprintln!(
                "lute play: {} vocabulary conflict(s); refusing to play",
                errs.len()
            );
            return Err(ExitCode::from(1));
        }
    };

    let mut artifacts: BTreeMap<String, Json> = BTreeMap::new();
    for (rel, art) in &compiled {
        match serde_json::to_value(art) {
            Ok(j) => {
                artifacts.insert(rel.clone(), j);
            }
            Err(e) => {
                eprintln!("lute play: cannot serialize the artifact of {rel}: {e}");
                return Err(ExitCode::from(2));
            }
        }
    }

    let mut state_table: BTreeMap<String, Json> = BTreeMap::new();
    for art in artifacts.values() {
        for e in art
            .get("state")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(path) = e.get("path").and_then(Json::as_str) {
                state_table
                    .entry(path.to_string())
                    .or_insert_with(|| e.clone());
            }
        }
    }
    let rules = serde_json::to_value(&index.rules).unwrap_or_else(|_| json!([]));
    let seed_facts = index
        .seed_facts
        .iter()
        .map(|f| (f.relation.clone(), f.args.clone()))
        .collect();
    let run_relations = index
        .relations
        .iter()
        .filter(|r| r.tier.as_deref() == Some("run"))
        .map(|r| r.name.clone())
        .collect();
    let quest_docs = artifacts
        .iter()
        .filter(|(_, a)| a.get("kind").and_then(Json::as_str) == Some("quest"))
        .map(|(rel, _)| rel.clone())
        .collect();
    let eval_json = json!({
        "kind": "scene",
        "commands": [],
        "rules": rules.clone(),
        "state": state_table.values().cloned().collect::<Vec<_>>(),
    });

    Ok(Project {
        artifacts,
        index,
        occasions,
        state_table,
        rules,
        seed_facts,
        run_relations,
        quest_docs,
        eval_json,
    })
}

/// The artifact JSON handed to a presentation's [`Runner`]: this document's
/// OWN `commands`/`meta`/`kind`/`prereqEdges`, with `rules`/`state` REPLACED
/// by the project-wide union — a relation asserted in one document is
/// derived over in another, and a `run.*`/`user.*`/`quest.*` path declared
/// elsewhere still needs its declared type here.
fn play_artifact_json(doc_json: &Json, p: &Project) -> Json {
    let mut v = doc_json.clone();
    if let Json::Object(map) = &mut v {
        map.insert("rules".to_string(), p.rules.clone());
        map.insert(
            "state".to_string(),
            Json::Array(p.state_table.values().cloned().collect()),
        );
    }
    v
}

/// Every step's usage-level check, before anything plays (exit 2 on `Err`):
/// the occasion exists, `target` is legal for it, `pick` is required
/// exactly for `select: all` and names a beat answering that occasion.
fn validate_steps(p: &Project, steps: &[ScriptStep]) -> Result<(), String> {
    let answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    for (i, step) in steps.iter().enumerate() {
        let ScriptStep::Occasion {
            occasion,
            target,
            pick,
        } = step
        else {
            continue;
        };
        let n = i + 1;
        let decl = p.occasions.get(occasion);
        if p.occasions.is_empty() {
            if !answered.contains(occasion.as_str()) {
                return Err(format!(
                    "step {n}: occasion `{occasion}` is answered by no beat in this project \
                     (no plugin declares occasions, so the beats' `on:` values are the vocabulary)"
                ));
            }
        } else if decl.is_none() {
            let declared: Vec<&str> = p.occasions.keys().map(String::as_str).collect();
            return Err(format!(
                "step {n}: occasion `{occasion}` is declared by no resolved plugin (declared: {})",
                declared.join(", ")
            ));
        }
        if let (Some(t), Some(d)) = (target, decl) {
            if !d.target {
                return Err(format!(
                    "step {n}: occasion `{occasion}` is not declared `target: true`, so it cannot \
                     be raised for `{t}`"
                ));
            }
        }
        match (p.select_of(occasion), pick) {
            (OccasionSelect::All, None) => {
                return Err(format!(
                    "step {n}: occasion `{occasion}` is `select: all` — name the beat the player \
                     takes with `pick:`"
                ))
            }
            (OccasionSelect::First, Some(pk)) => {
                return Err(format!(
                    "step {n}: `pick: {pk}` applies only to a `select: all` occasion; \
                     `{occasion}` is `select: first`"
                ))
            }
            (OccasionSelect::All, Some(pk)) => {
                if !p
                    .index
                    .beats
                    .iter()
                    .any(|b| &b.id == pk && is_candidate(b, occasion, target.as_deref()))
                {
                    return Err(format!(
                        "step {n}: `pick: {pk}` names no beat answering `{occasion}`{}",
                        target
                            .as_deref()
                            .map(|t| format!(" for `{t}`"))
                            .unwrap_or_default()
                    ));
                }
            }
            (OccasionSelect::First, None) => {}
        }
    }
    Ok(())
}

/// dsl 0.21.0 §4: a candidate answers `occasion` and its `target` is absent
/// or equal to the raised one.
fn is_candidate(b: &IndexBeat, occasion: &str, target: Option<&str>) -> bool {
    b.on == occasion && b.target.as_deref().is_none_or(|t| Some(t) == target)
}

// ===========================================================================
// The live playthrough.
// ===========================================================================

/// Everything that carries from one step to the next.
struct World {
    /// Persistent-tier state (`run.*`/`user.*`/`app.*`/`quest.*`/`entry.*`);
    /// `scene.*` never lives here — it resets at every scene boundary.
    state: BTreeMap<String, Value>,
    /// Base facts (the runner derives over them).
    facts: BTreeSet<Fact>,
    /// quest id -> `unset`/`active`/`complete`/`failed`.
    quests: BTreeMap<String, String>,
    /// Canonical ids of every presented scene (the `visited(…)` set).
    visited: BTreeSet<String>,
    /// Scene beats presented since the last `newRun` (`once: run`).
    spent_run: BTreeSet<String>,
    /// Scene beats presented in this play (`once: user`).
    spent_user: BTreeSet<String>,
}

fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => json!(*n as i64),
        Value::Num(n) => json!(n),
        Value::Str(s) => Json::String(s.clone()),
        Value::Unknown => Json::Null,
    }
}

/// Coerce a script `state:` literal against its declared type (mirrors the
/// runner's own mock-literal coercion).
fn coerce_literal(p: &Project, path: &str, lit: &str) -> Value {
    let ty = p
        .state_table
        .get(path)
        .and_then(|e| e.get("type"))
        .and_then(Json::as_str);
    let guess = || match lit {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => lit
            .parse::<f64>()
            .map(Value::Num)
            .unwrap_or(Value::Str(lit.to_string())),
    };
    match ty {
        Some("bool") | Some("number") => guess(),
        Some(_) => Value::Str(lit.to_string()),
        None => guess(),
    }
}

/// Parse a ground `"rel(a, b)"` fact.
fn parse_ground_fact(s: &str) -> Option<Fact> {
    let s = s.trim();
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    let rel = s[..open].trim();
    if rel.is_empty() {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        inner.split(',').map(|a| a.trim().to_string()).collect()
    };
    Some((rel.to_string(), args))
}

/// The playthrough's starting world: every declared default (scene tier
/// excluded), the script's `state:` over it, the project's seed facts plus
/// the script's `facts:`. A seed naming an undeclared path or relation is a
/// usage error, never a silent no-op.
fn seed_world(p: &Project, surfaces: &MockSet) -> Result<World, String> {
    let mut state = BTreeMap::new();
    for (path, e) in &p.state_table {
        if path.starts_with("scene.") {
            continue;
        }
        if let Some(v) = e.get("default").and_then(json_to_value) {
            state.insert(path.clone(), v);
        }
    }
    for (path, lit, _) in &surfaces.state {
        if !p.state_table.contains_key(path) {
            return Err(format!(
                "`state.{path}` is not a declared state path in this project"
            ));
        }
        if path.starts_with("scene.") {
            return Err(format!(
                "`state.{path}`: `scene.*` resets at every scene boundary and cannot be seeded"
            ));
        }
        state.insert(path.clone(), coerce_literal(p, path, lit));
    }
    let mut facts = p.seed_facts.clone();
    for f in &surfaces.facts {
        let Some(fact) = parse_ground_fact(f) else {
            return Err(format!("`facts:` entry `{f}` is not a ground fact `rel(arg, …)`"));
        };
        match p.index.relations.iter().find(|r| r.name == fact.0) {
            None => {
                return Err(format!(
                    "`facts:` entry `{f}` names an undeclared relation `{}`",
                    fact.0
                ))
            }
            Some(r) if r.derive => {
                return Err(format!(
                    "`facts:` entry `{f}`: `{}` is derived by rules and cannot be seeded",
                    fact.0
                ))
            }
            Some(r) if r.args.len() != fact.1.len() => {
                return Err(format!(
                    "`facts:` entry `{f}`: `{}` takes {} argument(s)",
                    fact.0,
                    r.args.len()
                ))
            }
            Some(_) => {}
        }
        facts.insert(fact);
    }
    Ok(World {
        state,
        facts,
        quests: BTreeMap::new(),
        visited: BTreeSet::new(),
        spent_run: BTreeSet::new(),
        spent_user: BTreeSet::new(),
    })
}

/// `newRun: true`: `run.*` state back to its declared defaults, `entry.*`
/// read flags too (run-tier, dsl 0.19.0 §5), run-tier facts back to the
/// project's seed facts, and `once: run` spending cleared. `user.*`/`app.*`/
/// `quest.*` state, other facts, `visited` and `once: user` spending persist.
fn new_run(p: &Project, w: &mut World) {
    let run_tier = |path: &str| path.starts_with("run.") || path.starts_with("entry.");
    w.state.retain(|k, _| !run_tier(k));
    for (path, e) in &p.state_table {
        if run_tier(path) {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                w.state.insert(path.clone(), v);
            }
        }
    }
    w.facts.retain(|(rel, _)| !p.run_relations.contains(rel));
    for f in &p.seed_facts {
        if p.run_relations.contains(&f.0) {
            w.facts.insert(f.clone());
        }
    }
    w.spent_run.clear();
}

/// Fold a finished runner back into the world: persistent tiers only
/// (`scene.*` never carries), facts, quest statuses.
fn absorb(w: &mut World, outcome: &RunnerOutcome) {
    for (k, v) in &outcome.state {
        if !k.starts_with("scene.") {
            w.state.insert(k.clone(), v.clone());
        }
    }
    w.facts = outcome.base_facts.clone();
    w.quests = outcome.quest_status.clone();
}

/// A scene's fresh starting state: its OWN `scene.*` defaults (never the
/// union's, which may carry another document's same-named scene path), with
/// the world's persistent tiers overlaid.
fn scene_initial_state(doc_json: &Json, live: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    for e in doc_json
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = e.get("path").and_then(Json::as_str) else {
            continue;
        };
        if path.starts_with("scene.") {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                state.insert(path.to_string(), v);
            }
        }
    }
    for (k, v) in live {
        state.insert(k.clone(), v.clone());
    }
    state
}

/// Why the playthrough stopped short of its last step.
enum PlayHalt {
    /// exit 1 — a `pick` that is not eligible.
    Error(String),
    /// exit 2 — the runner refused a malformed artifact / unknown command.
    Fatal(String),
    /// exit 3 — something the reference runner cannot decide.
    Incomplete(String),
}

impl PlayHalt {
    fn exit_code(&self) -> ExitCode {
        ExitCode::from(match self {
            PlayHalt::Error(_) => 1,
            PlayHalt::Fatal(_) => 2,
            PlayHalt::Incomplete(_) => 3,
        })
    }

    fn message(&self) -> &str {
        match self {
            PlayHalt::Error(m) | PlayHalt::Fatal(m) | PlayHalt::Incomplete(m) => m,
        }
    }

    fn exit_label(&self) -> &'static str {
        match self {
            PlayHalt::Incomplete(_) => "incomplete",
            PlayHalt::Error(_) | PlayHalt::Fatal(_) => "error",
        }
    }
}

/// What ends a playthrough before its last step.
enum Stop {
    /// A `::end` (dsl 0.8.0) — the whole playthrough is over, complete.
    End(String),
    Halt(PlayHalt),
}

fn describe_atoms(atoms: &[UnresolvedAtom]) -> String {
    let mut parts: Vec<String> = atoms
        .iter()
        .map(|a| match a {
            UnresolvedAtom::Path(p) => format!("state path `{p}` has no value"),
            UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => {
                format!("fact `{f}` is undetermined")
            }
            UnresolvedAtom::Time => {
                "now()/validAt(...) has no reference-runtime resolution".to_string()
            }
        })
        .collect();
    parts.dedup();
    if parts.is_empty() {
        "it does not evaluate to a bool".to_string()
    } else {
        parts.join("; ")
    }
}

/// The honesty gate every runner outcome passes (`what` names the
/// presentation or quest document): an unscripted decision, an undecidable
/// quest objective, `now()`/`validAt(...)`, an unresolved plugin effect.
fn outcome_halt(outcome: &RunnerOutcome, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    if outcome.incomplete {
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| {
            c.get("note").and_then(Json::as_str) == Some("no mock decision — incomplete")
        }) {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("choice");
            let id = rec
                .get("branch")
                .or_else(|| rec.get("hub"))
                .and_then(Json::as_str)
                .unwrap_or("?");
            let options = decision_options(doc_json, id);
            return Some(PlayHalt::Incomplete(format!(
                "{what} reached {kind} `{id}` with no scripted `choose:` decision (options: {})",
                if options.is_empty() {
                    "none".to_string()
                } else {
                    options.join(", ")
                }
            )));
        }
        if let Some(rec) = outcome.transcript.iter().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("objective")
                && c.get("done").is_some_and(Json::is_null)
        }) {
            return Some(PlayHalt::Incomplete(format!(
                "{what}: required objective `{}.{}` has a `done` condition that evaluates unknown",
                rec.get("quest").and_then(Json::as_str).unwrap_or("?"),
                rec.get("objective").and_then(Json::as_str).unwrap_or("?"),
            )));
        }
        return Some(PlayHalt::Incomplete(format!("{what} is incomplete")));
    }
    if outcome
        .unresolved
        .iter()
        .any(|a| matches!(a, UnresolvedAtom::Time))
    {
        return Some(PlayHalt::Incomplete(format!(
            "{what} depends on now()/validAt(...), which the reference runner cannot resolve"
        )));
    }
    if let Some(plugin) = outcome.transcript.iter().find(|c| {
        c.get("kind").and_then(Json::as_str) == Some("plugin")
            && c.get("unresolvedEffects")
                .and_then(Json::as_array)
                .is_some_and(|a| !a.is_empty())
    }) {
        return Some(PlayHalt::Incomplete(format!(
            "{what}: plugin `{}` left a `bridgeResult` effect unresolved (no bridge is invoked)",
            plugin.get("tag").and_then(Json::as_str).unwrap_or("?")
        )));
    }
    None
}

/// The option ids of the `choice`/`hub` `id` in an artifact, declared order.
fn decision_options(doc_json: &Json, id: &str) -> Vec<String> {
    doc_json
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .find(|c| {
            let key = match c.get("kind").and_then(Json::as_str) {
                Some("choice") => "branchId",
                Some("hub") => "id",
                _ => return false,
            };
            c.get(key).and_then(Json::as_str) == Some(id)
        })
        .and_then(|c| c.get("options"))
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
        .collect()
}

/// `::end` reason text, from the `end` record a terminated walk carries.
fn end_reason(outcome: &RunnerOutcome, what: &str) -> String {
    let reason = outcome
        .transcript
        .iter()
        .rev()
        .find(|c| c.get("kind").and_then(Json::as_str) == Some("end"))
        .and_then(|c| c.get("reason").and_then(Json::as_str))
        .map(|r| format!(" (reason: {r})"))
        .unwrap_or_default();
    format!("::end in {what}{reason}")
}

/// One quest document's lifecycle transitions from one advance.
struct QuestAdvance {
    document: String,
    transcript: Vec<Json>,
}

/// Advance every quest lifecycle to a fixpoint (dsl 0.21.0 §6, D-H): each
/// quest document's [`Runner::advance_quests`], repeated in path order until
/// a whole pass transitions nothing — a quest in one document may gate on
/// another's state.
fn advance_quests(p: &Project, w: &mut World) -> (Vec<QuestAdvance>, Option<Stop>) {
    let mut out = Vec::new();
    let passes = p.quest_docs.len() * 8 + 8;
    for _ in 0..passes {
        let mut moved = false;
        for doc in &p.quest_docs {
            let doc_json = &p.artifacts[doc];
            let mut runner = Runner::with_carryover(
                &play_artifact_json(doc_json, p),
                MockSet::default(),
                w.state.clone(),
                w.facts.clone(),
                w.quests.clone(),
            );
            let result = runner.advance_quests();
            let outcome = runner.into_outcome();
            absorb(w, &outcome);
            let what = format!("quest document `{doc}`");
            let stop = match result {
                Err(msg) => Some(Stop::Halt(PlayHalt::Fatal(format!("{what}: {msg}")))),
                Ok(()) => outcome_halt(&outcome, &what, doc_json)
                    .map(Stop::Halt)
                    .or_else(|| {
                        outcome
                            .terminated
                            .then(|| Stop::End(end_reason(&outcome, &what)))
                    }),
            };
            let transcript: Vec<Json> = outcome
                .transcript
                .into_iter()
                .filter(|c| !c.get("done").is_some_and(Json::is_null))
                .collect();
            if !transcript.is_empty() {
                moved = true;
                out.push(QuestAdvance {
                    document: doc.clone(),
                    transcript,
                });
            }
            if stop.is_some() {
                return (out, stop);
            }
        }
        if !moved {
            break;
        }
    }
    (out, None)
}

/// A candidate's verdict (dsl 0.21.0 §4).
enum Verdict {
    Eligible,
    Ineligible(String),
    /// `when` evaluated unknown — the detail names why.
    Unknown(String),
}

struct Candidate {
    id: String,
    kind: BeatKind,
    document: String,
    priority: i64,
    verdict: Verdict,
}

fn kind_label(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Scene => "scene",
        BeatKind::Entry => "entry",
    }
}

/// A scene's declared `after:` (its `prereqEdges` row), parsed by the
/// checker's restricted profile parser the compile gate already proved it
/// well-formed under.
fn scene_prereq(doc_json: &Json) -> Option<PrereqFormula> {
    let raw = doc_json
        .get("prereqEdges")
        .and_then(Json::as_array)?
        .first()?
        .get("after")
        .and_then(Json::as_str)?;
    if raw.trim().is_empty() {
        return None;
    }
    let span = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    lute_check::parse_prereq(raw, span).0
}

fn eval_prereq(f: &PrereqFormula, w: &World) -> bool {
    match f {
        PrereqFormula::Visited(k) => w.visited.contains(k),
        PrereqFormula::Completed(q) => w.quests.get(q).map(String::as_str) == Some("complete"),
        PrereqFormula::Active(q) => w.quests.get(q).map(String::as_str) == Some("active"),
        PrereqFormula::And(a, b) => eval_prereq(a, w) && eval_prereq(b, w),
        PrereqFormula::Or(a, b) => eval_prereq(a, w) || eval_prereq(b, w),
    }
}

/// The beat's `when` raw CEL: a scene's `meta.beat.when`, an entry's own
/// `when` on its `entry` record.
fn beat_when(p: &Project, beat: &IndexBeat) -> Option<String> {
    let doc = p.artifacts.get(&beat.document)?;
    let pair = match beat.kind {
        BeatKind::Scene => doc.get("meta")?.get("beat")?.get("when")?,
        BeatKind::Entry => doc
            .get("commands")?
            .as_array()?
            .iter()
            .find(|c| {
                c.get("kind").and_then(Json::as_str) == Some("entry")
                    && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
            })?
            .get("when")?,
    };
    pair.get("raw")
        .and_then(Json::as_str)
        .filter(|r| !r.trim().is_empty())
        .map(str::to_string)
}

/// Every candidate for `occasion`/`target` with its verdict, in selection
/// order: priority descending, then `ProjectIndex.beats` order.
fn candidates(p: &Project, w: &World, occasion: &str, target: Option<&str>) -> Vec<Candidate> {
    let mut eval = Runner::with_carryover(
        &p.eval_json,
        MockSet::default(),
        w.state.clone(),
        w.facts.clone(),
        w.quests.clone(),
    );
    let mut out: Vec<(usize, Candidate)> = Vec::new();
    for (idx, beat) in p.index.beats.iter().enumerate() {
        if !is_candidate(beat, occasion, target) {
            continue;
        }
        let spent = match beat.once {
            Some(BeatOnce::Run) if w.spent_run.contains(&beat.id) => {
                Some("once: run — already presented this run")
            }
            Some(BeatOnce::User) if w.spent_user.contains(&beat.id) => {
                Some("once: user — already presented")
            }
            _ => None,
        };
        let after_unmet = beat.kind == BeatKind::Scene
            && p.artifacts
                .get(&beat.document)
                .and_then(scene_prereq)
                .is_some_and(|f| !eval_prereq(&f, w));
        let verdict = if let Some(reason) = spent {
            Verdict::Ineligible(reason.to_string())
        } else if after_unmet {
            Verdict::Ineligible("after: prerequisite not satisfied".to_string())
        } else {
            match beat_when(p, beat) {
                None => Verdict::Eligible,
                Some(raw) => match eval.eval_guard(&raw) {
                    Ok(true) => Verdict::Eligible,
                    Ok(false) => Verdict::Ineligible("when: false".to_string()),
                    Err(atoms) => Verdict::Unknown(format!(
                        "`{raw}` evaluates unknown: {}",
                        describe_atoms(&atoms)
                    )),
                },
            }
        };
        out.push((
            idx,
            Candidate {
                id: beat.id.clone(),
                kind: beat.kind,
                document: beat.document.clone(),
                priority: beat.priority,
                verdict,
            },
        ));
    }
    out.sort_by_key(|(idx, c)| (std::cmp::Reverse(c.priority), *idx));
    out.into_iter().map(|(_, c)| c).collect()
}

/// The unknown `when` that could change this step's outcome, if any: on a
/// `select: first` occasion one ordered BEFORE the first definitely-eligible
/// beat (a later one can never win); on `select: all` any (the offered list
/// itself depends on it).
fn deciding_unknown(cands: &[Candidate], select: OccasionSelect) -> Option<&Candidate> {
    for c in cands {
        match c.verdict {
            Verdict::Eligible if select == OccasionSelect::First => return None,
            Verdict::Unknown(_) => return Some(c),
            _ => {}
        }
    }
    None
}

/// One presented beat.
struct Presented {
    id: String,
    kind: BeatKind,
    document: String,
    transcript: Vec<Json>,
    state_before: BTreeMap<String, Value>,
    state_after: BTreeMap<String, Value>,
}

/// Present `beat`: a scene through the runner (`scene.*` fresh), an entry
/// through the runner's entry path (first-read effects, `entry.<id>.read`).
fn present(p: &Project, w: &mut World, beat: &IndexBeat, mock: &MockSet) -> (Presented, Option<Stop>) {
    let doc_json = &p.artifacts[&beat.document];
    let state_before = w.state.clone();
    let runner = Runner::with_carryover(
        &play_artifact_json(doc_json, p),
        mock.clone(),
        scene_initial_state(doc_json, &w.state),
        w.facts.clone(),
        w.quests.clone(),
    );
    let mut runner = match beat.kind {
        BeatKind::Entry => runner.with_entry(&beat.id),
        BeatKind::Scene => runner,
    };
    let result = runner.run();
    let outcome = runner.into_outcome();
    absorb(w, &outcome);
    if beat.kind == BeatKind::Scene {
        w.visited.insert(beat.id.clone());
        w.spent_run.insert(beat.id.clone());
        w.spent_user.insert(beat.id.clone());
    }
    let what = format!("{} `{}` ({})", kind_label(beat.kind), beat.id, beat.document);
    let stop = match result {
        Err(msg) => Some(Stop::Halt(PlayHalt::Fatal(format!("{what}: {msg}")))),
        Ok(()) => outcome_halt(&outcome, &what, doc_json)
            .map(Stop::Halt)
            .or_else(|| {
                outcome
                    .terminated
                    .then(|| Stop::End(end_reason(&outcome, &what)))
            }),
    };
    let presented = Presented {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        transcript: outcome.transcript,
        state_before,
        state_after: w.state.clone(),
    };
    (presented, stop)
}

/// One script step's record.
enum StepBody {
    Occasion {
        occasion: String,
        target: Option<String>,
        select: OccasionSelect,
        pick: Option<String>,
        candidates: Vec<Candidate>,
        /// `None` with `decided: true` — the occasion passed with no story.
        winner: Option<String>,
        decided: bool,
        presented: Option<Presented>,
    },
    NewRun,
}

struct StepRecord {
    n: usize,
    body: StepBody,
    quests: Vec<QuestAdvance>,
}

/// The whole playthrough: the initial quest settle, then every step.
struct Playthrough {
    start: Vec<QuestAdvance>,
    steps: Vec<StepRecord>,
    outcome: Result<String, PlayHalt>,
}

fn execute(p: &Project, script: &PlayScript, mut w: World) -> Playthrough {
    let mock = MockSet {
        choose: script.surfaces.choose.clone(),
        ..MockSet::default()
    };
    let (start, stop) = advance_quests(p, &mut w);
    let mut steps = Vec::new();
    let finish = |start, steps, stop: Stop| Playthrough {
        start,
        steps,
        outcome: match stop {
            Stop::End(reason) => Ok(reason),
            Stop::Halt(h) => Err(h),
        },
    };
    if let Some(stop) = stop {
        return finish(start, steps, stop);
    }
    for (i, step) in script.steps.iter().enumerate() {
        let n = i + 1;
        let ScriptStep::Occasion {
            occasion,
            target,
            pick,
        } = step
        else {
            new_run(p, &mut w);
            steps.push(StepRecord {
                n,
                body: StepBody::NewRun,
                quests: Vec::new(),
            });
            continue;
        };
        let select = p.select_of(occasion);
        let cands = candidates(p, &w, occasion, target.as_deref());
        let halt = if let Some(c) = deciding_unknown(&cands, select) {
            let Verdict::Unknown(detail) = &c.verdict else {
                unreachable!("deciding_unknown returns only unknown verdicts")
            };
            Some(PlayHalt::Incomplete(format!(
                "step {n}: the `when` of {} `{}` ({}) decides the {occasion} outcome but {detail}",
                kind_label(c.kind),
                c.id,
                c.document
            )))
        } else if let Some(pk) = pick {
            match cands.iter().find(|c| &c.id == pk).map(|c| &c.verdict) {
                Some(Verdict::Eligible) => None,
                Some(Verdict::Ineligible(reason)) => Some(PlayHalt::Error(format!(
                    "step {n}: `pick: {pk}` is not eligible — {reason}"
                ))),
                _ => Some(PlayHalt::Error(format!(
                    "step {n}: `pick: {pk}` is not a candidate of {occasion}"
                ))),
            }
        } else {
            None
        };
        let decided = halt.is_none();
        let winner = match (decided, pick) {
            (false, _) => None,
            (true, Some(pk)) => Some(pk.clone()),
            (true, None) => cands
                .iter()
                .find(|c| matches!(c.verdict, Verdict::Eligible))
                .map(|c| c.id.clone()),
        };
        let beat = winner.as_ref().and_then(|id| {
            p.index
                .beats
                .iter()
                .find(|b| &b.id == id && is_candidate(b, occasion, target.as_deref()))
        });
        // A presentation that ended the walk (a halt, a `::end`) advances
        // nothing further; one that played through advances every quest.
        let (presented, quests, stop) = match (halt, beat) {
            (Some(h), _) => (None, Vec::new(), Some(Stop::Halt(h))),
            (None, None) => (None, Vec::new(), None),
            (None, Some(b)) => match present(p, &mut w, b, &mock) {
                (presented, Some(stop)) => (Some(presented), Vec::new(), Some(stop)),
                (presented, None) => {
                    let (quests, stop) = advance_quests(p, &mut w);
                    (Some(presented), quests, stop)
                }
            },
        };
        steps.push(StepRecord {
            n,
            body: StepBody::Occasion {
                occasion: occasion.clone(),
                target: target.clone(),
                select,
                pick: pick.clone(),
                candidates: cands,
                winner,
                decided,
                presented,
            },
            quests,
        });
        if let Some(stop) = stop {
            return finish(start, steps, stop);
        }
    }
    let n = steps.len();
    Playthrough {
        start,
        steps,
        outcome: Ok(format!("complete ({n} step{})", if n == 1 { "" } else { "s" })),
    }
}

// ===========================================================================
// Rendering. Human by default, `--json` one object; both reuse the runner's
// own transcript records rather than re-deriving per-kind semantics.
// ===========================================================================

fn render_attrs(cmd: &Json, skip: &[&str]) -> String {
    let Json::Object(map) = cmd else {
        return String::new();
    };
    let mut parts = Vec::new();
    for (k, v) in map {
        if skip.contains(&k.as_str()) {
            continue;
        }
        match v {
            Json::String(s) => parts.push(format!("{k}=\"{s}\"")),
            Json::Bool(b) => parts.push(format!("{k}={b}")),
            Json::Number(n) => parts.push(format!("{k}={n}")),
            _ => {}
        }
    }
    parts.join(" ")
}

fn render_options(opts: &[Json], chosen: Option<&str>) -> String {
    opts.iter()
        .filter_map(|o| o.get("id").and_then(Json::as_str))
        .map(|id| {
            if Some(id) == chosen {
                format!("[{id}]")
            } else {
                id.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn str_of<'a>(rec: &'a Json, key: &str) -> &'a str {
    rec.get(key).and_then(Json::as_str).unwrap_or("")
}

/// One runner transcript record -> a human line, enriched with the original
/// command's authored attrs (looked up by `addr`) for lines and staging.
fn render_record(rec: &Json, cmd_by_addr: &BTreeMap<&str, &Json>) -> String {
    let kind = str_of(rec, "kind");
    let orig = cmd_by_addr.get(str_of(rec, "addr")).copied();
    match kind {
        "line" => {
            let attrs = orig
                .map(|c| {
                    render_attrs(
                        c,
                        &[
                            "addr",
                            "kind",
                            "text",
                            "speaker",
                            "lineId",
                            "voiceKey",
                            "role",
                            "asLabel",
                            "as",
                            "placeholders",
                            "texts",
                        ],
                    )
                })
                .unwrap_or_default();
            let (speaker, text) = (str_of(rec, "speaker"), str_of(rec, "text"));
            if attrs.is_empty() {
                format!("@{speaker}: {text}")
            } else {
                format!("@{speaker}{{{attrs}}}: {text}")
            }
        }
        "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
            let attrs = orig
                .map(|c| render_attrs(c, &["addr", "kind"]))
                .unwrap_or_default();
            if attrs.is_empty() {
                format!("::{kind}")
            } else {
                format!("::{kind}{{{attrs}}}")
            }
        }
        "set" => format!(
            "  set {} = {}",
            str_of(rec, "path"),
            rec.get("value").map(Json::to_string).unwrap_or_default()
        ),
        "assert" => format!("  assert {}", str_of(rec, "fact")),
        "retract" => format!("  retract {}", str_of(rec, "pattern")),
        "choice" | "hub" => {
            let id = str_of(rec, if kind == "choice" { "branch" } else { "hub" });
            let chosen = rec.get("chose").and_then(Json::as_str);
            let opts: Vec<Json> = orig
                .and_then(|c| c.get("options"))
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let mut label = format!("{kind} {id}");
            if let Some(prompt) = orig.and_then(|c| c.get("prompt")).and_then(Json::as_str) {
                label.push_str(&format!(" \"{prompt}\""));
            }
            if let Some(t) = orig
                .and_then(|c| c.get("timeoutSec"))
                .and_then(Json::as_u64)
            {
                label.push_str(&format!(" ({t}s)"));
            }
            let rendered = render_options(&opts, chosen);
            match chosen {
                Some(c) => format!("▷ {label}: {rendered}        ← chosen: {c}"),
                None => format!("▷ {label}: {rendered}        ← INCOMPLETE (no decision)"),
            }
        }
        "match" => format!("  match -> {}", str_of(rec, "result")),
        "barrier" => "  barrier (no real clock simulated)".to_string(),
        "end" => match rec.get("reason").and_then(Json::as_str) {
            Some(r) => format!("  ::end reason={r}"),
            None => "  ::end".to_string(),
        },
        "plugin" => format!(
            "  plugin {} (external call, not invoked)",
            str_of(rec, "tag")
        ),
        "entry" => {
            let read = if rec.get("firstRead").and_then(Json::as_bool) == Some(true) {
                "first read"
            } else {
                "re-read: effects skipped"
            };
            format!("  entry {} ({read})", str_of(rec, "id"))
        }
        "skipped" => {
            let what = ["path", "fact", "pattern"]
                .iter()
                .find_map(|k| rec.get(*k).and_then(Json::as_str))
                .unwrap_or("");
            format!("  {} {what} (skipped: re-read)", str_of(rec, "effect"))
        }
        "objective" => format!(
            "  {}.{} done",
            str_of(rec, "quest"),
            str_of(rec, "objective")
        ),
        "quest" => format!(
            "  quest {} -> {}",
            str_of(rec, "quest"),
            str_of(rec, "state")
        ),
        "grant" => {
            let owner = match rec.get("objective").and_then(Json::as_str) {
                Some(oid) => format!("{}.{oid}", str_of(rec, "quest")),
                None => str_of(rec, "quest").to_string(),
            };
            let reward = rec.get("reward").cloned().unwrap_or(Json::Null);
            let amount = match reward.get("amount").and_then(Json::as_i64) {
                Some(n) => n.to_string(),
                None => match (
                    reward.get("amountMin").and_then(Json::as_i64),
                    reward.get("amountMax").and_then(Json::as_i64),
                ) {
                    (Some(lo), Some(hi)) => format!("{lo}..{hi}"),
                    _ => "?".to_string(),
                },
            };
            let target = reward
                .get("target")
                .and_then(Json::as_str)
                .map(|t| format!(" -> {t}"))
                .unwrap_or_default();
            let on_failed = if rec.get("onFailed").and_then(Json::as_bool) == Some(true) {
                " (on failed)"
            } else {
                ""
            };
            format!(
                "  grant {owner} {} {amount}{target}{on_failed}",
                str_of(&reward, "kind")
            )
        }
        _ => format!("  {kind}"),
    }
}

fn cmd_index(doc_json: Option<&Json>) -> BTreeMap<&str, &Json> {
    doc_json
        .and_then(|d| d.get("commands"))
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("addr").and_then(Json::as_str).map(|a| (a, c)))
        .collect()
}

fn render_records(out: &mut String, p: &Project, document: &str, records: &[Json]) {
    let by_addr = cmd_index(p.artifacts.get(document));
    for rec in records {
        out.push_str(&render_record(rec, &by_addr));
        out.push('\n');
    }
}

const RULE: &str = "──────────────";

fn render_candidate(c: &Candidate) -> String {
    let head = format!("{} [{}, priority {}]", c.id, kind_label(c.kind), c.priority);
    match &c.verdict {
        Verdict::Eligible => format!("  ✓ {head}\n"),
        Verdict::Ineligible(reason) => format!("  ✗ {head} — {reason}\n"),
        Verdict::Unknown(detail) => format!("  ? {head} — when: unknown ({detail})\n"),
    }
}

fn render_human(p: &Project, play: &Playthrough) -> String {
    let mut out = String::new();
    if !play.start.is_empty() {
        out.push_str(&format!("── start {RULE}\n"));
        for q in &play.start {
            render_records(&mut out, p, &q.document, &q.transcript);
        }
    }
    for s in &play.steps {
        match &s.body {
            StepBody::NewRun => {
                out.push_str(&format!("── step {} · new run {RULE}\n", s.n));
                out.push_str("  run.* state, run-tier facts and once: run reset\n");
            }
            StepBody::Occasion {
                occasion,
                target,
                select,
                pick,
                candidates,
                winner,
                decided,
                presented,
            } => {
                let mut header = format!("── step {} · {occasion}", s.n);
                if let Some(t) = target {
                    header.push_str(&format!(" → {t}"));
                }
                if *select == OccasionSelect::All {
                    header.push_str(&format!(
                        " (select: all, pick: {})",
                        pick.as_deref().unwrap_or("?")
                    ));
                }
                out.push_str(&format!("{header} {RULE}\n"));
                if candidates.is_empty() {
                    out.push_str("  (no candidates)\n");
                }
                for c in candidates
                    .iter()
                    .filter(|c| matches!(c.verdict, Verdict::Eligible))
                {
                    out.push_str(&render_candidate(c));
                }
                for c in candidates
                    .iter()
                    .filter(|c| !matches!(c.verdict, Verdict::Eligible))
                {
                    out.push_str(&render_candidate(c));
                }
                if *decided {
                    match winner {
                        Some(id) => out.push_str(&format!("  → {id}\n")),
                        None => out.push_str("  → (no eligible beat — the occasion passes)\n"),
                    }
                }
                if let Some(pr) = presented {
                    render_records(&mut out, p, &pr.document, &pr.transcript);
                }
            }
        }
        for q in &s.quests {
            render_records(&mut out, p, &q.document, &q.transcript);
        }
    }
    match &play.outcome {
        Ok(reason) => out.push_str(&format!("── end: {reason} {RULE}\n")),
        Err(h) => out.push_str(&format!("── halted: {} {RULE}\n", h.message())),
    }
    out
}

fn state_delta(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Json {
    let mut delta = serde_json::Map::new();
    for (k, v) in after {
        if before.get(k) != Some(v) {
            delta.insert(k.clone(), value_to_json(v));
        }
    }
    Json::Object(delta)
}

fn quests_json(quests: &[QuestAdvance]) -> Json {
    Json::Array(
        quests
            .iter()
            .map(|q| json!({ "document": q.document, "commands": q.transcript }))
            .collect(),
    )
}

fn render_json(play: &Playthrough) -> Json {
    let steps: Vec<Json> = play
        .steps
        .iter()
        .map(|s| match &s.body {
            StepBody::NewRun => json!({ "step": s.n, "newRun": true }),
            StepBody::Occasion {
                occasion,
                target,
                select,
                pick,
                candidates,
                winner,
                decided: _,
                presented,
            } => {
                let mut o = serde_json::Map::new();
                o.insert("step".into(), json!(s.n));
                o.insert("occasion".into(), json!(occasion));
                if let Some(t) = target {
                    o.insert("target".into(), json!(t));
                }
                o.insert(
                    "select".into(),
                    json!(match select {
                        OccasionSelect::First => "first",
                        OccasionSelect::All => "all",
                    }),
                );
                if let Some(pk) = pick {
                    o.insert("pick".into(), json!(pk));
                }
                let cands: Vec<Json> = candidates
                    .iter()
                    .map(|c| {
                        let mut m = serde_json::Map::new();
                        m.insert("id".into(), json!(c.id));
                        m.insert("kind".into(), json!(kind_label(c.kind)));
                        m.insert("document".into(), json!(c.document));
                        m.insert("priority".into(), json!(c.priority));
                        let (eligible, reason) = match &c.verdict {
                            Verdict::Eligible => (json!(true), None),
                            Verdict::Ineligible(r) => (json!(false), Some(r.clone())),
                            Verdict::Unknown(d) => (Json::Null, Some(format!("when: unknown ({d})"))),
                        };
                        m.insert("eligible".into(), eligible);
                        if let Some(r) = reason {
                            m.insert("reason".into(), json!(r));
                        }
                        Json::Object(m)
                    })
                    .collect();
                o.insert("candidates".into(), Json::Array(cands));
                o.insert("winner".into(), json!(winner));
                if let Some(pr) = presented {
                    o.insert(
                        "presented".into(),
                        json!({
                            "id": pr.id,
                            "kind": kind_label(pr.kind),
                            "document": pr.document,
                            "commands": pr.transcript,
                            "stateDelta": state_delta(&pr.state_before, &pr.state_after),
                        }),
                    );
                }
                o.insert("quests".into(), quests_json(&s.quests));
                Json::Object(o)
            }
        })
        .collect();
    let mut root = serde_json::Map::new();
    match &play.outcome {
        Ok(reason) => {
            root.insert("exit".into(), json!("complete"));
            root.insert("endReason".into(), json!(reason));
        }
        Err(h) => {
            root.insert("exit".into(), json!(h.exit_label()));
            root.insert("error".into(), json!({ "message": h.message() }));
        }
    }
    root.insert("start".into(), json!({ "quests": quests_json(&play.start) }));
    root.insert("steps".into(), Json::Array(steps));
    Json::Object(root)
}

// ===========================================================================
// CLI entry point.
// ===========================================================================

/// See [`crate::Command::Play`].
pub fn run_play(dir: &Path, script_path: &Path, json: bool) -> ExitCode {
    let script = match std::fs::read_to_string(script_path) {
        Ok(text) => match parse_script(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "lute play: invalid play script {}: {e}",
                    script_path.display()
                );
                return ExitCode::from(2);
            }
        },
        Err(e) => {
            eprintln!(
                "lute play: cannot read play script {}: {e}",
                script_path.display()
            );
            return ExitCode::from(2);
        }
    };
    if !dir.is_dir() {
        eprintln!("lute play: {} is not a project directory", dir.display());
        return ExitCode::from(2);
    }
    let project = match compile_project(dir) {
        Ok(p) => p,
        Err(code) => return code,
    };
    if let Err(e) = validate_steps(&project, &script.steps) {
        eprintln!("lute play: {}: {e}", script_path.display());
        return ExitCode::from(2);
    }
    let world = match seed_world(&project, &script.surfaces) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("lute play: {}: {e}", script_path.display());
            return ExitCode::from(2);
        }
    };

    let play = execute(&project, &script, world);
    if json {
        let v = render_json(&play);
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!("{}", render_human(&project, &play));
    }
    match &play.outcome {
        Ok(_) => ExitCode::SUCCESS,
        Err(h) => h.exit_code(),
    }
}
