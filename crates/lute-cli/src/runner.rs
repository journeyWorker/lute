//! `lute run` — the reference headless runner over a COMPILED artifact
//! (the executable counterpart of `docs/runtime/` +
//! `schemas/lute-ir-0.25.schema.json`).
//!
//! `lute run` is the *engine* side of the runtime contract. It loads a compiled
//! artifact (`lute compile` output), gates on `irVersion` by **MAJOR** only
//! (execution-model.md §"Version negotiation"), and executes the flat
//! `commands` stream headlessly against a `--mock` playthrough — the same mock
//! surfaces `lute trace --mock` reads (`state:`/`facts:`/`choose:`/`events:`/
//! `accepts:`). Distinct from `lute trace`, which previews the SOURCE document
//! under three-valued logic and refuses to run the engine machinery: `run`
//! consumes the ARTIFACT an engine would and actually does the engine's job.
//!
//! What it implements, grounded in the runtime contract docs:
//! - the **dispatcher loop** (execution-model.md): a program counter over
//!   `commands`, resolving every control-flow target (`jump`/`choice`/`hub`/
//!   `match`/`converge`) against an `addr → index` map, with fall-through
//!   resolution for a `converge` that points one past the last record;
//! - **CEL guards** (cel-and-facts.md): every guard/`::set` value is evaluated
//!   from its `raw` CEL via `lute_cel::parse_slot` + `lute_trace::eval` — the
//!   tree's one CEL evaluator — so guard semantics match the checker exactly
//!   (including the `holds`/`count` fact-query functions the structured `expr`
//!   AST deliberately omits);
//! - a **real stratified Datalog least-fixpoint** over the artifact's `rules`
//!   (cel-and-facts.md) — recomputed after every `assert`/`retract` delta — so
//!   a `derive: true` relation queried in a guard returns a *definite* answer.
//!   The evaluator is [`lute_trace::datalog`], the one `lute trace`/`lute
//!   test` apply too (dsl 0.22.0 §6); a mock's `derive: false` skips it;
//! - **`choice` / `hub` / `match`** control flow, with `hub` `once`/`exit`
//!   re-presentation driven by the mock's ordered `choose:` visit sequence;
//! - the **quest lifecycle** (quest-lifecycle.md): `start` activation, and
//!   accept activation of a `start`-less (accept-driven) quest from the
//!   mock's `accepts:` or an `accept` record (dsl 0.21.0 §7a.3), monotone
//!   objective completion (bodies play once), `on="<occasion>"` objectives
//!   judged only when the mock's `occasions:` / `--occasion` raise them
//!   (§7a.2), `fail` evaluated before derived completion, and `<on>`
//!   handlers fired on the engine-derived transitions
//!   (`questActive`/`questComplete`/`questFailed`) plus mock `events:`;
//! - **`visited('<scene id>')`** (dsl 0.21.0 §7a.1) over the mock's
//!   `visited:` set (`lute play`: the presented scenes);
//! - **lore entries** (lore-entries.md, dsl 0.19.0): `--entry <id>` presents
//!   ONE `entry` record of a lore artifact — its body segment runs to the
//!   next `entry` or `beat` record, `set`/`assert`/`retract` apply only
//!   while `entry.<id>.read` is false (recorded as `skipped` otherwise), and
//!   a completed first read sets `entry.<id>.read = true`;
//! - **bundle beats** (beats-and-occasions.md, dsl 0.23.0 §4): `--beat <id>`
//!   presents ONE `beat` record of a lore artifact by its canonical
//!   `<document id>.<beat id>` (or bare beat id) — its body segment runs to
//!   the next `entry` or `beat` record like a scene's, every effect applied.
//!   A lore artifact without exactly one of `--entry`/`--beat` (or either
//!   flag on another kind) is a usage error.
//!
//! Output: a human transcript by default; `--json` emits a stable machine
//! transcript `{ kind, irVersion, exit, commands, state, facts, quests }`.
//!
//! Exit codes: `0` a complete walk, `2` an I/O / usage failure (unreadable
//! artifact/mock, malformed artifact, an `irVersion` outside the implemented
//! MAJOR line, or an unknown command `kind`), `3` an incomplete walk (a
//! `choice`/`hub` reached with no mock decision — mirroring `lute trace`'s §4.5
//! incomplete convention).
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

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::process::ExitCode;

use lute_cel::CelArena;
use lute_check::{RelVocab, StateSchema};
use lute_trace::datalog::Program;
use lute_trace::{eval, EffectiveState, EvalEnv, FactStore, UnresolvedAtom, Value};
use serde_json::{json, Value as Json};

/// The IR major.minor line this reference runner implements, derived from
/// [`lute_compile::LUTE_IR_VERSION`] so it follows the compiler's IR
/// version forever. Parsing gates on **MAJOR only** (execution-model.md,
/// 0.13.0): an artifact from a different MAJOR is refused (exit 2); minor
/// and patch are compatible-by-default (fields are append-only within a
/// major line and unknown fields are ignored), and an unknown command
/// `kind` remains the hard error that catches a genuinely newer
/// capability. The minor is still carried here because the `--json`
/// transcript reports the full implemented line.
fn impl_ir_line() -> (u64, u64) {
    parse_major_minor(lute_compile::LUTE_IR_VERSION)
        .expect("LUTE_IR_VERSION must carry a major.minor prefix")
}

/// A ground fact: `(relation, args)` — the shared evaluator's
/// [`lute_trace::datalog::Fact`]. `pub(crate)` (dsl 0.21.0 §6): `lute play`
/// carries this shape across presentations via
/// [`RunnerOutcome`]/[`Runner::with_carryover`].
pub(crate) type Fact = lute_trace::datalog::Fact;

/// Execute a compiled artifact against a mock playthrough. See [`crate::Command::Run`].
/// `entry` / `beat` select the one `entry` record (dsl 0.19.0 §8) or bundle
/// `beat` record (dsl 0.23.0 §4) a lore artifact presents: exactly one is
/// required for `kind: "lore"`, either is refused for any other kind.
/// `occasions` are the `--occasion` flags, raised after the mock's own
/// `occasions:` (dsl 0.21.0 §7a.2).
pub fn run_artifact(
    artifact: &Path,
    mock: Option<&Path>,
    occasions: Vec<String>,
    json_out: bool,
    entry: Option<&str>,
    beat: Option<&str>,
) -> ExitCode {
    let text = match std::fs::read_to_string(artifact) {
        Ok(t) => t,
        Err(e) => {
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

    // ── Version negotiation (execution-model.md): gate on MAJOR only.
    // A minor/patch difference within the implemented major line is
    // compatible by contract (append-only fields; unknown command kinds
    // hard-error below at dispatch), so a 0.12.0 artifact runs on a
    // 0.13.0 runner and vice versa. ──
    let (impl_major, _impl_minor) = impl_ir_line();
    let ir_version = art.get("irVersion").and_then(Json::as_str).unwrap_or("");
    match parse_major_minor(ir_version) {
        Some((maj, _)) if maj == impl_major => {}
        _ => {
            eprintln!(
                "lute run: unsupported irVersion {ir_version:?}: this runner implements the \
                 major-{impl_major} line (engines gate on MAJOR; minor/patch are compatible)"
            );
            return ExitCode::from(2);
        }
    }

    if !art.get("commands").map(Json::is_array).unwrap_or(false) {
        eprintln!("lute run: artifact has no `commands` array");
        return ExitCode::from(2);
    }

    // ── dsl 0.19.0 §8: a lore artifact is looked up, never played. ──
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
                "lute run: {} is a lore artifact — there is no sequence to play; pass \
                 `--entry <id>` to present one entry or `--beat <id>` to present one bundle beat",
                artifact.display()
            );
            return ExitCode::from(2);
        }
        (false, Some((flag, id))) => {
            eprintln!(
                "lute run: `--{flag} {id}` needs a lore artifact; {} is kind {:?}",
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
                    eprintln!("lute run: invalid mock {}: {}", path.display(), d.message);
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                eprintln!("lute run: cannot read mock {}: {e}", path.display());
                return ExitCode::from(2);
            }
        },
    };

    mock_set.occasions.extend(occasions);
    let mut runner = Runner::new(&art, mock_set);
    runner.entry = entry.map(str::to_string);
    runner.bundle_beat = beat.map(str::to_string);
    match runner.run() {
        Err(msg) => {
            eprintln!("lute run: {msg}");
            ExitCode::from(2)
        }
        Ok(()) => {
            if json_out {
                runner.print_json();
            } else {
                runner.print_human(artifact);
            }
            ExitCode::from(if runner.incomplete { 3 } else { 0 })
        }
    }
}

/// Parse `"0.9.0"` → `(0, 9)`; `None` when it lacks a `major.minor` prefix.
fn parse_major_minor(v: &str) -> Option<(u64, u64)> {
    let mut it = v.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next()?.parse().ok()?;
    Some((maj, min))
}

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
    /// transitions (spec §3 D-D) — [`Runner::emit_quest_grants`] filters
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
/// evaluated at the grant instant via [`Runner::truthy`]; `None` here
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

/// The reference engine over one artifact. `pub(crate)` (dsl 0.21.0 §6):
/// `lute play` (`play.rs`) drives one `Runner` per presented beat and per
/// quest-lifecycle advance, threading state/facts/quest status across
/// instances via [`RunnerOutcome`]/[`Runner::with_carryover`] — never a
/// second dispatcher.
pub(crate) struct Runner {
    kind: String,
    commands: Vec<Json>,
    /// `addr → index` in `commands`.
    addr_index: BTreeMap<String, usize>,
    /// `(addr, index)` in stream (== addr-sorted) order, for fall-through.
    addr_order: Vec<(String, usize)>,
    /// Declared value-type per state path (from the artifact `state` table),
    /// so a mock literal is coerced against the same type the compiler folded.
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

    /// The artifact's Datalog rules ([`lute_trace::datalog`]).
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

    mock: lute_trace::MockSet,

    /// Executed-command transcript (machine records).
    transcript: Vec<Json>,
    /// Final quest statuses (quest-kind only).
    quest_status: BTreeMap<String, String>,

    /// A `choice`/`hub` was reached with no mock decision (exit 3).
    incomplete: bool,
    /// An `end` record executed (dsl 0.8.0): the walk is OVER. Distinct from
    /// [`Runner::incomplete`] — an `end` walk is a COMPLETE walk (exit 0),
    /// behaviorally identical to falling off the end of `commands`, with the
    /// author's `reason` surfaced in the transcript. Checked wherever
    /// `incomplete` is, so a terminator inside a hub option / quest body
    /// segment stops the WHOLE walk and not merely its bounded `run_range`.
    terminated: bool,
    /// An unknown command `kind` or malformed record (exit 2).
    fatal: Option<String>,
    /// The `fatal` message is a walk-time `E-TRACE-CHOICE` refusal — a
    /// scripted decision naming an option that is not eligible at its
    /// presentation point — not a malformed artifact. `lute run` exits 2
    /// for both; `lute play` reports a refusal as an error (exit 1), the
    /// same verdict an ineligible `pick:` gets.
    refused: bool,
    /// Per `<branch>` id: how many decisions of a multi-decision `choose:`
    /// list earlier presentations consumed. A single decision answers every
    /// presentation of its branch; a list of two or more is consumed one
    /// per presentation, in order (`lute play` carries this across its
    /// presentations via [`RunnerOutcome::choice_cursor`]).
    choice_cursor: BTreeMap<String, usize>,
    /// `lute play` drives this walk ([`Runner::with_carryover`]). The script
    /// is then held to what is really offered — forcing a spent `once` hub
    /// option is refused (`E-TRACE-CHOICE`), as `lute trace` refuses it —
    /// and the records carry what a play transcript shows: a line's
    /// identity and delivery (`role`/`lineId`/`voiceKey`/`as`/`emotion`),
    /// a menu's spent and guard-closed options. `false` for `lute run`, whose
    /// `--json` transcript is the byte-exact conformance contract
    /// (`conformance/`), including its skip of a repeat-forced `once` option.
    play: bool,
    /// Every [`UnresolvedAtom`] any CEL evaluation this walk performed
    /// produced ([`Runner::eval_raw`]'s one chokepoint). `lute run` never
    /// reads this — its exit code/output are byte-identical to before this
    /// field existed; it exists for [`RunnerOutcome`] to hand to `lute
    /// play`'s honesty gate (an unresolved surface halts it incomplete).
    unresolved: Vec<UnresolvedAtom>,
    /// `lute play` (dsl 0.21.0 §6, D-H): [`Runner::advance_quests`] RESUMES
    /// the quest lifecycle over carried-over state instead of starting a
    /// fresh walk — a quest keeps its carried status (only an `unset` quest
    /// may activate) and an objective whose
    /// `quest.<id>.objectives.<oid>.done` is already true is not completed
    /// (nor its body played) a second time. `false` for `lute run`.
    quest_resume: bool,
    /// dsl 0.19.0 §8: the `entry` id a lore artifact presents (`lute run
    /// --entry`). `None` for every scene/quest walk.
    entry: Option<String>,
    /// dsl 0.23.0 §4: the bundle `beat` a lore artifact presents (its
    /// canonical `<document id>.<beat id>`, or the bare beat id). `None`
    /// otherwise; `lute play` sets it for a `bundle` beat.
    bundle_beat: Option<String>,
    /// dsl 0.19.0 §6: `false` while presenting an entry whose
    /// `entry.<id>.read` is already true — `set`/`assert`/`retract` records
    /// are then recorded as `skipped` instead of applied.
    apply_effects: bool,
    /// dsl 0.21.0 §7a.1: the ids of the scenes presented in this save — what
    /// `visited('<id>')` reads. Seeded from the mock's `visited:` (`lute
    /// run`) or the playthrough's presentation history (`lute play`,
    /// [`Runner::with_visited`]).
    visited: BTreeSet<String>,
    /// dsl 0.21.0 §7a.3: the quest ids every `accept` record this walk
    /// executed named, in order. An accept-driven quest of THIS artifact
    /// activates from it on the next lifecycle round; `lute play` carries
    /// the rest to the quest documents via [`RunnerOutcome::accepted`].
    accepted: Vec<String>,
    /// dsl 0.24.0 §2: the quest ids every `accept` record with `applies:
    /// "nextRun"` named — queued; `lute play` applies them after the next
    /// run-start reset ([`RunnerOutcome::accepted_next_run`]).
    accepted_next_run: Vec<String>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids whose `by` came true while
    /// they were not done — failed, never judged again. `lute play` carries
    /// them across advances ([`Runner::with_failed_objectives`],
    /// [`RunnerOutcome::failed_objectives`]).
    failed_objectives: BTreeSet<String>,
    /// dsl 0.24.0 §5: the `bridges:` answers plugin calls consume — the
    /// mock's (`lute run`), or the playthrough's queue ([`Runner::with_bridges`]).
    bridges: BridgeAnswers,
    /// dsl 0.25.0 §1: every pair of relations the artifact's `relations[].excludes`
    /// declares exclusive (`a < b`); empty when none.
    excludes: Vec<(String, String)>,
    /// dsl 0.24.0 §2: why each `<quest>.<objective>` in `failed_objectives`
    /// failed (`by` / `until`) — read when a required objective's failure
    /// fails its quest, to stamp `quest.<id>.failedBy`.
    objective_failed_by: BTreeMap<String, &'static str>,
    /// dsl 0.24.0 §2.1: the raises (`name` / `name@target`) still to come in
    /// this step — the walk's own `occasions` (`lute run`, a play raise pass)
    /// plus the play step's ([`Runner::with_deferred_by`]). An `on=`
    /// objective one of them judges has its `by` deferred until that raise
    /// judged its `done` ([`Runner::judge_occasion`]): `done` wins over a
    /// deadline that came true in the step raising the occasion.
    defer_by: Vec<String>,
    /// `lute play` (dsl 0.24.0 §2): `Some` while answering a `judge: before`
    /// raise — a firing `<on>` handler's body is collected here (its `when`
    /// decided at the firing) instead of run, and play runs it after the
    /// occasion's beats ([`Runner::run_deferred_handlers`]). `None`: bodies
    /// run where they fire.
    deferred_handlers: Option<Vec<String>>,
}

/// `lute play`'s per-presentation carryover + transcript-reuse surface:
/// everything the playthrough needs to seed the NEXT
/// [`Runner::with_carryover`], plus the machine transcript play.rs's own
/// renderer reuses verbatim instead of re-implementing per-`kind` rendering
/// rules a second time.
pub(crate) struct RunnerOutcome {
    pub state: BTreeMap<String, Value>,
    pub base_facts: BTreeSet<Fact>,
    pub quest_status: BTreeMap<String, String>,
    /// A `choice`/`hub` was reached with no scripted `choose:` decision, or
    /// (quest advance) an active quest's required objective is undecidable.
    pub incomplete: bool,
    /// See [`Runner::unresolved`] — the honesty-gate signal.
    pub unresolved: Vec<UnresolvedAtom>,
    pub transcript: Vec<Json>,
    /// See [`Runner::accepted`] — the accepts a presentation made, which the
    /// playthrough hands to the next quest advance.
    pub accepted: Vec<String>,
    /// See [`Runner::accepted_next_run`].
    pub accepted_next_run: Vec<String>,
    /// See [`Runner::refused`].
    pub refused: bool,
    /// See [`Runner::choice_cursor`] — the consumption the next
    /// presentation resumes from.
    pub choice_cursor: BTreeMap<String, usize>,
    /// See [`Runner::failed_objectives`].
    pub failed_objectives: BTreeSet<String>,
    /// See [`Runner::bridges`] — the answers later walks consume.
    pub bridges: BridgeAnswers,
    /// See [`Runner::deferred_handlers`] — empty unless deferring.
    pub deferred_handlers: Vec<String>,
}

/// dsl 0.24.0 §5: the bridge answers a walk consumes — per plugin directive
/// tag, one answer per call, in call order. `step` (a `lute play` step's own
/// `bridges:`) is consumed before `top` (the play's top-level `bridges:`, or
/// a `lute run --mock`'s).
#[derive(Clone, Debug, Default)]
pub(crate) struct BridgeAnswers {
    pub step: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
    pub top: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
    /// dsl 0.25.0 §7: what content reads of the bridge results — the
    /// project's (`lute play`) or the artifact's (`lute run`).
    pub reads: std::sync::Arc<BridgeReads>,
}

/// dsl 0.25.0 §7: what content reads of the plugin calls' bridge results,
/// over a set of compiled artifacts ([`BridgeReads::of`]). A result field
/// no content reads MAY be left out of an answer.
#[derive(Clone, Debug, Default)]
pub(crate) struct BridgeReads {
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
    pub(crate) fn of<'a>(arts: impl IntoIterator<Item = &'a Json> + Clone) -> Self {
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
    pub(crate) fn with_result_types(
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
            for (field, _) in lute_trace::mock::bridge_result_writes(decl) {
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
    pub(crate) fn reads(&self, tag: &str, field: &str) -> bool {
        self.fields.get(tag).is_some_and(|f| f.contains(field))
    }

    /// dsl 0.26.0 §3.1: the capability-declared type of `tag`'s result `field`.
    pub(crate) fn result_type(&self, tag: &str, field: &str) -> Option<&'static str> {
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

impl BridgeAnswers {
    /// A `bridges:` surface as a queue.
    pub(crate) fn queue(
        answers: &BTreeMap<String, Vec<lute_trace::BridgeAnswer>>,
    ) -> BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>> {
        answers
            .iter()
            .filter(|(_, list)| !list.is_empty())
            .map(|(tag, list)| (tag.clone(), list.iter().cloned().collect()))
            .collect()
    }

    /// The next answer for a call of `tag`: the step's first, then the top
    /// level's.
    fn next(&mut self, tag: &str) -> Option<lute_trace::BridgeAnswer> {
        for tier in [&mut self.step, &mut self.top] {
            if let Some(q) = tier.get_mut(tag) {
                if let Some(a) = q.pop_front() {
                    if q.is_empty() {
                        tier.remove(tag);
                    }
                    return Some(a);
                }
            }
        }
        None
    }
}

impl Runner {
    /// Build a Runner skeleton straight off an artifact: addr map, typed
    /// state table (types only — NOT yet seeded with defaults; see below),
    /// parsed Datalog rules/strata. Live `state`/`base_facts` start EMPTY
    /// here — [`Runner::new`] (fresh `lute run` walk) seeds them from the
    /// artifact's own `state[].default`/`seedFacts`; [`Runner::with_carryover`]
    /// (`lute play`, dsl 0.21.0 §6) seeds them from the
    /// PRIOR scene's [`RunnerOutcome`] instead. Both then layer `mock`'s own
    /// `state:`/`facts:` seeds on top via [`Runner::apply_mock_seeds`] — the
    /// one place that override rule lives.
    fn blank(art: &Json, mock: lute_trace::MockSet) -> Self {
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
            .with_kinds(lute_trace::datalog::ir_kinds(art.get("entities")));
        let mut vocab = RelVocab::default();
        if !mock.derives() {
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
        // dsl 0.21.0 §7a.1: the mock's `visited:` seeds the presented set.
        let visited = mock.visited.iter().cloned().collect();
        // dsl 0.24.0 §5: the mock's `bridges:` answer this walk's calls.
        let bridges = BridgeAnswers {
            top: BridgeAnswers::queue(&mock.bridges),
            ..BridgeAnswers::default()
        };
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

        Runner {
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
            mock,
            transcript: Vec::new(),
            quest_status: BTreeMap::new(),
            incomplete: false,
            terminated: false,
            fatal: None,
            refused: false,
            choice_cursor: BTreeMap::new(),
            play: false,
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
            bridges,
            excludes,
        }
    }

    fn new(art: &Json, mock: lute_trace::MockSet) -> Self {
        let mut runner = Self::blank(art, mock);
        // dsl 0.25.0 §7: `lute run` walks one artifact — its content is the
        // reader of its bridge results (`lute play` hands the project's in
        // [`Runner::with_bridges`]).
        runner.bridges.reads = std::sync::Arc::new(BridgeReads::of([art]));
        runner.apply_mock_seeds();
        refresh_clock(art, &mut runner.state);
        runner.recompute_facts();
        runner
    }

    /// `lute play`'s chained-evaluation constructor (dsl 0.21.0 §6):
    /// seeds live state/facts/quest status from a PRIOR scene's
    /// [`RunnerOutcome`] instead of this artifact's own declared defaults —
    /// `play.rs` already decided what carries forward across the scene
    /// boundary (its own state-tier filter: `run.*`/`user.*`/`app.*`/
    /// `quest.*` persist, `scene.*` resets); this constructor does not
    /// re-derive that policy, only accepts its result. The play script's
    /// `choose:` rides in `mock`; `play.rs` never puts `state:`/`facts:`
    /// seeds there (it seeds the playthrough once, up front), so
    /// [`Runner::apply_mock_seeds`] layers nothing over the carryover.
    pub(crate) fn with_carryover(
        art: &Json,
        mock: lute_trace::MockSet,
        initial_state: BTreeMap<String, Value>,
        initial_facts: BTreeSet<Fact>,
        initial_quests: BTreeMap<String, String>,
    ) -> Self {
        let mut runner = Self::blank(art, mock);
        runner.state = initial_state;
        runner.base_facts = initial_facts;
        runner.quest_status = initial_quests;
        runner.play = true;
        runner.apply_mock_seeds();
        refresh_clock(art, &mut runner.state);
        runner.recompute_facts();
        runner
    }

    /// `lute play` (dsl 0.21.0 §6): present ONE `entry` record of a lore
    /// artifact — the same `--entry` surface `lute run` sets — so an entry
    /// beat follows the entry rules (first-read effects, `entry.<id>.read`).
    pub(crate) fn with_entry(mut self, id: &str) -> Self {
        self.entry = Some(id.to_string());
        self
    }

    /// dsl 0.23.0 §4: present ONE bundle `beat` record of a lore artifact
    /// (`lute play` for a `bundle` beat, `lute run --beat`).
    pub(crate) fn with_bundle_beat(mut self, id: &str) -> Self {
        self.bundle_beat = Some(id.to_string());
        self
    }

    /// Prerelease N8: the cast display names `{{occasion.target}}` renders a
    /// member by (`lute play`).
    pub(crate) fn with_display_names(mut self, names: &BTreeMap<String, String>) -> Self {
        self.display_names = names.clone();
        self
    }

    /// dsl 0.26.0 §5: the member a `target="kind:<kind>"` beat was raised
    /// for, readable as `occasion.target` (cleared with `None`).
    pub(crate) fn bind_occasion_target(&mut self, member: Option<&str>) {
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
    /// joined to any mock `visited:` seed, so `visited('<id>')` reads real
    /// presentation history.
    pub(crate) fn with_visited(mut self, visited: &BTreeSet<String>) -> Self {
        self.visited.extend(visited.iter().cloned());
        self
    }

    /// `lute play`: resume multi-decision `choose:` lists where earlier
    /// presentations left them (see [`Runner::choice_cursor`]).
    pub(crate) fn with_choice_cursor(mut self, cursor: &BTreeMap<String, usize>) -> Self {
        self.choice_cursor = cursor.clone();
        self
    }

    /// `lute play` (dsl 0.23.0 §2): the objectives earlier advances failed
    /// through their `by`, so a resumed walk neither completes nor fails them
    /// again.
    pub(crate) fn with_failed_objectives(mut self, failed: &BTreeSet<String>) -> Self {
        self.failed_objectives.extend(failed.iter().cloned());
        self
    }

    /// `lute play` (dsl 0.24.0 §2.1): the step raises `raise` (`name` or
    /// `name@target`) — the settles before it defer the `by` of the `on=`
    /// objectives it judges ([`Runner::defer_by`]).
    pub(crate) fn with_deferred_by(mut self, raise: Option<&str>) -> Self {
        self.defer_by.extend(raise.map(str::to_string));
        self
    }

    /// `lute play` (dsl 0.24.0 §2): answering a `judge: before` raise —
    /// collect the firing `<on>` handlers' bodies instead of running them
    /// ([`Runner::deferred_handlers`]).
    pub(crate) fn with_deferred_handlers(mut self, defer: bool) -> Self {
        self.deferred_handlers = defer.then(Vec::new);
        self
    }

    /// `lute play` (dsl 0.24.0 §5): the playthrough's bridge answers — the
    /// current step's, then the top level's — which this walk's plugin calls
    /// consume ([`RunnerOutcome::bridges`] hands back the rest). Under play
    /// a call left without an answer halts the walk at the call.
    pub(crate) fn with_bridges(mut self, bridges: &BridgeAnswers) -> Self {
        self.bridges = bridges.clone();
        self
    }

    /// Layer `self.mock`'s `state:`/`facts:` seeds over whatever live
    /// state/facts the constructor already set (override, per path/fact —
    /// never a reset): shared by [`Runner::new`] (over the artifact's own
    /// defaults) and [`Runner::with_carryover`] (over the prior scene's
    /// carryover), so the "mock wins on conflict" rule applies identically
    /// either way.
    fn apply_mock_seeds(&mut self) {
        let seeds: Vec<(String, String)> = self
            .mock
            .state
            .iter()
            .map(|(p, lit, _)| (p.clone(), lit.clone()))
            .collect();
        for (path, lit) in seeds {
            let v = self.coerce_literal(&path, &lit);
            self.state.insert(path, v);
        }
        let mock_facts: Vec<String> = self.mock.facts.clone();
        for f in mock_facts {
            if let Some(fact) = parse_ground_fact(&f) {
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
    /// through the shared [`lute_trace::datalog`] evaluator trace uses too.
    /// Under `derive: false` (dsl 0.22.0 §6) the rules are not applied: the
    /// derived relations are marked in `vocab`, so an unmatched one reads
    /// unknown ([`Runner::blank`]).
    fn recompute_facts(&mut self) {
        if self.mock.derives() {
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
    /// Reuses `lute_cel` (parse) + `lute_trace::eval` (the one CEL evaluator).
    /// The one chokepoint every CEL evaluation in this walk funnels through,
    /// so recording each produced [`UnresolvedAtom`] into `self.unresolved`
    /// here (rather than at each call site) covers guards, `::set` RHS
    /// values, and quest predicates alike with one line. `lute run` itself
    /// never reads `self.unresolved` — its own output/exit code are
    /// unchanged; the field exists for `lute play`'s honesty gate
    /// ([`RunnerOutcome::unresolved`], [`Runner::eval_guard`]).
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
    /// Three-valued like [`Runner::truthy`]: `None` = unknown, never a guess.
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

    /// Drive the whole walk. See [`crate::Command::Run`] for the `lute run`
    /// caller; `lute play` (dsl 0.21.0 §6) calls this once per presented
    /// beat, then [`Runner::into_outcome`] instead of [`Runner::print_json`]/
    /// [`Runner::print_human`].
    pub(crate) fn run(&mut self) -> Result<(), String> {
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

    /// Consume a Runner after [`Runner::run`] into `lute play`'s carryover +
    /// transcript-reuse surface. Only
    /// meaningful post-`run`; a pre-run outcome would just echo the seeds
    /// back, which no caller has a reason to do.
    pub(crate) fn into_outcome(self) -> RunnerOutcome {
        RunnerOutcome {
            state: self.state,
            base_facts: self.base_facts,
            quest_status: self.quest_status,
            incomplete: self.incomplete,
            unresolved: self.unresolved,
            transcript: self.transcript,
            accepted: self.accepted,
            accepted_next_run: self.accepted_next_run,
            refused: self.refused,
            choice_cursor: self.choice_cursor,
            failed_objectives: self.failed_objectives,
            bridges: self.bridges,
            deferred_handlers: self.deferred_handlers.unwrap_or_default(),
        }
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
    /// ([`Runner::is_accepted`]), and `lute play` hands the id to the quest
    /// documents' next advance. A quest this walk already knows to be past
    /// `unset` is left alone, and the record says so. dsl 0.24.0 §2: an
    /// accept with `applies: "nextRun"` is queued instead
    /// ([`Runner::accepted_next_run`]) and recorded with `at: "nextRun"`.
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
            self.transcript.push(Json::Object(rec));
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
        self.transcript.push(Json::Object(rec));
        self.accepted.push(quest);
    }

    /// An accept-driven quest's activation signal: a mock `accepts:` entry
    /// or an `accept` record this walk executed.
    fn is_accepted(&self, id: &str) -> bool {
        self.mock.accepts.iter().any(|a| a == id) || self.accepted.iter().any(|a| a == id)
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
        // `lute play`: the line's identity and delivery ride the record
        // verbatim, so a `--json` consumer never has to re-join the artifact
        // to know who spoke, how, and under which audio key. (`lute run`'s
        // record is the conformance contract and stays as it is.)
        if self.play {
            for key in ["role", "lineId", "voiceKey", "as", "emotion"] {
                if let Some(v) = cmd.get(key).filter(|v| !v.is_null()) {
                    rec.insert(key.into(), v.clone());
                }
            }
        }
        self.transcript.push(Json::Object(rec));
    }

    fn rec_stage(&mut self, cmd: &Json, kind: &str) {
        self.transcript.push(json!({
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
        self.transcript.push(json!({
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
        self.transcript.push(json!({
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
        self.transcript.push(json!({
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
            self.transcript
                .push(json!({ "kind": "exclusive", "text": v }));
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
        self.transcript.push(Json::Object(rec));
    }

    // ── control flow ───────────────────────────────────────────────────

    /// A walk-time `E-TRACE-CHOICE`: the script forced an option that is not
    /// offered at this presentation point. Halts like a fatal error, flagged
    /// so `lute play` can report it as the error (exit 1) it is.
    fn refuse(&mut self, msg: String) {
        self.fatal = Some(msg);
        self.refused = true;
    }

    /// The ids of `options` whose guard decides false right now — what a
    /// menu shows as not offered. Display-only: the evaluation never feeds
    /// [`Runner::unresolved`], so an unchosen option's unknown guard cannot
    /// halt a playthrough (an unknown guard counts as offered).
    fn closed_options(&mut self, options: &[Json]) -> Vec<String> {
        let mark = self.unresolved.len();
        let mut closed = Vec::new();
        for o in options {
            let (Some(oid), Some(when)) = (
                o.get("id").and_then(Json::as_str),
                o.get("when").and_then(Json::as_str),
            ) else {
                continue;
            };
            if self.truthy(when) == Some(false) {
                closed.push(oid.to_string());
            }
        }
        self.unresolved.truncate(mark);
        closed
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

        // A single scripted decision answers every presentation of this
        // branch; a list of two or more is consumed one decision per
        // presentation, in order — never silently truncated to its head.
        // Running out halts incomplete like an unscripted branch.
        let scripted = self.mock.choose.get(&branch).cloned().unwrap_or_default();
        let forced = match scripted.as_slice() {
            [] => None,
            [only] => Some(only.clone()),
            list => {
                let used = self.choice_cursor.entry(branch.clone()).or_insert(0);
                let next = list.get(*used).cloned();
                if next.is_some() {
                    *used += 1;
                }
                next
            }
        };
        // `lute play` menus mark what was not offered; `lute run`'s record
        // is the conformance contract.
        let closed = if self.play {
            self.closed_options(&options)
        } else {
            Vec::new()
        };
        let Some(forced) = forced else {
            self.incomplete = true;
            let mut rec = serde_json::Map::new();
            rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
            rec.insert("kind".into(), Json::String("choice".into()));
            rec.insert("branch".into(), Json::String(branch));
            rec.insert("chose".into(), Json::Null);
            rec.insert(
                "note".into(),
                Json::String("no mock decision — incomplete".into()),
            );
            if scripted.len() > 1 {
                rec.insert("scripted".into(), json!(scripted.len()));
            }
            if !closed.is_empty() {
                rec.insert("ineligible".into(), json!(closed));
            }
            self.transcript.push(Json::Object(rec));
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
        // evaluates CEL everywhere else in it (`do_match`). Only a DECIDED
        // false refuses: `None` means the guard read something with no mock
        // surface (`now()`/`validAt(...)`, a bridgeResult), and refusing on
        // that would refuse a legal replay. No opt-in flag — a flag to keep
        // the old behaviour re-creates the divergence under another name.
        if let Some(when) = opt.get("when").and_then(Json::as_str) {
            if self.truthy(when) == Some(false) {
                self.refuse(format!(
                    "[E-TRACE-CHOICE] `choose: {branch}: {forced}` names an option whose guard \
                     `{when}` decided false at this presentation point (dsl 0.4.0 §4.4)"
                ));
                return Step::Halt;
            }
        }
        if let Some(key) = record_key {
            self.state.insert(key, Value::Str(forced.clone()));
        }
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("choice".into()));
        rec.insert("branch".into(), Json::String(branch));
        rec.insert("chose".into(), Json::String(forced));
        if !closed.is_empty() {
            rec.insert("ineligible".into(), json!(closed));
        }
        self.transcript.push(Json::Object(rec));
        let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
        Step::Next(self.resolve(target))
    }

    /// The hub re-presentation loop: a `choose:` sequence is consumed ONE
    /// decision at a time — never the whole vector up front — so running out
    /// of scripted decisions can be told apart from a genuine natural
    /// convergence. Exhaustion with eligible options still standing halts
    /// incomplete (`self.incomplete = true`, exit 3) rather than silently
    /// converging. Every iteration consumes one scripted decision or leaves
    /// the loop, so it terminates within `forced.len() + 1` presentations.
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

        let forced: Vec<String> = self.mock.choose.get(&id).cloned().unwrap_or_default();
        let mut forced_cursor = 0usize;

        let mut visited_once: BTreeSet<String> = BTreeSet::new();
        loop {
            // Eligible = not an already-exhausted `once` option, and its
            // guard does not DECIDE false right now (an unknown guard stays
            // eligible — the same three-valued discipline `do_choice`'s
            // guard refusal below uses). The spent and guard-closed options
            // ride the visit record, so a menu shows what was really offered.
            let (mut eligible, mut spent, mut closed) = (Vec::new(), Vec::new(), Vec::new());
            for o in &options {
                let Some(oid) = o.get("id").and_then(Json::as_str) else {
                    continue;
                };
                let once = o.get("once").and_then(Json::as_bool).unwrap_or(false);
                if once && visited_once.contains(oid) {
                    spent.push(oid.to_string());
                    continue;
                }
                match o.get("when").and_then(Json::as_str) {
                    Some(w) if self.truthy(w) == Some(false) => closed.push(oid.to_string()),
                    _ => eligible.push(oid.to_string()),
                }
            }
            let play = self.play;
            let marks = |rec: &mut serde_json::Map<String, Json>| {
                if !play {
                    return;
                }
                if !spent.is_empty() {
                    rec.insert("spent".into(), json!(spent));
                }
                if !closed.is_empty() {
                    rec.insert("ineligible".into(), json!(closed));
                }
            };

            let choice_id = if forced_cursor < forced.len() {
                let c = forced[forced_cursor].clone();
                forced_cursor += 1;
                Some(c)
            } else {
                None
            };

            let Some(choice_id) = choice_id else {
                if eligible.is_empty() {
                    // Natural convergence: nothing left eligible to present.
                    break;
                }
                // The scripted sequence ran out but the hub would still be
                // re-presented (eligible options remain) — halt incomplete
                // rather than silently converging.
                self.incomplete = true;
                let mut rec = serde_json::Map::new();
                rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
                rec.insert("kind".into(), Json::String("hub".into()));
                rec.insert("hub".into(), Json::String(id));
                if let Some(p) = &prompt {
                    rec.insert("prompt".into(), Json::String(p.clone()));
                }
                rec.insert("chose".into(), Json::Null);
                rec.insert(
                    "note".into(),
                    Json::String("no mock decision — incomplete".into()),
                );
                marks(&mut rec);
                self.transcript.push(Json::Object(rec));
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
            if once && visited_once.contains(&choice_id) {
                // `lute run` (the conformance contract) skips a repeat force
                // of a spent `once` option. `lute play` refuses it, as `lute
                // trace` does: a silent skip lets the rest of the script drift
                // out of step with what the player was really offered.
                if !self.play {
                    continue;
                }
                self.refuse(format!(
                    "[E-TRACE-CHOICE] `choose: {id}: {choice_id}` names a `once` option already \
                     taken at this hub, so it is no longer offered (dsl 0.4.0 §4.4)"
                ));
                return Step::Halt;
            }
            // Same rule as `do_choice` (#20, D-C). A hub option is presented
            // repeatedly, so this is evaluated per visit against live state —
            // a guard false on the first pass may be true on the third, which
            // is precisely what a hub is for.
            if let Some(when) = opt.get("when").and_then(Json::as_str) {
                if self.truthy(when) == Some(false) {
                    self.refuse(format!(
                        "[E-TRACE-CHOICE] `choose: {id}: {choice_id}` names an option whose guard \
                         `{when}` decided false at this presentation point (dsl 0.4.0 §4.4)"
                    ));
                    return Step::Halt;
                }
            }
            if let Some(key) = &record_key {
                self.state
                    .insert(key.clone(), Value::Str(choice_id.clone()));
            }
            // hub visit record slot (scene.visited.<hub>.<opt>, state-lifecycle.md).
            self.state
                .insert(format!("scene.visited.{id}.{choice_id}"), Value::Bool(true));
            let mut rec = serde_json::Map::new();
            rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
            rec.insert("kind".into(), Json::String("hub".into()));
            rec.insert("hub".into(), Json::String(id.clone()));
            if let Some(p) = &prompt {
                rec.insert("prompt".into(), Json::String(p.clone()));
            }
            rec.insert("chose".into(), Json::String(choice_id.clone()));
            marks(&mut rec);
            self.transcript.push(Json::Object(rec));
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
                self.transcript.push(json!({
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
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "match",
            "result": result,
        }));
        Step::Next(self.resolve(target))
    }

    fn rec_barrier(&mut self, cmd: &Json) {
        self.transcript.push(json!({
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
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "end",
            "reason": cmd.get("reason").cloned().unwrap_or(Json::Null),
        }));
    }

    /// A `plugin` command (bridge-protocol.md). Effects that need no bridge
    /// result apply. A `bridgeResult` effect reads the call's `bridges:`
    /// answer (dsl 0.24.0 §5), the next one queued for its tag: the answered
    /// values are written, typed by each result slot's declared type; a
    /// field the answer leaves out (dsl 0.25.0 §7: one no content reads) is
    /// unresolved. With no answer, `lute run` records the effects unresolved
    /// and walks on (no host bridge is invoked); `lute play` halts the walk
    /// AT the call, incomplete, before anything after it — a default arm over
    /// the result slot included — is walked, when content reads one of the
    /// call's result slots (else it walks on, like `lute run`). `false` = the
    /// walk stops here (that halt, or an answer that does not fit the call,
    /// which is fatal).
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
            self.bridges.next(&tag)
        };
        // dsl 0.25.0 §7: whether content reads one of this call's results.
        let read = reads
            .iter()
            .any(|(_, p)| self.bridges.reads.paths.contains(p));
        let answered = match &answer {
            Some(a) => match self.bridge_values(&tag, &reads, a) {
                Ok(values) => Some(values),
                Err(msg) => {
                    self.fatal = Some(msg);
                    return false;
                }
            },
            None if self.play && read => {
                self.incomplete = true;
                // The fields every answer to the tag gives (dsl 0.25.0 §7).
                let (fields, paths): (Vec<&str>, Vec<&str>) = reads
                    .iter()
                    .filter(|(f, _)| self.bridges.reads.reads(&tag, f))
                    .map(|(f, p)| (f.as_str(), p.as_str()))
                    .unzip();
                self.transcript.push(json!({
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
        self.transcript.push(rec);
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
        answer: &lute_trace::BridgeAnswer,
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
                if !self.bridges.reads.reads(tag, field) {
                    continue;
                }
                let read: Vec<&str> = fields
                    .iter()
                    .copied()
                    .filter(|f| self.bridges.reads.reads(tag, f))
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
                .or_else(|| self.bridges.reads.result_type(tag, field));
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
    /// before` raise answered ([`Runner::with_deferred_handlers`]) — after
    /// the occasion's beats, in the order they fired. Their `when` was
    /// decided when they fired; a `::end` in one ends the rest.
    pub(crate) fn run_deferred_handlers(&mut self, bodies: &[String]) -> Result<(), String> {
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
                self.set_quest_state(&q.id, "active");
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
        self.defer_by.extend(self.mock.occasions.iter().cloned());
        self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);

        // Mock events fire in order; each re-evaluates the lifecycle. An `end`
        // inside a handler/objective body ends the WALK (dsl 0.8.0), so no
        // later event is delivered — nothing downstream of the terminator runs.
        let events: Vec<String> = self.mock.events.clone();
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
        let occasions: Vec<String> = self.mock.occasions.clone();
        for occasion in &occasions {
            if self.terminated {
                break;
            }
            let (name, target) = lute_trace::split_occasion(occasion);
            if !self.quest_resume {
                let mut rec = json!({ "kind": "occasion", "occasion": name });
                if let Some(t) = target {
                    rec["target"] = json!(t);
                }
                self.transcript.push(rec);
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
                            .any(|r| lute_trace::raise_judges(r, on, o.target.as_deref()))
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
                            self.transcript.push(json!({
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
    /// [`Runner::run`] settles it for `lute run` — activation, objectives,
    /// `fail` before completion, `<on>` handlers, `<reward>` grants — but
    /// RESUMED ([`Runner::quest_resume`]): nothing already active, terminal or
    /// done transitions again. Called once per quest artifact after every
    /// presentation, so later beat conditions see real quest progress.
    pub(crate) fn advance_quests(&mut self) -> Result<(), String> {
        self.quest_resume = true;
        self.run_quest();
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// `lute play` (dsl 0.21.0 §4): decide one beat `when` over this runner's
    /// live snapshot — state plus the Datalog fixpoint — through the same
    /// [`Runner::eval_raw`] chokepoint every guard of a walk uses. `Err`
    /// carries the [`UnresolvedAtom`]s of an undecided (or non-bool) result.
    pub(crate) fn eval_guard(&mut self, raw: &str) -> Result<bool, Vec<UnresolvedAtom>> {
        let before = self.unresolved.len();
        match self.eval_raw(raw) {
            Value::Bool(b) => Ok(b),
            _ => Err(self.unresolved.split_off(before)),
        }
    }

    /// `lute play` (dsl 0.22.0 §4): every fact that holds over this runner's
    /// live snapshot — base facts plus the derived fixpoint (base only under
    /// `derive: false`) — the end-of-play `facts:` expectations judge.
    pub(crate) fn all_facts(&self) -> &BTreeSet<Fact> {
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
                    self.set_quest_state(&q.id, "active");
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
                // ([`Runner::judge_occasion`]), never continuously; a failed
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
                // judged only at the occasion, [`Runner::judge_occasion`].)
                // An `on=` objective a raise still to come in this step
                // judges waits for it: its `done` is judged first.
                for (oi, o) in q.objectives.iter().enumerate() {
                    let deferred = o.on.as_deref().is_some_and(|on| {
                        self.defer_by
                            .iter()
                            .any(|r| lute_trace::raise_judges(r, on, o.target.as_deref()))
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
                    self.set_quest_state(&q.id, "complete");
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
        self.transcript.push(json!({
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
    /// settles of the step until this raise ([`Runner::defer_by`]) — at the
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
                    o.on.as_deref().is_some_and(|on| {
                        lute_trace::raise_judges(occasion, on, o.target.as_deref())
                    })
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
        self.transcript.push(json!({
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
        self.set_quest_state(id, "failed");
        self.state.insert(
            format!("quest.{id}.failedBy"),
            Value::Str(reason.to_string()),
        );
        if let Some(rec) = self.transcript.last_mut() {
            rec["failedBy"] = json!(reason);
        }
    }

    fn set_quest_state(&mut self, id: &str, state: &str) {
        self.state
            .insert(format!("quest.{id}.state"), Value::Str(state.to_string()));
        self.quest_status.insert(id.to_string(), state.to_string());
        self.transcript.push(json!({
            "kind": "quest",
            "quest": id,
            "state": state,
        }));
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
            self.transcript.push(Json::Object(rec));
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
        self.transcript.push(json!({
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
        self.transcript.push(json!({
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

    // ── output ─────────────────────────────────────────────────────────

    fn output_value(&self) -> Json {
        let state: serde_json::Map<String, Json> = self
            .state
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect();
        let facts: Vec<Json> = self
            .all_facts
            .iter()
            .map(|(r, a)| Json::String(render_fact(r, a)))
            .collect();
        let quests: serde_json::Map<String, Json> = self
            .quest_status
            .iter()
            .map(|(k, v)| (k.clone(), Json::String(v.clone())))
            .collect();
        let (ir_major, ir_minor) = impl_ir_line();
        json!({
            "kind": self.kind,
            "irVersion": format!("{ir_major}.{ir_minor}"),
            "exit": if self.incomplete { "incomplete" } else { "complete" },
            "commands": self.transcript,
            "state": state,
            "facts": facts,
            "quests": quests,
        })
    }

    fn print_json(&self) {
        // `serde_json` (no `preserve_order`) emits object keys sorted, so this
        // machine transcript is byte-stable across runs — the conformance
        // `expected.json` contract.
        println!(
            "{}",
            serde_json::to_string_pretty(&self.output_value()).unwrap_or_default()
        );
    }

    fn print_human(&self, artifact: &Path) {
        println!("run {} artifact {}", self.kind, artifact.display());
        for e in &self.transcript {
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
                "plugin" => format!(
                    "  {a}  plugin {} {}",
                    e.get("tag").and_then(Json::as_str).unwrap_or(""),
                    plugin_call_note(e)
                ),
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
                        " (on failed)"
                    } else {
                        ""
                    };
                    format!("  grant {owner}  {kind} {amount}{target}{annot}")
                }
                _ => format!("  {a}  {k}"),
            };
            println!("{line}");
        }
        println!("-- final state --");
        for (k, v) in &self.state {
            println!("  {k} = {}", value_to_string(v));
        }
        if !self.all_facts.is_empty() {
            println!("-- facts --");
            for (r, a) in &self.all_facts {
                println!("  {}", render_fact(r, a));
            }
        }
        if !self.quest_status.is_empty() {
            println!("-- quests --");
            for (k, v) in &self.quest_status {
                println!("  {k}: {v}");
            }
        }
        println!(
            "run {}",
            if self.incomplete {
                "incomplete"
            } else {
                "complete"
            }
        );
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
pub(crate) fn artifact_clock(art: &Json) -> Option<lute_manifest::clock::ClockDecl> {
    serde_json::from_value(art.get("clock")?.clone()).ok()
}

/// dsl 0.24.0 §1: re-derive the reserved `clock.*` values from the live
/// `day` / `slot` state — a runner starts from whatever the carried state
/// holds, and the clock paths are never stored, only derived.
fn refresh_clock(art: &Json, state: &mut BTreeMap<String, Value>) {
    if let Some(clock) = artifact_clock(art) {
        lute_trace::clock::refresh(&clock, state);
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
/// `kind` filters at grant time via [`Runner::emit_grants`]).
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

fn render_fact(rel: &str, args: &[String]) -> String {
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

/// The human annotation of a `plugin` transcript record (dsl 0.24.0 §5):
/// `(bridge answered: passed=true, margin=3)` when a `bridges:` answer
/// decided it, `(bridge unanswered: passed, margin)` when `lute play`
/// halted at it, `(external call, not invoked)` otherwise.
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
    "(external call, not invoked)".to_string()
}

/// A trace [`Value`] → JSON (integral numbers collapse to integers).
fn value_to_json(v: &Value) -> Json {
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

fn value_to_string(v: &Value) -> String {
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

fn json_scalar_str(j: Option<&Json>) -> String {
    match j {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::Null) | None => "unset".to_string(),
        Some(other) => other.to_string(),
    }
}
