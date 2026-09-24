//! `lute run` — the reference headless runner over a COMPILED artifact
//! (the executable counterpart of `docs/runtime/` +
//! `schemas/lute-ir-0.21.schema.json`).
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
//!   This is precisely the work `lute trace` refuses (D1); the runner is the
//!   leg that performs it;
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
//!   next `entry` record, `set`/`assert`/`retract` apply only while
//!   `entry.<id>.read` is false (recorded as `skipped` otherwise), and a
//!   completed first read sets `entry.<id>.read = true`. A lore artifact
//!   without `--entry` (or `--entry` on another kind) is a usage error.
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
//!   as an external call; its `op`/literal effects ARE applied, but a
//!   `bridgeResult` effect has no mock surface to read from and is recorded
//!   unresolved (the runner invokes no host service and ignores `wait`).
//! - **No narrative-time history.** `now()` / `validAt(...)` have no mock
//!   surface and read unknown; the fact store is valid-now (`holds`/`count`
//!   over the current least-fixpoint).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use lute_cel::CelArena;
use lute_check::{RelVocab, StateSchema};
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

/// A ground fact: `(relation, args)`. `pub(crate)` (dsl 0.21.0 §6): `lute
/// play` carries this shape across presentations via
/// [`RunnerOutcome`]/[`Runner::with_carryover`].
pub(crate) type Fact = (String, Vec<String>);

/// Execute a compiled artifact against a mock playthrough. See [`crate::Command::Run`].
/// `entry` selects the one `entry` record a lore artifact presents (dsl
/// 0.19.0 §8): required for `kind: "lore"`, refused for any other kind.
/// `occasions` are the `--occasion` flags, raised after the mock's own
/// `occasions:` (dsl 0.21.0 §7a.2).
pub fn run_artifact(
    artifact: &Path,
    mock: Option<&Path>,
    occasions: Vec<String>,
    json_out: bool,
    entry: Option<&str>,
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
    match (is_lore, entry) {
        (true, None) => {
            eprintln!(
                "lute run: {} is a lore artifact — there is no sequence to play; pass \
                 `--entry <id>` to present one entry",
                artifact.display()
            );
            return ExitCode::from(2);
        }
        (false, Some(id)) => {
            eprintln!(
                "lute run: `--entry {id}` needs a lore artifact; {} is kind {:?}",
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

/// One rule-body literal (cel-and-facts.md: atom / negated atom / comparison /
/// scalar guard).
enum Lit {
    Atom { atom: RAtom, negated: bool },
    Cmp { lhs: Term, rhs: Term, negated: bool },
    Guard { cel: String },
}

/// A rule atom: a relation applied to terms.
struct RAtom {
    rel: String,
    terms: Vec<Term>,
}

/// A rule term: a variable (bound during the join) or a ground constant.
#[derive(Clone)]
enum Term {
    Var(String),
    Const(String),
}

/// A parsed Datalog rule (head :- body).
struct Rule {
    head: RAtom,
    body: Vec<Lit>,
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

    // Evaluation environments — empty by construction: all live state lives in
    // `state`, so an empty `StateSchema` never shadows a read; an empty
    // `RelVocab` makes every relation non-derived, so `holds`/`count` over the
    // fully-materialized fixpoint return DEFINITE answers (the runner has run
    // the fixpoint, unlike trace).
    schema: StateSchema,
    vocab: RelVocab,

    rules: Vec<Rule>,
    /// Least-fixpoint stratum per derived relation.
    strata: BTreeMap<String, usize>,

    /// Live scalar state (path → value).
    state: BTreeMap<String, Value>,
    /// Base facts (seeds ∪ asserted − retracted), before derivation.
    base_facts: BTreeSet<Fact>,
    /// `base_facts` ∪ the derived least-fixpoint — what guards query.
    all_facts: BTreeSet<Fact>,

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
    /// dsl 0.8.0 `::end` executed: the WHOLE playthrough is over, not just
    /// this scene.
    pub terminated: bool,
    /// A `choice`/`hub` was reached with no scripted `choose:` decision, or
    /// (quest advance) an active quest's required objective is undecidable.
    pub incomplete: bool,
    /// See [`Runner::unresolved`] — the honesty-gate signal.
    pub unresolved: Vec<UnresolvedAtom>,
    pub transcript: Vec<Json>,
    /// See [`Runner::accepted`] — the accepts a presentation made, which the
    /// playthrough hands to the next quest advance.
    pub accepted: Vec<String>,
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
        let mut state = BTreeMap::new();
        if let Some(entries) = art.get("state").and_then(Json::as_array) {
            for e in entries {
                let path = e.get("path").and_then(Json::as_str).unwrap_or("");
                if path.is_empty() {
                    continue;
                }
                let ty = e.get("type").and_then(Json::as_str).unwrap_or("string");
                types.insert(path.to_string(), ty.to_string());
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

        // Parsed rules + derived set + strata.
        let rules = parse_rules(art);
        let mut derived = BTreeSet::new();
        for r in &rules {
            derived.insert(r.head.rel.clone());
        }
        let strata = compute_strata(&rules, &derived);
        // dsl 0.21.0 §7a.1: the mock's `visited:` seeds the presented set.
        let visited = mock.visited.iter().cloned().collect();

        Runner {
            kind,
            commands,
            addr_index,
            addr_order,
            types,
            schema: StateSchema::default(),
            vocab: RelVocab::default(),
            rules,
            strata,
            state,
            base_facts,
            all_facts: BTreeSet::new(),
            mock,
            transcript: Vec::new(),
            quest_status: BTreeMap::new(),
            incomplete: false,
            terminated: false,
            fatal: None,
            unresolved: Vec::new(),
            quest_resume: false,
            entry: None,
            apply_effects: true,
            visited,
            accepted: Vec::new(),
        }
    }

    fn new(art: &Json, mock: lute_trace::MockSet) -> Self {
        let mut runner = Self::blank(art, mock);
        runner.apply_mock_seeds();
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
        runner.apply_mock_seeds();
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

    /// `lute play` (dsl 0.21.0 §7a.1): the playthrough's presented scenes,
    /// joined to any mock `visited:` seed, so `visited('<id>')` reads real
    /// presentation history.
    pub(crate) fn with_visited(mut self, visited: &BTreeSet<String>) -> Self {
        self.visited.extend(visited.iter().cloned());
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

    /// Recompute the least-fixpoint: `all_facts = base ∪ derive(base)`.
    fn recompute_facts(&mut self) {
        self.all_facts = fixpoint(
            &self.base_facts,
            &self.rules,
            &self.strata,
            &self.state,
            &self.schema,
        );
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
        let mut fs = FactStore::new(&self.vocab);
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
                        "%" if b != 0.0 => Value::Num(a % b),
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
            self.run_entry();
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
            terminated: self.terminated,
            incomplete: self.incomplete,
            unresolved: self.unresolved,
            transcript: self.transcript,
            accepted: self.accepted,
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
                self.exec_plugin(&cmd);
                Step::Next(pc + 1)
            }
            "accept" => {
                self.exec_accept(&cmd);
                Step::Next(pc + 1)
            }
            // Declarations — inert in a linear walk (a quest artifact is driven
            // by `run_quest`, a lore artifact by `run_entry`, never linearly).
            "quest" | "on" | "entry" => Step::Next(pc + 1),
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
    /// `unset` is left alone, and the record says so.
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
        if let Some(state) = self
            .quest_status
            .get(&quest)
            .filter(|s| s.as_str() != "unset")
        {
            rec.insert(
                "ignored".into(),
                Json::String(format!("already {state}")),
            );
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
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "line",
            "speaker": speaker,
            "text": text,
        }));
    }

    fn rec_stage(&mut self, cmd: &Json, kind: &str) {
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": kind,
        }));
    }

    /// Substitute `{{…}}` markers with a resolved `path` value; `@ref`/reserved
    /// placeholders keep their verbatim marker (state-lifecycle.md).
    fn interpolate(&self, text: &str, placeholders: Option<&Vec<Json>>) -> String {
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
                        Some(v) => value_to_string(v),
                        None => marker.to_string(),
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
        self.base_facts.insert((rel.clone(), args.clone()));
        self.recompute_facts();
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "assert",
            "fact": render_fact(&rel, &args),
        }));
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

        let forced = match self
            .mock
            .choose
            .get(&branch)
            .and_then(|v| v.first())
            .cloned()
        {
            Some(c) => c,
            None => {
                self.incomplete = true;
                self.transcript.push(json!({
                    "addr": addr(cmd),
                    "kind": "choice",
                    "branch": branch,
                    "chose": Json::Null,
                    "note": "no mock decision — incomplete",
                }));
                return Step::Halt;
            }
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
                self.fatal = Some(format!(
                    "[E-TRACE-CHOICE] `choose: {branch}: {forced}` names an option whose guard \
                     `{when}` decided false at this presentation point (dsl 0.4.0 §4.4)"
                ));
                return Step::Halt;
            }
        }
        if let Some(key) = record_key {
            self.state.insert(key, Value::Str(forced.clone()));
        }
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "choice",
            "branch": branch,
            "chose": forced,
        }));
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
            // guard refusal below uses).
            let eligible: Vec<String> = options
                .iter()
                .filter_map(|o| {
                    let oid = o.get("id").and_then(Json::as_str)?;
                    let once = o.get("once").and_then(Json::as_bool).unwrap_or(false);
                    if once && visited_once.contains(oid) {
                        return None;
                    }
                    match o.get("when").and_then(Json::as_str) {
                        Some(w) if self.truthy(w) == Some(false) => None,
                        _ => Some(oid.to_string()),
                    }
                })
                .collect();

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
                self.transcript.push(json!({
                    "addr": addr(cmd),
                    "kind": "hub",
                    "hub": id,
                    "chose": Json::Null,
                    "note": "no mock decision — incomplete",
                }));
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
                // A `once` option cannot be re-presented; skip a repeat force.
                continue;
            }
            // Same rule as `do_choice` (#20, D-C). A hub option is presented
            // repeatedly, so this is evaluated per visit against live state —
            // a guard false on the first pass may be true on the third, which
            // is precisely what a hub is for. Placed after the `once` skip: a
            // repeat-forced `once` option is not a visit at all.
            if let Some(when) = opt.get("when").and_then(Json::as_str) {
                if self.truthy(when) == Some(false) {
                    self.fatal = Some(format!(
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
            self.transcript.push(json!({
                "addr": addr(cmd),
                "kind": "hub",
                "hub": id,
                "chose": choice_id,
            }));
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

    fn exec_plugin(&mut self, cmd: &Json) {
        let tag = cmd
            .get("tag")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        // Apply effects that need no bridge result; record a `bridgeResult`
        // effect as unresolved (no host bridge is invoked).
        let mut unresolved = Vec::new();
        if let Some(effects) = cmd.get("effects").and_then(Json::as_array) {
            for e in effects {
                let path = e
                    .get("path")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let from = e.get("from");
                if let Some(from) = from {
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
                    } else if from.get("bridgeResult").is_some() {
                        unresolved.push(path);
                    }
                }
            }
        }
        self.transcript.push(json!({
            "addr": addr(cmd),
            "kind": "plugin",
            "tag": tag,
            "external": true,
            "unresolvedEffects": unresolved,
            "note": "external bridge call — not invoked; bridgeResult effects unresolved",
        }));
    }

    // ── quest lifecycle (quest-lifecycle.md) ────────────────────────────

    fn run_quest(&mut self) {
        // Parse declarations.
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
                    });
                }
                _ => {}
            }
        }

        // Body-segment boundaries: every objective body + every `<on>` body.
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
                self.fire_event("questActive", Some(&q.id), &handlers, &seg_starts);
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
        self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);

        // Mock events fire in order; each re-evaluates the lifecycle. An `end`
        // inside a handler/objective body ends the WALK (dsl 0.8.0), so no
        // later event is delivered — nothing downstream of the terminator runs.
        let events: Vec<String> = self.mock.events.clone();
        for ev in events {
            if self.terminated {
                break;
            }
            self.fire_event(&ev, None, &handlers, &seg_starts);
            self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);
        }

        // dsl 0.21.0 §7a.2: occasions are raised in order after the walk
        // settles; each judges the `on="<occasion>"` objectives of every
        // active quest, then the lifecycle settles again. `lute run` records
        // the raise; `lute play`'s step header already names it.
        let occasions: Vec<String> = self.mock.occasions.clone();
        for occasion in &occasions {
            if self.terminated {
                break;
            }
            if !self.quest_resume {
                self.transcript
                    .push(json!({ "kind": "occasion", "occasion": occasion }));
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
        for q in &quests {
            if self.quest_status.get(&q.id).map(String::as_str) == Some("active") {
                for o in &q.objectives {
                    // An `on=` objective is judged only at its occasion; one
                    // this walk never raised is not stuck, it is waiting.
                    let judged = o
                        .on
                        .as_ref()
                        .is_none_or(|on| occasions.contains(on));
                    if judged && !o.optional && self.eval_raw(&o.done) == Value::Unknown {
                        self.incomplete = true;
                        // `lute play` names the stuck objective in its halt;
                        // `lute run`'s transcript is unchanged.
                        if self.quest_resume {
                            self.transcript.push(json!({
                                "kind": "objective",
                                "quest": q.id,
                                "objective": o.id,
                                "done": Json::Null,
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
            // parent is active). An unreferenced accept-driven quest
            // activates once an `accept` record this walk ran names it
            // (dsl 0.21.0 §7a.3 — an accept inside a handler/objective body).
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
                            None => true,
                            Some(raw) => self.truthy(raw) == Some(true),
                        }
                    }
                    None => q.start.is_none() && self.is_accepted(&q.id),
                };
                if activate {
                    self.set_quest_state(&q.id, "active");
                    self.fire_event("questActive", Some(&q.id), handlers, seg_starts);
                    changed = true;
                }
            }
            for (qi, q) in quests.iter().enumerate() {
                if self.quest_status.get(&q.id).map(String::as_str) != Some("active") {
                    continue;
                }
                // 1. objectives (monotone; body plays once). An `on=`
                // objective is judged only at its occasion
                // ([`Runner::judge_occasion`]), never continuously.
                for (oi, o) in q.objectives.iter().enumerate() {
                    if o.on.is_some() || done.contains(&(qi, oi)) {
                        continue;
                    }
                    if self.truthy(&o.done) == Some(true) {
                        self.complete_objective(q, qi, oi, seg_starts, done);
                        changed = true;
                    }
                }
                // 2. fail BEFORE derived completion (§6.3 precedence).
                if let Some(fail) = &q.fail {
                    if self.truthy(fail) == Some(true) {
                        self.set_quest_state(&q.id, "failed");
                        // dsl 0.16.0 §3 D-D: fresh `failed` → grant
                        // `on="failed"` quest rewards BEFORE `questFailed`
                        // handlers and BEFORE the §2.3 downward cascade.
                        self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Failed);
                        self.fire_event("questFailed", Some(&q.id), handlers, seg_starts);
                        self.cascade_children(&q.id, quests, parent_of, handlers, seg_starts);
                        changed = true;
                        continue;
                    }
                }
                // 3. derived completion: all non-optional objectives done.
                let complete = q
                    .objectives
                    .iter()
                    .enumerate()
                    .all(|(oi, o)| o.optional || done.contains(&(qi, oi)));
                if complete {
                    self.set_quest_state(&q.id, "complete");
                    // dsl 0.16.0 §3 D-D: fresh `complete` → grant this
                    // quest's default-on rewards BEFORE `questComplete`
                    // handlers play.
                    self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Complete);
                    self.fire_event("questComplete", Some(&q.id), handlers, seg_starts);
                    self.cascade_children(&q.id, quests, parent_of, handlers, seg_starts);
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
    /// not-yet-done `on="<occasion>"` objectives, document order. The
    /// caller settles the lifecycle afterwards (`fail` before completion).
    fn judge_occasion(
        &mut self,
        occasion: &str,
        quests: &[QuestDecl],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        for (qi, q) in quests.iter().enumerate() {
            for (oi, o) in q.objectives.iter().enumerate() {
                if self.terminated
                    || self.quest_status.get(&q.id).map(String::as_str) != Some("active")
                {
                    break;
                }
                if o.on.as_deref() != Some(occasion) || done.contains(&(qi, oi)) {
                    continue;
                }
                if self.truthy(&o.done) == Some(true) {
                    self.complete_objective(q, qi, oi, seg_starts, done);
                }
            }
        }
    }

    /// Downward cascade (subquest design 2026-08-31 §2.3): on `terminal`'s
    /// terminal transition, every still-`active` child transitions to
    /// `failed` and fires ITS OWN `questFailed` handlers; recursive (a
    /// cascaded failure is itself a terminal transition). A required child
    /// cannot be `active` when its parent completes (its completion is part
    /// of the parent's derived completion), so the `complete` arm only ever
    /// fails running optionals.
    fn cascade_children(
        &mut self,
        terminal: &str,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let mut stack = vec![terminal.to_string()];
        while let Some(parent) = stack.pop() {
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
                self.set_quest_state(&child, "failed");
                self.emit_grants(&child, None, child_rewards, GrantEvent::Failed);
                self.fire_event("questFailed", Some(&child), handlers, seg_starts);
                stack.push(child);
            }
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
    /// `None` (a mock world event) fires every matching handler.
    fn fire_event(
        &mut self,
        event: &str,
        scope: Option<&str>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let matching: Vec<usize> = handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                h.event == event && scope.is_none_or(|s| h.quest.as_deref() == Some(s))
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
                self.run_segment(&body, seg_starts);
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
    /// from `body` up to the next `entry` record, the `<on>`-body
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
        let stop = self.commands[at + 1..]
            .iter()
            .position(|c| c.get("kind").and_then(Json::as_str) == Some("entry"))
            .map(|i| at + 1 + i)
            .unwrap_or(self.commands.len());
        self.apply_effects = first_read;
        self.run_range(start, stop);
        self.apply_effects = true;
        if first_read && !self.incomplete && self.fatal.is_none() {
            self.state.insert(read_path, Value::Bool(true));
        }
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
                    "  {a}  hub    [{}] -> {}",
                    e.get("hub").and_then(Json::as_str).unwrap_or(""),
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
                "end" => match e.get("reason").and_then(Json::as_str) {
                    Some(r) => format!("  {a}  end    reason={r}"),
                    None => format!("  {a}  end"),
                },
                "plugin" => format!(
                    "  {a}  plugin {} (external call, not invoked)",
                    e.get("tag").and_then(Json::as_str).unwrap_or("")
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
                "occasion" => format!(
                    "  occasion {}",
                    e.get("occasion").and_then(Json::as_str).unwrap_or("")
                ),
                "objective" => format!(
                    "  {}.{} done",
                    e.get("quest").and_then(Json::as_str).unwrap_or(""),
                    e.get("objective").and_then(Json::as_str).unwrap_or("")
                ),
                "quest" => format!(
                    "  quest {} -> {}",
                    e.get("quest").and_then(Json::as_str).unwrap_or(""),
                    e.get("state").and_then(Json::as_str).unwrap_or("")
                ),
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
    }
}

fn parse_rules(art: &Json) -> Vec<Rule> {
    let Some(arr) = art.get("rules").and_then(Json::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|r| {
            let head = parse_atom(r.get("head")?)?;
            let body = r
                .get("body")
                .and_then(Json::as_array)
                .map(|b| b.iter().filter_map(parse_lit).collect())
                .unwrap_or_default();
            Some(Rule { head, body })
        })
        .collect()
}

fn parse_atom(a: &Json) -> Option<RAtom> {
    let rel = a.get("relation").and_then(Json::as_str)?.to_string();
    let terms = a
        .get("terms")
        .and_then(Json::as_array)
        .map(|ts| ts.iter().filter_map(parse_term).collect())
        .unwrap_or_default();
    Some(RAtom { rel, terms })
}

fn parse_term(t: &Json) -> Option<Term> {
    match t.get("kind").and_then(Json::as_str)? {
        "var" => Some(Term::Var(t.get("name").and_then(Json::as_str)?.to_string())),
        "const" => Some(Term::Const(
            t.get("value").and_then(Json::as_str)?.to_string(),
        )),
        _ => None,
    }
}

fn parse_lit(l: &Json) -> Option<Lit> {
    match l.get("kind").and_then(Json::as_str)? {
        "atom" => Some(Lit::Atom {
            atom: parse_atom(l.get("atom")?)?,
            negated: l.get("negated").and_then(Json::as_bool).unwrap_or(false),
        }),
        "cmp" => Some(Lit::Cmp {
            lhs: parse_term(l.get("lhs")?)?,
            rhs: parse_term(l.get("rhs")?)?,
            negated: l.get("negated").and_then(Json::as_bool).unwrap_or(false),
        }),
        "guard" => Some(Lit::Guard {
            cel: l.get("cel").and_then(Json::as_str)?.to_string(),
        }),
        _ => None,
    }
}

/// Assign a least stratum to each derived relation (cel-and-facts.md): a
/// positive body atom keeps the head at-or-above its stratum; a negated one
/// pushes the head strictly above. Stratification (checker-guaranteed) makes
/// this converge; a cap defends against a malformed artifact.
fn compute_strata(rules: &[Rule], derived: &BTreeSet<String>) -> BTreeMap<String, usize> {
    let mut strata: BTreeMap<String, usize> = derived.iter().map(|r| (r.clone(), 0)).collect();
    let cap = derived.len() + 2;
    for _ in 0..cap {
        let mut changed = false;
        for rule in rules {
            let h = &rule.head.rel;
            for lit in &rule.body {
                if let Lit::Atom { atom, negated } = lit {
                    if derived.contains(&atom.rel) {
                        let want = strata[&atom.rel] + usize::from(*negated);
                        if strata[h] < want {
                            strata.insert(h.clone(), want);
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    strata
}

/// The stratified least-fixpoint: `base ∪ derive(base)`, evaluated stratum by
/// stratum (cel-and-facts.md).
fn fixpoint(
    base: &BTreeSet<Fact>,
    rules: &[Rule],
    strata: &BTreeMap<String, usize>,
    state: &BTreeMap<String, Value>,
    schema: &StateSchema,
) -> BTreeSet<Fact> {
    let mut facts = base.clone();
    if rules.is_empty() {
        return facts;
    }
    let max = strata.values().copied().max().unwrap_or(0);
    for s in 0..=max {
        loop {
            let mut new: Vec<Fact> = Vec::new();
            for rule in rules {
                if strata.get(&rule.head.rel).copied().unwrap_or(0) != s {
                    continue;
                }
                for binding in solve_body(&rule.body, &facts, state, schema) {
                    let mut args = Vec::with_capacity(rule.head.terms.len());
                    let mut ok = true;
                    for t in &rule.head.terms {
                        match t {
                            Term::Const(c) => args.push(c.clone()),
                            Term::Var(v) => match binding.get(v) {
                                Some(val) => args.push(val.clone()),
                                None => {
                                    ok = false;
                                    break;
                                }
                            },
                        }
                    }
                    if ok {
                        let fact = (rule.head.rel.clone(), args);
                        if !facts.contains(&fact) && !new.contains(&fact) {
                            new.push(fact);
                        }
                    }
                }
            }
            if new.is_empty() {
                break;
            }
            facts.extend(new);
        }
    }
    facts
}

/// Enumerate every variable binding satisfying `body` over `facts`: join the
/// positive atoms, then filter by negated atoms, comparisons, and guards.
fn solve_body(
    body: &[Lit],
    facts: &BTreeSet<Fact>,
    state: &BTreeMap<String, Value>,
    schema: &StateSchema,
) -> Vec<BTreeMap<String, String>> {
    let mut bindings: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    // 1. positive atoms generate/extend bindings.
    for lit in body {
        if let Lit::Atom {
            atom,
            negated: false,
        } = lit
        {
            let mut next = Vec::new();
            for b in &bindings {
                for (rel, args) in facts {
                    if rel != &atom.rel || args.len() != atom.terms.len() {
                        continue;
                    }
                    if let Some(ext) = unify(&atom.terms, args, b) {
                        next.push(ext);
                    }
                }
            }
            bindings = next;
        }
    }
    // 2. filters.
    bindings.retain(|b| {
        body.iter().all(|lit| match lit {
            Lit::Atom { negated: false, .. } => true,
            Lit::Atom {
                atom,
                negated: true,
            } => {
                let ground: Option<Vec<String>> = atom
                    .terms
                    .iter()
                    .map(|t| match t {
                        Term::Const(c) => Some(c.clone()),
                        Term::Var(v) => b.get(v).cloned(),
                    })
                    .collect();
                match ground {
                    Some(g) => !facts.contains(&(atom.rel.clone(), g)),
                    None => true, // unbound (defensive; safety-checked away in practice)
                }
            }
            Lit::Cmp { lhs, rhs, negated } => {
                let l = ground_term(lhs, b);
                let r = ground_term(rhs, b);
                match (l, r) {
                    (Some(l), Some(r)) => (l == r) != *negated,
                    _ => false,
                }
            }
            Lit::Guard { cel } => eval_rule_guard(cel, b, state, schema),
        })
    });
    bindings
}

fn ground_term(t: &Term, b: &BTreeMap<String, String>) -> Option<String> {
    match t {
        Term::Const(c) => Some(c.clone()),
        Term::Var(v) => b.get(v).cloned(),
    }
}

/// Evaluate a rule-body CEL guard (cel-and-facts.md): a guard reads only
/// scalar state and the ground terms bound by the join — never facts. Each
/// bound (leading-uppercase) rule variable is substituted by its ground value,
/// then the fragment is parsed + evaluated over live state with an empty fact
/// store (a fact query in a rule guard is rejected by the checker, so none
/// reaches here).
fn eval_rule_guard(
    cel: &str,
    binding: &BTreeMap<String, String>,
    state: &BTreeMap<String, Value>,
    schema: &StateSchema,
) -> bool {
    let substituted = substitute_vars(cel, binding);
    let mut arena = CelArena::default();
    let Ok(handle) = lute_cel::parse_slot(&mut arena, &substituted, 0) else {
        return false;
    };
    let Some(ided) = arena.get(handle) else {
        return false;
    };
    let eff = EffectiveState::new(schema, state.clone());
    let vocab = RelVocab::default();
    let fs = FactStore::new(&vocab);
    let env = EvalEnv {
        state: &eff,
        facts: &fs,
    };
    let mut unresolved = Vec::new();
    matches!(eval(&ided.expr, &env, &mut unresolved), Value::Bool(true))
}

/// Substitute each bound rule variable (a leading-uppercase identifier) in a
/// guard fragment with its ground value — a numeric value inlined bare, any
/// other value quoted as a CEL string literal. String-literal regions are left
/// untouched (`lute_cel::cel_string_mask`), so a `'@gold'`-style member value
/// is never rewritten.
fn substitute_vars(cel: &str, binding: &BTreeMap<String, String>) -> String {
    if binding.is_empty() {
        return cel.to_string();
    }
    let mask = lute_cel::cel_string_mask(cel);
    let bytes = cel.as_bytes();
    let mut out = String::with_capacity(cel.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let in_string = mask.get(i).copied().unwrap_or(false);
        if !in_string && (c.is_ascii_alphabetic() || c == '_') {
            // Consume an identifier.
            let start = i;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            let ident = &cel[start..i];
            match binding.get(ident) {
                Some(val) => {
                    if val.parse::<f64>().is_ok() {
                        out.push_str(val);
                    } else {
                        out.push('\'');
                        out.push_str(&val.replace('\'', "\\'"));
                        out.push('\'');
                    }
                }
                None => out.push_str(ident),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Extend `binding` so `terms` matches `args`, or `None` on a conflict.
fn unify(
    terms: &[Term],
    args: &[String],
    binding: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let mut b = binding.clone();
    for (t, a) in terms.iter().zip(args) {
        match t {
            Term::Const(c) => {
                if c != a {
                    return None;
                }
            }
            Term::Var(v) => match b.get(v) {
                Some(existing) if existing != a => return None,
                Some(_) => {}
                None => {
                    b.insert(v.clone(), a.clone());
                }
            },
        }
    }
    Some(b)
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
