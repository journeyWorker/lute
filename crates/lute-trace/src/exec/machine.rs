//! The one walker over a COMPILED artifact (the executable counterpart of
//! `docs/runtime/` + `schemas/lute-ir-0.26.schema.json`), parameterised by a
//! [`Driver`] (`docs/design/runtime-unification.md` §3.2). `lute run`
//! (`RunDriver`) and `lute play` (`PlayDriver`) execute through it.
//!
//! What it implements, grounded in the runtime contract docs:
//! - the **dispatcher loop** (execution-model.md): a program counter over
//!   `commands`, resolving every control-flow target (`jump`/`choice`/`hub`/
//!   `match`/`converge`) against an `addr → index` map, with fall-through
//!   resolution for a `converge` that points one past the last record;
//! - **CEL guards** (cel-and-facts.md): every guard/`::set` value is evaluated
//!   from its `raw` CEL via `lute_cel::parse_slot` + [`crate::eval`] — the
//!   tree's one CEL evaluator — so guard semantics match the checker exactly
//!   (including the `holds`/`count` fact-query functions the structured `expr`
//!   AST deliberately omits);
//! - a **real stratified Datalog least-fixpoint** over the artifact's `rules`
//!   (cel-and-facts.md) — recomputed after every `assert`/`retract` delta — so
//!   a `derive: true` relation queried in a guard returns a *definite* answer.
//!   The evaluator is [`crate::datalog`], the one `lute trace`/`lute test`
//!   apply too (dsl 0.22.0 §6); a seed's `derive: false` skips it;
//! - **`choice` / `hub` / `match`** control flow, with `hub` `once`/`exit`
//!   re-presentation; every decision comes from [`Driver::choose`];
//! - the **quest lifecycle** (quest-lifecycle.md): `start` activation, and
//!   accept activation of a `start`-less (accept-driven) quest from the
//!   seed's `accepts` or an `accept` record (dsl 0.21.0 §7a.3), monotone
//!   objective completion (bodies play once), `on="<occasion>"` objectives
//!   judged only when the seed's `occasions` raise them (§7a.2), `fail`
//!   evaluated before derived completion, and `<on>` handlers fired on the
//!   engine-derived transitions (`questActive`/`questComplete`/`questFailed`)
//!   plus the seed's `events`;
//! - **`visited('<scene id>')`** (dsl 0.21.0 §7a.1) over the seed's
//!   `visited` set (`lute play`: the presented scenes);
//! - **lore entries** (lore-entries.md, dsl 0.19.0): [`Machine::with_entry`]
//!   presents ONE `entry` record of a lore artifact — its body segment runs
//!   to the next `entry` or `beat` record, `set`/`assert`/`retract` apply
//!   only while `entry.<id>.read` is false (recorded as `skipped`
//!   otherwise), and a completed first read sets `entry.<id>.read = true`;
//! - **bundle beats** (beats-and-occasions.md, dsl 0.23.0 §4):
//!   [`Machine::with_bundle_beat`] presents ONE `beat` record of a lore
//!   artifact by its canonical `<document id>.<beat id>` (or bare beat id) —
//!   its body segment runs to the next `entry` or `beat` record like a
//!   scene's, every effect applied.
//!
//! Records are `serde_json::Value`s in the shapes `lute run --json` prints
//! (the conformance contract); they always carry the fields a play
//! transcript shows (a line's `role`/`lineId`/`voiceKey`/`as`/`emotion`, a
//! menu's `spent`/`ineligible`) and reach the driver through
//! [`Driver::emit`], in execution order.
//!
//! Deliberately NOT implemented (host/engine policy the runtime contract
//! leaves unspecified; see also `conformance/README.md`): no real timeline
//! clock (a `barrier` is a transcript note), no real bridges (a
//! `bridgeResult` effect reads the driver's [`Driver::bridge`] answer), no
//! narrative-time history (`now()` / `validAt(...)` read unknown).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use lute_cel::CelArena;
use lute_check::{RelVocab, StateSchema};
use serde_json::{json, Value as Json};

use super::driver::{
    BridgeCall, BridgeReply, Driver, Forced, Menu, MenuKind, MenuOption, OnUnknown, Pick, SiteKind,
    UnknownSite, Verdict,
};
use crate::datalog::{Fact, Program};
use crate::{eval, EffectiveState, EvalEnv, FactStore, MockSet, UnresolvedAtom, Value};

/// A parsed quest declaration head (quest-lifecycle.md).
struct QuestDecl {
    id: String,
    /// `raw` activation predicate; `None` ⇒ activates at start / accept-driven.
    start: Option<String>,
    /// `raw` failure predicate, evaluated before derived completion.
    fail: Option<String>,
    objectives: Vec<Obj>,
    /// dsl 0.16.0 §3 D-D: owner-declared `<reward/>` entries in
    /// declaration order. Grants fire at fresh `complete`/`failed`
    /// transitions (spec §3 D-D) — [`Machine::emit_quest_grants`] filters
    /// by the per-entry `on=` marker.
    rewards: Vec<RewardRec>,
    /// dsl 0.24.0 §2: `QuestCmd.activate == "accept"` — a subquest child
    /// that activates only once accepted while its parent is active.
    accept_activated: bool,
    /// dsl 0.24.0 §2: `QuestCmd.complete == "any"` — ANY required
    /// objective done completes the quest.
    complete_any: bool,
}

struct Obj {
    id: String,
    done: String,
    optional: bool,
    /// `addr` of the completion body segment, or `None` (empty body).
    body: Option<String>,
    /// `ObjectiveEntry.quest` — the referenced child quest id (subquest
    /// design 2026-08-31 §3), or `None` for an authored-`done` objective.
    quest: Option<String>,
    /// dsl 0.16.0 §3 D-D: owner-declared `<reward/>` entries in
    /// declaration order. Fires ONCE at fresh `done` (spec §3 D-D),
    /// BEFORE any quest-level grant fires.
    rewards: Vec<RewardRec>,
    /// dsl 0.21.0 §7a.2: `ObjectiveEntry.on` — the occasion at which this
    /// objective's `done` is judged; `None` ⇒ judged continuously.
    on: Option<String>,
    /// dsl 0.23.0 §2: `ObjectiveEntry.by` raw — while not done, the first
    /// time it is true the objective fails.
    by: Option<String>,
    /// dsl 0.24.0 §2.1: `ObjectiveEntry.until` raw — judged only when the
    /// objective's occasion (and target) is raised, after its `done`.
    until: Option<String>,
    /// dsl 0.23.0 §2: `ObjectiveEntry.target` — with `on`, judged only by a
    /// raise for this target.
    target: Option<String>,
}

/// One `RewardEntry` (`ir.rs`, dsl 0.16.0 §3) parsed straight off the
/// artifact JSON, with `on` normalized to `Option<String>` (only ever
/// `Some("failed")` after the checker's `E-REWARD-ATTR` gate — the runner
/// filters strictly on that value). `when` is the raw CEL fragment
/// evaluated at the grant instant via [`Machine::truthy`]; `None` here
/// means an unconditional grant.
struct RewardRec {
    kind: String,
    target: Option<String>,
    amount: Option<i64>,
    amount_min: Option<i64>,
    amount_max: Option<i64>,
    when: Option<String>,
    on: Option<String>,
    /// dsl 0.23.0 §8: `RewardEntry.credits` — the state path a grant adds
    /// its (scalar) amount to.
    credits: Option<String>,
}

/// dsl 0.16.0 §3 D-D: which lifecycle transition is firing declarative
/// rewards. [`GrantEvent::Objective`] fires every objective-level reward
/// (objective entries never carry `on=`, spec §2); [`GrantEvent::Complete`]
/// fires quest-level rewards whose `on=` is unset (the default
/// "on complete"); [`GrantEvent::Failed`] fires quest-level rewards whose
/// `on == "failed"` (both authored-`fail` and §2.3 cascade paths hit this
/// arm, with the transcript's `onFailed: true` marking the transition kind).
#[derive(Clone, Copy)]
enum GrantEvent {
    Objective,
    Complete,
    Failed,
}

/// A parsed `<on>` handler. `quest` is the ENCLOSING quest's id, recovered
/// from stream order (an `on` record is emitted inside its quest's walk, so
/// it follows its own quest record and precedes the next one). Lifecycle
/// events (`questActive`/`questComplete`/`questFailed`) fire only for their
/// own enclosing quest (quest-lifecycle.md); world events are unscoped.
struct Handler {
    event: String,
    when: Option<String>,
    body: String,
    quest: Option<String>,
    /// dsl 0.24.0 §2: `OnCmd.target` — fires only for a raise of the
    /// same-named occasion for this target.
    target: Option<String>,
}

/// The bounded step outcome of the dispatcher.
enum Step {
    /// Continue at this command index (`>= commands.len()` ⇒ end).
    Next(usize),
    /// Halt the walk (incomplete decision or a hard error already recorded).
    Halt,
}

/// The `line` record fields that carry a line's identity and delivery —
/// what a play transcript shows and `lute run --json` (the conformance
/// contract) leaves out.
pub const LINE_DELIVERY_KEYS: [&str; 5] = ["role", "lineId", "voiceKey", "as", "emotion"];

/// The `choice` / `hub` record fields that mark what a menu did not offer
/// (spent `once` options, guard-closed options) — what a play menu shows and
/// `lute run --json` leaves out.
pub const MENU_MARK_KEYS: [&str; 2] = ["spent", "ineligible"];

/// The `note` of a `choice` / `hub` record where the driver made no
/// decision ([`Pick::Unscripted`]) — what `lute play`'s halt looks for.
pub const NOTE_NO_DECISION: &str = "no mock decision — incomplete";
/// The `note` of a `choice` / `hub` record where an automatic pick
/// ([`Pick::AutoFirst`] / [`Pick::HubAutoPass`]) found no open option.
pub const NOTE_NO_ELIGIBLE: &str = "no option decided eligible — incomplete";
/// The `note` of a `choice` record whose scripted decision the driver
/// dropped ([`Forced::Skip`] on a branch).
pub const NOTE_SKIPPED: &str = "scripted decision skipped — incomplete";

/// What a walk starts from beyond the artifact and any carryover — the
/// surfaces `lute run --mock` / `lute trace --mock` read (`state:`,
/// `facts:`, `visited:`, `accepts:`, `events:`, `occasions:`, `derive:`).
/// Decisions (`choose:`) and bridge answers (`bridges:`) belong to the
/// [`Driver`].
#[derive(Clone, Debug, Default)]
pub struct Seed {
    /// `(path, literal text)`, coerced against the path's declared type and
    /// layered over the starting state (override per path, never a reset).
    pub state: Vec<(String, String)>,
    /// Ground `"rel(a, b)"` facts added to the base facts.
    pub facts: Vec<String>,
    /// Scene ids `visited('<id>')` reads.
    pub visited: Vec<String>,
    /// Quest ids accepted (an accept-driven quest activates).
    pub accepts: Vec<String>,
    /// World events fired, in order, after the lifecycle settles.
    pub events: Vec<String>,
    /// Occasions raised (`name` / `name@target`), in order, after the events.
    pub occasions: Vec<String>,
    /// dsl 0.22.0 §6: whether the rules derive (`false`: `derive: false`).
    pub derive: bool,
}

impl From<&MockSet> for Seed {
    fn from(mock: &MockSet) -> Self {
        Seed {
            state: mock
                .state
                .iter()
                .map(|(p, lit, _)| (p.clone(), lit.clone()))
                .collect(),
            facts: mock.facts.clone(),
            visited: mock.visited.clone(),
            accepts: mock.accepts.clone(),
            events: mock.events.clone(),
            occasions: mock.occasions.clone(),
            derive: mock.derives(),
        }
    }
}

/// The world a walk hands to the next one ([`Machine::into_carry`]), and
/// what [`Machine::resume`] starts from (its `state`, `base_facts` and
/// `quest_status`; the walk's own flags start cleared).
#[derive(Clone, Debug, Default)]
pub struct Carry {
    pub state: BTreeMap<String, Value>,
    pub base_facts: BTreeSet<Fact>,
    pub quest_status: BTreeMap<String, String>,
    /// A decision was reached with no pick, or (quest advance) an active
    /// quest's required objective is undecidable, or the driver halted at
    /// an unknown.
    pub incomplete: bool,
    /// See [`Machine::unresolved`] — the honesty-gate signal.
    pub unresolved: Vec<UnresolvedAtom>,
    /// See [`Machine::accepted`] — the accepts a presentation made, which
    /// `lute play` hands to the next quest advance.
    pub accepted: Vec<String>,
    /// See [`Machine::accepted_next_run`].
    pub accepted_next_run: Vec<String>,
    /// See [`Machine::refused`].
    pub refused: bool,
    /// See [`Machine::failed_objectives`].
    pub failed_objectives: BTreeSet<String>,
    /// See [`Machine::deferred_handlers`] — empty unless deferring.
    pub deferred_handlers: Vec<String>,
}

impl Carry {
    /// A carried world to [`Machine::resume`] from.
    pub fn world(
        state: BTreeMap<String, Value>,
        base_facts: BTreeSet<Fact>,
        quest_status: BTreeMap<String, String>,
    ) -> Self {
        Carry {
            state,
            base_facts,
            quest_status,
            ..Carry::default()
        }
    }
}

/// The reference engine over one artifact. `lute play` drives one Machine
/// per presented beat and per quest-lifecycle advance, threading
/// state/facts/quest status across instances via [`Carry`] /
/// [`Machine::resume`] — never a second dispatcher.
pub struct Machine<D: Driver> {
    driver: D,
    kind: String,
    commands: Vec<Json>,
    /// `addr → index` in `commands`.
    addr_index: BTreeMap<String, usize>,
    /// `(addr, index)` in stream (== addr-sorted) order, for fall-through.
    addr_order: Vec<(String, usize)>,
    /// Declared value-type per state path (from the artifact `state` table),
    /// so a seed literal is coerced against the same type the compiler folded.
    types: BTreeMap<String, String>,
    /// dsl 0.24.0 §1: per state path, the member → display-label map its
    /// artifact `state[].labels` declares; `{{path}}` renders through it.
    labels: BTreeMap<String, BTreeMap<String, String>>,
    /// Prerelease N8: cast id -> display name, what an `occasionTarget`
    /// placeholder renders a member by (`lute play` fills it; empty renders
    /// the id).
    display_names: BTreeMap<String, String>,

    // Evaluation environments — empty by construction: all live state lives in
    // `state`, so an empty `StateSchema` never shadows a read; an empty
    // `RelVocab` makes every relation non-derived, so `holds`/`count` over the
    // fully-materialized fixpoint return DEFINITE answers. Under `derive:
    // false` (dsl 0.22.0 §6) `vocab` marks the derived relations instead.
    schema: StateSchema,
    vocab: RelVocab,

    /// The artifact's Datalog rules ([`crate::datalog`]).
    program: Program,

    /// Live scalar state (path → value).
    state: BTreeMap<String, Value>,
    /// Base facts (seeds ∪ asserted − retracted), before derivation.
    base_facts: BTreeSet<Fact>,
    /// `base_facts` ∪ the derived least-fixpoint — what guards query.
    all_facts: BTreeSet<Fact>,
    /// Derived relations whose last fixpoint read an undecided rule guard
    /// (dsl 0.24 T1-1): a guard querying one is unknown, so `lute play`
    /// halts on it instead of reading the relation as silently empty.
    undecided: BTreeMap<String, Vec<UnresolvedAtom>>,

    seed: Seed,

    /// Final quest statuses (quest-kind only).
    quest_status: BTreeMap<String, String>,

    /// A `choice`/`hub` was reached with no decision (exit 3).
    incomplete: bool,
    /// An `end` record executed (dsl 0.8.0): the walk is OVER. Distinct from
    /// [`Machine::incomplete`] — an `end` walk is a COMPLETE walk (exit 0),
    /// behaviorally identical to falling off the end of `commands`, with the
    /// author's `reason` surfaced in the transcript. Checked wherever
    /// `incomplete` is, so a terminator inside a hub option / quest body
    /// segment stops the WHOLE walk and not merely its bounded `run_range`.
    terminated: bool,
    /// An unknown command `kind` or malformed record (exit 2).
    fatal: Option<String>,
    /// The `fatal` message is a walk-time refusal — a scripted decision
    /// the driver refused ([`Forced::Refuse`], `E-TRACE-CHOICE`), or an
    /// exclusivity violation — not a malformed artifact. `lute run` exits 2
    /// for both; `lute play` reports a refusal as an error (exit 1), the
    /// same verdict an ineligible `pick:` gets.
    refused: bool,
    /// Every [`UnresolvedAtom`] any CEL evaluation this walk performed
    /// produced ([`Machine::eval_raw`]'s one chokepoint). `lute run` never
    /// reads this; it exists for [`Carry`] to hand to `lute play`'s honesty
    /// gate (an unresolved surface halts it incomplete).
    unresolved: Vec<UnresolvedAtom>,
    /// `lute play` (dsl 0.21.0 §6, D-H): [`Machine::advance_quests`] RESUMES
    /// the quest lifecycle over carried-over state instead of starting a
    /// fresh walk — a quest keeps its carried status (only an `unset` quest
    /// may activate) and an objective whose
    /// `quest.<id>.objectives.<oid>.done` is already true is not completed
    /// (nor its body played) a second time. `false` for `lute run`.
    quest_resume: bool,
    /// dsl 0.19.0 §8: the `entry` id a lore artifact presents
    /// ([`Machine::with_entry`]). `None` for every scene/quest walk.
    entry: Option<String>,
    /// dsl 0.23.0 §4: the bundle `beat` a lore artifact presents (its
    /// canonical `<document id>.<beat id>`, or the bare beat id). `None`
    /// otherwise ([`Machine::with_bundle_beat`]).
    bundle_beat: Option<String>,
    /// dsl 0.19.0 §6: `false` while presenting an entry whose
    /// `entry.<id>.read` is already true — `set`/`assert`/`retract` records
    /// are then recorded as `skipped` instead of applied.
    apply_effects: bool,
    /// dsl 0.21.0 §7a.1: the ids of the scenes presented in this save — what
    /// `visited('<id>')` reads. Seeded from the seed's `visited` and the
    /// playthrough's presentation history ([`Machine::with_visited`]).
    visited: BTreeSet<String>,
    /// dsl 0.21.0 §7a.3: the quest ids every `accept` record this walk
    /// executed named, in order. An accept-driven quest of THIS artifact
    /// activates from it on the next lifecycle round; `lute play` carries
    /// the rest to the quest documents via [`Carry::accepted`].
    accepted: Vec<String>,
    /// dsl 0.24.0 §2: the quest ids every `accept` record with `applies:
    /// "nextRun"` named — queued; `lute play` applies them after the next
    /// run-start reset ([`Carry::accepted_next_run`]).
    accepted_next_run: Vec<String>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids whose `by` came true while
    /// they were not done — failed, never judged again. `lute play` carries
    /// them across advances ([`Machine::with_failed_objectives`],
    /// [`Carry::failed_objectives`]).
    failed_objectives: BTreeSet<String>,
    /// dsl 0.25.0 §7: what content reads of the bridge results — the
    /// project's (`lute play`, [`Machine::with_bridge_reads`]) or the
    /// artifact's ([`Machine::new`]).
    bridge_reads: Arc<BridgeReads>,
    /// dsl 0.25.0 §1: every pair of relations the artifact's `relations[].excludes`
    /// declares exclusive (`a < b`); empty when none.
    excludes: Vec<(String, String)>,
    /// dsl 0.24.0 §2: why each `<quest>.<objective>` in `failed_objectives`
    /// failed (`by` / `until`) — read when a required objective's failure
    /// fails its quest, to stamp `quest.<id>.failedBy`.
    objective_failed_by: BTreeMap<String, &'static str>,
    /// dsl 0.24.0 §2.1: the raises (`name` / `name@target`) still to come in
    /// this step — the walk's own `occasions` (`lute run`, a play raise pass)
    /// plus the play step's ([`Machine::with_deferred_by`]). An `on=`
    /// objective one of them judges has its `by` deferred until that raise
    /// judged its `done` ([`Machine::judge_occasion`]): `done` wins over a
    /// deadline that came true in the step raising the occasion.
    defer_by: Vec<String>,
    /// `lute play` (dsl 0.24.0 §2): `Some` while answering a `judge: before`
    /// raise — a firing `<on>` handler's body is collected here (its `when`
    /// decided at the firing) instead of run, and play runs it after the
    /// occasion's beats ([`Machine::run_deferred_handlers`]). `None`: bodies
    /// run where they fire.
    deferred_handlers: Option<Vec<String>>,
}

/// dsl 0.25.0 §7: what content reads of the plugin calls' bridge results,
/// over a set of compiled artifacts ([`BridgeReads::of`]). A result field
/// no content reads MAY be left out of an answer.
#[derive(Clone, Debug, Default)]
pub struct BridgeReads {
    /// Every state path a CEL expression or a `{{…}}` placeholder reads.
    pub paths: BTreeSet<String>,
    /// Per plugin directive tag, the bridge result fields some call of it
    /// writes to a path in `paths` — what every answer to the tag gives
    /// (answers queue per tag, not per call), and what a hint lists.
    pub fields: BTreeMap<String, BTreeSet<String>>,
    /// dsl 0.26.0 §3.1: per plugin directive tag, the declared type (`bool`,
    /// `number`, `string`) of each bridge result field an effect of the tag
    /// reads, from the capability's `result:` shape — what types an answer
    /// whose landing path no state slot declares. Empty for `lute run` (an
    /// artifact carries no capability snapshot).
    pub result_types: BTreeMap<String, BTreeMap<String, &'static str>>,
}

impl BridgeReads {
    pub fn of<'a>(arts: impl IntoIterator<Item = &'a Json> + Clone) -> Self {
        let mut paths = BTreeSet::new();
        for art in arts.clone() {
            ir_read_paths(art, false, &mut paths);
        }
        let mut fields: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for art in arts {
            let cmds = art
                .get("commands")
                .and_then(Json::as_array)
                .into_iter()
                .flatten();
            for c in cmds.filter(|c| c.get("kind").and_then(Json::as_str) == Some("plugin")) {
                let tag = c.get("tag").and_then(Json::as_str).unwrap_or("");
                for e in c
                    .get("effects")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                {
                    let field = e.pointer("/from/bridgeResult").and_then(Json::as_str);
                    let path = e.get("path").and_then(Json::as_str);
                    if let (Some(field), Some(path)) = (field, path) {
                        if paths.contains(path) {
                            fields
                                .entry(tag.to_string())
                                .or_default()
                                .insert(field.to_string());
                        }
                    }
                }
            }
        }
        BridgeReads {
            paths,
            fields,
            result_types: BTreeMap::new(),
        }
    }

    /// dsl 0.26.0 §3.1: `snapshot`'s bridge result types, per directive tag
    /// ([`Self::result_types`]), merged into `self`.
    pub fn with_result_types(
        mut self,
        snapshot: &lute_manifest::snapshot::CapabilitySnapshot,
    ) -> Self {
        use lute_manifest::types::Type;
        for (tag, decl) in &snapshot.directives {
            let Some(bridge) = &decl.bridge else {
                continue;
            };
            let Some(cap) = snapshot
                .bridge_capabilities
                .get(&(bridge.service.clone(), bridge.operation.clone()))
            else {
                continue;
            };
            for (field, _) in crate::mock::bridge_result_writes(decl) {
                let Some(f) = cap.result.iter().find(|f| f.name == field) else {
                    continue;
                };
                let ty = match f.ty {
                    Type::Bool => "bool",
                    Type::Number => "number",
                    _ => "string",
                };
                self.result_types
                    .entry(tag.clone())
                    .or_default()
                    .insert(field.to_string(), ty);
            }
        }
        self
    }

    /// Whether content reads `field` of a `tag` call's result.
    pub fn reads(&self, tag: &str, field: &str) -> bool {
        self.fields.get(tag).is_some_and(|f| f.contains(field))
    }

    /// dsl 0.26.0 §3.1: the capability-declared type of `tag`'s result `field`.
    pub fn result_type(&self, tag: &str, field: &str) -> Option<&'static str> {
        self.result_types.get(tag)?.get(field).copied()
    }
}

/// Every state path `v` (an artifact, or any part of one) reads: each
/// `path` leaf of an expression tree (an `expr` — conditions, `::set`
/// values, match arms, `ref` placeholders with their def inlined) and each
/// `path` placeholder of a `{{…}}`. Writes (`set` / effect `path`s) sit
/// outside both and are not reads.
fn ir_read_paths(v: &Json, in_expr: bool, out: &mut BTreeSet<String>) {
    match v {
        Json::Object(map) => {
            for (k, child) in map {
                match (k.as_str(), child) {
                    ("path", Json::String(p)) if in_expr => {
                        out.insert(p.clone());
                    }
                    ("placeholders", Json::Array(items)) => {
                        for ph in items {
                            if ph.get("kind").and_then(Json::as_str) == Some("path") {
                                if let Some(p) = ph.get("path").and_then(Json::as_str) {
                                    out.insert(p.to_string());
                                }
                            }
                            ir_read_paths(ph, in_expr, out);
                        }
                    }
                    _ => ir_read_paths(child, in_expr || k == "expr", out),
                }
            }
        }
        Json::Array(items) => items.iter().for_each(|i| ir_read_paths(i, in_expr, out)),
        _ => {}
    }
}

impl<D: Driver> Machine<D> {
    /// Build a Machine skeleton straight off an artifact: addr map, typed
    /// state table (types only — NOT yet seeded with defaults; see below),
    /// parsed Datalog rules/strata. Live `state`/`base_facts` start as the
    /// artifact's own `state[].default`/`seedFacts` ([`Machine::new`], a
    /// fresh `lute run` walk); [`Machine::resume`] (`lute play`, dsl 0.21.0
    /// §6) replaces them with the PRIOR scene's [`Carry`]. Both then layer
    /// the seed's own `state`/`facts` on top via [`Machine::apply_seeds`] —
    /// the one place that override rule lives.
    fn blank(art: &Json, seed: Seed, driver: D) -> Self {
        let kind = art
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("scene")
            .to_string();
        let commands = art
            .get("commands")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        let mut addr_index = BTreeMap::new();
        let mut addr_order = Vec::new();
        for (i, c) in commands.iter().enumerate() {
            if let Some(a) = c.get("addr").and_then(Json::as_str) {
                addr_index.insert(a.to_string(), i);
                addr_order.push((a.to_string(), i));
            }
        }
        addr_order.sort();

        // Declared types + initial state defaults (state-lifecycle.md).
        let mut types = BTreeMap::new();
        let mut labels = BTreeMap::new();
        let mut state = BTreeMap::new();
        if let Some(entries) = art.get("state").and_then(Json::as_array) {
            for e in entries {
                let path = e.get("path").and_then(Json::as_str).unwrap_or("");
                if path.is_empty() {
                    continue;
                }
                let ty = e.get("type").and_then(Json::as_str).unwrap_or("string");
                types.insert(path.to_string(), ty.to_string());
                if let Some(map) = e.get("labels").and_then(Json::as_object) {
                    let map: BTreeMap<String, String> = map
                        .iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect();
                    labels.insert(path.to_string(), map);
                }
                if let Some(default) = e.get("default") {
                    if let Some(v) = json_to_value(default) {
                        state.insert(path.to_string(), v);
                    }
                }
            }
        }

        // Base facts: artifact seedFacts.
        let mut base_facts: BTreeSet<Fact> = BTreeSet::new();
        if let Some(seeds) = art.get("seedFacts").and_then(Json::as_array) {
            for s in seeds {
                let rel = s.get("relation").and_then(Json::as_str).unwrap_or("");
                let args: Vec<String> = s
                    .get("args")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().map(json_arg_to_string).collect())
                    .unwrap_or_default();
                if !rel.is_empty() {
                    base_facts.insert((rel.to_string(), args));
                }
            }
        }

        // Parsed rules (the shared evaluator). Under `derive: false` every
        // derived relation — declared `derive: true` or concluded by a rule —
        // is marked so, making an unmatched query unknown, not false.
        let program = Program::from_ir(art.get("rules"))
            .with_kinds(crate::datalog::ir_kinds(art.get("entities")));
        let mut vocab = RelVocab::default();
        if !seed.derive {
            let declared = art
                .get("relations")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter(|r| r.get("derive").and_then(Json::as_bool) == Some(true))
                .filter_map(|r| r.get("name").and_then(Json::as_str));
            let heads = art
                .get("rules")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| r.pointer("/head/relation").and_then(Json::as_str));
            for rel in declared.chain(heads) {
                vocab.relations.insert(
                    rel.to_string(),
                    lute_manifest::relations::RelationDecl {
                        derive: true,
                        ..Default::default()
                    },
                );
            }
        }
        // dsl 0.21.0 §7a.1: the seed's `visited` seeds the presented set.
        let visited = seed.visited.iter().cloned().collect();
        let excludes = art
            .get("relations")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .flat_map(|r| {
                let name = r
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                r.get("excludes")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Json::as_str)
                    .filter(|o| name.as_str() < *o)
                    .map(|o| (name.clone(), o.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect();

        Machine {
            driver,
            kind,
            commands,
            addr_index,
            addr_order,
            types,
            labels,
            display_names: BTreeMap::new(),
            schema: StateSchema::default(),
            vocab,
            program,
            state,
            base_facts,
            all_facts: BTreeSet::new(),
            undecided: BTreeMap::new(),
            seed,
            quest_status: BTreeMap::new(),
            incomplete: false,
            terminated: false,
            fatal: None,
            refused: false,
            unresolved: Vec::new(),
            quest_resume: false,
            entry: None,
            bundle_beat: None,
            apply_effects: true,
            visited,
            accepted: Vec::new(),
            accepted_next_run: Vec::new(),
            failed_objectives: BTreeSet::new(),
            objective_failed_by: BTreeMap::new(),
            defer_by: Vec::new(),
            deferred_handlers: None,
            bridge_reads: Arc::default(),
            excludes,
        }
    }

    /// A fresh walk (`lute run`) of `art`: its declared defaults and seed
    /// facts, the `seed` layered over them. The artifact's own content is
    /// the reader of its bridge results (dsl 0.25.0 §7; `lute play` hands
    /// the project's in [`Machine::with_bridge_reads`]).
    pub fn new(art: &Json, seed: Seed, driver: D) -> Self {
        let mut m = Self::blank(art, seed, driver);
        m.bridge_reads = Arc::new(BridgeReads::of([art]));
        m.apply_seeds();
        refresh_clock(art, &mut m.state);
        m.recompute_facts();
        m
    }

    /// `lute play`'s chained-evaluation constructor (dsl 0.21.0 §6):
    /// seeds live state/facts/quest status from a PRIOR walk's [`Carry`]
    /// instead of this artifact's own declared defaults — `play.rs` already
    /// decided what carries forward across the scene boundary (its own
    /// state-tier filter: `run.*`/`user.*`/`app.*`/`quest.*` persist,
    /// `scene.*` resets); this constructor does not re-derive that policy,
    /// only accepts its result. `play.rs` never puts `state`/`facts` seeds
    /// in `seed` (it seeds the playthrough once, up front), so
    /// [`Machine::apply_seeds`] layers nothing over the carryover. Only the
    /// carry's `state`, `base_facts` and `quest_status` are read.
    pub fn resume(art: &Json, seed: Seed, carry: Carry, driver: D) -> Self {
        let mut m = Self::blank(art, seed, driver);
        m.state = carry.state;
        m.base_facts = carry.base_facts;
        m.quest_status = carry.quest_status;
        m.apply_seeds();
        refresh_clock(art, &mut m.state);
        m.recompute_facts();
        m
    }

    /// `lute play` (dsl 0.21.0 §6): present ONE `entry` record of a lore
    /// artifact — the same `--entry` surface `lute run` sets — so an entry
    /// beat follows the entry rules (first-read effects, `entry.<id>.read`).
    pub fn with_entry(mut self, id: &str) -> Self {
        self.entry = Some(id.to_string());
        self
    }

    /// dsl 0.23.0 §4: present ONE bundle `beat` record of a lore artifact
    /// (`lute play` for a `bundle` beat, `lute run --beat`).
    pub fn with_bundle_beat(mut self, id: &str) -> Self {
        self.bundle_beat = Some(id.to_string());
        self
    }

    /// Prerelease N8: the cast display names `{{occasion.target}}` renders a
    /// member by (`lute play`).
    pub fn with_display_names(mut self, names: &BTreeMap<String, String>) -> Self {
        self.display_names = names.clone();
        self
    }

    /// dsl 0.26.0 §5: the member a `target="kind:<kind>"` beat was raised
    /// for, readable as `occasion.target` (cleared with `None`).
    pub fn bind_occasion_target(&mut self, member: Option<&str>) {
        let path = lute_check::beats::OCCASION_TARGET;
        match member {
            Some(m) => {
                self.state
                    .insert(path.to_string(), Value::Str(m.to_string()));
            }
            None => {
                self.state.remove(path);
            }
        }
    }

    /// `lute play` (dsl 0.21.0 §7a.1): the playthrough's presented scenes,
    /// joined to any seed `visited`, so `visited('<id>')` reads real
    /// presentation history.
    pub fn with_visited(mut self, visited: &BTreeSet<String>) -> Self {
        self.visited.extend(visited.iter().cloned());
        self
    }

    /// `lute play` (dsl 0.23.0 §2): the objectives earlier advances failed
    /// through their `by`, so a resumed walk neither completes nor fails them
    /// again.
    pub fn with_failed_objectives(mut self, failed: &BTreeSet<String>) -> Self {
        self.failed_objectives.extend(failed.iter().cloned());
        self
    }

    /// `lute play` (dsl 0.24.0 §2.1): the step raises `raise` (`name` or
    /// `name@target`) — the settles before it defer the `by` of the `on=`
    /// objectives it judges ([`Machine::defer_by`]).
    pub fn with_deferred_by(mut self, raise: Option<&str>) -> Self {
        self.defer_by.extend(raise.map(str::to_string));
        self
    }

    /// `lute play` (dsl 0.24.0 §2): answering a `judge: before` raise —
    /// collect the firing `<on>` handlers' bodies instead of running them
    /// ([`Machine::deferred_handlers`]).
    pub fn with_deferred_handlers(mut self, defer: bool) -> Self {
        self.deferred_handlers = defer.then(Vec::new);
        self
    }

    /// dsl 0.25.0 §7: what content reads of the bridge results — the
    /// project's, for `lute play` (whose documents read each other's).
    pub fn with_bridge_reads(mut self, reads: Arc<BridgeReads>) -> Self {
        self.bridge_reads = reads;
        self
    }

    /// Layer the seed's `state`/`facts` over whatever live state/facts the
    /// constructor already set (override, per path/fact — never a reset):
    /// shared by [`Machine::new`] (over the artifact's own defaults) and
    /// [`Machine::resume`] (over the prior scene's carryover), so the "seed
    /// wins on conflict" rule applies identically either way.
    fn apply_seeds(&mut self) {
        let seeds = std::mem::take(&mut self.seed.state);
        for (path, lit) in &seeds {
            let v = self.coerce_literal(path, lit);
            self.state.insert(path.clone(), v);
        }
        self.seed.state = seeds;
        for f in &self.seed.facts {
            if let Some(fact) = parse_ground_fact(f) {
                self.base_facts.insert(fact);
            }
        }
    }

    /// Coerce a raw mock literal against a path's declared value-type.
    fn coerce_literal(&self, path: &str, lit: &str) -> Value {
        match self.types.get(path).map(String::as_str) {
            Some("bool") => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Str(lit.to_string()),
            },
            Some("number") => lit
                .parse::<f64>()
                .map(Value::Num)
                .unwrap_or(Value::Str(lit.to_string())),
            // enum / string / reserved / unknown: keep verbatim, but recognize
            // an obvious bool/number so an un-typed seed still evaluates.
            _ => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => lit
                    .parse::<f64>()
                    .map(Value::Num)
                    .unwrap_or(Value::Str(lit.to_string())),
            },
        }
    }

    /// Recompute the least-fixpoint: `all_facts = base ∪ derive(base)`,
    /// through the shared [`crate::datalog`] evaluator trace uses too.
    /// Under `derive: false` (dsl 0.22.0 §6) the rules are not applied: the
    /// derived relations are marked in `vocab`, so an unmatched one reads
    /// unknown ([`Machine::blank`]).
    fn recompute_facts(&mut self) {
        if self.seed.derive {
            let eff = EffectiveState::new(&self.schema, self.state.clone());
            let closure = self.program.fixpoint(&self.base_facts, &eff);
            self.all_facts = closure.facts;
            self.undecided = closure.undecided;
        } else {
            self.all_facts = self.base_facts.clone();
            self.undecided.clear();
        }
    }

    /// Evaluate a `raw` CEL fragment over live state + the current fixpoint.
    /// Reuses `lute_cel` (parse) + `crate::eval` (the one CEL evaluator).
    /// The one chokepoint every CEL evaluation in this walk funnels through,
    /// so recording each produced [`UnresolvedAtom`] into `self.unresolved`
    /// here (rather than at each call site) covers guards, `::set` RHS
    /// values, and quest predicates alike with one line. `lute run` itself
    /// never reads `self.unresolved` — its own output/exit code are
    /// unchanged; the field exists for `lute play`'s honesty gate
    /// ([`Carry::unresolved`], [`Machine::eval_guard`]).
    fn eval_raw(&mut self, raw: &str) -> Value {
        if raw.trim().is_empty() {
            return Value::Unknown;
        }
        let mut arena = CelArena::default();
        let handle = match lute_cel::parse_slot(&mut arena, raw, 0) {
            Ok(h) => h,
            Err(_) => return Value::Unknown,
        };
        let ided = match arena.get(handle) {
            Some(e) => e,
            None => return Value::Unknown,
        };
        let eff = EffectiveState::new(&self.schema, self.state.clone());
        let mut fs = FactStore::new(&self.vocab).with_undecided(self.undecided.clone());
        for (rel, args) in &self.all_facts {
            fs.assert(rel, args);
        }
        for id in &self.visited {
            fs.visit(id);
        }
        let env = EvalEnv {
            state: &eff,
            facts: &fs,
        };
        let mut unresolved = Vec::new();
        let v = eval(&ided.expr, &env, &mut unresolved);
        self.unresolved.extend(unresolved);
        v
    }

    /// `Some(bool)` for a decided guard, `None` when unknown.
    fn truthy(&mut self, raw: &str) -> Option<bool> {
        match self.eval_raw(raw) {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// Truthiness of an IR structured `expr` node (`lute_compile::expr::ExprNode`'s
    /// serialized shape — `lit`/`path`/`op`/`cond`/`list`/`isSet`/`has`), the
    /// executable surface a compiled `<when is=…>` match arm carries (IR A13).
    /// Three-valued like [`Machine::truthy`]: `None` = unknown, never a guess.
    fn expr_node_truthy(&mut self, node: &Json) -> Option<bool> {
        match self.expr_node_value(node) {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// Evaluate one structured expr node against live state. Total over the
    /// `ExprNode` kind set; anything unimplementable against the runner's
    /// state model (`isSet`/`has` set-ness tracking, an unknown operator)
    /// evaluates `Unknown` rather than crashing or guessing — the same
    /// honesty rule every other guard surface follows.
    fn expr_node_value(&mut self, node: &Json) -> Value {
        if let Some(lit) = node.get("lit") {
            return json_to_value(lit).unwrap_or(Value::Unknown);
        }
        if let Some(path) = node.get("path").and_then(Json::as_str) {
            return self.state.get(path).cloned().unwrap_or(Value::Unknown);
        }
        // `isSet(p)` / `has(p)` are definite presence (D19): every live value
        // — a default, a carried or seeded value, a write — is in `state`.
        // Without this, every `isSet(…) && …` guard read unknown, so a
        // gated line on a maybe-unset path (dsl 0.23.0 §6 `prev.run.*`)
        // could never play.
        if let Some(path) = node
            .get("isSet")
            .or_else(|| node.get("has"))
            .and_then(Json::as_str)
        {
            return Value::Bool(self.state.contains_key(path));
        }
        if let (Some(cond), Some(then), Some(otherwise)) =
            (node.get("cond"), node.get("then"), node.get("else"))
        {
            return match self.expr_node_value(cond) {
                Value::Bool(true) => self.expr_node_value(then),
                Value::Bool(false) => self.expr_node_value(otherwise),
                _ => Value::Unknown,
            };
        }
        if let Some(op) = node.get("op").and_then(Json::as_str) {
            let l = node
                .get("l")
                .map(|n| self.expr_node_value(n))
                .unwrap_or(Value::Unknown);
            let r = node.get("r").map(|n| self.expr_node_value(n));
            return match (op, r) {
                ("!", None) => match l {
                    Value::Bool(b) => Value::Bool(!b),
                    _ => Value::Unknown,
                },
                ("-", None) => match l {
                    Value::Num(n) => Value::Num(-n),
                    _ => Value::Unknown,
                },
                ("&&", Some(r)) => match (l, r) {
                    (Value::Bool(false), _) | (_, Value::Bool(false)) => Value::Bool(false),
                    (Value::Bool(true), Value::Bool(true)) => Value::Bool(true),
                    _ => Value::Unknown,
                },
                ("||", Some(r)) => match (l, r) {
                    (Value::Bool(true), _) | (_, Value::Bool(true)) => Value::Bool(true),
                    (Value::Bool(false), Value::Bool(false)) => Value::Bool(false),
                    _ => Value::Unknown,
                },
                ("==", Some(r)) => expr_node_eq(&l, &r)
                    .map(Value::Bool)
                    .unwrap_or(Value::Unknown),
                ("!=", Some(r)) => expr_node_eq(&l, &r)
                    .map(|b| Value::Bool(!b))
                    .unwrap_or(Value::Unknown),
                ("<", Some(r)) => expr_node_cmp(&l, &r)
                    .map(|o| Value::Bool(o == std::cmp::Ordering::Less))
                    .unwrap_or(Value::Unknown),
                ("<=", Some(r)) => expr_node_cmp(&l, &r)
                    .map(|o| Value::Bool(o != std::cmp::Ordering::Greater))
                    .unwrap_or(Value::Unknown),
                (">", Some(r)) => expr_node_cmp(&l, &r)
                    .map(|o| Value::Bool(o == std::cmp::Ordering::Greater))
                    .unwrap_or(Value::Unknown),
                (">=", Some(r)) => expr_node_cmp(&l, &r)
                    .map(|o| Value::Bool(o != std::cmp::Ordering::Less))
                    .unwrap_or(Value::Unknown),
                ("+", Some(r)) => match (l, r) {
                    (Value::Num(a), Value::Num(b)) => Value::Num(a + b),
                    (Value::Str(a), Value::Str(b)) => Value::Str(format!("{a}{b}")),
                    _ => Value::Unknown,
                },
                ("-", Some(r)) | ("*", Some(r)) | ("/", Some(r)) | ("%", Some(r)) => match (l, r) {
                    (Value::Num(a), Value::Num(b)) => match op {
                        "-" => Value::Num(a - b),
                        "*" => Value::Num(a * b),
                        "/" if b != 0.0 => Value::Num(a / b),
                        // dsl 0.24.0 §1: integer `%` — the truncated
                        // remainder of two integral values (`+ 0.0` folds
                        // `-0`); a fractional operand or a zero divisor is
                        // unknown, as in trace (`lute_check::apply_op`).
                        "%" if a.fract() == 0.0 && b.fract() == 0.0 && b != 0.0 => {
                            Value::Num(a % b + 0.0)
                        }
                        _ => Value::Unknown,
                    },
                    _ => Value::Unknown,
                },
                _ => Value::Unknown,
            };
        }
        // `list` / `isSet` / `has` / anything newer: no runner-side model yet.
        Value::Unknown
    }

    /// Resolve a control-flow target `addr` to a command index. A `converge`
    /// that points "one past the last record" (execution-model.md) is not in
    /// the map → fall through to the first command whose addr sorts after it,
    /// or the end of the stream.
    fn resolve(&self, addr: &str) -> usize {
        if let Some(&i) = self.addr_index.get(addr) {
            return i;
        }
        for (a, i) in &self.addr_order {
            if a.as_str() > addr {
                return *i;
            }
        }
        self.commands.len()
    }

    /// Drive the whole walk: `lute run` once per artifact; `lute play` (dsl
    /// 0.21.0 §6) once per presented beat, then [`Machine::into_carry`].
    pub fn run(&mut self) -> Result<(), String> {
        if self.kind == "quest" {
            self.run_quest();
        } else if self.kind == "lore" {
            if self.bundle_beat.is_some() {
                self.run_bundle_beat();
            } else {
                self.run_entry();
            }
        } else {
            self.run_range(0, self.commands.len());
        }
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// Consume a Machine after [`Machine::run`] into the carried world plus
    /// its driver (which holds what the driver collected: the transcript,
    /// the scripted-choice cursor, the unconsumed bridge answers). Only
    /// meaningful post-`run`; a pre-run carry would just echo the seeds back.
    pub fn into_carry(self) -> (Carry, D) {
        let carry = Carry {
            state: self.state,
            base_facts: self.base_facts,
            quest_status: self.quest_status,
            incomplete: self.incomplete,
            unresolved: self.unresolved,
            accepted: self.accepted,
            accepted_next_run: self.accepted_next_run,
            refused: self.refused,
            failed_objectives: self.failed_objectives,
            deferred_handlers: self.deferred_handlers.unwrap_or_default(),
        };
        (carry, self.driver)
    }

    /// The driver this walk reports to.
    pub fn driver(&self) -> &D {
        &self.driver
    }

    /// The artifact's `kind` (`scene` when absent).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Live state (path → value).
    pub fn state(&self) -> &BTreeMap<String, Value> {
        &self.state
    }

    /// Quest statuses (quest id → `unset`/`active`/`complete`/`failed`).
    pub fn quest_status(&self) -> &BTreeMap<String, String> {
        &self.quest_status
    }

    /// Whether the walk halted incomplete.
    pub fn incomplete(&self) -> bool {
        self.incomplete
    }

    /// Whether an `end` record ended the walk.
    pub fn terminated(&self) -> bool {
        self.terminated
    }

    /// Whether the walk's error was a refusal ([`Machine::refused`]).
    pub fn refused(&self) -> bool {
        self.refused
    }

    /// Drive the dispatcher over `[start, stop)`. Used for the whole scene
    /// (`0..len`) and for bounded hub-option / quest-body segments. Once an
    /// `end` record has run the walk is over, so every LATER segment (a quest
    /// `<on>` body, an objective body) is a no-op — the one guard here is what
    /// makes that true for all of them at once.
    fn run_range(&mut self, start: usize, stop: usize) {
        if self.terminated {
            return;
        }
        let mut pc = start;
        let mut guard = 0usize;
        let limit = self.commands.len() * 64 + 1024;
        while pc >= start && pc < stop && pc < self.commands.len() {
            guard += 1;
            if guard > limit {
                self.fatal = Some("execution did not terminate (control-flow cycle?)".into());
                return;
            }
            match self.step(pc) {
                Step::Next(n) => {
                    if self.fatal.is_some() || self.incomplete || self.terminated {
                        return;
                    }
                    if n < start || n >= stop {
                        return;
                    }
                    pc = n;
                }
                Step::Halt => return,
            }
        }
    }

    /// Dispatch one command; returns the next index (or `Halt`).
    fn step(&mut self, pc: usize) -> Step {
        let cmd = self.commands[pc].clone();
        let kind = cmd.get("kind").and_then(Json::as_str).unwrap_or("");
        match kind {
            "line" => {
                self.rec_line(&cmd);
                Step::Next(pc + 1)
            }
            "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
                self.rec_stage(&cmd, kind);
                Step::Next(pc + 1)
            }
            "set" | "assert" | "retract" if !self.apply_effects => {
                self.rec_skipped(&cmd, kind);
                Step::Next(pc + 1)
            }
            "set" => {
                self.exec_set(&cmd);
                Step::Next(pc + 1)
            }
            "assert" => {
                self.exec_assert(&cmd);
                Step::Next(pc + 1)
            }
            "retract" => {
                self.exec_retract(&cmd);
                Step::Next(pc + 1)
            }
            "jump" => {
                let t = cmd.get("target").and_then(Json::as_str).unwrap_or("");
                Step::Next(self.resolve(t))
            }
            "choice" => self.do_choice(&cmd),
            "hub" => self.do_hub(&cmd),
            "match" => self.do_match(&cmd),
            "barrier" => {
                self.rec_barrier(&cmd);
                Step::Next(pc + 1)
            }
            // dsl 0.8.0: the walk terminator. `Halt` unwinds THIS range; the
            // `terminated` flag unwinds every enclosing one (hub option body,
            // quest segment) so the walk stops exactly as it would by running
            // off the end of `commands`.
            "end" => {
                self.rec_end(&cmd);
                Step::Halt
            }
            "plugin" => {
                if self.exec_plugin(&cmd) {
                    Step::Next(pc + 1)
                } else {
                    Step::Halt
                }
            }
            "accept" => {
                self.exec_accept(&cmd);
                Step::Next(pc + 1)
            }
            // Declarations — inert in a linear walk (a quest artifact is driven
            // by `run_quest`, a lore artifact by `run_entry` /
            // `run_bundle_beat`, never linearly).
            "quest" | "on" | "entry" | "beat" => Step::Next(pc + 1),
            other => {
                self.fatal = Some(format!(
                    "unknown command kind {other:?} (a new capability the runner cannot fake)"
                ));
                Step::Halt
            }
        }
    }

    // ── content & staging ──────────────────────────────────────────────

    /// dsl 0.21.0 §7a.3 (`docs/runtime/quest-lifecycle.md`): the player
    /// accepts quest `quest` here. Recorded as `quest <id> accepted`; the
    /// activation itself belongs to the quest lifecycle — an accept-driven
    /// quest of THIS artifact activates on the next lifecycle round
    /// ([`Machine::is_accepted`]), and `lute play` hands the id to the quest
    /// documents' next advance. A quest this walk already knows to be past
    /// `unset` is left alone, and the record says so. dsl 0.24.0 §2: an
    /// accept with `applies: "nextRun"` is queued instead
    /// ([`Machine::accepted_next_run`]) and recorded with `at: "nextRun"`.
    fn exec_accept(&mut self, cmd: &Json) {
        let quest = cmd
            .get("quest")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("accept".into()));
        rec.insert("quest".into(), Json::String(quest.clone()));
        if cmd.get("applies").and_then(Json::as_str) == Some("nextRun") {
            rec.insert("at".into(), Json::String("nextRun".into()));
            self.driver.emit(Json::Object(rec));
            self.accepted_next_run.push(quest);
            return;
        }
        if let Some(state) = self
            .quest_status
            .get(&quest)
            .filter(|s| s.as_str() != "unset")
        {
            rec.insert("ignored".into(), Json::String(format!("already {state}")));
        }
        self.driver.emit(Json::Object(rec));
        self.accepted.push(quest);
    }

    /// An accept-driven quest's activation signal: a mock `accepts:` entry
    /// or an `accept` record this walk executed.
    fn is_accepted(&self, id: &str) -> bool {
        self.seed.accepts.iter().any(|a| a == id) || self.accepted.iter().any(|a| a == id)
    }

    fn rec_line(&mut self, cmd: &Json) {
        let speaker = cmd.get("speaker").and_then(Json::as_str).unwrap_or("");
        let raw = cmd.get("text").and_then(Json::as_str).unwrap_or("");
        let text = self.interpolate(raw, cmd.get("placeholders").and_then(Json::as_array));
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("line".into()));
        rec.insert("speaker".into(), Json::String(speaker.to_string()));
        rec.insert("text".into(), Json::String(text));
        // The line's identity and delivery ride the record verbatim, so a
        // `lute play --json` consumer never has to re-join the artifact to
        // know who spoke, how, and under which audio key. (`lute run`'s
        // driver drops them: its record is the conformance contract.)
        for key in LINE_DELIVERY_KEYS {
            if let Some(v) = cmd.get(key).filter(|v| !v.is_null()) {
                rec.insert(key.into(), v.clone());
            }
        }
        self.driver.emit(Json::Object(rec));
    }

    fn rec_stage(&mut self, cmd: &Json, kind: &str) {
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": kind,
        }));
    }

    /// Substitute `{{…}}` markers: a `path` with its live value, a `ref` by
    /// evaluating its inlined def body (`expr.raw`, lute 0.21.1). A marker whose
    /// value is unknown (unset path, undecided ref) or a reserved token keeps
    /// its verbatim text (state-lifecycle.md). A placeholder's `format`
    /// (dsl 0.24.0 §4) applies to the value: `ordinal` renders a number as an
    /// English ordinal ([`formatted`]).
    fn interpolate(&mut self, text: &str, placeholders: Option<&Vec<Json>>) -> String {
        let Some(phs) = placeholders else {
            return text.to_string();
        };
        if phs.is_empty() {
            return text.to_string();
        }
        let mut out = String::new();
        let mut rest = text;
        let mut it = phs.iter();
        while let Some(open) = rest.find("{{") {
            out.push_str(&rest[..open]);
            let Some(rel_close) = rest[open..].find("}}") else {
                break;
            };
            let end = open + rel_close + 2;
            let marker = &rest[open..end];
            let rendered = match it.next() {
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("path") => {
                    let path = ph.get("path").and_then(Json::as_str).unwrap_or("");
                    match self.state.get(path) {
                        Some(v) => formatted(ph, v).unwrap_or_else(|| self.path_text(path, v)),
                        None => marker.to_string(),
                    }
                }
                // Prerelease N8: the raised member of a kind beat, by its cast
                // display name when it is a cast id, else the id.
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("occasionTarget") => {
                    match self.state.get(lute_check::beats::OCCASION_TARGET) {
                        Some(Value::Str(m)) => self
                            .display_names
                            .get(m)
                            .cloned()
                            .unwrap_or_else(|| m.clone()),
                        Some(v) => value_to_string(v),
                        None => marker.to_string(),
                    }
                }
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("ref") => {
                    let raw = ph.pointer("/expr/raw").and_then(Json::as_str).unwrap_or("");
                    match self.eval_raw(raw) {
                        Value::Unknown => marker.to_string(),
                        v => formatted(ph, &v).unwrap_or_else(|| value_to_string(&v)),
                    }
                }
                _ => marker.to_string(),
            };
            out.push_str(&rendered);
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    /// A `{{path}}` value as text: an enum member with a declared label
    /// renders the label (dsl 0.24.0 §1) — `prev.run.X` shares `run.X`'s —
    /// anything else its plain value.
    fn path_text(&self, path: &str, v: &Value) -> String {
        if let Value::Str(s) = v {
            let labels = self
                .labels
                .get(path)
                .or_else(|| path.strip_prefix("prev.").and_then(|p| self.labels.get(p)));
            if let Some(label) = labels.and_then(|l| l.get(s)) {
                return label.clone();
            }
        }
        value_to_string(v)
    }

    // ── state & facts ──────────────────────────────────────────────────

    fn exec_set(&mut self, cmd: &Json) {
        let path = cmd
            .get("path")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let op = cmd.get("op").and_then(Json::as_str).unwrap_or("=");
        let rhs_raw = cmd.get("value").and_then(Json::as_str).unwrap_or("");
        let rhs = self.eval_raw(rhs_raw);
        let new = if op == "=" {
            rhs
        } else {
            // Compound arithmetic op: fold against the current value (0 default).
            let cur = match self.state.get(&path) {
                Some(Value::Num(n)) => *n,
                _ => 0.0,
            };
            let by = match rhs {
                Value::Num(n) => n,
                _ => 0.0,
            };
            let folded = match op {
                "+=" => cur + by,
                "-=" => cur - by,
                "*=" => cur * by,
                "/=" if by != 0.0 => cur / by,
                _ => cur,
            };
            Value::Num(folded)
        };
        self.state.insert(path.clone(), new.clone());
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "set",
            "path": path,
            "value": value_to_json(&new),
        }));
    }

    fn exec_assert(&mut self, cmd: &Json) {
        let rel = cmd
            .get("relation")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let args: Vec<String> = cmd
            .get("args")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(json_arg_to_string).collect())
            .unwrap_or_default();
        let before = self.exclusive_now();
        self.base_facts.insert((rel.clone(), args.clone()));
        self.recompute_facts();
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "assert",
            "fact": render_fact(&rel, &args),
        }));
        self.exclusive_check(&before);
    }

    fn exec_retract(&mut self, cmd: &Json) {
        let rel = cmd
            .get("relation")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let args: Vec<String> = cmd
            .get("args")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(json_arg_to_string).collect())
            .unwrap_or_default();
        let before = self.exclusive_now();
        // `_` positions are a bulk wildcard over the ground positions.
        self.base_facts.retain(|(r, a)| {
            !(r == &rel
                && a.len() == args.len()
                && args.iter().zip(a).all(|(p, v)| p == "_" || p == v))
        });
        self.recompute_facts();
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "retract",
            "pattern": render_fact(&rel, &args),
        }));
        self.exclusive_check(&before);
    }

    /// dsl 0.25.0 §1: every pair of facts of exclusive relations holding now
    /// (derived ones included), rendered `a(x) and b(x) both hold`.
    fn exclusive_now(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (a, b) in &self.excludes {
            for (_, args) in self.all_facts.iter().filter(|(r, _)| r == a) {
                if self.all_facts.contains(&(b.clone(), args.clone())) {
                    out.push(format!(
                        "{} and {} both hold",
                        render_fact(a, args),
                        render_fact(b, args)
                    ));
                }
            }
        }
        out
    }

    /// dsl 0.25.0 §1: a write that made exclusive relations hold together —
    /// even for a moment a later write undoes — is recorded at the write and
    /// halts the walk like `lute trace` refuses it (`lute play` exit 1).
    fn exclusive_check(&mut self, before: &[String]) {
        if self.excludes.is_empty() {
            return;
        }
        let new: Vec<String> = self
            .exclusive_now()
            .into_iter()
            .filter(|v| !before.contains(v))
            .collect();
        if new.is_empty() {
            return;
        }
        for v in &new {
            self.driver.emit(json!({ "kind": "exclusive", "text": v }));
        }
        self.refuse(format!(
            "exclusive relations hold together — {} (dsl 0.25.0 §1)",
            new.join("; ")
        ));
    }

    /// dsl 0.19.0 §6: a first-read-only effect record NOT applied on a
    /// re-read — recorded with the same identifying field its applied form
    /// carries (`path` / `fact` / `pattern`), never evaluated.
    fn rec_skipped(&mut self, cmd: &Json, kind: &str) {
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("skipped".into()));
        rec.insert("effect".into(), Json::String(kind.to_string()));
        match kind {
            "set" => {
                let path = cmd.get("path").and_then(Json::as_str).unwrap_or("");
                rec.insert("path".into(), Json::String(path.to_string()));
            }
            _ => {
                let rel = cmd.get("relation").and_then(Json::as_str).unwrap_or("");
                let args: Vec<String> = cmd
                    .get("args")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().map(json_arg_to_string).collect())
                    .unwrap_or_default();
                let key = if kind == "assert" { "fact" } else { "pattern" };
                rec.insert(key.into(), Json::String(render_fact(rel, &args)));
            }
        }
        self.driver.emit(Json::Object(rec));
    }

    // ── control flow ───────────────────────────────────────────────────

    /// A walk-time `E-TRACE-CHOICE`: the script forced an option that is not
    /// offered at this presentation point. Halts like a fatal error, flagged
    /// so `lute play` can report it as the error (exit 1) it is.
    fn refuse(&mut self, msg: String) {
        self.fatal = Some(msg);
        self.refused = true;
    }

    /// Judge one option guard for a menu: `Open` / `Closed`, or `Unknown`
    /// with the atoms this evaluation recorded (left in
    /// [`Machine::unresolved`]).
    fn option_verdict(&mut self, when: &str) -> Verdict {
        let mark = self.unresolved.len();
        match self.truthy(when) {
            Some(true) => Verdict::Open,
            Some(false) => Verdict::Closed,
            None => Verdict::Unknown(self.unresolved[mark..].to_vec()),
        }
    }

    /// A branch's options judged right now — what its menu shows. Display
    /// only: the evaluation never feeds [`Machine::unresolved`], so an
    /// unchosen option's unknown guard cannot halt a playthrough (an unknown
    /// guard counts as offered).
    fn branch_verdicts(&mut self, options: &[Json]) -> Vec<MenuOption> {
        let mark = self.unresolved.len();
        let mut out = Vec::new();
        for o in options {
            let Some(oid) = o.get("id").and_then(Json::as_str) else {
                continue;
            };
            let verdict = match o.get("when").and_then(Json::as_str) {
                Some(when) => self.option_verdict(when),
                None => Verdict::Open,
            };
            out.push(MenuOption {
                id: oid.to_string(),
                verdict,
                exit: o.get("exit").and_then(Json::as_bool).unwrap_or(false),
                once: o.get("once").and_then(Json::as_bool).unwrap_or(false),
            });
        }
        self.unresolved.truncate(mark);
        out
    }

    /// The `E-TRACE-CHOICE` text for a picked option the driver refused.
    fn refusal(construct: &str, id: &str, option: &str, when: &str, verdict: &Verdict) -> String {
        match verdict {
            Verdict::Spent => format!(
                "[E-TRACE-CHOICE] `choose: {id}: {option}` names a `once` option already \
                 taken at this {construct}, so it is no longer offered (dsl 0.4.0 §4.4)"
            ),
            Verdict::Unknown(_) => format!(
                "[E-TRACE-CHOICE] `choose: {id}: {option}` names an option whose guard \
                 `{when}` is undecided at this presentation point (dsl 0.4.0 §4.4)"
            ),
            Verdict::Open | Verdict::Closed => format!(
                "[E-TRACE-CHOICE] `choose: {id}: {option}` names an option whose guard \
                 `{when}` decided false at this presentation point (dsl 0.4.0 §4.4)"
            ),
        }
    }

    fn do_choice(&mut self, cmd: &Json) -> Step {
        let branch = cmd
            .get("branchId")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let record_key = cmd
            .get("recordKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let options = cmd
            .get("options")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        let judged = self.branch_verdicts(&options);
        // A menu marks what was not offered.
        let closed: Vec<&str> = judged
            .iter()
            .filter(|o| o.verdict == Verdict::Closed)
            .map(|o| o.id.as_str())
            .collect();
        let menu = Menu {
            construct: MenuKind::Branch,
            id: &branch,
            addr: addr(cmd),
            presentation: 0,
            options: &judged,
        };
        let pick = self.driver.choose(&menu);
        let auto = matches!(pick, Pick::AutoFirst | Pick::HubAutoPass);
        let (chosen, scripted) = match pick {
            Pick::Option(id) => (Some(id), 0),
            Pick::AutoFirst | Pick::HubAutoPass => (
                judged
                    .iter()
                    .find(|o| o.verdict == Verdict::Open)
                    .map(|o| o.id.clone()),
                0,
            ),
            Pick::Unscripted { scripted } => (None, scripted),
        };
        let incomplete_rec = |note: &str| {
            let mut rec = serde_json::Map::new();
            rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
            rec.insert("kind".into(), Json::String("choice".into()));
            rec.insert("branch".into(), Json::String(branch.clone()));
            rec.insert("chose".into(), Json::Null);
            rec.insert("note".into(), Json::String(note.into()));
            if scripted > 1 {
                rec.insert("scripted".into(), json!(scripted));
            }
            if !closed.is_empty() {
                rec.insert("ineligible".into(), json!(closed));
            }
            Json::Object(rec)
        };
        let Some(forced) = chosen else {
            self.incomplete = true;
            let note = if auto {
                NOTE_NO_ELIGIBLE
            } else {
                NOTE_NO_DECISION
            };
            self.driver.emit(incomplete_rec(note));
            return Step::Halt;
        };
        let opt = match options
            .iter()
            .find(|o| o.get("id").and_then(Json::as_str) == Some(&forced))
        {
            Some(o) => o.clone(),
            None => {
                self.fatal = Some(format!("choice `{branch}` has no option `{forced}`"));
                return Step::Halt;
            }
        };
        // #20 / T8.5, D-C: `lute trace` refuses a forced selection whose guard
        // decided false, and `lute test` inherits that refusal. `run` played
        // it in full at exit 0 — one question, three tools, two answers. The
        // guard is in the artifact as `option.when` and this walk already
        // evaluates CEL everywhere else in it (`do_match`). The driver rules
        // on a pick that is not open ([`Driver::forced`]); `run` and `play`
        // refuse only a DECIDED false — an unknown guard read something with
        // no mock surface (`now()`/`validAt(...)`, a bridgeResult), and
        // refusing on that would refuse a legal replay.
        if !auto {
            if let Some(when) = opt.get("when").and_then(Json::as_str) {
                let verdict = self.option_verdict(when);
                if verdict != Verdict::Open {
                    match self.driver.forced(&menu, &forced, &verdict) {
                        Forced::Take => {}
                        Forced::Refuse => {
                            self.refuse(Self::refusal("branch", &branch, &forced, when, &verdict));
                            return Step::Halt;
                        }
                        Forced::Skip => {
                            self.incomplete = true;
                            self.driver.emit(incomplete_rec(NOTE_SKIPPED));
                            return Step::Halt;
                        }
                    }
                }
            }
        }
        if let Some(key) = record_key {
            self.state.insert(key, Value::Str(forced.clone()));
        }
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("choice".into()));
        rec.insert("branch".into(), Json::String(branch.clone()));
        rec.insert("chose".into(), Json::String(forced));
        if !closed.is_empty() {
            rec.insert("ineligible".into(), json!(closed));
        }
        self.driver.emit(Json::Object(rec));
        let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
        Step::Next(self.resolve(target))
    }

    /// The hub re-presentation loop: the driver answers ONE presentation at
    /// a time — a script's sequence is never consumed up front — so running
    /// out of scripted decisions can be told apart from a genuine natural
    /// convergence. Exhaustion with eligible options still standing halts
    /// incomplete (`self.incomplete = true`, exit 3) rather than silently
    /// converging. Every iteration consumes one decision or leaves the loop.
    fn do_hub(&mut self, cmd: &Json) -> Step {
        let id = cmd
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let record_key = cmd
            .get("recordKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        // dsl 0.23.0 §4: `<hub prompt>` rides every presentation record.
        let prompt = cmd.get("prompt").and_then(Json::as_str).map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let converge_idx = self.resolve(converge);
        let options = cmd
            .get("options")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        // Segment boundaries: every option target + the converge, so an option
        // body runs from its target up to the NEXT boundary (a non-`exit`
        // option falls through into the next option's body in the stream).
        let mut boundaries: Vec<usize> = options
            .iter()
            .filter_map(|o| o.get("target").and_then(Json::as_str))
            .map(|t| self.resolve(t))
            .collect();
        boundaries.push(converge_idx);
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut presentation = 0usize;
        // [`Pick::HubAutoPass`]: the next menu position the pass considers.
        let mut auto_at = 0usize;
        let mut visited_once: BTreeSet<String> = BTreeSet::new();
        loop {
            // Eligible = not an already-exhausted `once` option, and its
            // guard does not DECIDE false right now (an unknown guard stays
            // eligible — the same three-valued discipline `do_choice`'s
            // guard refusal uses). The spent and guard-closed options ride
            // the visit record, so a menu shows what was really offered.
            let mut judged = Vec::new();
            for o in &options {
                let Some(oid) = o.get("id").and_then(Json::as_str) else {
                    continue;
                };
                let once = o.get("once").and_then(Json::as_bool).unwrap_or(false);
                let verdict = if once && visited_once.contains(oid) {
                    Verdict::Spent
                } else {
                    match o.get("when").and_then(Json::as_str) {
                        Some(w) => self.option_verdict(w),
                        None => Verdict::Open,
                    }
                };
                judged.push(MenuOption {
                    id: oid.to_string(),
                    verdict,
                    exit: o.get("exit").and_then(Json::as_bool).unwrap_or(false),
                    once,
                });
            }
            let with = |v: fn(&Verdict) -> bool| -> Vec<&str> {
                judged
                    .iter()
                    .filter(|o| v(&o.verdict))
                    .map(|o| o.id.as_str())
                    .collect()
            };
            let spent = with(|v| *v == Verdict::Spent);
            let closed = with(|v| *v == Verdict::Closed);
            let any_eligible = judged
                .iter()
                .any(|o| !matches!(o.verdict, Verdict::Spent | Verdict::Closed));
            let marks = |rec: &mut serde_json::Map<String, Json>| {
                if !spent.is_empty() {
                    rec.insert("spent".into(), json!(spent));
                }
                if !closed.is_empty() {
                    rec.insert("ineligible".into(), json!(closed));
                }
            };
            let head = |chose: Json| {
                let mut rec = serde_json::Map::new();
                rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
                rec.insert("kind".into(), Json::String("hub".into()));
                rec.insert("hub".into(), Json::String(id.clone()));
                if let Some(p) = &prompt {
                    rec.insert("prompt".into(), Json::String(p.clone()));
                }
                rec.insert("chose".into(), chose);
                rec
            };

            let menu = Menu {
                construct: MenuKind::Hub,
                id: &id,
                addr: addr(cmd),
                presentation,
                options: &judged,
            };
            presentation += 1;
            let first_open =
                |exit: Option<bool>, from: usize| {
                    judged.iter().enumerate().skip(from).find(|(_, o)| {
                        o.verdict == Verdict::Open && exit.is_none_or(|e| o.exit == e)
                    })
                };
            let (choice_id, auto) = match self.driver.choose(&menu) {
                Pick::Option(c) => (Some(c), false),
                Pick::Unscripted { .. } => (None, false),
                Pick::AutoFirst => (first_open(None, 0).map(|(_, o)| o.id.clone()), true),
                Pick::HubAutoPass => match first_open(Some(false), auto_at) {
                    Some((i, o)) => {
                        auto_at = i + 1;
                        (Some(o.id.clone()), true)
                    }
                    None => {
                        auto_at = judged.len();
                        (first_open(Some(true), 0).map(|(_, o)| o.id.clone()), true)
                    }
                },
            };

            let Some(choice_id) = choice_id else {
                if !auto && !any_eligible {
                    // Natural convergence: nothing left eligible to present.
                    break;
                }
                // The decisions ran out but the hub would still be
                // re-presented (eligible options remain) — halt incomplete
                // rather than silently converging.
                self.incomplete = true;
                let mut rec = head(Json::Null);
                let note = if auto {
                    NOTE_NO_ELIGIBLE
                } else {
                    NOTE_NO_DECISION
                };
                rec.insert("note".into(), Json::String(note.into()));
                marks(&mut rec);
                self.driver.emit(Json::Object(rec));
                return Step::Halt;
            };

            let opt = match options
                .iter()
                .find(|o| o.get("id").and_then(Json::as_str) == Some(&choice_id))
            {
                Some(o) => o.clone(),
                None => {
                    self.fatal = Some(format!("hub `{id}` has no option `{choice_id}`"));
                    return Step::Halt;
                }
            };
            let once = opt.get("once").and_then(Json::as_bool).unwrap_or(false);
            let is_exit = opt.get("exit").and_then(Json::as_bool).unwrap_or(false);
            if !auto {
                // A repeat force of a spent `once` option: `lute run` (the
                // conformance contract) skips it; `lute play` refuses it, as
                // `lute trace` does — a silent skip lets the rest of the
                // script drift out of step with what the player was offered.
                if once && visited_once.contains(&choice_id) {
                    match self.driver.forced(&menu, &choice_id, &Verdict::Spent) {
                        Forced::Take => {}
                        Forced::Skip => continue,
                        Forced::Refuse => {
                            self.refuse(Self::refusal("hub", &id, &choice_id, "", &Verdict::Spent));
                            return Step::Halt;
                        }
                    }
                }
                // Same rule as `do_choice` (#20, D-C). A hub option is
                // presented repeatedly, so this is evaluated per visit
                // against live state — a guard false on the first pass may
                // be true on the third, which is precisely what a hub is for.
                if let Some(when) = opt.get("when").and_then(Json::as_str) {
                    let verdict = self.option_verdict(when);
                    if verdict != Verdict::Open {
                        match self.driver.forced(&menu, &choice_id, &verdict) {
                            Forced::Take => {}
                            Forced::Skip => continue,
                            Forced::Refuse => {
                                self.refuse(Self::refusal("hub", &id, &choice_id, when, &verdict));
                                return Step::Halt;
                            }
                        }
                    }
                }
            }
            if let Some(key) = &record_key {
                self.state
                    .insert(key.clone(), Value::Str(choice_id.clone()));
            }
            // hub visit record slot (scene.visited.<hub>.<opt>, state-lifecycle.md).
            self.state
                .insert(format!("scene.visited.{id}.{choice_id}"), Value::Bool(true));
            let mut rec = head(Json::String(choice_id.clone()));
            marks(&mut rec);
            self.driver.emit(Json::Object(rec));
            let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
            let start = self.resolve(target);
            let stop = boundaries
                .iter()
                .find(|&&b| b > start)
                .copied()
                .unwrap_or(self.commands.len());
            self.run_range(start, stop);
            if self.fatal.is_some() || self.incomplete || self.terminated {
                return Step::Halt;
            }
            if once {
                visited_once.insert(choice_id);
            }
            if is_exit {
                break;
            }
        }
        Step::Next(converge_idx)
    }

    fn do_match(&mut self, cmd: &Json) -> Step {
        let arms = cmd
            .get("arms")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        for (i, arm) in arms.iter().enumerate() {
            // An `is`-form arm compiles to an EMPTY `test` plus a structured
            // `expr` (IR A13, `stage.rs::walk_match`) — the executable surface
            // an engine must read. A `test`-form arm carries raw CEL. Prefer
            // the structured expr whenever present; falling back to the raw
            // `test` keeps pre-A13 artifacts working. Evaluating ONLY `test`
            // here was a defect: every `is` arm read as empty→unknown and the
            // whole match fell through to `otherwise`.
            let matched = match arm.get("expr") {
                Some(expr) => self.expr_node_truthy(expr),
                None => {
                    let test = arm.get("test").and_then(Json::as_str).unwrap_or("");
                    self.truthy(test)
                }
            };
            if matched == Some(true) {
                let target = arm.get("target").and_then(Json::as_str).unwrap_or(converge);
                self.driver.emit(json!({
                    "addr": addr(cmd),
                    "kind": "match",
                    "result": format!("arm {}", i + 1),
                }));
                return Step::Next(self.resolve(target));
            }
        }
        // No arm matched → otherwise, else converge.
        let (result, target) = match cmd.get("otherwise").and_then(Json::as_str) {
            Some(o) => ("otherwise".to_string(), o),
            None => ("converge".to_string(), converge),
        };
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "match",
            "result": result,
        }));
        Step::Next(self.resolve(target))
    }

    fn rec_barrier(&mut self, cmd: &Json) {
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "barrier",
            "timeline": cmd.get("timeline").cloned().unwrap_or(Json::Null),
            "at": cmd.get("at").cloned().unwrap_or(Json::Null),
            "note": "timeline join — no real clock simulated",
        }));
    }

    /// Record the `end` record and mark the walk over (dsl 0.8.0). `reason` is
    /// optional in the IR; it rides the transcript as JSON `null` when absent so
    /// the machine record's key set never varies with authoring.
    fn rec_end(&mut self, cmd: &Json) {
        self.terminated = true;
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "end",
            "reason": cmd.get("reason").cloned().unwrap_or(Json::Null),
        }));
    }

    /// A `plugin` command (bridge-protocol.md). Effects that need no bridge
    /// result apply. A `bridgeResult` effect reads the driver's answer to
    /// the call ([`Driver::bridge`]; dsl 0.24.0 §5): the answered values are
    /// written, typed by each result slot's declared type; a field the
    /// answer leaves out (dsl 0.25.0 §7: one no content reads) is
    /// unresolved. With no answer the effects are recorded unresolved and
    /// the walk goes on (no host bridge is invoked) — unless content reads
    /// one of the call's result slots and the driver halts at the
    /// [`SiteKind::BridgeResult`] site (`lute play`): the walk then stops
    /// AT the call, incomplete, before anything after it — a default arm
    /// over the result slot included — is walked. `false` = the walk stops
    /// here (that halt, or an answer that does not fit the call, which is
    /// fatal).
    fn exec_plugin(&mut self, cmd: &Json) -> bool {
        let tag = cmd
            .get("tag")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let effects = cmd
            .get("effects")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let reads: Vec<(String, String)> = effects
            .iter()
            .filter_map(|e| {
                let field = e.get("from")?.get("bridgeResult")?.as_str()?;
                let path = e.get("path").and_then(Json::as_str).unwrap_or("");
                Some((field.to_string(), path.to_string()))
            })
            .collect();
        let answer = if reads.is_empty() {
            None
        } else {
            let call = BridgeCall {
                tag: &tag,
                addr: addr(cmd),
                reads: &reads,
            };
            match self.driver.bridge(&call) {
                BridgeReply::Answer(a) => Some(a),
                BridgeReply::Unanswered => None,
            }
        };
        // dsl 0.25.0 §7: whether content reads one of this call's results.
        let read = reads
            .iter()
            .any(|(_, p)| self.bridge_reads.paths.contains(p));
        let halt = answer.is_none()
            && read
            && self.driver.unknown(&UnknownSite {
                kind: SiteKind::BridgeResult,
                id: &tag,
                addr: addr(cmd),
                raw: "",
                atoms: &[],
            }) == OnUnknown::Halt;
        let answered = match &answer {
            Some(a) => match self.bridge_values(&tag, &reads, a) {
                Ok(values) => Some(values),
                Err(msg) => {
                    self.fatal = Some(msg);
                    return false;
                }
            },
            None if halt => {
                self.incomplete = true;
                // The fields every answer to the tag gives (dsl 0.25.0 §7).
                let (fields, paths): (Vec<&str>, Vec<&str>) = reads
                    .iter()
                    .filter(|(f, _)| self.bridge_reads.reads(&tag, f))
                    .map(|(f, p)| (f.as_str(), p.as_str()))
                    .unzip();
                self.driver.emit(json!({
                    "addr": addr(cmd),
                    "kind": "plugin",
                    "tag": tag,
                    "external": true,
                    "unresolvedEffects": paths,
                    "unanswered": fields,
                    "note": "external bridge call — no `bridges:` answer; halted at the call",
                }));
                return false;
            }
            None => None,
        };
        let mut unresolved = Vec::new();
        for e in &effects {
            let path = e
                .get("path")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let Some(from) = e.get("from") else {
                continue;
            };
            if let Some(lit) = from.as_bool() {
                self.state.insert(path, Value::Bool(lit));
            } else if let Some(n) = from.as_f64() {
                self.state.insert(path, Value::Num(n));
            } else if let Some(s) = from.as_str() {
                self.state.insert(path, Value::Str(s.to_string()));
            } else if from.get("op").is_some() {
                let by = from.get("by").and_then(Json::as_f64).unwrap_or(0.0);
                let cur = match self.state.get(&path) {
                    Some(Value::Num(n)) => *n,
                    _ => 0.0,
                };
                let op = from.get("op").and_then(Json::as_str).unwrap_or("");
                let v = match op {
                    "increment" => cur + by,
                    "decrement" => cur - by,
                    _ => cur,
                };
                self.state.insert(path, Value::Num(v));
            } else if let Some(field) = from.get("bridgeResult").and_then(Json::as_str) {
                match answered
                    .as_ref()
                    .and_then(|a| a.iter().find(|(f, _)| f == field))
                {
                    Some((_, v)) => {
                        self.state.insert(path, v.clone());
                    }
                    None => unresolved.push(path),
                }
            }
        }
        let rec = match answered {
            Some(values) => {
                let fields: Vec<Json> = values
                    .iter()
                    .map(|(f, v)| json!({ "field": f, "value": value_to_json(v) }))
                    .collect();
                json!({
                    "addr": addr(cmd),
                    "kind": "plugin",
                    "tag": tag,
                    "external": true,
                    "unresolvedEffects": unresolved,
                    "answered": fields,
                    "note": "external bridge call — answered from `bridges:`",
                })
            }
            None => json!({
                "addr": addr(cmd),
                "kind": "plugin",
                "tag": tag,
                "external": true,
                "unresolvedEffects": unresolved,
                "note": "external bridge call — not invoked; bridgeResult effects unresolved",
            }),
        };
        self.driver.emit(rec);
        true
    }

    /// One `bridges:` answer against the call it answers (dsl 0.24.0 §5):
    /// only `bridgeResult` fields the call's effects read, every one content
    /// reads among them (dsl 0.25.0 §7), each a literal of its result slot's
    /// declared type. `(field, value)` in the effects' order; `Err` names the
    /// misfit (a usage error).
    fn bridge_values(
        &self,
        tag: &str,
        reads: &[(String, String)],
        answer: &crate::BridgeAnswer,
    ) -> Result<Vec<(String, Value)>, String> {
        let fields: Vec<&str> = reads.iter().map(|(f, _)| f.as_str()).collect();
        let at = format!("the `bridges.{tag}` answer to plugin call `{tag}`");
        if let Some((extra, _)) = answer.iter().find(|(f, _)| !fields.contains(&f.as_str())) {
            return Err(format!(
                "{at} gives `{extra}`, which no effect of the call reads (it reads: {})",
                fields.join(", ")
            ));
        }
        let mut out = Vec::with_capacity(reads.len());
        for (field, path) in reads {
            if out.iter().any(|(f, _): &(String, Value)| f == field) {
                continue;
            }
            let Some((_, lit)) = answer.iter().find(|(f, _)| f == field) else {
                if !self.bridge_reads.reads(tag, field) {
                    continue;
                }
                let read: Vec<&str> = fields
                    .iter()
                    .copied()
                    .filter(|f| self.bridge_reads.reads(tag, f))
                    .collect();
                return Err(format!(
                    "{at} lacks `{field}`, which content reads — an answer gives every bridge \
                     result of the call content reads ({})",
                    read.join(", ")
                ));
            };
            // dsl 0.26.0 §3.1: typed by the result slot, else by the bridge
            // capability's `result:` shape; an untyped answer is refused —
            // never stored as a string that no bool/number read can match.
            let ty = self
                .types
                .get(path)
                .map(String::as_str)
                .or_else(|| self.bridge_reads.result_type(tag, field));
            let Some(ty) = ty else {
                return Err(format!(
                    "{at}: `{field}` lands on `{path}`, which no state slot of this artifact \
                     declares, and no bridge capability declares a `result:` type for it — \
                     an untyped answer is refused (dsl 0.26.0 §3.1)"
                ));
            };
            let v = match ty {
                "bool" => match lit.as_str() {
                    "true" => Some(Value::Bool(true)),
                    "false" => Some(Value::Bool(false)),
                    _ => None,
                },
                "number" => lit.parse::<f64>().ok().map(Value::Num),
                _ => Some(Value::Str(lit.clone())),
            };
            let Some(v) = v else {
                return Err(format!(
                    "{at}: `{field}: {lit}` does not fit `{path}`, a `{ty}`"
                ));
            };
            out.push((field.clone(), v));
        }
        Ok(out)
    }

    // ── quest lifecycle (quest-lifecycle.md) ────────────────────────────

    /// The quest artifact's declarations: its quests, its `<on>` handlers
    /// (each with its enclosing quest), and the body-segment boundaries —
    /// every objective body and every `<on>` body.
    fn quest_program(&self) -> (Vec<QuestDecl>, Vec<Handler>, Vec<usize>) {
        let mut quests: Vec<QuestDecl> = Vec::new();
        let mut handlers: Vec<Handler> = Vec::new();
        for cmd in &self.commands {
            match cmd.get("kind").and_then(Json::as_str) {
                Some("quest") => quests.push(parse_quest(cmd)),
                Some("on") => {
                    handlers.push(Handler {
                        event: cmd
                            .get("event")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        when: cel_raw(cmd.get("when")),
                        body: cmd
                            .get("body")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        // Stream order recovers the enclosing quest: the `on`
                        // record is emitted inside its quest's walk, after the
                        // quest declaration head (stage.rs `walk_quest`).
                        quest: quests.last().map(|q| q.id.clone()),
                        target: cmd.get("target").and_then(Json::as_str).map(str::to_string),
                    });
                }
                _ => {}
            }
        }
        let mut seg_starts: Vec<usize> = Vec::new();
        for q in &quests {
            for o in &q.objectives {
                if let Some(b) = &o.body {
                    seg_starts.push(self.resolve(b));
                }
            }
        }
        for h in &handlers {
            seg_starts.push(self.resolve(&h.body));
        }
        seg_starts.sort_unstable();
        seg_starts.dedup();
        (quests, handlers, seg_starts)
    }

    /// `lute play` (dsl 0.24.0 §2): run the `<on>` handler bodies a `judge:
    /// before` raise answered ([`Machine::with_deferred_handlers`]) — after
    /// the occasion's beats, in the order they fired. Their `when` was
    /// decided when they fired; a `::end` in one ends the rest.
    pub fn run_deferred_handlers(&mut self, bodies: &[String]) -> Result<(), String> {
        self.quest_resume = true;
        let (_, _, seg_starts) = self.quest_program();
        for body in bodies {
            if self.terminated {
                break;
            }
            self.run_segment(body, &seg_starts);
        }
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    fn run_quest(&mut self) {
        let (quests, handlers, seg_starts) = self.quest_program();

        // A fresh walk (`lute run`) starts every quest `unset`. A resumed one
        // (`lute play`) keeps each carried status and registers only a quest it
        // has never seen — populating its `quest.<id>.state` as `unset` so a
        // beat `when` over it decides instead of reading an unset path.
        for q in &quests {
            if !self.quest_resume {
                self.quest_status.insert(q.id.clone(), "unset".to_string());
            } else if !self.quest_status.contains_key(&q.id) {
                self.quest_status.insert(q.id.clone(), "unset".to_string());
                self.state
                    .insert(format!("quest.{}.state", q.id), Value::Str("unset".into()));
            }
        }

        // Parent→child edges (subquest design 2026-08-31 §2.4/§3): a child is
        // any quest some objective references via `ObjectiveEntry.quest`.
        let parent_of: BTreeMap<String, String> = quests
            .iter()
            .flat_map(|q| {
                q.objectives
                    .iter()
                    .filter_map(|o| o.quest.clone().map(|c| (c, q.id.clone())))
            })
            .collect();

        // Activation (quest-lifecycle.md §Activation). A REFERENCED child is
        // parent-activation-driven (§2.4): it never activates at walk start
        // and is not accept-driven — `reevaluate` activates it once its parent
        // is `active` (no `start`: immediately; with `start`: when the
        // predicate holds while the parent is active). An unreferenced quest
        // with no `start` is ACCEPT-DRIVEN (dsl 0.21.0 §7a.3): it stays
        // `unset` until a mock `accepts:` entry or an `accept` record names
        // it — the rule `lute trace` (dsl 0.4.0 §4.4) always applied.
        for q in &quests {
            if parent_of.contains_key(&q.id)
                || self.quest_status.get(&q.id).map(String::as_str) != Some("unset")
            {
                continue;
            }
            let activate = match &q.start {
                None => false,
                Some(raw) => self.truthy(raw) == Some(true),
            } || self.is_accepted(&q.id);
            if activate {
                self.set_quest_state(&q.id, "active", None);
                self.fire_event("questActive", Some(&q.id), None, &handlers, &seg_starts);
            }
        }

        // Track which objectives have completed (monotone). A resumed walk
        // seeds this from the carried `quest.<id>.objectives.<oid>.done`, so an
        // objective completed by an earlier advance never completes again.
        let mut done: BTreeSet<(usize, usize)> = BTreeSet::new();
        if self.quest_resume {
            for (qi, q) in quests.iter().enumerate() {
                for (oi, o) in q.objectives.iter().enumerate() {
                    let path = format!("quest.{}.objectives.{}.done", q.id, o.id);
                    if self.state.get(&path) == Some(&Value::Bool(true)) {
                        done.insert((qi, oi));
                    }
                }
            }
        }
        // dsl 0.24.0 §2.1: the settles before this walk's raises defer the
        // `by` of the `on=` objectives those raises judge.
        self.defer_by.extend(self.seed.occasions.iter().cloned());
        self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);

        // Mock events fire in order; each re-evaluates the lifecycle. An `end`
        // inside a handler/objective body ends the WALK (dsl 0.8.0), so no
        // later event is delivered — nothing downstream of the terminator runs.
        let events: Vec<String> = self.seed.events.clone();
        for ev in events {
            if self.terminated {
                break;
            }
            self.fire_event(&ev, None, None, &handlers, &seg_starts);
            self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);
        }

        // dsl 0.21.0 §7a.2: occasions are raised in order after the walk
        // settles; each judges the `on="<occasion>"` objectives of every
        // active quest, then the lifecycle settles again. `lute run` records
        // the raise; `lute play`'s step header already names it. 0.23.1: a
        // raise also fires the same-named world event — the `<on event>`
        // handlers run first, then the occasion judges. (A lifecycle event
        // name is never raised this way: the runner fires those itself.)
        let occasions: Vec<String> = self.seed.occasions.clone();
        for occasion in &occasions {
            if self.terminated {
                break;
            }
            let (name, target) = crate::split_occasion(occasion);
            if !self.quest_resume {
                let mut rec = json!({ "kind": "occasion", "occasion": name });
                if let Some(t) = target {
                    rec["target"] = json!(t);
                }
                self.driver.emit(rec);
            }
            if !lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name) {
                // dsl 0.24.0 §2: an `<on target>` answers only a raise for
                // its target.
                self.fire_event(name, None, target, &handlers, &seg_starts);
                if self.terminated {
                    break;
                }
            }
            self.judge_occasion(occasion, &quests, &seg_starts, &mut done);
            self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);
        }

        // Incomplete if an active quest is stuck on an undecidable required
        // objective (a missing mock left the `done` predicate unknown). An
        // `end` record makes this moot: the author declared the walk finished,
        // so an unsettled objective is a deliberate outcome, not a missing mock.
        if self.terminated {
            return;
        }
        for (qi, q) in quests.iter().enumerate() {
            if self.quest_status.get(&q.id).map(String::as_str) == Some("active") {
                for (oi, o) in q.objectives.iter().enumerate() {
                    if o.optional
                        || done.contains(&(qi, oi))
                        || self
                            .failed_objectives
                            .contains(&format!("{}.{}", q.id, o.id))
                    {
                        continue;
                    }
                    // An `on=` objective is judged only at its occasion (for
                    // its target, dsl 0.23.0 §2); one this walk never raised
                    // is not stuck, it is waiting.
                    let judged = o.on.as_ref().is_none_or(|on| {
                        occasions
                            .iter()
                            .any(|r| crate::raise_judges(r, on, o.target.as_deref()))
                    });
                    // dsl 0.23.0 §2 / 0.24.0 §2.1: an undecidable deadline
                    // could still fail the quest — as stuck as an undecidable
                    // `done`. `by` is judged at every settle; `until` only
                    // where the objective is judged.
                    let unknown = |this: &mut Self, slot: &Option<String>| {
                        slot.as_ref()
                            .is_some_and(|c| this.eval_raw(c) == Value::Unknown)
                    };
                    let (key, stuck) = if judged && self.eval_raw(&o.done) == Value::Unknown {
                        ("done", true)
                    } else if unknown(self, &o.by) || (judged && unknown(self, &o.until)) {
                        ("failed", true)
                    } else {
                        ("done", false)
                    };
                    if stuck {
                        self.incomplete = true;
                        // `lute play` names the stuck objective in its halt;
                        // `lute run`'s transcript is unchanged.
                        if self.quest_resume {
                            self.driver.emit(json!({
                                "kind": "objective",
                                "quest": q.id,
                                "objective": o.id,
                                key: Json::Null,
                            }));
                        }
                    }
                }
            }
        }
    }

    /// `lute play` (dsl 0.21.0 §6, D-H): advance this quest artifact's
    /// lifecycle over carried-over state, facts and statuses exactly as
    /// [`Machine::run`] settles it for `lute run` — activation, objectives,
    /// `fail` before completion, `<on>` handlers, `<reward>` grants — but
    /// RESUMED ([`Machine::quest_resume`]): nothing already active, terminal or
    /// done transitions again. Called once per quest artifact after every
    /// presentation, so later beat conditions see real quest progress.
    pub fn advance_quests(&mut self) -> Result<(), String> {
        self.quest_resume = true;
        self.run_quest();
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// `lute play` (dsl 0.21.0 §4): decide one beat `when` over this runner's
    /// live snapshot — state plus the Datalog fixpoint — through the same
    /// [`Machine::eval_raw`] chokepoint every guard of a walk uses. `Err`
    /// carries the [`UnresolvedAtom`]s of an undecided (or non-bool) result.
    pub fn eval_guard(&mut self, raw: &str) -> Result<bool, Vec<UnresolvedAtom>> {
        let before = self.unresolved.len();
        match self.eval_raw(raw) {
            Value::Bool(b) => Ok(b),
            _ => Err(self.unresolved.split_off(before)),
        }
    }

    /// `lute play` (dsl 0.22.0 §4): every fact that holds over this runner's
    /// live snapshot — base facts plus the derived fixpoint (base only under
    /// `derive: false`) — the end-of-play `facts:` expectations judge.
    pub fn all_facts(&self) -> &BTreeSet<Fact> {
        &self.all_facts
    }

    /// Re-evaluate the quest lifecycle to a fixpoint: referenced-child
    /// activation (subquest §2.4), then per active quest objectives
    /// (monotone), then `fail` before derived completion (quest-lifecycle.md
    /// §Re-evaluation cadence). A terminal transition cascades every
    /// still-`active` child to `failed` (subquest §2.3, recursive) — the one
    /// downward rule a per-artifact compile cannot synthesize.
    fn reevaluate(
        &mut self,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
        handlers: &[Handler],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        let mut changed = true;
        let mut rounds = 0;
        while changed && !self.terminated && rounds < quests.len() * 8 + 16 {
            changed = false;
            rounds += 1;
            // 0. referenced-child activation (§2.4): a pending child whose
            // parent is `active` activates — immediately without `start`, or
            // when its `start` predicate holds (evaluated only while the
            // parent is active). dsl 0.24.0 §2: an `activate="accept"`
            // child additionally waits for an accept naming it. An
            // unreferenced accept-driven quest activates once an `accept`
            // record this walk ran names it (dsl 0.21.0 §7a.3 — an accept
            // inside a handler/objective body).
            for q in quests {
                if self.quest_status.get(&q.id).map(String::as_str) != Some("unset") {
                    continue;
                }
                let activate = match parent_of.get(&q.id) {
                    Some(parent) => {
                        if self.quest_status.get(parent).map(String::as_str) != Some("active") {
                            continue;
                        }
                        match &q.start {
                            None if q.accept_activated => self.is_accepted(&q.id),
                            None => true,
                            Some(raw) => self.truthy(raw) == Some(true),
                        }
                    }
                    None => q.start.is_none() && self.is_accepted(&q.id),
                };
                if activate {
                    self.set_quest_state(&q.id, "active", None);
                    self.fire_event("questActive", Some(&q.id), None, handlers, seg_starts);
                    changed = true;
                }
            }
            for (qi, q) in quests.iter().enumerate() {
                if self.quest_status.get(&q.id).map(String::as_str) != Some("active") {
                    continue;
                }
                // 1. objectives (monotone; body plays once). An `on=`
                // objective is judged only at its occasion
                // ([`Machine::judge_occasion`]), never continuously; a failed
                // one never again.
                for (oi, o) in q.objectives.iter().enumerate() {
                    if o.on.is_some()
                        || done.contains(&(qi, oi))
                        || self
                            .failed_objectives
                            .contains(&format!("{}.{}", q.id, o.id))
                    {
                        continue;
                    }
                    if self.truthy(&o.done) == Some(true) {
                        self.complete_objective(q, qi, oi, seg_starts, done);
                        changed = true;
                    }
                }
                // 1b. dsl 0.23.0 §2, 0.24.0 §2.1: deadlines, after the
                // objectives were judged (`done` wins a tie) — a not-done
                // objective whose `by` is true fails the first time, `on=`
                // or not: a deadline is a moment, not a place. (`until` is
                // judged only at the occasion, [`Machine::judge_occasion`].)
                // An `on=` objective a raise still to come in this step
                // judges waits for it: its `done` is judged first.
                for (oi, o) in q.objectives.iter().enumerate() {
                    let deferred = o.on.as_deref().is_some_and(|on| {
                        self.defer_by
                            .iter()
                            .any(|r| crate::raise_judges(r, on, o.target.as_deref()))
                    });
                    if !deferred {
                        changed |= self.judge_deadline(q, qi, oi, done, "by");
                    }
                }
                // 2. fail BEFORE derived completion (§6.3 precedence): an
                // authored `fail`, or a required objective whose `by`
                // failed it (in this settle or at the raise before it).
                // dsl 0.24.0 §2: a `complete="any"` quest is not failed by
                // one missed alternative — its synthesized `fail` fails it
                // once every required objective has failed.
                let missed = if q.complete_any {
                    None
                } else {
                    q.objectives.iter().find_map(|o| {
                        let key = format!("{}.{}", q.id, o.id);
                        (!o.optional && self.failed_objectives.contains(&key))
                            .then(|| self.objective_failed_by.get(&key).copied().unwrap_or("by"))
                    })
                };
                let failed_by = match missed {
                    Some(kind) => Some(kind),
                    None => q
                        .fail
                        .as_ref()
                        .is_some_and(|fail| self.truthy(fail) == Some(true))
                        .then_some("fail"),
                };
                if let Some(reason) = failed_by {
                    self.set_quest_failed(&q.id, reason);
                    // dsl 0.16.0 §3 D-D: fresh `failed` → grant
                    // `on="failed"` quest rewards BEFORE `questFailed`
                    // handlers and BEFORE the §2.3 downward cascade.
                    self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Failed);
                    self.fire_event("questFailed", Some(&q.id), None, handlers, seg_starts);
                    self.cascade_children(
                        &q.id, "cascade", quests, parent_of, handlers, seg_starts,
                    );
                    changed = true;
                    continue;
                }
                // 3. derived completion: all non-optional objectives done —
                // or, for `complete="any"` (dsl 0.24.0 §2), any one of them.
                let complete = if q.complete_any {
                    q.objectives
                        .iter()
                        .enumerate()
                        .any(|(oi, o)| !o.optional && done.contains(&(qi, oi)))
                } else {
                    q.objectives
                        .iter()
                        .enumerate()
                        .all(|(oi, o)| o.optional || done.contains(&(qi, oi)))
                };
                if complete {
                    self.set_quest_state(&q.id, "complete", None);
                    // dsl 0.16.0 §3 D-D: fresh `complete` → grant this
                    // quest's default-on rewards BEFORE `questComplete`
                    // handlers play.
                    self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Complete);
                    self.fire_event("questComplete", Some(&q.id), None, handlers, seg_starts);
                    // dsl 0.24.0 §2: the alternatives an `any` quest did not
                    // take are superseded, not cascaded.
                    let reason = if q.complete_any {
                        "superseded"
                    } else {
                        "cascade"
                    };
                    self.cascade_children(&q.id, reason, quests, parent_of, handlers, seg_starts);
                    changed = true;
                }
            }
        }
    }

    /// A fresh `done` (monotone — `done` records it; the body plays once):
    /// write `quest.<id>.objectives.<oid>.done`, record it, fire the
    /// objective's rewards (dsl 0.16.0 §3 D-D: BEFORE the body runs, and
    /// before any quest-level grant or `questComplete` handler), then play
    /// the completion body.
    fn complete_objective(
        &mut self,
        q: &QuestDecl,
        qi: usize,
        oi: usize,
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        let o = &q.objectives[oi];
        done.insert((qi, oi));
        self.state.insert(
            format!("quest.{}.objectives.{}.done", q.id, o.id),
            Value::Bool(true),
        );
        self.driver.emit(json!({
            "kind": "objective",
            "quest": q.id,
            "objective": o.id,
            "done": true,
        }));
        self.emit_grants(&q.id, Some(&o.id), &o.rewards, GrantEvent::Objective);
        if let Some(body) = &o.body {
            self.run_segment(body, seg_starts);
        }
    }

    /// dsl 0.21.0 §7a.2: raise `occasion` — every ACTIVE quest judges its
    /// not-yet-done `on="<occasion>"` objectives, document order. The raise
    /// is `name` or `name@target` (dsl 0.23.0 §2): an objective with a
    /// `target` is judged only by a raise for it; a failed one never. Its
    /// `until` is judged here, after its `done`; its `by` — deferred by the
    /// settles of the step until this raise ([`Machine::defer_by`]) — at the
    /// caller's settle right after (`fail` before completion), so `done`
    /// wins over a deadline that came true in this step.
    fn judge_occasion(
        &mut self,
        occasion: &str,
        quests: &[QuestDecl],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        self.defer_by.retain(|r| r != occasion);
        for (qi, q) in quests.iter().enumerate() {
            let judged: Vec<usize> = q
                .objectives
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.on.as_deref()
                        .is_some_and(|on| crate::raise_judges(occasion, on, o.target.as_deref()))
                })
                .map(|(oi, _)| oi)
                .collect();
            for &oi in &judged {
                if self.terminated
                    || self.quest_status.get(&q.id).map(String::as_str) != Some("active")
                {
                    break;
                }
                let o = &q.objectives[oi];
                if done.contains(&(qi, oi))
                    || self
                        .failed_objectives
                        .contains(&format!("{}.{}", q.id, o.id))
                {
                    continue;
                }
                if self.truthy(&o.done) == Some(true) {
                    self.complete_objective(q, qi, oi, seg_starts, done);
                }
            }
            for &oi in &judged {
                if self.terminated
                    || self.quest_status.get(&q.id).map(String::as_str) != Some("active")
                {
                    break;
                }
                self.judge_deadline(q, qi, oi, done, "until");
            }
        }
    }

    /// dsl 0.23.0 §2, 0.24.0 §2.1: judge objective `oi`'s deadline `kind` —
    /// `by` (every settle) or `until` (at its occasion's raise) — skipped
    /// when it has none, is done, or already failed. The first time it is
    /// true the objective fails (recorded with the kind as its `failedBy`);
    /// a failed required objective fails its quest at the next settle.
    /// `true` when it failed now.
    fn judge_deadline(
        &mut self,
        q: &QuestDecl,
        qi: usize,
        oi: usize,
        done: &BTreeSet<(usize, usize)>,
        kind: &'static str,
    ) -> bool {
        let o = &q.objectives[oi];
        let slot = if kind == "until" { &o.until } else { &o.by };
        let Some(cond) = slot else { return false };
        let key = format!("{}.{}", q.id, o.id);
        if done.contains(&(qi, oi)) || self.failed_objectives.contains(&key) {
            return false;
        }
        if self.truthy(cond) != Some(true) {
            return false;
        }
        self.record_objective_failure(&q.id, &o.id, kind);
        self.driver.emit(json!({
            "kind": "objective",
            "quest": q.id,
            "objective": o.id,
            "failed": true,
            "failedBy": kind,
        }));
        true
    }

    /// dsl 0.24.0 §2: objective `objective` of `quest` failed for `kind`
    /// (`by` / `until`): never judged again, the reserved
    /// `quest.<id>.objectives.<oid>.failed` reads `true`, and a required
    /// one fails its quest with that `failedBy`.
    fn record_objective_failure(&mut self, quest: &str, objective: &str, kind: &'static str) {
        let key = format!("{quest}.{objective}");
        self.state.insert(
            format!("quest.{quest}.objectives.{objective}.failed"),
            Value::Bool(true),
        );
        self.objective_failed_by.insert(key.clone(), kind);
        self.failed_objectives.insert(key);
    }

    /// Downward cascade (subquest design 2026-08-31 §2.3): on `terminal`'s
    /// terminal transition, every still-`active` child transitions to
    /// `failed` and fires ITS OWN `questFailed` handlers; recursive (a
    /// cascaded failure is itself a terminal transition). A required child
    /// cannot be `active` when its parent completes (its completion is part
    /// of the parent's derived completion), so the `complete` arm only ever
    /// fails running optionals. dsl 0.24.0 §2: `reason` is the direct
    /// children's `failedBy` — `cascade`, or `superseded` when a
    /// `complete="any"` parent completed; deeper levels are `cascade`.
    fn cascade_children(
        &mut self,
        terminal: &str,
        reason: &'static str,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let mut stack = vec![(terminal.to_string(), reason)];
        while let Some((parent, reason)) = stack.pop() {
            if self.terminated {
                return;
            }
            let children: Vec<String> = quests
                .iter()
                .filter(|q| {
                    parent_of.get(&q.id) == Some(&parent)
                        && self.quest_status.get(&q.id).map(String::as_str) == Some("active")
                })
                .map(|q| q.id.clone())
                .collect();
            for child in children {
                // dsl 0.16.0 §3 D-D: cascade-fail IS a fresh `failed`
                // transition (§2.3). Grant `on="failed"` rewards on the
                // cascaded child before firing its own `questFailed` — same
                // ordering as an authored `fail`, so a consumer reads no
                // structural difference.
                let child_rewards = quests
                    .iter()
                    .find(|q| q.id == child)
                    .map(|q| q.rewards.as_slice())
                    .unwrap_or(&[]);
                self.set_quest_failed(&child, reason);
                self.emit_grants(&child, None, child_rewards, GrantEvent::Failed);
                self.fire_event("questFailed", Some(&child), None, handlers, seg_starts);
                stack.push((child, "cascade"));
            }
        }
    }

    /// The `→ failed` transition with its reason (dsl 0.24.0 §2): the
    /// reserved `quest.<id>.failedBy` reads `fail`, `by`, `until`,
    /// `cascade` or `superseded` from here on; the transcript record
    /// carries it as `failedBy`.
    fn set_quest_failed(&mut self, id: &str, reason: &str) {
        self.set_quest_state(id, "failed", Some(reason));
    }

    /// The `→ state` transition: the reserved `quest.<id>.state`, the
    /// status, and the `quest` record (with `failedBy` for a failure, whose
    /// reserved path is written after the state's).
    fn set_quest_state(&mut self, id: &str, state: &str, failed_by: Option<&str>) {
        self.state
            .insert(format!("quest.{id}.state"), Value::Str(state.to_string()));
        self.quest_status.insert(id.to_string(), state.to_string());
        let mut rec = json!({
            "kind": "quest",
            "quest": id,
            "state": state,
        });
        if let Some(reason) = failed_by {
            self.state.insert(
                format!("quest.{id}.failedBy"),
                Value::Str(reason.to_string()),
            );
            rec["failedBy"] = json!(reason);
        }
        self.driver.emit(rec);
    }

    /// dsl 0.16.0 §3 D-D: emit a `grant` transcript record for every
    /// entry in `rewards` (declaration order) whose `on=` filter matches
    /// `event` AND whose `when=` gate is `Some(true)` against the LIVE
    /// state at the grant instant. `objective_id: Some(oid)` marks an
    /// objective-level grant (the checker rejects `on=` on those, so the
    /// filter is a no-op here — passed as [`GrantEvent::Objective`]).
    /// Ranges are passed through verbatim on the wire (spec D-C: never
    /// pre-rolled — `amountMin`/`amountMax` land on the transcript entry
    /// exactly as the artifact carried them).
    fn emit_grants(
        &mut self,
        quest_id: &str,
        objective_id: Option<&str>,
        rewards: &[RewardRec],
        event: GrantEvent,
    ) {
        for r in rewards {
            if r.kind.trim().is_empty() {
                continue;
            }
            let on_failed = matches!(r.on.as_deref(), Some("failed"));
            let matches = match event {
                GrantEvent::Objective => true,
                GrantEvent::Complete => !on_failed,
                GrantEvent::Failed => on_failed,
            };
            if !matches {
                continue;
            }
            if let Some(raw) = &r.when {
                if self.truthy(raw) != Some(true) {
                    continue;
                }
            }
            let mut reward = serde_json::Map::new();
            reward.insert("kind".into(), Json::String(r.kind.clone()));
            if let Some(t) = &r.target {
                reward.insert("target".into(), Json::String(t.clone()));
            }
            if let Some(n) = r.amount {
                reward.insert("amount".into(), json!(n));
            }
            if let Some(lo) = r.amount_min {
                reward.insert("amountMin".into(), json!(lo));
            }
            if let Some(hi) = r.amount_max {
                reward.insert("amountMax".into(), json!(hi));
            }
            let mut rec = serde_json::Map::new();
            rec.insert("kind".into(), Json::String("grant".into()));
            rec.insert("quest".into(), Json::String(quest_id.to_string()));
            if let Some(oid) = objective_id {
                rec.insert("objective".into(), Json::String(oid.to_string()));
            }
            rec.insert("reward".into(), Json::Object(reward));
            // dsl 0.23.0 §8: a kind that credits a path adds the amount there.
            // A range is the engine's roll (D-C), so the reference runner
            // credits scalar amounts only.
            if let (Some(path), Some(n)) = (&r.credits, r.amount) {
                let before = match self.state.get(path) {
                    Some(Value::Num(v)) => *v,
                    _ => 0.0,
                };
                let after = before + n as f64;
                self.state.insert(path.clone(), Value::Num(after));
                rec.insert(
                    "credited".into(),
                    json!({ "path": path, "value": value_to_json(&Value::Num(after)) }),
                );
            }
            if matches!(event, GrantEvent::Failed) {
                rec.insert("onFailed".into(), Json::Bool(true));
            }
            self.driver.emit(Json::Object(rec));
        }
    }

    /// Fire every handler matching `event` whose `when` holds over the current
    /// (pre-event) state snapshot, running each body once. `scope` is the
    /// transitioning quest's id for the engine-derived lifecycle events —
    /// those fire ONLY for their own enclosing quest (quest-lifecycle.md);
    /// `None` (a mock world event) fires every matching handler — under
    /// `lute play` (dsl 0.22.0 §9) only those of an ACTIVE quest, as `lute
    /// trace` delivers `events:`. `target` is the target an occasion was
    /// raised for (dsl 0.24.0 §2): a handler with a `target` fires only for
    /// that target, never for a plain event or a lifecycle transition.
    fn fire_event(
        &mut self,
        event: &str,
        scope: Option<&str>,
        target: Option<&str>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let matching: Vec<usize> = handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                h.event == event
                    && h.target.as_deref().is_none_or(|t| Some(t) == target)
                    && match scope {
                        Some(s) => h.quest.as_deref() == Some(s),
                        None => {
                            !self.quest_resume
                                || h.quest.as_ref().is_some_and(|q| {
                                    self.quest_status.get(q).map(String::as_str) == Some("active")
                                })
                        }
                    }
            })
            .map(|(i, _)| i)
            .collect();
        for i in matching {
            let h = &handlers[i];
            let when_ok = match &h.when {
                None => true,
                Some(raw) => self.truthy(raw) == Some(true),
            };
            if when_ok {
                let body = h.body.clone();
                match &mut self.deferred_handlers {
                    Some(later) => later.push(body),
                    None => self.run_segment(&body, seg_starts),
                }
            }
        }
    }

    /// Run a quest body segment: from `body_addr` up to the next segment start
    /// (or end of the stream). Bodies are forward-only (quest-lifecycle.md).
    fn run_segment(&mut self, body_addr: &str, seg_starts: &[usize]) {
        let start = self.resolve(body_addr);
        let stop = seg_starts
            .iter()
            .find(|&&s| s > start)
            .copied()
            .unwrap_or(self.commands.len());
        self.run_range(start, stop);
    }

    /// Present ONE lore entry (dsl 0.19.0 §6, `docs/runtime/lore-entries.md`
    /// `present()`): `firstRead = !entry.<id>.read`; run the body segment —
    /// from `body` up to the next `entry` or `beat` record (dsl 0.23.0 §4:
    /// a lore artifact interleaves both), the `<on>`-body
    /// termination rule — with `set`/`assert`/`retract` applied only on a
    /// first read; then, on a first read that ran to completion, the
    /// ENGINE's `entry.<id>.read = true` write. `when` is evaluated and
    /// recorded as `eligible` (true / false / null = unknown) on the `entry`
    /// transcript event, not enforced: `--entry` asks for the presentation.
    fn run_entry(&mut self) {
        let id = self.entry.clone().unwrap_or_default();
        let Some(at) = self.commands.iter().position(|c| {
            c.get("kind").and_then(Json::as_str) == Some("entry")
                && c.get("id").and_then(Json::as_str) == Some(id.as_str())
        }) else {
            let declared: Vec<&str> = self
                .commands
                .iter()
                .filter(|c| c.get("kind").and_then(Json::as_str) == Some("entry"))
                .filter_map(|c| c.get("id").and_then(Json::as_str))
                .collect();
            self.fatal = Some(format!(
                "`--entry {id}` names no entry in this artifact (declared: {})",
                declared.join(", ")
            ));
            return;
        };
        let cmd = self.commands[at].clone();
        let read_path = format!("entry.{id}.read");
        let first_read = self.state.get(&read_path) != Some(&Value::Bool(true));
        let eligible = match cel_raw(cmd.get("when")) {
            None => Json::Bool(true),
            Some(raw) => self.truthy(&raw).map(Json::Bool).unwrap_or(Json::Null),
        };
        self.driver.emit(json!({
            "addr": addr(&cmd),
            "kind": "entry",
            "id": id,
            "firstRead": first_read,
            "eligible": eligible,
        }));
        let body = cmd.get("body").and_then(Json::as_str).unwrap_or("");
        let start = self.resolve(body);
        let stop = self.segment_stop(at);
        self.apply_effects = first_read;
        self.run_range(start, stop);
        self.apply_effects = true;
        if first_read && !self.incomplete && self.fatal.is_none() {
            self.state.insert(read_path, Value::Bool(true));
        }
    }

    /// Where the body segment of the lore head record at `at` ends: the next
    /// `entry` or `beat` head record, or the end of the artifact.
    fn segment_stop(&self, at: usize) -> usize {
        self.commands[at + 1..]
            .iter()
            .position(|c| matches!(c.get("kind").and_then(Json::as_str), Some("entry" | "beat")))
            .map(|i| at + 1 + i)
            .unwrap_or(self.commands.len())
    }

    /// Present ONE bundle beat (dsl 0.23.0 §4): its body segment runs like a
    /// scene's — every effect applies (a beat is spent by presentation, not
    /// by a read flag). `when` is evaluated and recorded as `eligible` on the
    /// `beat` transcript event, not enforced, exactly as an entry's. The id
    /// matches the record's canonical `<document id>.<beat id>`, or its bare
    /// beat id.
    fn run_bundle_beat(&mut self) {
        let id = self.bundle_beat.clone().unwrap_or_default();
        let suffix = format!(".{id}");
        let beats: Vec<usize> = (0..self.commands.len())
            .filter(|&i| self.commands[i].get("kind").and_then(Json::as_str) == Some("beat"))
            .collect();
        let record_id = |i: usize| {
            self.commands[i]
                .get("id")
                .and_then(Json::as_str)
                .unwrap_or("")
        };
        let at = beats
            .iter()
            .copied()
            .find(|&i| record_id(i) == id)
            .or_else(|| {
                beats
                    .iter()
                    .copied()
                    .find(|&i| record_id(i).ends_with(&suffix))
            });
        let Some(at) = at else {
            let declared: Vec<&str> = beats.iter().map(|&i| record_id(i)).collect();
            self.fatal = Some(format!(
                "`--beat {id}` names no beat in this artifact (declared: {})",
                declared.join(", ")
            ));
            return;
        };
        let cmd = self.commands[at].clone();
        let eligible = match cel_raw(cmd.get("when")) {
            None => Json::Bool(true),
            Some(raw) => self.truthy(&raw).map(Json::Bool).unwrap_or(Json::Null),
        };
        self.driver.emit(json!({
            "addr": addr(&cmd),
            "kind": "beat",
            "id": cmd.get("id").cloned().unwrap_or(Json::Null),
            "eligible": eligible,
        }));
        let body = cmd.get("body").and_then(Json::as_str).unwrap_or("");
        let start = self.resolve(body);
        let stop = self.segment_stop(at);
        self.run_range(start, stop);
    }
}

// ── free helpers ────────────────────────────────────────────────────────

/// Value equality for structured-arm evaluation: same-type compares decide;
/// an Unknown or cross-type pair is undecidable (`None`), mirroring CEL's
/// three-valued reads rather than JS-style coercion.
fn expr_node_eq(l: &Value, r: &Value) -> Option<bool> {
    match (l, r) {
        (Value::Bool(a), Value::Bool(b)) => Some(a == b),
        (Value::Num(a), Value::Num(b)) => Some(a == b),
        (Value::Str(a), Value::Str(b)) => Some(a == b),
        _ => None,
    }
}

/// Ordering for structured-arm comparison: numbers numerically, strings
/// lexicographically; anything else undecidable.
fn expr_node_cmp(l: &Value, r: &Value) -> Option<std::cmp::Ordering> {
    match (l, r) {
        (Value::Num(a), Value::Num(b)) => a.partial_cmp(b),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn addr(cmd: &Json) -> &str {
    cmd.get("addr").and_then(Json::as_str).unwrap_or("")
}

/// dsl 0.24.0 §1: the artifact's declared clock (`clock`), if any.
fn artifact_clock(art: &Json) -> Option<lute_manifest::clock::ClockDecl> {
    serde_json::from_value(art.get("clock")?.clone()).ok()
}

/// dsl 0.24.0 §1: re-derive the reserved `clock.*` values from the live
/// `day` / `slot` state — a runner starts from whatever the carried state
/// holds, and the clock paths are never stored, only derived.
fn refresh_clock(art: &Json, state: &mut BTreeMap<String, Value>) {
    if let Some(clock) = artifact_clock(art) {
        crate::clock::refresh(&clock, state);
    }
}

/// The `raw` of a `{raw, expr}` CEL pair, when present and non-empty.
fn cel_raw(pair: Option<&Json>) -> Option<String> {
    pair.and_then(|p| p.get("raw"))
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

fn parse_quest(cmd: &Json) -> QuestDecl {
    let id = cmd
        .get("id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let objectives = cmd
        .get("objectives")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .map(|o| Obj {
                    id: o.get("id").and_then(Json::as_str).unwrap_or("").to_string(),
                    done: o
                        .get("done")
                        .and_then(|d| d.get("raw"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    optional: o.get("optional").and_then(Json::as_bool).unwrap_or(false),
                    body: o.get("body").and_then(Json::as_str).map(str::to_string),
                    quest: o.get("quest").and_then(Json::as_str).map(str::to_string),
                    rewards: parse_rewards(o),
                    on: o.get("on").and_then(Json::as_str).map(str::to_string),
                    by: cel_raw(o.get("by")),
                    target: o.get("target").and_then(Json::as_str).map(str::to_string),
                    until: cel_raw(o.get("until")),
                })
                .collect()
        })
        .unwrap_or_default();
    QuestDecl {
        id,
        start: cel_raw(cmd.get("start")),
        fail: cel_raw(cmd.get("fail")),
        objectives,
        rewards: parse_rewards(cmd),
        accept_activated: cmd.get("activate").and_then(Json::as_str) == Some("accept"),
        complete_any: cmd.get("complete").and_then(Json::as_str) == Some("any"),
    }
}

/// dsl 0.16.0 §3: parse the `rewards:` array off a `QuestCmd`/
/// `ObjectiveEntry` JSON record into the runner's [`RewardRec`] shape.
/// The array is `skip_serializing_if = "Vec::is_empty"` on the compile
/// side, so a rewardless owner has no `rewards` key at all; this
/// gracefully returns an empty vector in that case. Fields map directly
/// from the wire (`kind`/`target`/`amount`/`amountMin`/`amountMax`/
/// `when.raw`/`on`); a malformed entry keeps default values (empty
/// `kind` filters at grant time via [`Machine::emit_grants`]).
fn parse_rewards(owner: &Json) -> Vec<RewardRec> {
    owner
        .get("rewards")
        .and_then(Json::as_array)
        .map(|arr| arr.iter().map(parse_reward).collect())
        .unwrap_or_default()
}

fn parse_reward(r: &Json) -> RewardRec {
    RewardRec {
        kind: r
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        target: r.get("target").and_then(Json::as_str).map(str::to_string),
        amount: r.get("amount").and_then(Json::as_i64),
        amount_min: r.get("amountMin").and_then(Json::as_i64),
        amount_max: r.get("amountMax").and_then(Json::as_i64),
        when: cel_raw(r.get("when")),
        on: r.get("on").and_then(Json::as_str).map(str::to_string),
        credits: r.get("credits").and_then(Json::as_str).map(str::to_string),
    }
}

/// Parse a ground `"rel(a, b)"` fact-pattern string into `(rel, args)`.
fn parse_ground_fact(s: &str) -> Option<Fact> {
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    if close < open {
        return None;
    }
    let rel = s[..open].trim().to_string();
    if rel.is_empty() {
        return None;
    }
    let inner = s[open + 1..close].trim();
    let args: Vec<String> = if inner.is_empty() {
        Vec::new()
    } else {
        inner.split(',').map(|a| a.trim().to_string()).collect()
    };
    Some((rel, args))
}

pub fn render_fact(rel: &str, args: &[String]) -> String {
    format!("{rel}({})", args.join(", "))
}

/// A JSON artifact scalar → a trace [`Value`]; `None` for a non-scalar default.
fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

/// A fact-arg JSON scalar → its ground string (bools as `"true"`/`"false"`).
fn json_arg_to_string(j: &Json) -> String {
    match j {
        Json::String(s) => s.clone(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        _ => j.to_string(),
    }
}

/// A trace [`Value`] → JSON (integral numbers collapse to integers).
pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => json!(b),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.007e15 {
                json!(*n as i64)
            } else {
                json!(n)
            }
        }
        Value::Str(s) => json!(s),
        Value::Unknown => Json::Null,
    }
}

/// dsl 0.24.0 §4 / 0.25.0 §8: a placeholder's `format` applied to its value
/// ([`lute_syntax::ast::format_number`]) — `ordinal` renders a number as an
/// English ordinal (`3rd`, `11th`), `ordinalWord` as a word (`third`) up to
/// `twentieth`. `None` when the placeholder carries no format or the value
/// has no ordinal (a fraction, a negative number): the value then renders
/// unchanged.
fn formatted(ph: &Json, v: &Value) -> Option<String> {
    match (ph.get("format").and_then(Json::as_str), v) {
        (Some(format), Value::Num(n)) => lute_syntax::ast::format_number(format, *n),
        _ => None,
    }
}

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.007e15 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Unknown => "unset".to_string(),
    }
}
