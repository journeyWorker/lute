//! The [`World`] a playthrough carries from one step to the next: its
//! seeding from a save, the clock's position, the exclusivity check and the
//! [`WorldView`] expectations judge, and the value and fact renderings.

use std::collections::{BTreeMap, BTreeSet};

use lute_compile::index::BeatKind;
use serde_json::{json, Value as Json};

use super::present::{share_of, spend_shared};
use super::project::ExecProject;
use super::resolve::{ever_read_path, resolve_bridges, resolve_fact, resolve_state};
use super::walk::PlayDriver;
use crate::datalog::Fact;
use crate::exec::{BridgeQueues, Carry, Machine, Seed};
use crate::{MockSet, Value};

/// Everything that carries from one step to the next.
#[derive(Clone, Default)]
pub struct World {
    /// Persistent-tier state (`run.*`/`user.*`/`app.*`/`quest.*`/`entry.*`);
    /// `scene.*` never lives here — it resets at every scene boundary.
    /// Carries `entry.<id>.everRead` for every entry (dsl 0.22.0 §7).
    pub state: BTreeMap<String, Value>,
    /// Base facts (the runner derives over them).
    pub facts: BTreeSet<Fact>,
    /// quest id -> `unset`/`active`/`complete`/`failed`.
    pub quests: BTreeMap<String, String>,
    /// Canonical ids of every presented scene — the `visited(…)` set both
    /// `after:` and CEL `visited('<id>')` read (dsl 0.21.0 §7a.1).
    pub visited: BTreeSet<String>,
    /// Scene beats presented since the last `newRun` (`once: run`) — and
    /// (dsl 0.25.0 §2) every beat, entries included, of a `share` key one of
    /// them spent this run.
    pub spent_run: BTreeSet<String>,
    /// Scene beats presented in this play (`once: user`), and every beat of
    /// a `share` key one of them spent.
    pub spent_user: BTreeSet<String>,
    /// dsl 0.24.0 §1: where on the clock each beat was last presented (an
    /// entry: read) — what spends `once: day` / `once: slot`; a presented
    /// shared beat records every beat of its key (dsl 0.25.0 §2).
    pub spent_at: BTreeMap<String, lute_manifest::clock::ClockAt>,
    /// dsl 0.25.0 §2: `share` key → the beat whose presentation last spent
    /// it, for the reason a spent sibling gives.
    pub share_spent_by: BTreeMap<String, String>,
    /// Quest ids `accept` records named since the last quest advance (dsl
    /// 0.21.0 §7a.3) — the next advance activates those still `unset`.
    pub accepts: Vec<String>,
    /// dsl 0.24.0 §2: quest ids `::accept{… at="nextRun"}` queued — handed
    /// to `accepts` right after the next `newRun` reset.
    pub next_run_accepts: Vec<String>,
    /// Per `<branch>` id: the decisions of a multi-decision `choose:` list
    /// earlier presentations consumed ([`ScriptedChoices::cursor`]).
    ///
    /// [`ScriptedChoices::cursor`]: crate::exec::ScriptedChoices::cursor
    pub choice_cursor: BTreeMap<String, usize>,
    /// The script-wide `choose:` every presentation is scripted by (a
    /// step's own `choose:` replaces it key by key, dsl 0.22.0 §2).
    pub choose: BTreeMap<String, Vec<String>>,
    /// `Some(false)` under `--no-derive` / `derive: false` (dsl 0.22.0 §6):
    /// handed to every runner's mock.
    pub derive: Option<bool>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids a `by` deadline failed —
    /// carried to every quest advance so a failed objective stays failed.
    pub failed_objectives: BTreeSet<String>,
    /// dsl 0.24.0 §5: the bridge answers not yet consumed — the running
    /// step's own, then the script's top-level ones; every presentation and
    /// quest advance hands them to its [`PlayDriver`] and takes back the rest.
    pub bridges: BridgeQueues,
    /// dsl 0.24.0 §2.1: the raise (`name` / `name@target`) the running step
    /// makes, until it is made — the settles before it defer the `by` of
    /// the `on=` objectives it judges ([`Machine::with_deferred_by`]).
    pub defer_by: Option<String>,
    /// dsl 0.24.0 §2: `true` while a `judge: before` raise is answered —
    /// the `<on>` handlers it fires are collected in `deferred_handlers`
    /// (`(quest document, body addr)`, firing order) and run after the
    /// occasion's beats ([`run_deferred_handlers`](super::run_deferred_handlers)).
    pub defer_handlers: bool,
    pub deferred_handlers: Vec<(String, String)>,
    /// dsl 0.27.0 §4 (T2-5): a finite clock raised its last `dayEnd` and
    /// stopped — every later `advance:` is `E-CLOCK-END`. Cleared by a
    /// `newRun`, which starts the clock over.
    pub clock_ended: bool,
    /// dsl 0.27.0 §5: the seasons' and rearms' last observed conditions and
    /// the season-scoped spends.
    pub cadence: crate::exec::cadence::Cadence,
}

impl World {
    /// A mock carrying the playthrough's derive setting.
    pub fn mock(&self) -> MockSet {
        MockSet {
            derive: self.derive,
            ..MockSet::default()
        }
    }

    /// The world as a [`Machine::resume`] carry: its state, facts, quests.
    pub fn carry(&self) -> Carry {
        Carry::world(self.state.clone(), self.facts.clone(), self.quests.clone())
    }

    /// A Machine over `art` resumed from this world that only evaluates
    /// (a `when`, the fact closure) — it never walks, so its driver has no
    /// script.
    pub fn evaluator(&self, art: &Json) -> Machine<PlayDriver> {
        Machine::resume(
            art,
            Seed::from(&self.mock()),
            self.carry(),
            PlayDriver::default(),
        )
    }
}

pub fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => json!(*n as i64),
        Value::Num(n) => json!(n),
        Value::Str(s) => Json::String(s.clone()),
        Value::Unknown => Json::Null,
    }
}

/// Parse a ground `"rel(a, b)"` fact.
pub fn parse_ground_fact(s: &str) -> Option<Fact> {
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

/// The lifecycle values `quest.<id>.state` takes (always assigned: a quest
/// nothing has activated yet is `unset`).
pub const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The quest id of a `quest.<id>.state` path.
pub fn quest_state_id(path: &str) -> Option<&str> {
    path.strip_prefix("quest.")?
        .strip_suffix(".state")
        .filter(|id| !id.is_empty() && !id.contains('.'))
}

/// Register a save's quest status (a `quests:` entry or a `quest.<id>.state`
/// seed) so the start settle resumes it instead of starting the quest over.
pub fn seed_quest(
    p: &ExecProject,
    w: &mut World,
    at: &str,
    id: &str,
    status: &str,
) -> Result<(), String> {
    if !p.quest_objectives.contains_key(id) {
        let declared: Vec<&str> = p.quest_objectives.keys().map(String::as_str).collect();
        return Err(format!(
            "{at}: no quest `{id}` is declared in this project (quests: {})",
            if declared.is_empty() {
                "none".to_string()
            } else {
                declared.join(", ")
            }
        ));
    }
    if !QUEST_STATES.contains(&status) {
        return Err(format!(
            "{at}: `{status}` — a quest state is one of {}",
            QUEST_STATES.join(", ")
        ));
    }
    w.quests.insert(id.to_string(), status.to_string());
    w.state
        .insert(format!("quest.{id}.state"), Value::Str(status.to_string()));
    Ok(())
}

/// `Err` naming an id a save seed names that the project does not declare,
/// with a did-you-mean.
pub fn unknown_id<'a>(
    at: &str,
    id: &str,
    what: &str,
    known: impl Iterator<Item = &'a str>,
) -> String {
    let hint = lute_manifest::suggest::nearest(id, known, 2)
        .map(|k| format!(" — did you mean `{k}`?"))
        .unwrap_or_default();
    format!("{at} names `{id}`, which is no {what} in this project{hint}")
}

/// The save a playthrough starts from (dsl 0.22.0 §3), as written.
#[derive(Default)]
pub struct SaveSeed {
    pub visited: Vec<String>,
    pub presented_user: Vec<String>,
    pub presented_run: Vec<String>,
    pub quests: Vec<(String, String)>,
    pub entries_run: Vec<String>,
    pub entries_user: Vec<String>,
}

/// Everything a world is seeded from: the trace-mock surfaces (`state:`,
/// `facts:`, `choose:`, `bridges:`), the save, and the `derive:` setting.
pub struct WorldSeed<'a> {
    pub surfaces: &'a MockSet,
    pub save: &'a SaveSeed,
    pub derive: Option<bool>,
}

/// The playthrough's starting world: every declared default (scene tier
/// excluded; `entry.<id>.everRead` false for every entry), the script's
/// `state:` over it, the save seeds (dsl 0.22.0 §3: `visited:`,
/// `presented:`, `quests:`, `entriesRead:`), the project's seed facts plus
/// the script's `facts:`. A `quest.<id>.state` seed is a `quests:` entry.
/// A seed naming an undeclared path, id, quest or relation — or a value that
/// does not fit — is a usage error, never a silent no-op.
pub fn seed_world(p: &ExecProject, seed: &WorldSeed<'_>) -> Result<World, String> {
    let mut w = World {
        state: BTreeMap::new(),
        facts: p.seed_facts.clone(),
        quests: BTreeMap::new(),
        visited: BTreeSet::new(),
        spent_run: BTreeSet::new(),
        spent_user: BTreeSet::new(),
        spent_at: BTreeMap::new(),
        share_spent_by: BTreeMap::new(),
        accepts: Vec::new(),
        next_run_accepts: Vec::new(),
        choice_cursor: BTreeMap::new(),
        choose: seed.surfaces.choose.clone(),
        derive: None,
        failed_objectives: BTreeSet::new(),
        bridges: BridgeQueues {
            top: resolve_bridges(p, "top level", &seed.surfaces.bridges)?,
            step: BTreeMap::new(),
        },
        defer_by: None,
        defer_handlers: false,
        deferred_handlers: Vec::new(),
        clock_ended: false,
        cadence: Default::default(),
    };
    for (path, e) in &p.state_table {
        if path.starts_with("scene.") {
            continue;
        }
        if let Some(v) = e.get("default").and_then(json_to_value) {
            w.state.insert(path.clone(), v);
        }
    }
    for id in &p.entry_ids {
        w.state.insert(ever_read_path(id), Value::Bool(false));
    }
    for (path, lit, _) in &seed.surfaces.state {
        let at = format!("`state.{path}`");
        if let Some(id) = quest_state_id(path) {
            seed_quest(p, &mut w, &at, id, lit)?;
            continue;
        }
        let v = resolve_state(p, path, lit).map_err(|e| format!("{at} {e}"))?;
        w.state.insert(path.clone(), v);
    }
    let save = seed.save;
    for (id, status) in &save.quests {
        seed_quest(p, &mut w, &format!("`quests.{id}`"), id, status)?;
    }
    for id in &save.visited {
        if !p.scene_ids.contains(id) {
            return Err(unknown_id(
                "`visited:`",
                id,
                "scene",
                p.scene_ids.iter().map(String::as_str),
            ));
        }
        w.visited.insert(id.clone());
    }
    for (ids, tier) in [(&save.presented_run, "run"), (&save.presented_user, "user")] {
        let at = format!("`presented.{tier}`");
        for id in ids {
            match p.index.beats.iter().find(|b| &b.id == id).map(|b| b.kind) {
                Some(BeatKind::Scene | BeatKind::Bundle) => {}
                Some(BeatKind::Entry) => {
                    return Err(format!(
                        "{at} names entry `{id}` — an entry's read history is `entriesRead:`"
                    ))
                }
                None => {
                    return Err(unknown_id(
                        &at,
                        id,
                        "scene beat",
                        p.index
                            .beats
                            .iter()
                            .filter(|b| matches!(b.kind, BeatKind::Scene | BeatKind::Bundle))
                            .map(|b| b.id.as_str()),
                    ))
                }
            }
            // A beat presented this run was also presented ever, and a
            // presented scene is a visited one — as a live presentation
            // records it (spending its `share` key, dsl 0.25.0 §2).
            spend_shared(p, &mut w, id, tier == "run");
            w.visited.insert(id.clone());
        }
    }
    for (ids, tier) in [(&save.entries_run, "run"), (&save.entries_user, "user")] {
        for id in ids {
            // dsl 0.26.0 §7 (T3-10): `<doc>.<entry>` names the entry too.
            let id = &p.entry_id(id).to_string();
            if !p.entry_ids.contains(id) {
                return Err(unknown_id(
                    &format!("`entriesRead.{tier}`"),
                    id,
                    "entry",
                    p.entry_ids.iter().map(String::as_str),
                ));
            }
            // Read this run ⇒ read ever.
            if tier == "run" {
                w.state
                    .insert(format!("entry.{id}.read"), Value::Bool(true));
            }
            w.state.insert(ever_read_path(id), Value::Bool(true));
            if share_of(p, id).is_some() {
                spend_shared(p, &mut w, id, tier == "run");
            }
        }
    }
    for f in &seed.surfaces.facts {
        let fact = resolve_fact(p, f).map_err(|e| format!("`facts:` entry `{f}` {e}"))?;
        w.facts.insert(fact);
    }
    w.derive = seed.derive.filter(|d| !d);
    Ok(w)
}

/// `rel(a, b)` — the runner's rendering of a ground fact.
pub fn render_fact((rel, args): &Fact) -> String {
    format!("{rel}({})", args.join(", "))
}

/// dsl 0.24.0 §1: the declared clock's position in `w` — `None` without a
/// clock, or while its day/slot do not name one.
pub fn clock_at(p: &ExecProject, w: &World) -> Option<lute_manifest::clock::ClockAt> {
    crate::clock::position(p.index.clock.as_ref()?, &w.state)
}

/// dsl 0.24.0 §1: re-derive the reserved `clock.*` values from the live
/// day/slot after a write the runner did not make.
pub fn refresh_clock(p: &ExecProject, w: &mut World) {
    if let Some(clock) = &p.index.clock {
        crate::clock::refresh(clock, &mut w.state);
    }
}

/// dsl 0.25.0 §1: every pair of facts that hold together in `w` (after
/// derivation) although their relations exclude each other, rendered
/// `seenAfter(elias) and fell(elias) both hold`. Derives only when the
/// project declares an exclusion.
pub fn exclusive_violations(p: &ExecProject, w: &World) -> Vec<String> {
    let pairs: Vec<(&str, &str)> = p
        .index
        .relations
        .iter()
        .flat_map(|r| {
            r.excludes
                .iter()
                .filter(|o| r.name.as_str() < o.as_str())
                .map(|o| (r.name.as_str(), o.as_str()))
        })
        .collect();
    if pairs.is_empty() {
        return Vec::new();
    }
    let evaluator = w.evaluator(&p.eval_json);
    let facts = evaluator.all_facts();
    let mut out = Vec::new();
    for (a, b) in pairs {
        for (_, args) in facts.iter().filter(|(rel, _)| rel == a) {
            if facts.contains(&(b.to_string(), args.clone())) {
                out.push(format!(
                    "{} and {} both hold",
                    render_fact(&(a.to_string(), args.clone())),
                    render_fact(&(b.to_string(), args.clone()))
                ));
            }
        }
    }
    out
}

/// The world at one moment of a play: the effective state, every fact that
/// holds after derivation (rendered `rel(a, b)`), every declared quest's
/// status, and the declared clock's position.
#[derive(Clone, Debug, Default)]
pub struct WorldView {
    pub state: BTreeMap<String, Value>,
    pub facts: BTreeSet<String>,
    pub quests: BTreeMap<String, String>,
    /// `None` without a declared clock, or while its day/slot paths name no
    /// position on it.
    pub clock: Option<ClockView>,
}

/// Where the declared clock stands (dsl 0.26.0 §7, T2-5: step
/// `expect.clock`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClockView {
    pub day: i64,
    /// The slot's name — `None` on a day-granular clock.
    pub slot: Option<String>,
    /// `clock.weekday` — `None` without a `week:`.
    pub weekday: Option<i64>,
    /// `clock.weekdayLabel` — `None` without week labels.
    pub weekday_label: Option<String>,
}

/// The world `w` as expectations judge it: the effective state, every
/// declared quest's status, and — when `with_facts` — every fact that holds
/// after derivation (a fixpoint, so computed only on request). The reserved
/// quest reads the lifecycle writes only on a transition read as an engine
/// reads them before it (dsl 0.24.0 §2): `failedBy` `unset`, an objective's
/// `done` and `failed` `false`.
pub fn world_view(p: &ExecProject, w: &World, with_facts: bool) -> WorldView {
    let facts = if with_facts {
        w.evaluator(&p.eval_json)
            .all_facts()
            .iter()
            .map(render_fact)
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut state = w.state.clone();
    let mut quests = BTreeMap::new();
    for (id, objectives) in &p.quest_objectives {
        let status = w.quests.get(id).map_or("unset", String::as_str);
        quests.insert(id.clone(), status.to_string());
        state
            .entry(format!("quest.{id}.failedBy"))
            .or_insert_with(|| Value::Str("unset".to_string()));
        for oid in objectives {
            for flag in ["done", "failed"] {
                state
                    .entry(format!("quest.{id}.objectives.{oid}.{flag}"))
                    .or_insert(Value::Bool(false));
            }
        }
    }
    // dsl 0.26.0 §7 (T2-5): where the clock stands, for step `expect.clock`.
    let clock = p.index.clock.as_ref().and_then(|decl| {
        let at = clock_at(p, w)?;
        Some(ClockView {
            day: at.day,
            slot: decl.slot_name(at.slot).map(str::to_string),
            weekday: decl.weekday(at.day),
            weekday_label: decl.weekday_label(at.day).map(str::to_string),
        })
    });
    WorldView {
        state,
        facts,
        quests,
        clock,
    }
}
