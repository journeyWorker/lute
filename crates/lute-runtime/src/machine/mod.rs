//! The one walker over a COMPILED execution IR (the executable counterpart of
//! `docs/runtime/` + `schemas/lute-ir-0.51.schema.json`), parameterised by a
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
//!   arm is the slot's IR `expr`, decoded once per [`Code`] and evaluated by
//!   [`crate::expr`] — no CEL is parsed at run time (spec 0.38.0 §13);
//!   the slot's `cel` text is kept for messages;
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
#[cfg(test)]
mod hub_leave_tests;
#[cfg(test)]
mod loop_tests;
mod menu;
mod plugin;
mod quest;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroU32;
use std::sync::Arc;

use serde_json::{json, Value as Json};

use super::driver::{Driver, OnUnknown, SiteKind, UnknownSite};
use super::expr::Slot;
pub use super::store::render_fact;
use super::store::Store;
use crate::datalog::Fact;
use crate::eval::Read;
use crate::{MockSet, UnresolvedAtom, Value};
pub use format::{value_to_json, value_to_string};
pub(crate) use plugin::dynamic_raise_fault;
pub use plugin::BridgeReads;

/// The bounded step outcome of the dispatcher.
pub(crate) enum Step {
    /// Continue at this command index (`>= commands.len()` ⇒ end).
    Next(usize),
    /// dsl 0.51.0 §3.7: a taken bounded backward jump — continue at this
    /// index (its head); the range's runaway guard counts from here.
    Loop(usize),
    /// dsl 0.52.0 §5.3: a jump that leaves `hubs` hub option or `<return>`
    /// ranges — the range ends at once, and each enclosing hub visit it
    /// leaves ends ([`Machine::do_hub`]) until control continues at `to`.
    Leave { to: usize, hubs: NonZeroU32 },
    /// Halt the walk (incomplete decision or a hard error already recorded).
    Halt,
}

/// How a range ([`Machine::run_range`]) ended (dsl 0.52.0 §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum RangeEnd {
    /// It ran off its end, a transfer left it, or the walk stopped.
    Done,
    /// A [`Step::Leave`] was taken: the hub visits it leaves end.
    Leave { to: usize, hubs: NonZeroU32 },
}

/// Why a runner walk stopped short ([`Machine::run`],
/// [`Machine::advance_quests`], [`Machine::run_deferred_handlers`]): an
/// unknown command kind or malformed record, or a walk-time refusal
/// ([`Machine::refused`] tells which). `Display` is the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkError {
    pub message: String,
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WalkError {}

/// The `line` record fields that carry a line's identity, delivery and
/// rendered translations — what a play transcript shows and `lute run
/// --json` (the conformance contract) leaves out.
pub const LINE_DELIVERY_KEYS: [&str; 8] = [
    "role",
    "lineId",
    "voiceKey",
    "as",
    "emotion",
    "segments",
    "texts",
    "localeSegments",
];

/// The `plugin` record fields that carry its `text` attributes' identity
/// and rendered translations (0.51.0 §5.6) — what a play transcript shows
/// and `lute run --json` leaves out, as it leaves out a line's.
pub const PLUGIN_DELIVERY_KEYS: [&str; 1] = ["localized"];

/// The `choice` / `hub` record fields that mark what a menu did not offer
/// (spent `once` options, guard-closed options) — what a play menu shows and
/// `lute run --json` leaves out.
pub const MENU_MARK_KEYS: [&str; 2] = ["spent", "ineligible"];

/// The `note` of a `choice` / `hub` record where the driver made no
/// decision ([`Pick::Unscripted`]) — what `lute play`'s halt looks for.
///
/// [`Pick::Unscripted`]: crate::Pick::Unscripted
pub const NOTE_NO_DECISION: &str = "no mock decision — incomplete";
/// The `note` of a `choice` / `hub` record where an automatic pick
/// ([`Pick::AutoFirst`] / [`Pick::HubAutoPass`]) found no open option.
///
/// [`Pick::AutoFirst`]: crate::Pick::AutoFirst
/// [`Pick::HubAutoPass`]: crate::Pick::HubAutoPass
pub const NOTE_NO_ELIGIBLE: &str = "no option decided eligible — incomplete";
/// The `note` of a `choice` record whose scripted decision the driver
/// dropped ([`Forced::Skip`] on a branch).
///
/// [`Forced::Skip`]: crate::Forced::Skip
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

impl<B, C> From<&MockSet<B, C>> for Seed {
    fn from(mock: &MockSet<B, C>) -> Self {
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
/// what [`Machine::resume`] starts from (its `state`, `base_facts`,
/// `quest_status` and save-wide quest instance counters; the walk's own flags
/// start cleared).
#[derive(Clone, Debug, Default)]
pub struct Carry {
    pub state: BTreeMap<String, Value>,
    pub base_facts: BTreeSet<Fact>,
    pub quest_status: BTreeMap<String, String>,
    /// Save-wide quest instance number, keyed by quest id. The first
    /// activation is instance 1; resets never clear this map.
    pub quest_instances: BTreeMap<String, u64>,
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
    /// The presentation stage reduced by the walk's staging records.
    pub stage: crate::Stage,
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

/// Every command `kind` [`Machine`]'s dispatch executes — what
/// `Runtime::load` accepts (spec 0.38.0 §4.1). Keep in step with the
/// `match kind` of the step dispatch.
pub(crate) const COMMAND_KINDS: &[&str] = &[
    "accept", "actor", "assert", "barrier", "beat", "bg", "camera", "cg", "choice", "end", "entry",
    "hub", "jump", "line", "match", "music", "on", "plugin", "quest", "retract", "sequence", "set",
    "sfx", "vfx", "video",
];

/// An artifact's command stream as a [`Machine`] walks it: read-only, so the
/// Machines a playthrough builds over one document can share it.
pub(crate) struct Code {
    pub(crate) kind: String,
    pub(crate) commands: Vec<Json>,
    /// `addr → index` in `commands`.
    pub(crate) addr_index: BTreeMap<String, usize>,
    /// `(addr, index)` in stream (== addr-sorted) order, for fall-through.
    pub(crate) addr_order: Vec<(String, usize)>,
    /// Every CEL slot of `commands`, decoded once ([`Code::slot`]).
    slots: HashMap<usize, Arc<Slot>>,
    /// dsl 0.51.0 §3.7: the stream's bounded loops ([`Code::loop_table`]).
    loops: Loops,
    /// Why the loop topology of `commands` is refused at load
    /// ([`Code::loop_table`]); a walk over this Code does not start.
    pub(crate) fault: Option<String>,
}

/// dsl 0.51.0 §3.7: the bounded jumps of a command stream; dsl 0.52.0 §5.3:
/// its jumps that leave hubs.
#[derive(Debug, Default)]
struct Loops {
    /// Loop head index → the index of every bounded jump to it. A bounded
    /// jump at *j* to head *t* ≤ *j* owns the region [*t*, *j*] of the flat
    /// stream (*t* = *j*: an empty loop, a label right before its jump).
    heads: BTreeMap<usize, Vec<usize>>,
    /// Every `jump` with `max`, by index: its bound.
    bounds: BTreeMap<usize, u8>,
    /// Every `jump` with `leaves`, by index: how many hub visits it ends
    /// (validated against the hub ranges around it, [`Code::hub_leaves`]).
    leaves: BTreeMap<usize, NonZeroU32>,
}

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Code")
            .field("kind", &self.kind)
            .field("commands", &self.commands.len())
            .finish_non_exhaustive()
    }
}

impl Code {
    /// The command stream of `art` (`kind` defaults to `scene`).
    pub(crate) fn of(art: &Json) -> Self {
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
        let mut slots = HashMap::new();
        for (i, c) in commands.iter().enumerate() {
            if let Some(a) = c.get("position").and_then(Json::as_str) {
                addr_index.insert(a.to_string(), i);
                addr_order.push((a.to_string(), i));
            }
            collect_slots(c, &mut slots);
        }
        addr_order.sort();
        let mut code = Code {
            kind,
            commands,
            addr_index,
            addr_order,
            slots,
            loops: Loops::default(),
            fault: None,
        };
        match code.loop_table() {
            Ok(loops) => code.loops = loops,
            Err(fault) => code.fault = Some(fault),
        }
        code
    }

    /// Resolve a control-flow target `addr` to a command index. A `converge`
    /// that points "one past the last record" (execution-model.md) is not in
    /// the map → fall through to the first command whose addr sorts after
    /// it, or the end of the stream.
    pub(crate) fn resolve(&self, addr: &str) -> usize {
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

    /// dsl 0.51.0 §3.7: the loops of the stream, validated as the checker's
    /// loop rules leave them (§3.2), so every walk terminates within the
    /// product of the enclosing `max`es. Every control transfer of every
    /// record ([`transfers`]) is a position string where the field is
    /// required or present, and goes forward, except a `jump` with `max` (a
    /// bounded loop); a `max` is an integer in 1..=99; and two loops nest or
    /// are disjoint. dsl 0.52.0 §5.3: a `leaves` is a positive count, never
    /// beside `max`, and reaches its target ([`Code::hub_leaves`]). `Err`
    /// names the malformed record.
    fn loop_table(&self) -> Result<Loops, String> {
        let mut loops = Loops::default();
        let mut regions = Vec::new();
        for (j, cmd) in self.commands.iter().enumerate() {
            // An unknown kind has no transfers to check; it is refused as
            // unknown (`Runtime::load`, or when the walk reaches it).
            let Some(fields) = transfers(cmd) else {
                continue;
            };
            let kind = cmd.get("kind").and_then(Json::as_str).unwrap_or("");
            let position = cmd.get("position").and_then(Json::as_str).unwrap_or("?");
            let bound = match cmd.get("max") {
                Some(max) if kind == "jump" => Some(
                    max.as_u64()
                        .and_then(|m| u8::try_from(m).ok())
                        .filter(|m| (1..=JUMP_MAX_LIMIT).contains(m))
                        .ok_or_else(|| {
                            format!(
                                "malformed `jump` record at {position}: `max` must be an \
                                 integer from 1 to {JUMP_MAX_LIMIT}, got {max}"
                            )
                        })?,
                ),
                _ => None,
            };
            // dsl 0.52.0 §5.3: `leaves` is a count of at least 1, never on a
            // bounded jump (a bounded jump never leaves a hub).
            if let (Some(leaves), "jump") = (cmd.get("leaves"), kind) {
                let count = leaves
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(NonZeroU32::new)
                    .ok_or_else(|| {
                        format!(
                            "malformed `jump` record at {position}: `leaves` must be a positive \
                             integer, got {leaves}"
                        )
                    })?;
                if bound.is_some() {
                    return Err(format!(
                        "malformed `jump` record at {position}: it carries both `max` and \
                         `leaves`, but a bounded jump never leaves a hub"
                    ));
                }
                loops.leaves.insert(j, count);
            }
            for Transfer {
                field,
                value,
                required,
            } in fields
            {
                let target = match value {
                    Some(Json::String(target)) => target,
                    // An optional target is absent or `null` (an empty
                    // objective body).
                    None | Some(Json::Null) if !required => continue,
                    _ => {
                        return Err(format!(
                            "malformed `{kind}` record at {position}: `{field}` is missing or \
                             not a position"
                        ))
                    }
                };
                let to = self.resolve(target);
                if let Some(max) = bound {
                    loops.bounds.insert(j, max);
                    if to <= j {
                        loops.heads.entry(to).or_default().push(j);
                        regions.push((to, j));
                    }
                } else if to <= j {
                    return Err(format!(
                        "malformed `{kind}` record at {position}: its `{field}` goes back to \
                         {target}, a cycle that never ends (only a `jump` with `max` may go \
                         back)"
                    ));
                }
            }
        }
        // Nest, never cross: in (head ascending, end descending) order an
        // open region must contain every region that starts inside it.
        regions.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut open: Vec<(usize, usize)> = Vec::new();
        for &(head, end) in &regions {
            while open.last().is_some_and(|&(_, e)| e < head) {
                open.pop();
            }
            if let Some(&(_, outer_end)) = open.last() {
                if end > outer_end {
                    let at = |i: usize| {
                        self.commands[i]
                            .get("position")
                            .and_then(Json::as_str)
                            .unwrap_or("?")
                    };
                    return Err(format!(
                        "malformed `jump` records at {} and {}: their loops overlap without \
                         nesting",
                        at(outer_end),
                        at(end)
                    ));
                }
            }
            open.push((head, end));
        }
        self.hub_leaves(&loops.leaves)?;
        Ok(loops)
    }

    /// dsl 0.52.0 §5.3: the ranges [`Machine::do_hub`] runs for the hub
    /// record `cmd`, a well-formed record ([`Code::loop_table`]).
    pub(crate) fn hub_ranges(&self, cmd: &Json) -> HubRanges {
        let options: &[Json] = cmd
            .get("options")
            .and_then(Json::as_array)
            .map_or(&[], Vec::as_slice);
        // Segment starts in stream order: every option target, then the
        // `<return>` segment (dsl 0.28.0 §5), then the converge. A segment
        // runs from its start up to the NEXT start in this order (a
        // non-`exit` option never falls through into the next option's body
        // or the return block). Two starts on one address mean an empty
        // segment — never the following segment's body.
        let back = cmd
            .get("return")
            .and_then(Json::as_str)
            .map(|t| self.resolve(t));
        let converge = self.resolve(required_target(cmd, "converge"));
        let mut starts: Vec<usize> = options
            .iter()
            .map(|o| self.resolve(required_target(o, "target")))
            .collect();
        starts.extend(back);
        starts.push(converge);
        HubRanges {
            starts,
            back,
            converge,
        }
    }

    /// dsl 0.52.0 §5.3 (Review052 F1): every `jump` with `leaves` = *n*
    /// leaves exactly the *n* innermost hub option or `<return>` ranges
    /// around it for its target's range. With those removed, the innermost
    /// remaining range — a hub range, or the body segment the jump runs in
    /// (a scene's whole stream, an objective, handler, entry or beat body) —
    /// holds the target, its end included (a jump to a range's end ends that
    /// range), and no hub range inside it holds the target. An undercount
    /// would present an outer hub again (#216), an overcount would skip the
    /// rest of a body it should stay in, and a target in a sibling body is
    /// in no remaining range: each is refused, naming the jump and the count
    /// that reaches its target.
    fn hub_leaves(&self, leaves: &BTreeMap<usize, NonZeroU32>) -> Result<(), String> {
        if leaves.is_empty() {
            return Ok(());
        }
        let hubs: Vec<(usize, usize)> = self
            .commands
            .iter()
            .filter(|c| c.get("kind").and_then(Json::as_str) == Some("hub"))
            .flat_map(|c| {
                let ranges = self.hub_ranges(c);
                ranges
                    .starts
                    .windows(2)
                    .map(|w| (w[0], w[1]))
                    .collect::<Vec<_>>()
            })
            .collect();
        let segments = self.segment_bounds();
        for (&j, &n) in leaves {
            let cmd = &self.commands[j];
            let position = cmd.get("position").and_then(Json::as_str).unwrap_or("?");
            let target = required_target(cmd, "target");
            let to = self.resolve(target);
            // The hub ranges around the jump, innermost first, then its body
            // segment.
            let mut chain: Vec<(usize, usize)> = hubs
                .iter()
                .copied()
                .filter(|&(s, e)| s <= j && j < e)
                .collect();
            chain.sort_by(|a, b| (a.1 - a.0).cmp(&(b.1 - b.0)).then(b.0.cmp(&a.0)));
            let start = segments
                .iter()
                .rev()
                .copied()
                .find(|&s| s <= j)
                .unwrap_or(0);
            let stop = segments
                .iter()
                .copied()
                .find(|&s| s > j)
                .unwrap_or(self.commands.len());
            chain.push((start, stop));
            let reaches = |(s, e): (usize, usize)| {
                (s..=e).contains(&to)
                    && !hubs
                        .iter()
                        .any(|&(hs, he)| s < hs && he <= e && (hs..he).contains(&to))
            };
            let left = usize::try_from(n.get()).expect("a u32 count fits in usize");
            if chain.get(left).is_some_and(|&range| reaches(range)) {
                continue;
            }
            return Err(match chain.iter().position(|&range| reaches(range)) {
                Some(expected) => format!(
                    "malformed `jump` record at {position}: `leaves` is {n}, but reaching its \
                     target {target} leaves {expected} hub option or `<return>` bodies"
                ),
                None => format!(
                    "malformed `jump` record at {position}: `leaves` is {n}, but its target \
                     {target} lies in a body the jump is not in"
                ),
            });
        }
        Ok(())
    }

    /// The start of every body segment a quest or lore artifact runs on its
    /// own (objective, handler, entry and beat bodies, and the entry and
    /// beat head records that end the segment before them), sorted: a
    /// segment runs from one to the next ([`Machine::run_segment`],
    /// `segment_stop`). Empty for a scene, which runs as one range.
    fn segment_bounds(&self) -> Vec<usize> {
        let mut bounds = Vec::new();
        for (i, cmd) in self.commands.iter().enumerate() {
            match cmd.get("kind").and_then(Json::as_str) {
                Some("quest") => {
                    let objectives = cmd
                        .get("objectives")
                        .and_then(Json::as_array)
                        .map_or(&[][..], Vec::as_slice);
                    bounds.extend(
                        objectives
                            .iter()
                            .filter_map(|o| o.get("body").and_then(Json::as_str))
                            .map(|b| self.resolve(b)),
                    );
                }
                Some("on") => bounds.push(self.resolve(required_target(cmd, "body"))),
                Some("entry" | "beat") => {
                    bounds.push(i);
                    bounds.push(self.resolve(required_target(cmd, "body")));
                }
                _ => {}
            }
        }
        bounds.sort_unstable();
        bounds.dedup();
        bounds
    }

    /// The decoded slot of a `{cel, expr}` pair (`None` when absent or its
    /// text is blank): from the table when `pair` lies in
    /// [`Code::commands`], else decoded now (a record copied out of the
    /// stream).
    pub(crate) fn slot(&self, pair: Option<&Json>) -> Option<Arc<Slot>> {
        let pair = pair?;
        cel_raw(Some(pair))?;
        match self.slots.get(&slot_key(pair)) {
            Some(slot) => Some(Arc::clone(slot)),
            None => Slot::of(pair),
        }
    }
}

/// dsl 0.51.0 §3.1: the largest `max` a bounded jump may carry.
const JUMP_MAX_LIMIT: u8 = 99;

/// The ranges of one hub record ([`Code::hub_ranges`]).
pub(crate) struct HubRanges {
    /// Every option's range start in order, then the `<return>` range's,
    /// then the converge: range *i* is `starts[i]..starts[i + 1]`.
    pub(crate) starts: Vec<usize>,
    /// The `<return>` range's start.
    pub(crate) back: Option<usize>,
    /// Where control continues after the hub.
    pub(crate) converge: usize,
}

/// One control transfer field of a record ([`transfers`]).
struct Transfer<'a> {
    field: &'static str,
    /// The field's value, when present.
    value: Option<&'a Json>,
    /// The executor reads it unconditionally ([`required_target`]): absent
    /// or not a position string, the record is malformed.
    required: bool,
}

/// Every control transfer `cmd` makes — the one list the load check walks
/// ([`Code::loop_table`]). `None` for a kind the runtime does not execute;
/// every kind of [`COMMAND_KINDS`] is named here, so a new kind has to be
/// classified. The only structural back-transfer the compiler emits by
/// design has no target field: a `<hub>` presents itself again inside
/// [`Machine::do_hub`]'s loop (its options, `return` and `converge` all point
/// forward).
fn transfers(cmd: &Json) -> Option<Vec<Transfer<'_>>> {
    fn field<'a>(field: &'static str, of: &'a Json, required: bool) -> Transfer<'a> {
        // `options[].target` names the `target` key of each option.
        let key = field.rsplit('.').next().unwrap_or(field);
        Transfer {
            field,
            value: of.get(key),
            required,
        }
    }
    let each = |key: &str| {
        cmd.get(key)
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    match cmd.get("kind").and_then(Json::as_str).unwrap_or("") {
        "jump" => out.push(field("target", cmd, true)),
        "choice" | "hub" => {
            for option in each("options") {
                out.push(field("options[].target", option, true));
            }
            out.push(field("return", cmd, false));
            out.push(field("converge", cmd, true));
        }
        "match" => {
            for arm in each("arms") {
                out.push(field("arms[].target", arm, true));
            }
            out.push(field("otherwise", cmd, false));
            out.push(field("converge", cmd, true));
        }
        // Segment starts: the body a quest objective, handler, entry or
        // beat runs, after its declaration.
        "quest" => {
            for objective in each("objectives") {
                out.push(field("objectives[].body", objective, false));
            }
        }
        "on" | "entry" | "beat" => out.push(field("body", cmd, true)),
        "accept" | "actor" | "assert" | "barrier" | "bg" | "camera" | "cg" | "end" | "line"
        | "music" | "plugin" | "retract" | "sequence" | "set" | "sfx" | "vfx" | "video" => {}
        _ => return None,
    }
    Some(out)
}

/// The required target `key` of `cmd` (a [`Transfer`] with `required`).
/// Every executed Code passed the load check ([`Code::loop_table`]; a
/// refused Code runs nothing), so the field is a position string here.
pub(crate) fn required_target<'a>(cmd: &'a Json, key: &str) -> &'a str {
    cmd.get(key).and_then(Json::as_str).expect(
        "a required target is a position string: checked at load (Code::loop_table), and a \
         refused Code runs nothing",
    )
}

/// [`Code::slots`]' key: the pair's address. `commands` is never mutated
/// after [`Code::of`], so the address of every value inside it is stable
/// and unique for the Code's lifetime; a value outside it can never share
/// one while the Code is alive.
fn slot_key(pair: &Json) -> usize {
    pair as *const Json as usize
}

/// Every `{cel, expr}` pair under `v`, decoded into `out`.
fn collect_slots(v: &Json, out: &mut HashMap<usize, Arc<Slot>>) {
    match v {
        Json::Object(map) => {
            if map.get("cel").is_some_and(Json::is_string) && map.contains_key("expr") {
                if let Some(slot) = Slot::of(v) {
                    out.insert(slot_key(v), slot);
                }
                return;
            }
            for c in map.values() {
                collect_slots(c, out);
            }
        }
        Json::Array(items) => {
            for c in items {
                collect_slots(c, out);
            }
        }
        _ => {}
    }
}

/// A read-only snapshot supplied to an [`EvalObserver`].
#[derive(Debug)]
pub struct EvalSnapshot<'a> {
    pub state: &'a BTreeMap<String, Value>,
    pub state_types: &'a BTreeMap<String, String>,
    pub base_facts: &'a BTreeSet<crate::datalog::Fact>,
    pub facts: &'a BTreeSet<crate::datalog::Fact>,
    pub visited: &'a BTreeSet<String>,
    /// Bound engine occasion target, retained for condition dumps even when
    /// the target is not part of the persistent state map.
    pub occasion_target: Option<&'a str>,
    pub quest_status: &'a BTreeMap<String, String>,
    pub reads: &'a [(String, Read)],
}

/// A callback invoked for every CEL evaluation performed by a machine.
pub type EvalObserver = Box<dyn for<'a> FnMut(&str, &Value, &[UnresolvedAtom], EvalSnapshot<'a>)>;
pub struct Machine<D: Driver> {
    driver: D,
    /// The artifact's command stream, shared by every Machine over it.
    code: Arc<Code>,
    /// Prerelease N8: cast id -> display name, what an `occasionTarget`
    /// placeholder renders a member by (`lute play` and `lute trace` fill
    /// it; empty renders the id).
    display_names: BTreeMap<String, String>,
    occasion_target: Option<String>,

    /// State, facts, the closure and the visited set ([`Store`]).
    store: Store,
    /// Persistent presentation stage, reduced before staging records emit.
    pub(crate) stage: crate::Stage,

    seed: Seed,

    /// Final quest statuses (quest-kind only).
    quest_status: BTreeMap<String, String>,
    /// Save-wide quest instance numbers, carried with quest status.
    quest_instances: BTreeMap<String, u64>,
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
    eval_observer: Option<EvalObserver>,
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
    /// dsl 0.51.0 §3.7: how many times each bounded backward jump (by
    /// command index) was taken since its head was last entered from outside
    /// its region. Walk-local: a resumed input replays the walk and rebuilds
    /// it, so no snapshot carries it.
    loop_counts: BTreeMap<usize, u8>,
}

impl<D: Driver> std::fmt::Debug for Machine<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `driver` and `eval_observer` are not `Debug`.
        f.debug_struct("Machine")
            .field("occasion_target", &self.occasion_target)
            .field("incomplete", &self.incomplete)
            .field("terminated", &self.terminated)
            .field("fatal", &self.fatal)
            .field("refused", &self.refused)
            .finish_non_exhaustive()
    }
}

impl<D: Driver> Machine<D> {
    /// Install an observer for every CEL evaluation in this walk.
    pub fn with_eval_observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> FnMut(&str, &Value, &[UnresolvedAtom], EvalSnapshot<'a>) + 'static,
    {
        self.store.enable_read_capture();
        self.eval_observer = Some(Box::new(observer));
        self
    }

    /// The `{cel, expr}` pair `pair` of this walk's command stream, decoded
    /// once ([`Code::slot`]); `None` when absent or blank.
    pub(crate) fn slot(&self, pair: Option<&Json>) -> Option<Arc<Slot>> {
        self.code.slot(pair)
    }

    /// Evaluate `slot` over live state + the closure.
    pub(crate) fn eval_atoms(&mut self, slot: &Slot) -> (Value, Vec<UnresolvedAtom>) {
        let (v, atoms) = self.store.eval(&slot.expr);
        if let Some(observer) = &mut self.eval_observer {
            observer(
                &slot.raw,
                &v,
                &atoms,
                EvalSnapshot {
                    state: &self.store.values,
                    state_types: &self.store.types,
                    base_facts: self.store.base_facts(),
                    facts: self.store.all_facts(),
                    visited: &self.store.visited,
                    occasion_target: self
                        .store
                        .values
                        .get(lute_manifest::semantics::beats::OCCASION_TARGET)
                        .and_then(|v| match v {
                            Value::Str(s) => Some(s.as_str()),
                            _ => None,
                        })
                        .or(self.occasion_target.as_deref()),
                    quest_status: &self.quest_status,
                    reads: &self.store.last_reads,
                },
            );
        }
        self.unresolved.extend(atoms.iter().cloned());
        (v, atoms)
    }

    fn eval_value(&mut self, slot: &Slot) -> Value {
        self.eval_atoms(slot).0
    }

    /// A guard judged at `site`: `Some(bool)` when decided; `None` when
    /// undecided — the driver was asked ([`Driver::unknown`]) and, when it
    /// halts, the walk is incomplete from here.
    fn judge(&mut self, slot: &Slot, site: Site<'_>) -> Option<bool> {
        let (v, atoms) = self.eval_atoms(slot);
        match v {
            Value::Bool(b) => Some(b),
            Value::Error(_) => Some(false),
            _ => {
                self.at_unknown(site, &slot.raw, &atoms);
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
        self.code.resolve(addr)
    }

    /// `Err` when this walk's Code was refused at load
    /// ([`Code::loop_table`]): no part of it runs, however often a walk
    /// entry point is called.
    pub(crate) fn code_fault(&self) -> Result<(), WalkError> {
        match &self.code.fault {
            Some(message) => Err(WalkError {
                message: message.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Drive the whole walk: `lute run` once per artifact; `lute play` (dsl
    /// 0.21.0 §6) once per presented beat, then [`Machine::into_carry`].
    pub fn run(&mut self) -> Result<(), WalkError> {
        self.code_fault()?;
        if self.code.kind == "quest" {
            self.run_quest();
        } else if self.code.kind == "lore" {
            if self.bundle_beat.is_some() {
                self.run_bundle_beat();
            } else {
                self.run_entry();
            }
        } else {
            self.run_body(0, self.code.commands.len());
        }
        self.store.derive();
        self.take_fatal()
    }

    /// The walk's outcome: `Err` with the fatal message it recorded, which
    /// it takes.
    pub(crate) fn take_fatal(&mut self) -> Result<(), WalkError> {
        match self.fatal.take() {
            Some(message) => Err(WalkError { message }),
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
        &self.code.kind
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
    /// dsl 0.52.0 §5.3: a [`Step::Leave`] ends the range at once, as
    /// [`RangeEnd::Leave`]; only [`Machine::do_hub`] acts on it.
    fn run_range(&mut self, start: usize, stop: usize) -> RangeEnd {
        if self.stopped() {
            return RangeEnd::Done;
        }
        let mut pc = start;
        self.arrive(None, start);
        let mut guard = 0usize;
        let limit = self.code.commands.len() * 64 + 1024;
        while pc >= start && pc < stop && pc < self.code.commands.len() {
            guard += 1;
            if guard > limit {
                self.fatal = Some("execution did not terminate (control-flow cycle?)".into());
                return RangeEnd::Done;
            }
            let (n, took_loop) = match self.step(pc) {
                Step::Next(n) => (n, false),
                Step::Loop(n) => (n, true),
                Step::Leave { to, hubs } => return RangeEnd::Leave { to, hubs },
                Step::Halt => return RangeEnd::Done,
            };
            if self.fatal.is_some() || self.incomplete || self.terminated {
                return RangeEnd::Done;
            }
            if n < start || n >= stop {
                return RangeEnd::Done;
            }
            // dsl 0.51.0 §3.7: between two bounded takes the walk only moves
            // forward, so the guard counts from the last take; a cycle without
            // `max` still trips it.
            if took_loop {
                guard = 0;
            }
            self.arrive(Some(pc), n);
            pc = n;
        }
        RangeEnd::Done
    }

    /// [`Machine::run_range`] over a body segment no hub encloses: the whole
    /// scene, or an objective, handler, entry or beat body. A jump never
    /// leaves more hubs than enclose it (validated at load,
    /// [`Code::hub_leaves`]), so such a range never ends in a leave.
    pub(crate) fn run_body(&mut self, start: usize, stop: usize) {
        match self.run_range(start, stop) {
            RangeEnd::Done => {}
            RangeEnd::Leave { .. } => unreachable!(
                "a `jump` leaves at most the hub ranges around it (Code::hub_leaves), and a body \
                 segment runs inside none"
            ),
        }
    }

    /// dsl 0.51.0 §3.7, reset by position: control moves from `from` (`None`:
    /// the range starts) to `to`. Every bounded jump whose head is `to` and
    /// whose region [`to`, jump] does not contain `from` starts counting
    /// again; a take from inside the region keeps its count.
    fn arrive(&mut self, from: Option<usize>, to: usize) {
        let Some(jumps) = self.code.loops.heads.get(&to) else {
            return;
        };
        for &jump in jumps {
            if from.is_none_or(|p| !(to..=jump).contains(&p)) {
                self.loop_counts.remove(&jump);
            }
        }
    }

    /// A `jump` record at `pc`. Not a transcript record: `lute trace` reports
    /// an authored `::jump` and the source-only steps that ride on the jump.
    /// dsl 0.51.0 §3.7: a jump with `max` (validated at load,
    /// [`Code::loop_table`]) is taken while its counter is below `max`
    /// ([`Step::Loop`], observed with the take number), then falls through
    /// (observed as exhausted). dsl 0.52.0 §5.3: a jump with `leaves` is a
    /// [`Step::Leave`].
    fn do_jump(&mut self, cmd: &Json, pc: usize) -> Step {
        let position = cmd.get("position");
        let target = self.resolve(required_target(cmd, "target"));
        let Some(&max) = self.code.loops.bounds.get(&pc) else {
            self.driver
                .observe(json!({ "kind": "jump", "position": position }));
            return match self.code.loops.leaves.get(&pc) {
                Some(&hubs) => Step::Leave { to: target, hubs },
                None => Step::Next(target),
            };
        };
        let taken = self.loop_counts.get(&pc).copied().unwrap_or(0);
        if taken >= max {
            self.driver.observe(
                json!({ "kind": "jump", "position": position, "exhausted": true, "max": max }),
            );
            return Step::Next(pc + 1);
        }
        self.loop_counts.insert(pc, taken + 1);
        self.driver.observe(
            json!({ "kind": "jump", "position": position, "taken": taken + 1, "max": max }),
        );
        Step::Loop(target)
    }

    /// Dispatch one command; returns the next index (or `Halt`).
    fn step(&mut self, pc: usize) -> Step {
        // The command is read through a handle on the shared stream, not
        // copied: executing it needs `self` mutably.
        let code = Arc::clone(&self.code);
        let cmd = &code.commands[pc];
        let kind = cmd.get("kind").and_then(Json::as_str).unwrap_or("");
        match kind {
            "line" => {
                self.rec_line(cmd);
                Step::Next(pc + 1)
            }
            "bg" | "music" | "sfx" | "vfx" | "actor" | "camera" | "cg" | "video" | "sequence" => {
                self.rec_stage(cmd, kind);
                Step::Next(pc + 1)
            }
            "plugin" if !self.apply_effects => {
                self.rec_skipped_plugin(cmd);
                Step::Next(pc + 1)
            }
            "set" | "assert" | "retract" if !self.apply_effects => {
                self.rec_skipped(cmd, kind);
                Step::Next(pc + 1)
            }
            "set" => {
                self.exec_set(cmd);
                Step::Next(pc + 1)
            }
            "assert" => {
                self.exec_assert(cmd);
                Step::Next(pc + 1)
            }
            "retract" => {
                self.exec_retract(cmd);
                Step::Next(pc + 1)
            }
            "jump" => self.do_jump(cmd, pc),
            "choice" => self.do_choice(cmd),
            "hub" => self.do_hub(cmd),
            "match" => self.do_match(cmd),
            "barrier" => {
                self.rec_barrier(cmd);
                Step::Next(pc + 1)
            }
            // dsl 0.8.0: the walk terminator. `Halt` unwinds THIS range; the
            // `terminated` flag unwinds every enclosing one (hub option body,
            // quest segment) so the walk stops exactly as it would by running
            // off the end of `commands`.
            "end" => {
                self.rec_end(cmd);
                Step::Halt
            }
            "plugin" => {
                if self.exec_plugin(cmd) {
                    Step::Next(pc + 1)
                } else {
                    Step::Halt
                }
            }
            "accept" => {
                self.exec_accept(cmd);
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

    /// `lute play` (dsl 0.21.0 §4): decide one condition over this runner's
    /// live snapshot — state plus the Datalog fixpoint — through the same
    /// [`Machine::eval_atoms`] chokepoint every guard of a walk uses. `Err`
    /// carries the [`UnresolvedAtom`]s of an undecided (or non-bool) result.
    pub fn eval_guard(&mut self, slot: &Slot) -> Result<bool, Vec<UnresolvedAtom>> {
        let before = self.unresolved.len();
        match self.eval_value(slot) {
            Value::Bool(b) => Ok(b),
            Value::Error(_) => Ok(false),
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
pub(crate) struct Site<'a> {
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
        (Value::Int(a), Value::Int(b)) => match op {
            "+=" => a
                .checked_add(*b)
                .map_or_else(|| Value::Error("integer overflow".into()), Value::Int),
            "-=" => a
                .checked_sub(*b)
                .map_or_else(|| Value::Error("integer overflow".into()), Value::Int),
            "*=" => a
                .checked_mul(*b)
                .map_or_else(|| Value::Error("integer overflow".into()), Value::Int),
            "/=" if *b != 0 => a
                .checked_div(*b)
                .map_or_else(|| Value::Error("integer overflow".into()), Value::Int),
            "/=" => Value::Error("division by zero".into()),
            _ => Value::Error("invalid compound assignment".into()),
        },
        (Value::Double(a), Value::Double(b)) => match op {
            "+=" => Value::Double(a + b),
            "-=" => Value::Double(a - b),
            "*=" => Value::Double(a * b),
            "/=" => Value::Double(a / b),
            _ => Value::Error("invalid compound assignment".into()),
        },
        _ => Value::Error("compound assignment requires matching numeric types".into()),
    }
}

fn addr(cmd: &Json) -> &str {
    cmd.get("position").and_then(Json::as_str).unwrap_or("")
}

/// The canonical CEL text of a slot in an execution-IR `{cel, expr}` pair.
fn cel_raw(pair: Option<&Json>) -> Option<&str> {
    pair.and_then(|p| p.get("cel"))
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::fold_op;
    use crate::Value;

    #[test]
    fn compound_double_division_by_zero_follows_ieee() {
        assert_eq!(
            fold_op("/=", &Value::Double(1.0), &Value::Double(0.0)),
            Value::Double(f64::INFINITY)
        );
        assert!(matches!(
            fold_op("/=", &Value::Double(0.0), &Value::Double(0.0)),
            Value::Double(v) if v.is_nan()
        ));
        assert!(matches!(
            fold_op("/=", &Value::Int(1), &Value::Int(0)),
            Value::Error(_)
        ));
    }
}
