//! The one walker over a COMPILED artifact (the executable counterpart of
//! `docs/runtime/` + `schemas/lute-ir-0.26.schema.json`), parameterised by a
//! [`Driver`] (`docs/design/runtime-unification.md` §3.2). `lute run`
//! (`RunDriver`), `lute play` (`PlayDriver`) and `lute trace` / `lute test`
//! (`TraceDriver`) execute through it.
//!
//! What it implements, grounded in the runtime contract docs:
//! - the **dispatcher loop** (execution-model.md): a program counter over
//!   `commands`, resolving every control-flow target (`jump`/`choice`/`hub`/
//!   `match`/`converge`) against an `addr → index` map, with fall-through
//!   resolution for a `converge` that points one past the last record;
//! - **CEL guards** (cel-and-facts.md): every guard, `::set` value and match
//!   arm is evaluated from CEL via `lute_cel::parse_slot` + [`crate::eval`] —
//!   the tree's one CEL evaluator (an `is` arm without `test` text through
//!   its structured `expr`, [`expr_to_cel`]) — so guard semantics match the
//!   checker exactly (including `holds`/`count`);
//! - **one write path** ([`Machine::write`] over the [`Store`]): every state
//!   write — `::set`, a directive effect, a bridge answer, a grant credit, a
//!   menu's record key, a quest or entry flag — refreshes the clock when it
//!   moves it, marks the Datalog closure stale when a rule reads state (a
//!   **real stratified least-fixpoint** over the artifact's `rules`,
//!   recomputed lazily at the next query, so a derived relation is never
//!   stale after a `::set`), and is checked for exclusive relations holding
//!   together (dsl 0.25.0 §1); a seed's `derive: false` skips the rules;
//! - every undecidable value the walk needs is an [`UnknownSite`]: the
//!   driver decides whether it halts the walk ([`Driver::unknown`]);
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

mod build;
mod commands;
mod entry;
mod format;
mod menu;
mod plugin;
mod quest;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{json, Value as Json};

use super::driver::{Driver, OnUnknown, SiteKind, UnknownSite};
pub use super::store::render_fact;
use super::store::Store;
use crate::datalog::Fact;
use crate::eval::Read;
use crate::{MockSet, UnresolvedAtom, Value};
pub use format::{value_to_json, value_to_string};
pub use menu::expr_to_cel;
pub use plugin::BridgeReads;

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
///
/// [`Pick::Unscripted`]: crate::exec::Pick::Unscripted
pub const NOTE_NO_DECISION: &str = "no mock decision — incomplete";
/// The `note` of a `choice` / `hub` record where an automatic pick
/// ([`Pick::AutoFirst`] / [`Pick::HubAutoPass`]) found no open option.
///
/// [`Pick::AutoFirst`]: crate::exec::Pick::AutoFirst
/// [`Pick::HubAutoPass`]: crate::exec::Pick::HubAutoPass
pub const NOTE_NO_ELIGIBLE: &str = "no option decided eligible — incomplete";
/// The `note` of a `choice` record whose scripted decision the driver
/// dropped ([`Forced::Skip`] on a branch).
///
/// [`Forced::Skip`]: crate::exec::Forced::Skip
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
    /// Prerelease N8: cast id -> display name, what an `occasionTarget`
    /// placeholder renders a member by (`lute play` and `lute trace` fill
    /// it; empty renders the id).
    display_names: BTreeMap<String, String>,

    /// State, facts, the closure and the visited set ([`Store`]).
    store: Store,

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
    /// Quests whose `never` / `awaiting accept` judgment was already
    /// observed ([`Driver::observe`]): a quest a later round judges the same
    /// way is not reported again.
    observed_waiting: BTreeSet<String>,
    /// Observe every `expr` arm judgment with the state it read
    /// ([`Machine::with_arm_probe`]).
    probe_arms: bool,
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

impl<D: Driver> Machine<D> {
    /// Evaluate a `raw` CEL fragment over live state + the closure, through
    /// the one CEL evaluator. The one chokepoint every CEL evaluation in
    /// this walk funnels through, so recording each produced
    /// [`UnresolvedAtom`] into `self.unresolved` here covers guards, `::set`
    /// values, and quest predicates alike. `lute play`'s honesty gate reads
    /// it ([`Carry::unresolved`]); the returned atoms feed an
    /// [`UnknownSite`].
    fn eval_atoms(&mut self, raw: &str) -> (Value, Vec<UnresolvedAtom>) {
        let (v, atoms) = self.store.eval(raw);
        self.unresolved.extend(atoms.iter().cloned());
        (v, atoms)
    }

    fn eval_raw(&mut self, raw: &str) -> Value {
        self.eval_atoms(raw).0
    }

    /// A guard judged at `site`: `Some(bool)` when decided; `None` when
    /// undecided — the driver was asked ([`Driver::unknown`]) and, when it
    /// halts, the walk is incomplete from here.
    fn judge(&mut self, raw: &str, site: Site<'_>) -> Option<bool> {
        let (v, atoms) = self.eval_atoms(raw);
        match v {
            Value::Bool(b) => Some(b),
            _ => {
                self.at_unknown(site, raw, &atoms);
                None
            }
        }
    }

    /// Report an undecided value at `site` to the driver; a halt makes the
    /// walk incomplete. `true` when it halted.
    fn at_unknown(&mut self, site: Site<'_>, raw: &str, atoms: &[UnresolvedAtom]) -> bool {
        let halt = self.driver.unknown(&UnknownSite {
            kind: site.kind,
            id: site.id,
            addr: site.addr,
            raw,
            atoms,
            quest: site.quest,
            arm: site.arm,
        }) == OnUnknown::Halt;
        if halt {
            self.incomplete = true;
        }
        halt
    }

    /// Whether the walk stopped: ended, halted, or failed.
    fn stopped(&self) -> bool {
        self.terminated || self.incomplete || self.fatal.is_some()
    }

    /// The one state write (design §3.4): the [`Store`] write (clock
    /// refresh, closure staleness), then exclusivity — a write that makes
    /// facts of exclusive relations hold together is recorded right under
    /// it and refuses the walk (dsl 0.25.0 §1).
    fn write(&mut self, path: &str, v: Value) {
        if !self.store.has_excludes() {
            self.store.write(path, v);
            return;
        }
        let before = self.store.exclusive_now();
        self.store.write(path, v);
        self.exclusive_check(&before);
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
        self.store.derive();
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// The driver this walk reports to.
    pub fn driver(&self) -> &D {
        &self.driver
    }

    /// The driver, to hand it what it needs between walks.
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    /// The artifact's `kind` (`scene` when absent).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Live state (path → value).
    pub fn state(&self) -> &BTreeMap<String, Value> {
        &self.store.values
    }

    /// A state path's value in the [`Store`]'s read order (reserved
    /// defaults included).
    pub fn read(&self, path: &str) -> Read {
        self.store.read(path)
    }

    /// Base facts (seeds ∪ asserted − retracted), before derivation.
    pub fn base_facts(&self) -> &BTreeSet<Fact> {
        self.store.base_facts()
    }

    /// Derived relations whose closure read an undecided rule guard.
    pub fn undecided(&self) -> &BTreeMap<String, Vec<UnresolvedAtom>> {
        self.store.undecided()
    }

    /// dsl 0.5.1 §1.3: every reserved quest path an evaluation read and
    /// whether it read a seeded value (`false`) or the reserved default
    /// (`true`).
    pub fn reserved_reads(&self) -> BTreeMap<String, bool> {
        self.store
            .reserved_reads()
            .iter()
            .map(|(p, k)| (p.clone(), *k == crate::eval::ReservedReadKind::Defaulted))
            .collect()
    }

    /// dsl 0.25.0 §1: every pair of exclusive facts holding together now.
    pub fn exclusive_violations(&mut self) -> Vec<String> {
        self.store.exclusive_now()
    }

    /// dsl 0.22.0 §6: the derived relations a query read under `derive:
    /// false` — looked up, not derived.
    pub fn derived_reads(&self) -> &BTreeSet<String> {
        self.store.derived_reads()
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
    /// `end` record has run (or the walk halted) the walk is over, so every
    /// LATER segment (a quest `<on>` body, an objective body) is a no-op —
    /// the one guard here is what makes that true for all of them at once.
    fn run_range(&mut self, start: usize, stop: usize) {
        if self.stopped() {
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
            "plugin" if !self.apply_effects => {
                self.rec_skipped_plugin(&cmd);
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
                // Not a transcript record: `lute trace` reports an authored
                // `::next` and the source-only steps that ride on the jump.
                self.driver
                    .observe(json!({ "kind": "jump", "addr": cmd.get("addr") }));
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
        self.store.all_facts()
    }
}

// ── free helpers ────────────────────────────────────────────────────────

/// Where an undecided value was met ([`UnknownSite`] minus the evaluation).
#[derive(Clone, Copy)]
struct Site<'a> {
    kind: SiteKind,
    id: &'a str,
    addr: &'a str,
    quest: Option<&'a str>,
    arm: Option<usize>,
}

impl<'a> Site<'a> {
    fn new(kind: SiteKind, id: &'a str, addr: &'a str) -> Self {
        Site {
            kind,
            id,
            addr,
            quest: None,
            arm: None,
        }
    }

    /// A quest-level site (a quest, objective or reward) of quest `quest`.
    fn quest(kind: SiteKind, id: &'a str, quest: &'a str) -> Self {
        Site {
            quest: Some(quest),
            ..Site::new(kind, id, "")
        }
    }
}

/// A compound assignment `op` (`+=`, `-=`, `*=`, `/=`) of `by` into `cur`:
/// numbers fold; anything else — an unknown operand, another type, a zero
/// divisor — is unknown (D5: never a guessed `0`).
fn fold_op(op: &str, cur: &Value, by: &Value) -> Value {
    match (cur, by) {
        (Value::Num(a), Value::Num(b)) => match op {
            "+=" => Value::Num(a + b),
            "-=" => Value::Num(a - b),
            "*=" => Value::Num(a * b),
            "/=" if *b != 0.0 => Value::Num(a / b),
            _ => Value::Unknown,
        },
        _ => Value::Unknown,
    }
}

fn addr(cmd: &Json) -> &str {
    cmd.get("addr").and_then(Json::as_str).unwrap_or("")
}

/// The `raw` of a `{raw, expr}` CEL pair, when present and non-empty.
fn cel_raw(pair: Option<&Json>) -> Option<String> {
    pair.and_then(|p| p.get("raw"))
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}
