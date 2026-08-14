//! `lute play <PROJECT_DIR>` — the schedule-driven chained playthrough
//! (design spec
//! `docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md` v2,
//! §4): the reviewer-facing transcript of exactly what a player following
//! ONE route sees, in the order they see it. `lute run` is the reference
//! evaluator for ONE compiled artifact; this lifts that SAME evaluator
//! ([`crate::runner::Runner`]) from one artifact to a schedule-ordered
//! sequence of them, threading state/facts/quest status across scene
//! boundaries — never a second dispatcher.
//!
//! ## Requires a schedule (spec §2)
//! There is no `after:`-graph fallback: sibling route files are unguarded by
//! design (file split IS the route), so a topological walk cannot select
//! ONE route — it would play every sibling. A project with no
//! `schedule.yaml` is a hard usage error (exit 2) naming this spec.
//!
//! ## Scoping decision: quest-kind docs are compiled, never executed
//! Spec §4.2 unions quest declarations project-wide alongside
//! relations/rules/seedFacts/state, "including quest docs, which are never
//! placed". This module honors the DATA half of that (quest-declared state
//! paths like `quest.<id>.state` are in the project-wide state-table union,
//! §4.2, so a guard referencing one type-checks) but deliberately does NOT
//! drive a quest artifact's own `<quest>`/`<on>` lifecycle as part of the
//! chain: [`crate::runner::Runner::run_quest`] is built for ONE fresh
//! fixpoint settle per invocation (its objective-completion tracking is a
//! LOCAL set, re-initialized every call) — invoking it repeatedly across
//! scene boundaries as the chain progresses would silently RE-FIRE an
//! already-complete objective's body every time, which is worse than not
//! running it at all. Making that safe needs a Runner "resume" capability
//! that is real, undesigned scope (a candidate for a future delivery-plan
//! item). Consequence, honestly surfaced per §4.5's "no silent unknowns":
//! `completed(...)`/`active(...)` in an `after:` causality check always
//! evaluate against an EMPTY set for the whole playthrough — a placement
//! causally gated on quest completion halts loudly
//! ([`PlayHalt::AfterOrder`], exit 1), never silently fakes completion.
//! Confirmed harmless for the driving consumer (OSHiZ onboarding): no
//! `after:` anywhere in that project gates on `completed`/`active`, and it
//! ships no quest docs yet.
//!
//! ## §4.5 honesty gate — unresolved reference-runtime surfaces
//! A guard/effect this reference runner cannot resolve halts the walk
//! incomplete (exit 3) rather than silently deciding it: `now()`/
//! `validAt(...)` ([`lute_trace::UnresolvedAtom::Time`], surfaced via
//! [`crate::runner::RunnerOutcome::unresolved`]), an unresolved plugin
//! `bridgeResult` effect (already visible on the reused transcript's
//! `plugin` record), and — per the scoping decision above — a scene's
//! `after:` gated on quest completion. Wall-clock `<timeline>` pacing is
//! NEVER simulated (mirrors `lute run`'s own honesty: a `barrier` record is
//! a transcript note only) and never gates a halt on its own, since it has
//! no observable effect on state/control-flow to be honest ABOUT.
//!
//! ## What is reused, never re-implemented
//! - CEL evaluation: `lute_cel::parse_slot` + `lute_trace::eval` — the ONE
//!   evaluator (mirrors [`crate::runner::Runner::eval_raw`], never
//!   duplicated logic, just the same two calls run again over a live
//!   snapshot outside a Runner instance for schedule variant/causality
//!   checks that happen BETWEEN scenes).
//! - Whole-project compile + gate: [`crate::reconciled_project_results`] +
//!   [`crate::gate_for_doc`] + `lute_compile::compile_with_check`, the exact
//!   pattern `compile --all` ([`crate::compile_all`]) runs.
//! - Declaration union: `lute_compile::index::build_index` — the SAME union
//!   `compile --all` writes to `project.index.json`, spliced into each
//!   scene's own artifact JSON before it is handed to a `Runner` (state-table
//!   union is this module's own addition — `build_index` does not cover it).
//! - Mock-shaped scene decisions: [`crate::runner::Runner::with_carryover`]
//!   still takes a `lute_trace::MockSet` per scene (its `choose:`/`state:`/
//!   `facts:` surfaces) — but the ROUTE SCRIPT itself (`*.play.yaml`) is
//!   parsed by THIS module (§4.4), not `lute_trace`'s mock parser: v1 of
//!   this design claimed reuse there and v2's review called that an error —
//!   a route script's `choose:` keys are event-qualified
//!   (`<event>/<hubOrBranchId>`), a shape the mock family's closed grammar
//!   has no notion of.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_cel::CelArena;
use lute_check::{PrereqFormula, RelVocab, StateSchema};
use lute_compile::index::{build_index, IndexInput};
use lute_compile::Artifact;
use lute_core_span::Severity;
use lute_trace::{eval, EffectiveState, EvalEnv, FactStore, UnresolvedAtom, Value};
use serde_json::{json, Value as Json};

use crate::runner::{Fact, Runner};
use crate::schedule::{self, Placement, Schedule, SchedDiag};

// ===========================================================================
// CLI value parsers (`crate::Command::Play`'s `value_parser`s live here,
// mirroring how `crate::parse_deny_code` sits next to `Command::Check`'s use
// of it — flag-shape validation stays with the command that owns the flag).
// ===========================================================================

/// `--auto <POLICY>` (design spec §4.4): only `first` is implemented today.
/// A typo/future-policy name is a clap usage error (exit 2), never a silent
/// no-op — the same discipline `crate::parse_deny_code` applies to `--deny`.
pub fn parse_auto_policy(raw: &str) -> Result<String, String> {
    match raw {
        "first" => Ok(raw.to_string()),
        other => Err(format!("unknown --auto policy `{other}` (only `first` is implemented)")),
    }
}

/// `--lanes user|all` (design spec §4, default `user` — the strict player
/// view; world-lane scenes still EXECUTE either way, §4.3, only the
/// transcript omits them under `user`).
pub fn parse_lanes_flag(raw: &str) -> Result<String, String> {
    match raw {
        "user" | "all" => Ok(raw.to_string()),
        other => Err(format!("unknown --lanes value `{other}` (expected `user` or `all`)")),
    }
}

// ===========================================================================
// Route script (`*.play.yaml`, design spec §4.4) — this module's OWN closed
// grammar, never `lute_trace`'s `--mock` family (see module doc).
// ===========================================================================

/// The complete legal top-level key set of a route script.
const PLAY_SCRIPT_KEYS: &[&str] = &["state", "facts", "choose"];

/// A parsed `*.play.yaml`. `choose` keys are RAW as authored — event-
/// qualified (`<event>/<id>`) or bare (`<id>`, legal only when unique
/// project-wide) — resolved against the schedule's own decision points by
/// [`resolve_choose_keys`], never here (this parser has no schedule to
/// check uniqueness against).
#[derive(Default, Clone, Debug)]
pub(crate) struct RouteScript {
    pub state: Vec<(String, String)>,
    pub facts: Vec<String>,
    pub choose: BTreeMap<String, Vec<String>>,
}

fn scalar_to_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Parse a `*.play.yaml` route script (design spec §4.4): `state:` (map of
/// path -> literal), `facts:` (list of ground-fact-pattern strings),
/// `choose:` (map of `<event>/<id>` or bare `<id>` -> a choice id or a list
/// of them). Every key optional; an absent/`null` document yields an empty
/// [`RouteScript`]. Total: never panics, `Err` names exactly what is wrong.
fn parse_route_script(text: &str) -> Result<RouteScript, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| format!("malformed route script YAML: {e}"))?;
    if matches!(value, serde_yaml::Value::Null) {
        return Ok(RouteScript::default());
    }
    let serde_yaml::Value::Mapping(top) = value else {
        return Err(
            "a route script must be a YAML mapping with `state:`/`facts:`/`choose:` keys (design spec §4.4)"
                .to_string(),
        );
    };
    for (k, _) in &top {
        let Some(key) = k.as_str() else {
            return Err("a route script's top-level keys must be strings".to_string());
        };
        if !PLAY_SCRIPT_KEYS.contains(&key) {
            return Err(format!(
                "unknown top-level key `{key}` in a route script (legal: {})",
                PLAY_SCRIPT_KEYS.join(", ")
            ));
        }
    }

    let mut script = RouteScript::default();

    if let Some(v) = top.get("state") {
        let serde_yaml::Value::Mapping(m) = v else {
            return Err("`state:` must be a mapping of path -> literal".to_string());
        };
        for (k, v) in m {
            let Some(path) = k.as_str() else {
                return Err("`state:` keys must be strings".to_string());
            };
            let Some(literal) = scalar_to_text(v) else {
                return Err(format!("`state.{path}` must be a scalar literal (bool/number/string)"));
            };
            script.state.push((path.to_string(), literal));
        }
    }

    if let Some(v) = top.get("facts") {
        let serde_yaml::Value::Sequence(items) = v else {
            return Err("`facts:` must be a list of quoted fact patterns".to_string());
        };
        for item in items {
            let Some(s) = item.as_str() else {
                return Err("every `facts:` entry must be a string".to_string());
            };
            script.facts.push(s.to_string());
        }
    }

    if let Some(v) = top.get("choose") {
        let serde_yaml::Value::Mapping(m) = v else {
            return Err(
                "`choose:` must be a mapping of `<event>/<id>` (or bare `<id>`) -> choice id(s)".to_string(),
            );
        };
        for (k, v) in m {
            let Some(key) = k.as_str() else {
                return Err("`choose:` keys must be strings".to_string());
            };
            let ids = match v {
                serde_yaml::Value::String(s) => vec![s.clone()],
                serde_yaml::Value::Sequence(items) => {
                    let mut out = Vec::new();
                    for item in items {
                        let Some(s) = item.as_str() else {
                            return Err(format!("`choose.{key}` list entries must be strings"));
                        };
                        out.push(s.to_string());
                    }
                    out
                }
                _ => return Err(format!("`choose.{key}` must be a choice id or a list of choice ids")),
            };
            script.choose.insert(key.to_string(), ids);
        }
    }

    Ok(script)
}

// ===========================================================================
// Whole-project compile + declaration union (design spec §4.2). Mirrors
// `compile_all.rs`'s own per-document loop exactly (module doc), minus the
// disk-write tail — every artifact is kept in memory.
// ===========================================================================

/// `path` relative to `root`, forward-slash joined — the SAME convention
/// `compile_all.rs`'s private `rel_slash` uses (project-relative artifact
/// identity, portable across machines). Duplicated rather than imported:
/// `rel_slash` is private to `compile_all`, and this join is not worth a
/// cross-module visibility change for.
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

/// Compile every non-component document under `project_dir` in memory
/// (scene AND quest kind — design spec §4.2's "declaration union first",
/// including quest docs, which are never placed). `Err` on a gate failure
/// (diagnostics already printed) or an I/O failure — exactly `compile_all`'s
/// own all-or-nothing contract, refusing to play a project that does not
/// wholly compile.
fn compile_project(project_dir: &Path) -> Result<BTreeMap<String, Artifact>, ExitCode> {
    match crate::manifests::validate_manifests_under(project_dir) {
        Ok(mut verdicts) => {
            crate::manifests::mark_inert_under(&mut verdicts, project_dir);
            if crate::manifests::report_and_gate(&verdicts) {
                return Err(ExitCode::from(1));
            }
        }
        Err(e) => {
            eprintln!("lute play: cannot walk {} for manifests: {e}", project_dir.display());
            return Err(ExitCode::from(2));
        }
    }

    let reconciled = match crate::reconciled_project_results(project_dir, None) {
        Ok(r) => r,
        Err(code) => return Err(code),
    };
    let identity = lute_manifest::project::load_project(project_dir)
        .ok()
        .flatten()
        .map(|p| p.identity)
        .unwrap_or_default();

    let mut artifacts: BTreeMap<String, Artifact> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let policy = crate::DenyPolicy::default();

    for (file, base) in &reconciled.per_doc {
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, project_dir) else {
            eprintln!("lute play: {} is not under {}", file.display(), project_dir.display());
            return Err(ExitCode::from(2));
        };
        let Some(built) = crate::build_input(file, None, Some(project_dir)) else {
            return Err(ExitCode::from(2));
        };
        built.report_project_diags();
        let crate::BuiltInput { input, resolve_error, .. } = built;
        if resolve_error {
            return Err(ExitCode::from(1));
        }
        let gate = crate::gate_for_doc(&reconciled, file, base);
        match lute_compile::compile_with_check(&input, gate, &identity) {
            Ok(artifact) => {
                artifacts.insert(rel, artifact);
            }
            Err(diags) => {
                failures.insert(file.clone(), crate::render_diagnostics(file, &diags, &policy));
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
            failures.len() + artifacts.len()
        );
        return Err(ExitCode::from(1));
    }
    Ok(artifacts)
}

/// The project-wide declaration union (design spec §4.2): relations/enums/
/// rules/seedFacts/prereqEdges via `lute_compile::index::build_index` (the
/// SAME union `compile --all` writes), plus a state-TABLE union
/// `build_index` does not cover — first-declared-path-wins across a stable,
/// path-sorted document walk (`artifacts` is already a `BTreeMap`).
struct ProjectUnion {
    /// path -> its `StateEntry`-shaped JSON (`type`/`domain`/`default`).
    state: BTreeMap<String, Json>,
    rules: Json,
    seed_facts: Json,
    /// enum-typed state path -> its domain members — [`schedule::route_space_check`]'s
    /// `enums` parameter.
    enums: BTreeMap<String, Vec<String>>,
}

fn build_project_union(artifacts: &BTreeMap<String, Artifact>) -> Result<ProjectUnion, Vec<String>> {
    let inputs: Vec<IndexInput> = artifacts
        .iter()
        .map(|(rel, art)| IndexInput { path: rel.clone(), artifact_path: format!("{rel}.json"), artifact: art })
        .collect();
    let index = build_index(lute_compile::LUTE_IR_VERSION, &inputs).map_err(|errs| {
        errs.iter().map(|e| e.to_string()).collect::<Vec<_>>()
    })?;
    let rules = serde_json::to_value(&index.rules).unwrap_or_else(|_| json!([]));
    let seed_facts = serde_json::to_value(&index.seed_facts).unwrap_or_else(|_| json!([]));

    let mut state: BTreeMap<String, Json> = BTreeMap::new();
    for art in artifacts.values() {
        let Ok(art_json) = serde_json::to_value(art) else { continue };
        if let Some(entries) = art_json.get("state").and_then(Json::as_array) {
            for e in entries {
                let Some(path) = e.get("path").and_then(Json::as_str) else { continue };
                state.entry(path.to_string()).or_insert_with(|| e.clone());
            }
        }
    }
    let mut enums = BTreeMap::new();
    for (path, e) in &state {
        if e.get("type").and_then(Json::as_str) == Some("enum") {
            if let Some(domain) = e.get("domain").and_then(Json::as_array) {
                let members: Vec<String> = domain.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
                if !members.is_empty() {
                    enums.insert(path.clone(), members);
                }
            }
        }
    }
    Ok(ProjectUnion { state, rules, seed_facts, enums })
}

/// The artifact JSON handed to a scene's [`Runner`]: this scene's OWN
/// `commands`/`meta`/`kind`/`prereqEdges`, but `rules`/`seedFacts`/`state`
/// REPLACED by the project-wide union — a relation asserted in one scene
/// must be visible to a guard in another (design spec §4.2), and a
/// `run.*`/`user.*`/`app.*`/`quest.*` path declared in a DIFFERENT doc still
/// needs its correct declared type/default here.
fn play_artifact_json(scene_json: &Json, union: &ProjectUnion) -> Json {
    let mut v = scene_json.clone();
    if let Json::Object(map) = &mut v {
        map.insert("rules".to_string(), union.rules.clone());
        map.insert("seedFacts".to_string(), union.seed_facts.clone());
        map.insert("state".to_string(), Json::Array(union.state.values().cloned().collect()));
    }
    v
}

// ===========================================================================
// CEL evaluation over a live (state, facts) snapshot — the SAME evaluator
// `Runner::eval_raw` uses (`lute_cel::parse_slot` + `lute_trace::eval`,
// never a second one), reused here for schedule variant `when:` guards and
// route-space seeding, which must be decided BETWEEN scenes, before any
// scene's own `Runner` exists for that presentation slot.
// ===========================================================================

fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

/// Coerce a raw `--state`/route-script literal against `path`'s declared
/// type in the project-wide union — mirrors `Runner::coerce_literal`
/// (private to `runner.rs`; duplicated here rather than widened, since it is
/// ten lines with no other caller).
fn coerce_literal(union: &ProjectUnion, path: &str, lit: &str) -> Value {
    let ty = union.state.get(path).and_then(|e| e.get("type")).and_then(Json::as_str);
    match ty {
        Some("bool") => match lit {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::Str(lit.to_string()),
        },
        Some("number") => lit.parse::<f64>().map(Value::Num).unwrap_or(Value::Str(lit.to_string())),
        _ => match lit {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => lit.parse::<f64>().map(Value::Num).unwrap_or(Value::Str(lit.to_string())),
        },
    }
}

fn parse_ground_fact(s: &str) -> Option<Fact> {
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    let rel = s[..open].trim();
    if rel.is_empty() {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    let args: Vec<String> = if inner.trim().is_empty() {
        Vec::new()
    } else {
        inner.split(',').map(|a| a.trim().to_string()).collect()
    };
    Some((rel.to_string(), args))
}

/// Evaluate a `raw` CEL fragment against a state/fact snapshot. An empty
/// `StateSchema`/`RelVocab` (mirrors `Runner`'s own construction): every
/// relation is non-derived, so `holds`/`count` return DEFINITE answers over
/// exactly the supplied `facts`.
fn eval_cel(raw: &str, state: &BTreeMap<String, Value>, facts: &BTreeSet<Fact>) -> (Value, Vec<UnresolvedAtom>) {
    if raw.trim().is_empty() {
        return (Value::Unknown, Vec::new());
    }
    let mut arena = CelArena::default();
    let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, 0) else {
        return (Value::Unknown, Vec::new());
    };
    let Some(ided) = arena.get(handle) else {
        return (Value::Unknown, Vec::new());
    };
    let schema = StateSchema::default();
    let vocab = RelVocab::default();
    let eff = EffectiveState::new(&schema, state.clone());
    let mut fs = FactStore::new(&vocab);
    for (rel, args) in facts {
        fs.assert(rel, args);
    }
    let env = EvalEnv { state: &eff, facts: &fs };
    let mut unresolved = Vec::new();
    let v = eval(&ided.expr, &env, &mut unresolved);
    (v, unresolved)
}

fn truthy_cel(raw: &str, state: &BTreeMap<String, Value>, facts: &BTreeSet<Fact>) -> Option<bool> {
    match eval_cel(raw, state, facts).0 {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

// ===========================================================================
// Scene artifact introspection — canonical key, `after:` causality, and
// choice/hub decision points, all read straight off the compiled JSON (this
// module compiles every doc itself, so it always has it at hand).
// ===========================================================================

fn scene_canonical_key(art_json: &Json) -> Option<String> {
    let meta = art_json.get("meta")?;
    let character = meta.get("character").and_then(Json::as_str)?;
    let season = meta.get("season").and_then(Json::as_i64)?;
    let episode = meta.get("episode").and_then(Json::as_i64)?;
    let episode_id = meta.get("episodeId").and_then(Json::as_str);
    Some(lute_check::meta::canonical_episode_key(character, season, episode, episode_id))
}

/// This scene's own declared `after:` prerequisite, parsed via
/// `lute_check`'s restricted profile parser — the SAME parser/grammar the
/// project-wide compile gate (step (a)) already proved this document's
/// `after:` well-formed under, so a `None` here means "this document simply
/// declares no prerequisite", never "failed to parse" (that would have
/// already refused the whole-project compile gate).
fn scene_prereq(art_json: &Json) -> Option<PrereqFormula> {
    let edges = art_json.get("prereqEdges").and_then(Json::as_array)?;
    let raw = edges.first()?.get("after").and_then(Json::as_str)?;
    if raw.trim().is_empty() {
        return None;
    }
    let span =
        lute_core_span::Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) };
    lute_check::parse_prereq(raw, span).0
}

/// Design spec §4.2's causality check: `visited`/`completed`/`active` sets
/// accumulated so far, in PRESENTATION order (never story-tick order — a
/// cold-open-first schedule is exactly what makes the two diverge).
/// `completed`/`active` are always empty for the whole playthrough per this
/// module's scoping decision (module doc) — an honest empty set, not a
/// faked one.
fn eval_prereq(f: &PrereqFormula, visited: &BTreeSet<String>, completed: &BTreeSet<String>, active: &BTreeSet<String>) -> bool {
    match f {
        PrereqFormula::Visited(k) => visited.contains(k),
        PrereqFormula::Completed(q) => completed.contains(q),
        PrereqFormula::Active(q) => active.contains(q),
        PrereqFormula::And(a, b) => {
            eval_prereq(a, visited, completed, active) && eval_prereq(b, visited, completed, active)
        }
        PrereqFormula::Or(a, b) => {
            eval_prereq(a, visited, completed, active) || eval_prereq(b, visited, completed, active)
        }
    }
}

/// Every `choice`/`hub` command in a compiled scene artifact: its
/// presentation id (`branchId`/`id`) and its full `options` JSON array, IN
/// DECLARED ORDER (design spec §4.4's "first eligible option" for
/// `--auto first`, and the "eligible options" an incomplete halt names).
fn scene_decision_points(art_json: &Json) -> Vec<(String, Vec<Json>)> {
    let mut out = Vec::new();
    let Some(commands) = art_json.get("commands").and_then(Json::as_array) else { return out };
    for cmd in commands {
        let kind = cmd.get("kind").and_then(Json::as_str).unwrap_or("");
        let id = match kind {
            "choice" => cmd.get("branchId").and_then(Json::as_str),
            "hub" => cmd.get("id").and_then(Json::as_str),
            _ => None,
        };
        let Some(id) = id else { continue };
        let opts: Vec<Json> =
            cmd.get("options").and_then(Json::as_array).cloned().unwrap_or_default();
        out.push((id.to_string(), opts));
    }
    out
}

/// The first option NOT decided-false against `state`/`facts` — an
/// approximation of "first ELIGIBLE option" (design spec §4.4) using only
/// the scene's STARTING carried state (a guard depending on a `::set`
/// EARLIER IN THE SAME SCENE cannot be foreseen without a live callback into
/// the Runner mid-walk, which this module does not have). Falls back to the
/// literal first option when every option is decided-false; `Runner`'s own
/// existing forced-selection guard refusal (`[E-TRACE-CHOICE]`) then
/// correctly refuses it rather than this module silently picking a bad one.
fn first_eligible_option(opts: &[Json], state: &BTreeMap<String, Value>, facts: &BTreeSet<Fact>) -> Option<String> {
    let mut fallback = None;
    for o in opts {
        let Some(id) = o.get("id").and_then(Json::as_str) else { continue };
        if fallback.is_none() {
            fallback = Some(id.to_string());
        }
        let eligible = match o.get("when").and_then(Json::as_str) {
            None => true,
            Some(w) => truthy_cel(w, state, facts) != Some(false),
        };
        if eligible {
            return Some(id.to_string());
        }
    }
    fallback
}

/// `id -> distinct owning event names`, scanned across EVERY placement's
/// EVERY variant doc (route-independent — the whole schedule, design spec
/// §4.4), for bare `choose:` key resolution/uniqueness.
fn decision_point_events(schedule: &Schedule, art_json: &BTreeMap<String, Json>) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in &schedule.placements {
        for v in &p.variants {
            let Some(doc) = art_json.get(&v.doc) else { continue };
            for (id, _) in scene_decision_points(doc) {
                out.entry(id).or_default().insert(p.event.clone());
            }
        }
    }
    out
}

/// Resolve a merged `choose:` map (route-script ∪ CLI `--choose`, CLI
/// winning on a same-key conflict) into `(event, id) -> ordered picks`
/// (design spec §4.4). An ambiguous bare key names every colliding event.
fn resolve_choose_keys(
    choose: &BTreeMap<String, Vec<String>>,
    owners: &BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeMap<(String, String), Vec<String>>, String> {
    let mut out = BTreeMap::new();
    for (key, picks) in choose {
        if let Some((event, id)) = key.split_once('/') {
            out.insert((event.to_string(), id.to_string()), picks.clone());
            continue;
        }
        match owners.get(key) {
            None => {
                // Unknown anywhere in the schedule: never consulted, but not
                // fatal here — an unused route-script entry is harmless.
            }
            Some(events) if events.len() == 1 => {
                let event = events.iter().next().unwrap().clone();
                out.insert((event, key.clone()), picks.clone());
            }
            Some(events) => {
                let mut names: Vec<&String> = events.iter().collect();
                names.sort();
                let list = names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                return Err(format!(
                    "`choose: {key}` is ambiguous — that hub/branch id appears in multiple scheduled events ({list}); use the qualified `<event>/{key}` form"
                ));
            }
        }
    }
    Ok(out)
}

/// Build this scene's [`lute_trace::MockSet`]: route-script/`--choose`
/// entries qualified to THIS event first, `--auto first` filling any OTHER
/// decision point in the scene when set, leaving anything else uncovered
/// (Runner's own existing "no mock decision — incomplete" halt handles it).
fn scene_mock(
    event: &str,
    scene_json: &Json,
    choose: &BTreeMap<(String, String), Vec<String>>,
    auto_first: bool,
    state: &BTreeMap<String, Value>,
    facts: &BTreeSet<Fact>,
) -> lute_trace::MockSet {
    let mut mock = lute_trace::MockSet::default();
    for (id, opts) in scene_decision_points(scene_json) {
        if let Some(picks) = choose.get(&(event.to_string(), id.clone())) {
            mock.choose.insert(id, picks.clone());
        } else if auto_first {
            if let Some(pick) = first_eligible_option(&opts, state, facts) {
                mock.choose.insert(id, vec![pick]);
            }
        }
    }
    mock
}

fn variant_label_of(doc: &str) -> String {
    Path::new(doc).file_stem().and_then(|s| s.to_str()).unwrap_or(doc).to_string()
}

/// W-SCHED-TIME-MISMATCH (design spec §5): the scene's FIRST `::bg time=`
/// vs. the placement's START bucket (`day` matches morning..late_afternoon).
/// `Some((bg_time, bucket))` on a mismatch; `None` when clean or inapplicable
/// (no `background` record, or an unresolvable clock).
fn first_bg_time_mismatch(scene_json: &Json, clock: &schedule::Clock, tick: u32) -> Option<(String, String)> {
    let commands = scene_json.get("commands").and_then(Json::as_array)?;
    let bg = commands.iter().find(|c| c.get("kind").and_then(Json::as_str) == Some("background"))?;
    let time = bg.get("time").and_then(Json::as_str)?;
    let bucket = bucket_of(clock, tick)?;
    let ok = time == bucket || (time == "day" && matches!(bucket, "morning" | "late_morning" | "afternoon" | "late_afternoon"));
    if ok {
        None
    } else {
        Some((time.to_string(), bucket.to_string()))
    }
}

fn bucket_of(clock: &schedule::Clock, tick: u32) -> Option<&str> {
    if clock.ticks_per_bucket == 0 || clock.buckets.is_empty() {
        return None;
    }
    let per_day = (clock.buckets.len() as u32).saturating_mul(clock.ticks_per_bucket);
    if per_day == 0 {
        return None;
    }
    let rem = tick % per_day;
    let idx = (rem / clock.ticks_per_bucket) as usize;
    clock.buckets.get(idx).map(String::as_str)
}

// ===========================================================================
// Variant resolution + presentation order (design spec §4.1/§4.2).
// ===========================================================================

/// The halt reasons a chained playthrough can stop for, each with its own
/// exit-code tier (mirrors the workspace's existing 0/1/2/3 convention:
/// 1 = schedule-runtime/gate failure, 2 = I/O-ish/engine-fatal, 3 =
/// incomplete).
enum PlayHalt {
    VariantGap { event: String },
    VariantAmbig { event: String, docs: Vec<String> },
    AfterOrder { event: String, doc: String },
    /// A Runner-reported fatal (malformed artifact, unknown command kind, an
    /// unresolvable placement tick despite a clean gate).
    Fatal(String),
    UnscriptedDecision { event: String, doc: String, kind: String, id: String, options: Vec<String> },
    UnresolvedSurface { event: String, doc: String, detail: String },
    /// Review fix #3 (design spec §4.5 honesty): a placement's variant
    /// selection depends on an unresolved reference-runtime surface
    /// (`now()`/`validAt(...)`, an undecided derived fact) THAT AFFECTS
    /// which variant (if any) would be selected — never silently folded to
    /// `false` the way a merely-decided-false guard is.
    UnresolvedVariantGuard { event: String, detail: String },
}

impl PlayHalt {
    fn exit_code(&self) -> ExitCode {
        match self {
            PlayHalt::VariantGap { .. } | PlayHalt::VariantAmbig { .. } | PlayHalt::AfterOrder { .. } => {
                ExitCode::from(1)
            }
            PlayHalt::Fatal(_) => ExitCode::from(2),
            PlayHalt::UnscriptedDecision { .. }
            | PlayHalt::UnresolvedSurface { .. }
            | PlayHalt::UnresolvedVariantGuard { .. } => ExitCode::from(3),
        }
    }

    /// Whether this halt maps to the incomplete-walk exit tier (3) — used by
    /// [`render_json`] rather than re-deriving the tier from [`ExitCode`],
    /// which exposes no public way to inspect its wrapped value.
    fn is_incomplete(&self) -> bool {
        matches!(
            self,
            PlayHalt::UnscriptedDecision { .. }
                | PlayHalt::UnresolvedSurface { .. }
                | PlayHalt::UnresolvedVariantGuard { .. }
        )
    }

    fn message(&self) -> String {
        match self {
            PlayHalt::VariantGap { event } => format!(
                "[E-SCHED-VARIANT-GAP] event `{event}` has no satisfiable variant for the current route state"
            ),
            PlayHalt::VariantAmbig { event, docs } => format!(
                "[E-SCHED-VARIANT-AMBIG] event `{event}` has {} co-satisfiable variants for the current route state: {}",
                docs.len(),
                docs.join(", ")
            ),
            PlayHalt::AfterOrder { event, doc } => format!(
                "[E-SCHED-AFTER-ORDER] `{doc}` (event `{event}`) declares an `after:` prerequisite not yet satisfied in presentation order"
            ),
            PlayHalt::Fatal(msg) => format!("lute play: {msg}"),
            PlayHalt::UnscriptedDecision { event, doc, kind, id, options } => format!(
                "incomplete: event `{event}` (`{doc}`) reached {kind} `{id}` with no scripted/`--auto` decision (eligible options: {})",
                if options.is_empty() { "none".to_string() } else { options.join(", ") }
            ),
            PlayHalt::UnresolvedSurface { event, doc, detail } => format!(
                "incomplete: event `{event}` (`{doc}`) depends on an unresolved reference-runtime surface — {detail}"
            ),
            PlayHalt::UnresolvedVariantGuard { event, detail } => format!(
                "incomplete: event `{event}` has a variant guard whose selection depends on an unresolved \
                 reference-runtime surface (design spec §4.5) — {detail}"
            ),
        }
    }
}

/// One `UnresolvedAtom` -> a human-readable reason (mirrors
/// `lute_trace::walk::render_atom`'s intent, which is private to that
/// crate — this is schedule variant selection's own small surface, never
/// worth a cross-crate visibility change for three match arms).
fn describe_unresolved_atom(a: &UnresolvedAtom) -> String {
    match a {
        UnresolvedAtom::Path(p) => format!("state path `{p}` has no effective value"),
        UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => format!("fact `{f}` is undetermined"),
        UnresolvedAtom::Time => "now()/validAt(...) has no reference-runtime resolution".to_string(),
    }
}

/// Resolve a placement's active variant against a (state, facts) snapshot
/// (design spec §4.2's boundary loop): exactly one DEFINITELY-satisfied
/// variant -> that variant; zero on `optional: true` -> `Ok(None)` (skip);
/// zero otherwise -> [`PlayHalt::VariantGap`]; two or more ->
/// [`PlayHalt::VariantAmbig`].
///
/// Review fix #3: a guard that evaluates to neither `true` nor `false`
/// (`now()`/`validAt(...)`, an undecided derived fact — `Value::Unknown`
/// carrying [`UnresolvedAtom`]s) is NEVER folded to `false` the way a
/// decided-false guard is. Its variant instead joins `maybe_true` — the
/// unresolved guard is only IGNORED (treated as if it read false) when the
/// boundary's verdict cannot possibly change no matter how it resolves:
/// two or more variants are ALREADY definitely satisfied (ambiguous
/// either way). Otherwise the unresolved surface genuinely affects
/// selection (it could turn a gap into a play, a single pick into an
/// ambiguity, or vice versa) and the walk halts honestly incomplete
/// ([`PlayHalt::UnresolvedVariantGuard`], exit 3) rather than guessing.
fn resolve_active_variant(p: &Placement, state: &BTreeMap<String, Value>, facts: &BTreeSet<Fact>) -> Result<Option<usize>, PlayHalt> {
    let mut definite_true = Vec::new();
    let mut maybe_count = 0usize;
    let mut atoms: Vec<UnresolvedAtom> = Vec::new();
    for (vi, v) in p.variants.iter().enumerate() {
        match &v.when {
            None => {
                definite_true.push(vi);
                maybe_count += 1;
            }
            Some(raw) => match eval_cel(raw, state, facts) {
                (Value::Bool(true), _) => {
                    definite_true.push(vi);
                    maybe_count += 1;
                }
                (Value::Bool(false), _) => {}
                (_, unresolved) if !unresolved.is_empty() => {
                    maybe_count += 1;
                    atoms.extend(unresolved);
                }
                (_, _) => {}
            },
        }
    }
    if !atoms.is_empty() && definite_true.len() < 2 {
        let mut seen = BTreeSet::new();
        let mut reasons: Vec<String> = Vec::new();
        for a in &atoms {
            let d = describe_unresolved_atom(a);
            if seen.insert(d.clone()) {
                reasons.push(d);
            }
        }
        debug_assert!(maybe_count > definite_true.len());
        return Err(PlayHalt::UnresolvedVariantGuard { event: p.event.clone(), detail: reasons.join("; ") });
    }
    match definite_true.len() {
        0 if p.optional => Ok(None),
        0 => Err(PlayHalt::VariantGap { event: p.event.clone() }),
        1 => Ok(Some(definite_true[0])),
        _ => Err(PlayHalt::VariantAmbig {
            event: p.event.clone(),
            docs: definite_true.iter().map(|&vi| p.variants[vi].doc.clone()).collect(),
        }),
    }
}

/// Derive the NEXT user-lane placement to present (design spec §4.1/§4.2
/// boundary loop, review fix #2): scans every not-yet-decided user
/// placement, re-resolving its active variant against LIVE state/facts —
/// never a seed snapshot frozen before any scene has played. An earlier
/// scene's `::set` can flip a placement whose guard read false at seed time
/// (an `optional` placement is therefore never permanently excluded — only
/// "not satisfied on THIS boundary", reconsidered on every later one), and
/// a placement whose active variant switches route mid-playthrough always
/// sorts by THAT variant's own declared `(presentation, at, decl_index)` —
/// itself a static, schedule.yaml-resolved number (phase 1), so the sort
/// TUPLE stays exactly as static as the design calls for even though WHICH
/// variant supplies it is live. Returns the smallest-keyed currently-
/// satisfiable candidate's `(placement_idx, variant_idx)`, or `Ok(None)`
/// when no undecided placement currently resolves to `Some` — either
/// every user placement is decided, or every remaining one is `optional`
/// and unsatisfied FOR NOW (never a permanent verdict by itself; the
/// caller simply has nothing left to present this boundary).
fn next_user_step(
    schedule: &Schedule,
    decided: &BTreeSet<usize>,
    live_state: &BTreeMap<String, Value>,
    live_facts: &BTreeSet<Fact>,
) -> Result<Option<(usize, usize)>, PlayHalt> {
    let mut best: Option<(u32, u32, u32, usize, usize)> = None;
    for (pi, p) in schedule.placements.iter().enumerate() {
        if p.lane != "user" || decided.contains(&pi) {
            continue;
        }
        if let Some(vi) = resolve_active_variant(p, live_state, live_facts)? {
            let v = &p.variants[vi];
            let key = (v.presentation, v.at.unwrap_or(u32::MAX), p.decl_index, pi, vi);
            if best.as_ref().map(|b| key < *b).unwrap_or(true) {
                best = Some(key);
            }
        }
    }
    Ok(best.map(|(_, _, _, pi, vi)| (pi, vi)))
}

// ===========================================================================
// The chain executor (design spec §4.2/§4.3).
// ===========================================================================

/// One presented scene, ready for rendering.
struct SceneRun {
    lane: &'static str,
    event: String,
    doc: String,
    variant_label: String,
    tick: u32,
    tick_label: String,
    end_tick: u32,
    fast_forward_from: Option<u32>,
    rewind_from: Option<(u32, String)>,
    world_in_flashback: bool,
    time_mismatch: Option<(String, String)>,
    transcript: Vec<Json>,
    state_before: BTreeMap<String, Value>,
    state_after: BTreeMap<String, Value>,
}

/// Everything [`run_one_scene`] produced.
struct OneRun {
    scene: SceneRun,
    terminated: bool,
    halt: Option<PlayHalt>,
}

/// Design spec §4.2 state-tier carryover, done right (review fix #1): a
/// fresh state map seeded from THIS SPECIFIC SCENE's own `state[].default`
/// (`scene_json` — never `play_json`, which swaps in the PROJECT-WIDE union
/// for type-checking purposes and would silently carry the path-sorted-
/// FIRST document's default/type into every OTHER document that happens to
/// reuse the same `scene.*` path), with only the PERSISTENT
/// `run.*`/`user.*`/`app.*`/`quest.*` tiers of `live_state` overlaid on top
/// — `scene.*` always resets to the artifact's own declared default at
/// every scene boundary, never inherited from a sibling document or a
/// previous scene.
fn scene_initial_state(scene_json: &Json, live_state: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    if let Some(entries) = scene_json.get("state").and_then(Json::as_array) {
        for e in entries {
            let Some(path) = e.get("path").and_then(Json::as_str) else { continue };
            if let Some(default) = e.get("default") {
                if let Some(v) = json_to_value(default) {
                    state.insert(path.to_string(), v);
                }
            }
        }
    }
    for (k, v) in live_state {
        if !k.starts_with("scene.") {
            state.insert(k.clone(), v.clone());
        }
    }
    state
}

/// Run exactly one scene (user or world lane), threading the chain's live
/// state/facts/quest status through it. State-tier carryover (design spec
/// §4.2): `scene.*` resets to whatever THIS scene's own artifact carries
/// (never written back from `outcome.state`); `run.*`/`user.*`/`app.*`/
/// `quest.*` persist.
#[allow(clippy::too_many_arguments)]
fn run_one_scene(
    lane: &'static str,
    event: &str,
    doc: &str,
    t_start: u32,
    t_end: u32,
    clock: &schedule::Clock,
    scene_json: &Json,
    union: &ProjectUnion,
    live_state: &mut BTreeMap<String, Value>,
    live_facts: &mut BTreeSet<Fact>,
    live_quests: &mut BTreeMap<String, String>,
    choose: &BTreeMap<(String, String), Vec<String>>,
    auto_first: bool,
) -> OneRun {
    let mock = scene_mock(event, scene_json, choose, auto_first, live_state, live_facts);
    let play_json = play_artifact_json(scene_json, union);
    let state_before = live_state.clone();
    let initial_state = scene_initial_state(scene_json, live_state);
    let mut runner = Runner::with_carryover(&play_json, mock, initial_state, live_facts.clone(), live_quests.clone())
        .with_auto_first(auto_first);
    let run_result = runner.run();
    let outcome = runner.into_outcome();

    for (k, v) in &outcome.state {
        if !k.starts_with("scene.") {
            live_state.insert(k.clone(), v.clone());
        }
    }
    *live_facts = outcome.base_facts.clone();
    *live_quests = outcome.quest_status.clone();

    let mut halt = run_result.err().map(PlayHalt::Fatal);
    if halt.is_none() && outcome.incomplete {
        let rec = outcome
            .transcript
            .iter()
            .rev()
            .find(|c| c.get("note").and_then(Json::as_str) == Some("no mock decision — incomplete"));
        let (kind, id) = match rec {
            Some(r) => {
                let k = r.get("kind").and_then(Json::as_str).unwrap_or("choice").to_string();
                let id = r
                    .get("branch")
                    .or_else(|| r.get("hub"))
                    .and_then(Json::as_str)
                    .unwrap_or("?")
                    .to_string();
                (k, id)
            }
            None => ("choice".to_string(), "?".to_string()),
        };
        let options = scene_decision_points(scene_json)
            .into_iter()
            .find(|(i, _)| i == &id)
            .map(|(_, opts)| opts.iter().filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string)).collect())
            .unwrap_or_default();
        halt = Some(PlayHalt::UnscriptedDecision { event: event.to_string(), doc: doc.to_string(), kind, id, options });
    }
    if halt.is_none() {
        if outcome.unresolved.iter().any(|a| matches!(a, UnresolvedAtom::Time)) {
            halt = Some(PlayHalt::UnresolvedSurface {
                event: event.to_string(),
                doc: doc.to_string(),
                detail: "now()/validAt(...) has no reference-runtime resolution (design spec §4.5)".to_string(),
            });
        }
    }
    if halt.is_none() {
        if let Some(plugin) = outcome.transcript.iter().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("plugin")
                && c.get("unresolvedEffects").and_then(Json::as_array).map(|a| !a.is_empty()).unwrap_or(false)
        }) {
            let tag = plugin.get("tag").and_then(Json::as_str).unwrap_or("?");
            halt = Some(PlayHalt::UnresolvedSurface {
                event: event.to_string(),
                doc: doc.to_string(),
                detail: format!(
                    "plugin `{tag}` left a `bridgeResult` effect unresolved (design spec §4.5, no bridge invoked)"
                ),
            });
        }
    }

    let time_mismatch = first_bg_time_mismatch(scene_json, clock, t_start);
    let tick_label = clock.label(t_start);

    OneRun {
        scene: SceneRun {
            lane,
            event: event.to_string(),
            doc: doc.to_string(),
            variant_label: variant_label_of(doc),
            tick: t_start,
            tick_label,
            end_tick: t_end,
            fast_forward_from: None,
            rewind_from: None,
            world_in_flashback: false,
            time_mismatch,
            transcript: outcome.transcript,
            state_before,
            state_after: live_state.clone(),
        },
        terminated: outcome.terminated,
        halt,
    }
}

/// Drain unfired world placements due in `[*world_cursor, t_end)`,
/// atomically, one at a time in `(at, decl_index)` order (design spec §4.3)
/// — BEFORE the next user placement's guards are evaluated (enforced by the
/// caller's own sequencing, this function is only ever called between two
/// user-placement boundaries or at the final flush).
///
/// Review fix #4 (rescan): the scan for "what's next" re-runs from scratch
/// after EVERY fired world scene, never once upfront — a fired scene can
/// change live state so a placement that was NOT a candidate a moment ago
/// (guard false, or a different variant/tick active) becomes due inside
/// THIS SAME `[world_cursor, t_end)` window; a stale one-shot candidate
/// list would advance `world_cursor` straight past it.
///
/// Review fix #5 (defer): a placement's guard is resolved (and can raise
/// `E-SCHED-VARIANT-GAP`/`-AMBIG`) ONLY once at least one of its variants'
/// STATIC, schedule.yaml-resolved `at` falls in `[scan_from, t_end)` —
/// never for a placement every one of whose variants is due strictly
/// later. A non-optional world event whose guard only becomes true after a
/// LATER user scene therefore never raises a premature gap during an
/// EARLIER drain; its guard is examined only once its own boundary is
/// actually reached.
///
/// Returns `Ok(Some(end_reason))` when a drained world scene's `::end`
/// terminates the whole playthrough or `--steps` is exhausted mid-drain,
/// `Ok(None)` to continue, `Err` on a runtime schedule halt.
#[allow(clippy::too_many_arguments)]
fn drain_world(
    world_placements: &[&Placement],
    world_fired: &mut BTreeSet<u32>,
    world_cursor: &mut u32,
    t_end: u32,
    in_rewind: bool,
    art_json: &BTreeMap<String, Json>,
    union: &ProjectUnion,
    clock: &schedule::Clock,
    live_state: &mut BTreeMap<String, Value>,
    live_facts: &mut BTreeSet<Fact>,
    live_quests: &mut BTreeMap<String, String>,
    visited: &mut BTreeSet<String>,
    choose: &BTreeMap<(String, String), Vec<String>>,
    auto_first: bool,
    scenes: &mut Vec<SceneRun>,
    presented_count: &mut u32,
    steps_limit: Option<u32>,
) -> Result<Option<String>, PlayHalt> {
    let scan_from = *world_cursor;
    loop {
        // Fresh scan every iteration (review fix #4): find the smallest
        // `(at, decl_index)` among unfired placements whose LIVE-resolved
        // variant is due in `[scan_from, t_end)` right now.
        let mut winner: Option<(u32, u32, usize, usize)> = None; // (at, decl_index, world_placements idx, variant idx)
        for (pi, p) in world_placements.iter().enumerate() {
            if world_fired.contains(&p.decl_index) {
                continue;
            }
            // Review fix #5: cheap, guard-free pre-filter off each
            // variant's STATIC resolved `at` — a placement none of whose
            // variants could possibly land in this window never has its
            // guard evaluated here, so it can never raise a premature
            // GAP/AMBIG for a boundary it has not reached yet.
            let could_be_due = p.variants.iter().any(|v| v.at.map(|at| at >= scan_from && at < t_end).unwrap_or(false));
            if !could_be_due {
                continue;
            }
            let Some(vi) = resolve_active_variant(p, live_state, live_facts)? else {
                continue;
            };
            let Some(at) = p.variants[vi].at else { continue };
            if at < scan_from || at >= t_end {
                // The LIVE-active variant is not the one(s) that made this
                // placement pass the pre-filter — not due in this window
                // after all; a later drain (whose range covers its actual
                // tick) reconsiders it.
                continue;
            }
            let key = (at, p.decl_index, pi, vi);
            if winner.as_ref().map(|w| (key.0, key.1) < (w.0, w.1)).unwrap_or(true) {
                winner = Some(key);
            }
        }
        let Some((t_start, decl_index, pi, vi)) = winner else { break };

        if let Some(limit) = steps_limit {
            if *presented_count >= limit {
                return Ok(Some(format!("stopped after {limit} step(s)")));
            }
        }
        let p = world_placements[pi];
        let variant = &p.variants[vi];
        let t_end_v = t_start.saturating_add(variant.size);
        let Some(scene_json) = art_json.get(&variant.doc) else {
            return Err(PlayHalt::Fatal(format!("world event `{}` names an uncompiled doc `{}`", p.event, variant.doc)));
        };
        if let Some(formula) = scene_prereq(scene_json) {
            if !eval_prereq(&formula, visited, &BTreeSet::new(), &BTreeSet::new()) {
                return Err(PlayHalt::AfterOrder { event: p.event.clone(), doc: variant.doc.clone() });
            }
        }
        let mut run = run_one_scene(
            "world",
            &p.event,
            &variant.doc,
            t_start,
            t_end_v,
            clock,
            scene_json,
            union,
            live_state,
            live_facts,
            live_quests,
            choose,
            auto_first,
        );
        run.scene.world_in_flashback = in_rewind;
        world_fired.insert(decl_index);
        *presented_count += 1;
        if let Some(k) = scene_canonical_key(scene_json) {
            visited.insert(k);
        }
        let terminated = run.terminated;
        let halt = run.halt.take();
        scenes.push(run.scene);
        if terminated {
            return Ok(Some(format!("`::end` (world `{}`)", p.event)));
        }
        if let Some(h) = halt {
            return Err(h);
        }
        // Loop again (review fix #4): this firing may have changed live
        // state, so rescan from `scan_from` — a placement newly due inside
        // THIS window is picked up now rather than left behind when
        // `world_cursor` advances to `t_end` below.
    }
    *world_cursor = t_end;
    Ok(None)
}

/// The chain executor. Returns every scene played (even on a halt — a
/// partial transcript is still useful) and either the completion reason or
/// the halt that stopped it.
#[allow(clippy::too_many_arguments)]
fn execute(
    schedule: &Schedule,
    art_json: &BTreeMap<String, Json>,
    union: &ProjectUnion,
    seed_state: BTreeMap<String, Value>,
    seed_facts: BTreeSet<Fact>,
    choose: &BTreeMap<(String, String), Vec<String>>,
    auto_first: bool,
    steps_limit: Option<u32>,
) -> (Vec<SceneRun>, Result<String, PlayHalt>) {
    let mut scenes: Vec<SceneRun> = Vec::new();
    let mut live_state = seed_state;
    let mut live_facts = seed_facts;
    let mut live_quests: BTreeMap<String, String> = BTreeMap::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let completed: BTreeSet<String> = BTreeSet::new();
    let active: BTreeSet<String> = BTreeSet::new();
    // Presented (or permanently forfeited) user placements, by
    // `schedule.placements` index — review fix #2's `next_user_step` is
    // re-derived fresh from LIVE state at every boundary, so this is the
    // ONLY thing that needs to persist across iterations to keep a played
    // placement from being reconsidered.
    let mut decided: BTreeSet<usize> = BTreeSet::new();

    let mut prev_user_start: Option<u32> = None;
    let mut prev_user_end: Option<u32> = None;
    let mut world_cursor: u32 = 0;
    let mut high_water: u32 = 0;
    let mut in_rewind = false;
    let mut world_fired: BTreeSet<u32> = BTreeSet::new();
    let mut presented_count: u32 = 0;

    let world_placements: Vec<&Placement> = schedule.placements.iter().filter(|p| p.lane == "world").collect();

    loop {
        if let Some(limit) = steps_limit {
            if presented_count >= limit {
                return (scenes, Ok(format!("stopped after {limit} step(s)")));
            }
        }
        let (placement_idx, vi) = match next_user_step(schedule, &decided, &live_state, &live_facts) {
            Ok(Some(step)) => step,
            Ok(None) => break,
            Err(h) => return (scenes, Err(h)),
        };
        decided.insert(placement_idx);
        let placement = &schedule.placements[placement_idx];
        let variant = &placement.variants[vi];
        let Some(t_start) = variant.at else {
            return (
                scenes,
                Err(PlayHalt::Fatal(format!(
                    "event `{}` has no resolved tick — schedule.yaml carries an unresolved placement",
                    placement.event
                ))),
            );
        };
        let t_end = t_start.saturating_add(variant.size);

        let is_new_segment = prev_user_start.map(|prev| t_start < prev).unwrap_or(true);
        let mut fast_forward_from = None;
        let mut rewind_from = None;
        if is_new_segment {
            if let Some(prev_end) = prev_user_end {
                if t_start < high_water {
                    rewind_from = Some((prev_end, schedule.clock.label(prev_end)));
                }
            }
            world_cursor = t_start;
            // W-SCHED-WORLD-IN-FLASHBACK (design spec §4.3): a CONSERVATIVE
            // approximation of "covered only by a segment that plays in the
            // future of a later-presented segment" — every world scene
            // drained anywhere inside a segment that itself starts BEHIND
            // the highest story tick already reached is flagged, rather
            // than tracing whether some STILL-LATER segment would also have
            // covered it. A human reviewer sees the flag and the segment's
            // own tick label either way; a false positive here costs a
            // glance, a false negative costs a missed smell — this errs
            // toward the cheaper mistake.
            in_rewind = t_start < high_water;
        } else if let Some(prev_end) = prev_user_end {
            if prev_end < t_start {
                fast_forward_from = Some(prev_end);
            }
        }

        let Some(scene_json) = art_json.get(&variant.doc) else {
            return (
                scenes,
                Err(PlayHalt::Fatal(format!("event `{}` names an uncompiled doc `{}`", placement.event, variant.doc))),
            );
        };
        if let Some(formula) = scene_prereq(scene_json) {
            if !eval_prereq(&formula, &visited, &completed, &active) {
                return (scenes, Err(PlayHalt::AfterOrder { event: placement.event.clone(), doc: variant.doc.clone() }));
            }
        }

        let mut run = run_one_scene(
            "user",
            &placement.event,
            &variant.doc,
            t_start,
            t_end,
            &schedule.clock,
            scene_json,
            union,
            &mut live_state,
            &mut live_facts,
            &mut live_quests,
            choose,
            auto_first,
        );
        run.scene.fast_forward_from = fast_forward_from;
        run.scene.rewind_from = rewind_from;
        presented_count += 1;
        if let Some(k) = scene_canonical_key(scene_json) {
            visited.insert(k);
        }
        let terminated = run.terminated;
        let halt = run.halt.take();
        scenes.push(run.scene);
        if terminated {
            return (scenes, Ok(format!("`::end` (`{}`)", placement.event)));
        }
        if let Some(h) = halt {
            return (scenes, Err(h));
        }

        prev_user_start = Some(t_start);
        prev_user_end = Some(t_end);
        high_water = high_water.max(t_end);

        match drain_world(
            &world_placements,
            &mut world_fired,
            &mut world_cursor,
            t_end,
            in_rewind,
            art_json,
            union,
            &schedule.clock,
            &mut live_state,
            &mut live_facts,
            &mut live_quests,
            &mut visited,
            choose,
            auto_first,
            &mut scenes,
            &mut presented_count,
            steps_limit,
        ) {
            Ok(Some(reason)) => return (scenes, Ok(reason)),
            Ok(None) => {}
            Err(h) => return (scenes, Err(h)),
        }
    }

    // Final flush: drain anything left uncovered up to the clock's end.
    match drain_world(
        &world_placements,
        &mut world_fired,
        &mut world_cursor,
        schedule.clock.total_ticks(),
        in_rewind,
        art_json,
        union,
        &schedule.clock,
        &mut live_state,
        &mut live_facts,
        &mut live_quests,
        &mut visited,
        choose,
        auto_first,
        &mut scenes,
        &mut presented_count,
        steps_limit,
    ) {
        Ok(Some(reason)) => (scenes, Ok(reason)),
        Ok(None) => (scenes, Ok(format!("clock exhausted (tick {})", schedule.clock.total_ticks()))),
        Err(h) => (scenes, Err(h)),
    }
}

// ===========================================================================
// Rendering (design spec §4.6). Human by default; `--json` a structured
// stream. Both reuse `SceneRun.transcript` — the exact JSON records
// `Runner::print_json` already carries — rather than re-deriving per-kind
// dispatch semantics a second time.
// ===========================================================================

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Unknown => "unknown".to_string(),
    }
}

fn render_attrs(cmd: &Json, skip: &[&str]) -> String {
    let Json::Object(map) = cmd else { return String::new() };
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
        .map(|id| if Some(id) == chosen { format!("[{id}]") } else { id.to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One transcript record -> a human line, enriched with the original
/// command's own attrs for `line`/staging kinds (the Runner transcript
/// record itself carries only `addr`/`kind`/a few dispatch fields; the
/// authored attrs live in the artifact's own `commands`, looked up here by
/// `addr`).
fn render_record(rec: &Json, cmd_by_addr: &BTreeMap<&str, &Json>) -> String {
    let addr = rec.get("addr").and_then(Json::as_str).unwrap_or("");
    let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("");
    let orig = cmd_by_addr.get(addr).copied();
    match kind {
        "line" => {
            let speaker = rec.get("speaker").and_then(Json::as_str).unwrap_or("");
            let text = rec.get("text").and_then(Json::as_str).unwrap_or("");
            let attrs = orig.map(|c| render_attrs(c, &["addr", "kind", "text", "speaker", "lineId", "role", "asLabel", "as", "placeholders", "texts"])).unwrap_or_default();
            if attrs.is_empty() {
                format!("@{speaker}: {text}")
            } else {
                format!("@{speaker}{{{attrs}}}: {text}")
            }
        }
        "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
            let attrs = orig.map(|c| render_attrs(c, &["addr", "kind"])).unwrap_or_default();
            if attrs.is_empty() {
                format!("::{kind}")
            } else {
                format!("::{kind}{{{attrs}}}")
            }
        }
        "set" => format!(
            "  set {} = {}",
            rec.get("path").and_then(Json::as_str).unwrap_or(""),
            rec.get("value").map(|v| v.to_string()).unwrap_or_default()
        ),
        "assert" => format!("  assert {}", rec.get("fact").and_then(Json::as_str).unwrap_or("")),
        "retract" => format!("  retract {}", rec.get("pattern").and_then(Json::as_str).unwrap_or("")),
        "choice" | "hub" => {
            let id_key = if kind == "choice" { "branch" } else { "hub" };
            let id = rec.get(id_key).and_then(Json::as_str).unwrap_or("?");
            let chosen = rec.get("chose").and_then(Json::as_str);
            let opts: Vec<Json> = orig.and_then(|c| c.get("options")).and_then(Json::as_array).cloned().unwrap_or_default();
            let rendered = render_options(&opts, chosen);
            match chosen {
                Some(c) => format!("▷ {kind} {id}: {rendered}        ← chosen: {c}"),
                None => format!("▷ {kind} {id}: {rendered}        ← INCOMPLETE (no decision)"),
            }
        }
        "match" => format!("  match -> {}", rec.get("result").and_then(Json::as_str).unwrap_or("")),
        "barrier" => "  barrier (no real clock simulated)".to_string(),
        "end" => match rec.get("reason").and_then(Json::as_str) {
            Some(r) => format!("  ::end reason={r}"),
            None => "  ::end".to_string(),
        },
        "plugin" => format!("  plugin {} (external call, not invoked)", rec.get("tag").and_then(Json::as_str).unwrap_or("")),
        "objective" => format!(
            "  {}.{} done",
            rec.get("quest").and_then(Json::as_str).unwrap_or(""),
            rec.get("objective").and_then(Json::as_str).unwrap_or("")
        ),
        "quest" => format!(
            "  quest {} -> {}",
            rec.get("quest").and_then(Json::as_str).unwrap_or(""),
            rec.get("state").and_then(Json::as_str).unwrap_or("")
        ),
        _ => format!("  {kind}"),
    }
}

fn render_human(art_json: &BTreeMap<String, Json>, scenes: &[SceneRun], outcome: &Result<String, PlayHalt>, lanes_all: bool) -> String {
    let mut out = String::new();
    for s in scenes {
        if s.lane == "world" && !lanes_all {
            continue;
        }
        if let Some((from_tick, from_label)) = &s.rewind_from {
            out.push_str(&format!("⏪ {from_label} → {} (rewind, tick {from_tick} → {})\n", s.tick_label, s.tick));
        }
        if let Some(from) = s.fast_forward_from {
            out.push_str(&format!("⏩ tick {from} → {} (fast-forward, empty user lane)\n", s.tick));
        }
        out.push_str(&format!(
            "── {} (tick {}) · {} · {}/{} ──────────────\n",
            s.tick_label, s.tick, s.lane, s.event, s.variant_label
        ));
        let doc_json = art_json.get(&s.doc);
        let cmd_by_addr: BTreeMap<&str, &Json> = doc_json
            .and_then(Json::as_object)
            .and_then(|o| o.get("commands"))
            .and_then(Json::as_array)
            .map(|arr| arr.iter().filter_map(|c| c.get("addr").and_then(Json::as_str).map(|a| (a, c))).collect())
            .unwrap_or_default();
        for rec in &s.transcript {
            out.push_str(&render_record(rec, &cmd_by_addr));
            out.push('\n');
        }
        if let Some((bg_time, bucket)) = &s.time_mismatch {
            out.push_str(&format!(
                "⚠ W-SCHED-TIME-MISMATCH: ::bg time=\"{bg_time}\" vs schedule bucket \"{bucket}\"\n"
            ));
        }
        if s.world_in_flashback {
            out.push_str(&format!(
                "⚠ W-SCHED-WORLD-IN-FLASHBACK: `{}` drains during a rewound segment\n",
                s.event
            ));
        }
    }
    match outcome {
        Ok(reason) => out.push_str(&format!("── end: {reason} ──────────────────────────\n")),
        Err(h) => out.push_str(&format!("── halted: {} ──────────────────────────\n", h.message())),
    }
    out
}

fn state_delta(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Json {
    let mut delta = serde_json::Map::new();
    for (k, v) in after {
        if before.get(k) != Some(v) {
            delta.insert(k.clone(), json!(value_to_string(v)));
        }
    }
    Json::Object(delta)
}

fn render_json(scenes: &[SceneRun], outcome: &Result<String, PlayHalt>) -> Json {
    let scene_json: Vec<Json> = scenes
        .iter()
        .map(|s| {
            json!({
                "lane": s.lane,
                "event": s.event,
                "doc": s.doc,
                "variant": s.variant_label,
                "tick": s.tick,
                "tickLabel": s.tick_label,
                "endTick": s.end_tick,
                "fastForwardFrom": s.fast_forward_from,
                "rewindFrom": s.rewind_from.as_ref().map(|(t, l)| json!({"tick": t, "label": l})),
                "worldInFlashback": s.world_in_flashback,
                "timeMismatch": s.time_mismatch.as_ref().map(|(t, b)| json!({"bgTime": t, "bucket": b})),
                "commands": s.transcript,
                "stateDelta": state_delta(&s.state_before, &s.state_after),
            })
        })
        .collect();
    match outcome {
        Ok(reason) => json!({ "exit": "complete", "scenes": scene_json, "endReason": reason }),
        Err(h) => json!({
            "exit": if h.is_incomplete() { "incomplete" } else { "error" },
            "scenes": scene_json,
            "error": { "message": h.message() },
        }),
    }
}

// ===========================================================================
// CLI entry point.
// ===========================================================================

/// Merge a route script's `state:`/`facts:`/`choose:` (design spec §4.4)
/// with CLI `--state`/`--fact`/`--choose` overrides — CLI wins on a
/// same-key conflict, facts union (the module doc's `merge()` idiom) —
/// then resolve the project-wide seed state (declared defaults, then every
/// override coerced against its declared type) and seed facts (the
/// project's own `seedFacts:` union, then every parsed `--fact`/script
/// literal). Shared by a single `--script` playthrough and each
/// `--coverage` corpus member (design spec §4.7): one merge rule, never
/// re-derived per script.
fn build_seeds(
    union: &ProjectUnion,
    owners: &BTreeMap<String, BTreeSet<String>>,
    route_script: RouteScript,
    cli_state: &[(String, String)],
    cli_fact: &[String],
    cli_choose: &[(String, Vec<String>)],
) -> Result<(BTreeMap<String, Value>, BTreeSet<Fact>, BTreeMap<(String, String), Vec<String>>), String> {
    let mut state_seeds: BTreeMap<String, String> = route_script.state.into_iter().collect();
    for (k, v) in cli_state {
        state_seeds.insert(k.clone(), v.clone());
    }
    let mut fact_seeds: Vec<String> = route_script.facts;
    for f in cli_fact {
        if !fact_seeds.contains(f) {
            fact_seeds.push(f.clone());
        }
    }
    let mut raw_choose: BTreeMap<String, Vec<String>> = route_script.choose;
    for (k, v) in cli_choose {
        raw_choose.insert(k.clone(), v.clone());
    }
    let choose_resolved = resolve_choose_keys(&raw_choose, owners)?;

    let mut seed_state: BTreeMap<String, Value> = BTreeMap::new();
    for (path, entry) in &union.state {
        if let Some(default) = entry.get("default") {
            if let Some(v) = json_to_value(default) {
                seed_state.insert(path.clone(), v);
            }
        }
    }
    for (path, lit) in &state_seeds {
        seed_state.insert(path.clone(), coerce_literal(union, path, lit));
    }
    let mut seed_facts: BTreeSet<Fact> = BTreeSet::new();
    if let Some(entries) = union.seed_facts.as_array() {
        for e in entries {
            let rel = e.get("relation").and_then(Json::as_str).unwrap_or("");
            let args: Vec<String> = e
                .get("args")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if !rel.is_empty() {
                seed_facts.insert((rel.to_string(), args));
            }
        }
    }
    for f in &fact_seeds {
        if let Some(fact) = parse_ground_fact(f) {
            seed_facts.insert(fact);
        }
    }

    Ok((seed_state, seed_facts, choose_resolved))
}

/// See [`crate::Command::Play`].
#[allow(clippy::too_many_arguments)]
pub fn run_play(
    dir: &Path,
    state: Vec<(String, String)>,
    fact: Vec<String>,
    script: Option<&Path>,
    choose: Vec<(String, Vec<String>)>,
    auto: Option<String>,
    lanes: Option<String>,
    steps: Option<u32>,
    coverage: Vec<PathBuf>,
    json: bool,
) -> ExitCode {
    // Design spec §4.7: `--coverage` replays a whole corpus of route
    // scripts through the chain executor with per-script transcript
    // rendering suppressed — a single playthrough's own `--script`/
    // `--choose`/`--steps` do not compose with that (which script's
    // choices? which step limit, applied to every corpus member?).
    if !coverage.is_empty() && (script.is_some() || !choose.is_empty() || steps.is_some()) {
        eprintln!("lute play: --coverage is exclusive with --script/--choose/--steps (design spec §4.7)");
        return ExitCode::from(2);
    }
    let auto_first = auto.as_deref() == Some("first");
    let lanes_all = lanes.as_deref() == Some("all");

    // Design spec §2: no schedule, no play.
    let (schedule, sched_diags) = match schedule::load_schedule(dir) {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            eprintln!(
                "lute play: {} has no schedule.yaml — `lute play` requires a schedule (design spec §2, docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md)",
                dir.display()
            );
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("lute play: {e}");
            return ExitCode::from(2);
        }
    };
    if !print_sched_diags(dir, &sched_diags) {
        return ExitCode::from(1);
    }

    let artifacts = match compile_project(dir) {
        Ok(a) => a,
        Err(code) => return code,
    };
    let art_json: BTreeMap<String, Json> = artifacts
        .iter()
        .filter_map(|(k, v)| serde_json::to_value(v).ok().map(|j| (k.clone(), j)))
        .collect();

    let union = match build_project_union(&artifacts) {
        Ok(u) => u,
        Err(errs) => {
            for e in &errs {
                eprintln!("lute play: {e}");
            }
            eprintln!("lute play: {} vocabulary conflict(s); refusing to play", errs.len());
            return ExitCode::from(1);
        }
    };

    let route_diags = schedule::route_space_check(&schedule, &union.enums);
    if !print_sched_diags(dir, &route_diags) {
        return ExitCode::from(1);
    }

    // `id -> owning event names`, needed to resolve both this single
    // playthrough's `choose:`/`--choose` and every `--coverage` corpus
    // member's own script `choose:` — computed once here, shared by both.
    let owners = decision_point_events(&schedule, &art_json);

    if !coverage.is_empty() {
        return run_coverage(&schedule, &art_json, &union, &owners, &coverage, &state, &fact, auto_first, json);
    }

    // Route script (state:/facts:/choose:) + CLI flags (design spec §4.4);
    // `build_seeds` owns the merge rule, shared with `--coverage`.
    let route_script = match script {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(text) => match parse_route_script(&text) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("lute play: invalid route script {}: {e}", p.display());
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                eprintln!("lute play: cannot read route script {}: {e}", p.display());
                return ExitCode::from(2);
            }
        },
        None => RouteScript::default(),
    };

    let (seed_state, seed_facts, choose_resolved) =
        match build_seeds(&union, &owners, route_script, &state, &fact, &choose) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lute play: {e}");
                return ExitCode::from(2);
            }
        };

    let (scenes, outcome) =
        execute(&schedule, &art_json, &union, seed_state, seed_facts, &choose_resolved, auto_first, steps);

    if json {
        let v = render_json(&scenes, &outcome);
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!("{}", render_human(&art_json, &scenes, &outcome, lanes_all));
    }

    match outcome {
        Ok(_) => ExitCode::SUCCESS,
        Err(h) => h.exit_code(),
    }
}

// ===========================================================================
// Coverage (design spec §4.7): replay every `--coverage <FILE>` route
// script through the SAME chain executor (§4.2/§4.3) a single `--script`
// playthrough uses — never a second walker — with per-script transcript
// rendering suppressed, and report every placement/variant/hub-choice
// option the corpus as a whole never exercises. The review-gap detector.
// ===========================================================================

/// One `--coverage` corpus member's outcome — enough to report per-script
/// completion without retaining its (possibly large) transcript.
struct CoverageScript {
    path: PathBuf,
    scene_count: usize,
    /// `Ok(end reason)` — the same string [`execute`] returns on a clean
    /// stop (`::end`, clock exhaustion). `Err(halt message)` on ANY
    /// [`PlayHalt`] (every variant, not just the incomplete-tier ones a
    /// single playthrough maps to exit 3) — a corpus member that stops
    /// early for ANY reason contributes only partial coverage, which is
    /// what makes the WHOLE corpus report exit 3, regardless of which
    /// single-play exit tier that particular halt would otherwise carry.
    outcome: Result<String, String>,
}

/// Corpus-wide visitation, keyed by schedule coordinates — a placement's
/// [`Placement::decl_index`] and a variant's index within it, never
/// `event` alone (`(event, lane)`, not `event` alone, is schedule.yaml's
/// own uniqueness key, `E_SCHED_EVENT_DUP`) — plus every hub/choice
/// decision point actually reached by any corpus member, keyed
/// `(event, id)` matching the route-script `choose:` key shape (design
/// spec §4.4).
#[derive(Default)]
struct CoverageAgg {
    presented: BTreeSet<u32>,
    selected: BTreeSet<(u32, usize)>,
    /// Every option id actually OFFERED (present on a `choice`/`hub`
    /// transcript record's original command) across the whole corpus.
    offered: BTreeMap<(String, String), BTreeSet<String>>,
    /// Every option id actually CHOSEN (`chose` on that same record)
    /// across the whole corpus — a strict subset of `offered` unless a
    /// scene halted mid-decision (no `chose` recorded that run).
    chosen: BTreeMap<(String, String), BTreeSet<String>>,
}

impl CoverageAgg {
    /// Fold one presented scene's placement/variant identity and every
    /// `choice`/`hub` transcript record's offered/chosen option ids into
    /// the running corpus-wide aggregate.
    fn record(
        &mut self,
        scene: &SceneRun,
        lookup: &BTreeMap<(String, String), usize>,
        schedule: &Schedule,
        art_json: &BTreeMap<String, Json>,
    ) {
        if let Some(&pidx) = lookup.get(&(scene.event.clone(), scene.lane.to_string())) {
            let placement = &schedule.placements[pidx];
            self.presented.insert(placement.decl_index);
            if let Some(vi) = placement.variants.iter().position(|v| v.doc == scene.doc) {
                self.selected.insert((placement.decl_index, vi));
            }
        }

        let cmd_by_addr: BTreeMap<&str, &Json> = art_json
            .get(&scene.doc)
            .and_then(Json::as_object)
            .and_then(|o| o.get("commands"))
            .and_then(Json::as_array)
            .map(|arr| arr.iter().filter_map(|c| c.get("addr").and_then(Json::as_str).map(|a| (a, c))).collect())
            .unwrap_or_default();
        for rec in &scene.transcript {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("");
            if kind != "choice" && kind != "hub" {
                continue;
            }
            let id_key = if kind == "choice" { "branch" } else { "hub" };
            let Some(id) = rec.get(id_key).and_then(Json::as_str) else { continue };
            let key = (scene.event.clone(), id.to_string());
            let addr = rec.get("addr").and_then(Json::as_str).unwrap_or("");
            if let Some(opts) = cmd_by_addr.get(addr).and_then(|c| c.get("options")).and_then(Json::as_array) {
                let entry = self.offered.entry(key.clone()).or_default();
                for o in opts {
                    if let Some(oid) = o.get("id").and_then(Json::as_str) {
                        entry.insert(oid.to_string());
                    }
                }
            }
            if let Some(chose) = rec.get("chose").and_then(Json::as_str) {
                self.chosen.entry(key).or_default().insert(chose.to_string());
            }
        }
    }
}

/// `(event, lane) -> Schedule::placements` index — schedule.yaml's own
/// uniqueness key (`E_SCHED_EVENT_DUP`), never `event` alone (two lanes may
/// legally share an event name).
fn placement_lookup(schedule: &Schedule) -> BTreeMap<(String, String), usize> {
    schedule.placements.iter().enumerate().map(|(i, p)| ((p.event.clone(), p.lane.clone()), i)).collect()
}

/// `--coverage` entry point (design spec §4.7): replay every corpus member
/// through [`execute`] (transcript suppressed — no `render_human`/
/// `render_json` call per member), fold each into [`CoverageAgg`], then
/// report the corpus-wide gap. Exit `0` full coverage, `1` a gap remains,
/// `2` an I/O/usage failure (a corpus member's file/parse error), `3` at
/// least one corpus member halted before completion.
#[allow(clippy::too_many_arguments)]
fn run_coverage(
    schedule: &Schedule,
    art_json: &BTreeMap<String, Json>,
    union: &ProjectUnion,
    owners: &BTreeMap<String, BTreeSet<String>>,
    coverage: &[PathBuf],
    cli_state: &[(String, String)],
    cli_fact: &[String],
    auto_first: bool,
    json: bool,
) -> ExitCode {
    let lookup = placement_lookup(schedule);
    let mut agg = CoverageAgg::default();
    let mut scripts: Vec<CoverageScript> = Vec::new();
    let no_choose: Vec<(String, Vec<String>)> = Vec::new();

    for path in coverage {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lute play: cannot read route script {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let route_script = match parse_route_script(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lute play: invalid route script {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let (seed_state, seed_facts, choose_resolved) =
            match build_seeds(union, owners, route_script, cli_state, cli_fact, &no_choose) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("lute play: {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            };
        let (scenes, outcome) =
            execute(schedule, art_json, union, seed_state, seed_facts, &choose_resolved, auto_first, None);
        for scene in &scenes {
            agg.record(scene, &lookup, schedule, art_json);
        }
        scripts.push(CoverageScript {
            path: path.clone(),
            scene_count: scenes.len(),
            outcome: outcome.map_err(|h| h.message()),
        });
    }

    let mut uncovered_placements: Vec<&Placement> = Vec::new();
    let mut uncovered_variants: Vec<(&Placement, &schedule::Variant)> = Vec::new();
    for p in &schedule.placements {
        if !agg.presented.contains(&p.decl_index) {
            uncovered_placements.push(p);
        }
        for (vi, v) in p.variants.iter().enumerate() {
            if !agg.selected.contains(&(p.decl_index, vi)) {
                uncovered_variants.push((p, v));
            }
        }
    }
    let mut uncovered_options: Vec<((String, String), Vec<String>)> = Vec::new();
    for (key, offered) in &agg.offered {
        let chosen = agg.chosen.get(key);
        let mut missing: Vec<String> =
            offered.iter().filter(|o| !chosen.map(|c| c.contains(*o)).unwrap_or(false)).cloned().collect();
        if !missing.is_empty() {
            missing.sort();
            uncovered_options.push((key.clone(), missing));
        }
    }

    let any_incomplete = scripts.iter().any(|s| s.outcome.is_err());
    let any_uncovered = !uncovered_placements.is_empty() || !uncovered_variants.is_empty() || !uncovered_options.is_empty();
    let exit = if any_incomplete {
        ExitCode::from(3)
    } else if any_uncovered {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    };

    if json {
        let v = render_coverage_json(schedule, &scripts, &uncovered_placements, &uncovered_variants, &uncovered_options, &agg);
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!(
            "{}",
            render_coverage_human(schedule, &scripts, &uncovered_placements, &uncovered_variants, &uncovered_options, &agg)
        );
    }
    exit
}

#[allow(clippy::too_many_arguments)]
fn render_coverage_human(
    schedule: &Schedule,
    scripts: &[CoverageScript],
    uncovered_placements: &[&Placement],
    uncovered_variants: &[(&Placement, &schedule::Variant)],
    uncovered_options: &[((String, String), Vec<String>)],
    agg: &CoverageAgg,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("lute play coverage: {} script(s) replayed\n", scripts.len()));
    for s in scripts {
        match &s.outcome {
            Ok(reason) => out.push_str(&format!("  \u{2713} {}: {reason} ({} scene(s))\n", s.path.display(), s.scene_count)),
            Err(msg) => out.push_str(&format!(
                "  \u{2717} {}: INCOMPLETE \u{2014} {msg} ({} scene(s) before halt)\n",
                s.path.display(),
                s.scene_count
            )),
        }
    }
    out.push('\n');

    let total_placements = schedule.placements.len();
    let total_variants: usize = schedule.placements.iter().map(|p| p.variants.len()).sum();
    let total_option_instances: usize = agg.offered.values().map(BTreeSet::len).sum();
    let uncovered_option_instances: usize = uncovered_options.iter().map(|(_, m)| m.len()).sum();

    out.push_str(&format!(
        "placements: {}/{total_placements} presented\n",
        total_placements - uncovered_placements.len()
    ));
    for p in uncovered_placements {
        out.push_str(&format!("  \u{2717} never presented: `{}` ({} lane)\n", p.event, p.lane));
    }

    out.push_str(&format!(
        "variants: {}/{total_variants} selected\n",
        total_variants - uncovered_variants.len()
    ));
    for (p, v) in uncovered_variants {
        out.push_str(&format!("  \u{2717} never selected: `{}` variant `{}`\n", p.event, variant_label_of(&v.doc)));
    }

    out.push_str(&format!(
        "hub/choice options: {}/{total_option_instances} chosen\n",
        total_option_instances - uncovered_option_instances
    ));
    for ((event, id), missing) in uncovered_options {
        out.push_str(&format!("  \u{2717} never chosen: `{event}/{id}` \u{2014} {}\n", missing.join(", ")));
    }

    let verdict = if scripts.iter().any(|s| s.outcome.is_err()) {
        "INCOMPLETE \u{2014} at least one script halted before completion; its coverage contribution is partial"
    } else if !uncovered_placements.is_empty() || !uncovered_variants.is_empty() || !uncovered_options.is_empty() {
        "UNCOVERED \u{2014} gap(s) remain"
    } else {
        "COVERED"
    };
    out.push_str(&format!("\u{2500}\u{2500} {verdict} \u{2500}\u{2500}\n"));
    out
}

fn render_coverage_json(
    schedule: &Schedule,
    scripts: &[CoverageScript],
    uncovered_placements: &[&Placement],
    uncovered_variants: &[(&Placement, &schedule::Variant)],
    uncovered_options: &[((String, String), Vec<String>)],
    agg: &CoverageAgg,
) -> Json {
    let total_placements = schedule.placements.len();
    let total_variants: usize = schedule.placements.iter().map(|p| p.variants.len()).sum();
    let total_option_instances: usize = agg.offered.values().map(BTreeSet::len).sum();
    let uncovered_option_instances: usize = uncovered_options.iter().map(|(_, m)| m.len()).sum();

    let any_incomplete = scripts.iter().any(|s| s.outcome.is_err());
    let any_uncovered = !uncovered_placements.is_empty() || !uncovered_variants.is_empty() || !uncovered_options.is_empty();
    let exit = if any_incomplete {
        "incomplete"
    } else if any_uncovered {
        "uncovered"
    } else {
        "covered"
    };

    json!({
        "scripts": scripts.iter().map(|s| json!({
            "path": s.path.to_string_lossy(),
            "scenes": s.scene_count,
            "complete": s.outcome.is_ok(),
            "outcome": match &s.outcome {
                Ok(r) => r.clone(),
                Err(m) => m.clone(),
            },
        })).collect::<Vec<_>>(),
        "placements": {
            "total": total_placements,
            "presented": total_placements - uncovered_placements.len(),
            "uncovered": uncovered_placements.iter().map(|p| json!({ "event": p.event, "lane": p.lane })).collect::<Vec<_>>(),
        },
        "variants": {
            "total": total_variants,
            "selected": total_variants - uncovered_variants.len(),
            "uncovered": uncovered_variants.iter().map(|(p, v)| json!({
                "event": p.event,
                "lane": p.lane,
                "doc": v.doc,
            })).collect::<Vec<_>>(),
        },
        "options": {
            "total": total_option_instances,
            "chosen": total_option_instances - uncovered_option_instances,
            "uncovered": uncovered_options.iter().map(|((event, id), missing)| json!({
                "event": event,
                "id": id,
                "missing": missing,
            })).collect::<Vec<_>>(),
        },
        "exit": exit,
    })
}

/// Print every [`SchedDiag`], warnings to stderr and errors to stderr too
/// (this doc kind has no span to anchor a stdout diagnostic line against,
/// mirroring `crate::manifests::spanless_line`'s own convention for the same
/// reason). Returns `false` when any Error-severity diagnostic is present —
/// the caller's exit-1 gate signal.
fn print_sched_diags(dir: &Path, diags: &[SchedDiag]) -> bool {
    let mut ok = true;
    for d in diags {
        let sev = if d.severity == Severity::Error {
            ok = false;
            "error"
        } else {
            "warning"
        };
        eprintln!("{}: {sev} [{}] {}", dir.join("schedule.yaml").display(), d.code, d.message);
    }
    ok
}
