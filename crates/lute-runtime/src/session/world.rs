//! The [`World`] a playthrough carries from one step to the next: its
//! seeding from a save, the clock's position, the exclusivity check and the
//! [`WorldView`] expectations judge, and the value and fact renderings.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::index::BeatKind;
use serde_json::{json, Value as Json};

use super::present::{share_of, spend_shared};
use super::project::ExecProject;
use super::resolve::{ever_read_path, resolve_fact, resolve_state};
use crate::datalog::Fact;
use crate::{
    BridgeCall, BridgeReply, Carry, Driver, EvalSnapshot, Forced, Machine, Menu, MockSet, OnUnknown,
    Pick, Seed, UnknownSite, UnresolvedAtom, Value,
};

/// A shared observer for every CEL evaluation made during a play.
///
/// The callback is invoked synchronously while the machine still owns its
/// evaluation snapshot; callers must not retain the snapshot references.
pub type SessionEvalObserver =
    Rc<dyn for<'a> Fn(&str, &Value, &[UnresolvedAtom], EvalSnapshot<'a>)>;
/// A driver used only by evaluator machines; it cannot make a play decision.
#[derive(Default)]
pub struct NoDecisionDriver;

impl Driver for NoDecisionDriver {
    fn choose(&mut self, _menu: &Menu<'_>) -> Pick {
        Pick::Unscripted { scripted: 0 }
    }
    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, _verdict: &crate::Verdict) -> Forced {
        Forced::Refuse
    }
    fn bridge(&mut self, _call: &BridgeCall<'_>) -> BridgeReply {
        BridgeReply::Unanswered
    }
    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        OnUnknown::Halt
    }
    fn emit(&mut self, _rec: Json) {}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Persistent-tier state (`run.*`/`user.*`/`app.*`/`quest.*`/`entry.*`);
    /// `scene.*` never lives here — it resets at every scene boundary.
    /// Carries `entry.<id>.everRead` for every entry (dsl 0.22.0 §7).
    pub state: BTreeMap<String, Value>,
    /// Base facts (the runner derives over them).
    pub facts: BTreeSet<Fact>,
    /// quest id -> `unset`/`active`/`complete`/`failed`.
    pub quests: BTreeMap<String, String>,
    /// Save-wide quest instance number, keyed by quest id. Reset paths
    /// increment this map but never clear it.
    pub quest_instances: BTreeMap<String, u64>,
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
    /// `Some(false)` under `--no-derive` / `derive: false` (dsl 0.22.0 §6):
    /// handed to every runner's mock.
    pub derive: Option<bool>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids a `by` deadline failed —
    /// carried to every quest advance so a failed objective stays failed.
    pub failed_objectives: BTreeSet<String>,
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
    /// dsl 0.31.0 §1: the last presentation moved the clock because its
    /// beat declared `advances`; the play driver uses this to explain an
    /// immediately following explicit `advance:` without suppressing it.
    pub clock_advanced_by_beat: bool,
    /// Number of nested `advances:` beat presentations in the current
    /// clock-raise cascade. A declared beat can raise another declared beat;
    /// this bounds a malformed cycle without changing ordinary play steps.
    pub advance_cascade_depth: usize,
    /// dsl 0.27.0 §5: the seasons' and rearms' last observed conditions and
    /// the season-scoped spends.
    pub cadence: crate::cadence::Cadence,
}

impl World {
    /// A mock carrying the playthrough's derive setting.
    pub fn mock(&self) -> MockSet {
        MockSet {
            derive: self.derive,
            ..MockSet::default()
        }
    }

    /// The world as a [`Machine::resume`] carry: its state, facts, quests and
    /// save-wide quest instance counters.
    pub fn carry(&self) -> Carry {
        let mut carry = Carry::world(self.state.clone(), self.facts.clone(), self.quests.clone());
        carry.quest_instances = self.quest_instances.clone();
        carry
    }

    /// A Machine over `art` resumed from this world that only evaluates
    /// (a `when`, the fact closure) — it never walks, so its driver has no
    /// script.
    pub fn evaluator(&self, art: &Json) -> Machine<NoDecisionDriver> {
        Machine::resume(
            art,
            Seed::from(&self.mock()),
            self.carry(),
            NoDecisionDriver,
        )
    }
    /// Project evaluator using the schema decoded once by `ExecProject`.
    pub(crate) fn evaluator_with_schema(
        &self,
        art: &Json,
        schema: std::sync::Arc<crate::store::StoreSchema>,
        observer: Option<SessionEvalObserver>,
    ) -> Machine<NoDecisionDriver> {
        let machine = Machine::resume_with_project_schema(
            art,
            std::sync::Arc::new(crate::machine::Code::of(art)),
            Seed::from(&self.mock()),
            self.carry(),
            NoDecisionDriver,
            schema,
        );
        let Some(observer) = observer else {
            return machine;
        };
        machine.with_eval_observer(move |raw, value, atoms, snapshot| {
            observer(raw, value, atoms, snapshot);
        })
    }
}

pub fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_i64().map(Value::Int).or_else(|| n.as_f64().map(Value::Double)),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) => json!(*n),
        Value::Double(n) => json!(*n),
        Value::Str(s) => Json::String(s.clone()),
        Value::Unknown | Value::Error(_) => Json::Null,
    }
}

/// Parse a ground `"rel(a, b)"` fact. A quoted argument (`at("lab-b2")`,
/// `at('lab-b2')`) is the same name written bare.
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
        inner
            .split(',')
            .map(|a| unquote(a.trim()).to_string())
            .collect()
    };
    Some((rel.to_string(), args))
}

/// `"x"` / `'x'` → `x`; anything else as is.
fn unquote(a: &str) -> &str {
    [b'"', b'\'']
        .iter()
        .find_map(|&q| {
            let b = a.as_bytes();
            (b.len() >= 2 && b[0] == q && b[b.len() - 1] == q).then(|| &a[1..a.len() - 1])
        })
        .unwrap_or(a)
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
    /// Save-wide quest instance counters, keyed by quest id.
    pub quest_instances: Vec<(String, u64)>,
    pub entries_run: Vec<String>,
    pub entries_user: Vec<String>,
}

/// Everything a world is seeded from: the trace-mock surfaces (`state:`,
/// `facts:`, `choose:`, `bridges:`), the save, and the `derive:` setting.
pub struct WorldSeed<'a, B = (), C = ()> {
    pub surfaces: &'a MockSet<B, C>,
    pub save: &'a SaveSeed,
    pub derive: Option<bool>,
}

/// One seed of a save the project cannot take: the script key path it was
/// written at (`["state", "run.day"]`; `["facts"]` with the list item) and
/// why — a usage error the script locates there.
#[derive(Clone, Debug, PartialEq)]
pub struct SeedError {
    pub keys: Vec<String>,
    pub item: Option<usize>,
    pub msg: String,
}

impl SeedError {
    fn at(keys: &[&str], msg: String) -> Self {
        SeedError {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            item: None,
            msg,
        }
    }

    fn item(keys: &[&str], item: usize, msg: String) -> Self {
        SeedError {
            item: Some(item),
            ..SeedError::at(keys, msg)
        }
    }
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

/// The playthrough's starting world: every declared default (scene tier
/// excluded; `entry.<id>.everRead` false for every entry), the script's
/// `state:` over it, the save seeds (`visited:`, `presented:`, `quests:`,
/// `entriesRead:`), the project's seed facts plus the script's `facts:`. A
/// `quest.<id>.state` seed is a `quests:` entry. A `visited:` scene was
/// presented, so its `once: user` is spent — a save cannot hold one without
/// the other. A seed naming an undeclared path, id, quest or relation — or a
/// value that does not fit — is a usage error, never a silent no-op; every
/// one is reported.
pub fn seed_world<B, C>(
    p: &ExecProject,
    seed: &WorldSeed<'_, B, C>,
) -> Result<World, Vec<SeedError>> {
    let mut errs: Vec<SeedError> = Vec::new();
    let mut w = World {
        state: BTreeMap::new(),
        facts: p.seed_facts.clone(),
        quests: BTreeMap::new(),
        quest_instances: BTreeMap::new(),
        visited: BTreeSet::new(),
        spent_run: BTreeSet::new(),
        spent_user: BTreeSet::new(),
        spent_at: BTreeMap::new(),
        share_spent_by: BTreeMap::new(),
        accepts: Vec::new(),
        next_run_accepts: Vec::new(),
        derive: None,
        failed_objectives: BTreeSet::new(),
        defer_by: None,
        defer_handlers: false,
        deferred_handlers: Vec::new(),
        clock_advanced_by_beat: false,
        advance_cascade_depth: 0,
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
            if let Err(e) = seed_quest(p, &mut w, &at, id, lit) {
                errs.push(SeedError::at(&["state", path], e));
            }
            continue;
        }
        match resolve_state(p, path, lit) {
            Ok(v) => {
                w.state.insert(path.clone(), v);
            }
            Err(e) => errs.push(SeedError::at(&["state", path], format!("{at} {e}"))),
        }
    }
    let save = seed.save;
    for (id, status) in &save.quests {
        if let Err(e) = seed_quest(p, &mut w, &format!("`quests.{id}`"), id, status) {
            errs.push(SeedError::at(&["quests", id], e));
        }
    }
    for (id, count) in &save.quest_instances {
        if !p.quest_objectives.contains_key(id) {
            errs.push(SeedError::at(
                &["questInstances", id],
                unknown_id(
                    "`questInstances:`",
                    id,
                    "quest",
                    p.quest_objectives.keys().map(String::as_str),
                ),
            ));
        } else if *count == 0 {
            errs.push(SeedError::at(
                &["questInstances", id],
                "a quest instance counter must be at least 1".to_string(),
            ));
        } else {
            w.quest_instances.insert(id.clone(), *count);
        }
    }
    for (i, id) in save.visited.iter().enumerate() {
        if !p.scene_ids.contains(id) {
            errs.push(SeedError::item(
                &["visited"],
                i,
                unknown_id(
                    "`visited:`",
                    id,
                    "scene",
                    p.scene_ids.iter().map(String::as_str),
                ),
            ));
            continue;
        }
        // Visiting a scene presented it: its `once: user` (and its `share`
        // key's) is spent, as a live presentation spends it.
        spend_shared(p, &mut w, id, None, false);
        w.visited.insert(id.clone());
    }
    for (ids, tier) in [(&save.presented_run, "run"), (&save.presented_user, "user")] {
        let at = format!("`presented.{tier}`");
        for (i, id) in ids.iter().enumerate() {
            match p.index.beats.iter().find(|b| &b.id == id).map(|b| b.kind) {
                Some(BeatKind::Scene | BeatKind::Bundle) => {}
                Some(BeatKind::Entry) => {
                    errs.push(SeedError::item(
                        &["presented", tier],
                        i,
                        format!(
                            "{at} names entry `{id}` — an entry's read history is `entriesRead:`"
                        ),
                    ));
                    continue;
                }
                None => {
                    errs.push(SeedError::item(
                        &["presented", tier],
                        i,
                        unknown_id(
                            &at,
                            id,
                            "scene beat",
                            p.index
                                .beats
                                .iter()
                                .filter(|b| matches!(b.kind, BeatKind::Scene | BeatKind::Bundle))
                                .map(|b| b.id.as_str()),
                        ),
                    ));
                    continue;
                }
            }
            // A beat presented this run was also presented ever, and a
            // presented scene is a visited one — as a live presentation
            // records it (spending its `share` key).
            spend_shared(p, &mut w, id, None, tier == "run");
            w.visited.insert(id.clone());
        }
    }
    for (ids, tier) in [(&save.entries_run, "run"), (&save.entries_user, "user")] {
        for (i, id) in ids.iter().enumerate() {
            // `<doc>.<entry>` names the entry too.
            let id = &p.entry_id(id).to_string();
            if !p.entry_ids.contains(id) {
                errs.push(SeedError::item(
                    &["entriesRead", tier],
                    i,
                    unknown_id(
                        &format!("`entriesRead.{tier}`"),
                        id,
                        "entry",
                        p.entry_ids.iter().map(String::as_str),
                    ),
                ));
                continue;
            }
            // Read this run ⇒ read ever.
            if tier == "run" {
                w.state
                    .insert(format!("entry.{id}.read"), Value::Bool(true));
            }
            w.state.insert(ever_read_path(id), Value::Bool(true));
            if share_of(p, id).is_some() {
                spend_shared(p, &mut w, id, None, tier == "run");
            }
        }
    }
    let facts = &seed.surfaces.facts;
    for (i, f) in facts.iter().enumerate() {
        let next = facts.get(i + 1).map(String::as_str);
        match resolve_fact(p, f, next) {
            Ok(fact) => {
                w.facts.insert(fact);
            }
            Err(e) => errs.push(SeedError::item(
                &["facts"],
                i,
                format!("`facts:` entry `{f}` {e}"),
            )),
        }
    }
    w.derive = seed.derive.filter(|d| !d);
    if errs.is_empty() {
        Ok(w)
    } else {
        Err(errs)
    }
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
    let evaluator = w.evaluator_with_schema(
        &p.eval_json,
        p.store_schemas[w.derive.unwrap_or(true) as usize].clone(),
        None,
    );
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
    /// `clock.ended` — `None` on a clock that never ends.
    pub ended: Option<bool>,
    /// Where the clock stands, when that is a finite clock's last position
    /// and it has not ended yet — it ends on the next advance past it.
    pub last: Option<String>,
}

/// The world `w` as expectations judge it: the effective state, every
/// declared quest's status, and — when `with_facts` — every fact that holds
/// after derivation (a fixpoint, so computed only on request). The reserved
/// quest reads the lifecycle writes only on a transition read as an engine
/// reads them before it (dsl 0.24.0 §2): `failedBy` `unset`, an objective's
/// `done` and `failed` `false`.
pub fn world_view(p: &ExecProject, w: &World, with_facts: bool) -> WorldView {
    let facts = if with_facts {
        w.evaluator_with_schema(
            &p.eval_json,
            p.store_schemas[w.derive.unwrap_or(true) as usize].clone(),
            None,
        )
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
        let ended = decl.is_finite().then(|| crate::clock::ended(&w.state));
        Some(ClockView {
            day: at.day,
            slot: decl.slot_name(at.slot).map(str::to_string),
            weekday: decl.weekday(at.day),
            weekday_label: decl.weekday_label(at.day).map(str::to_string),
            ended,
            last: (ended == Some(false) && decl.last_at() == Some(at)).then(|| decl.describe(at)),
        })
    });
    WorldView {
        state,
        facts,
        quests,
        clock,
    }
}
