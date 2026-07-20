//! `lute run` — the reference headless runner over a COMPILED artifact
//! (the executable counterpart of `docs/runtime/` +
//! `schemas/lute-ir-0.7.schema.json`).
//!
//! `lute run` is the *engine* side of the runtime contract. It loads a compiled
//! artifact (`lute compile` output), gates on `irVersion` by **major.minor**
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
//! - the **quest lifecycle** (quest-lifecycle.md): `start`/accept activation,
//!   monotone objective completion (bodies play once), `fail` evaluated before
//!   derived completion, and `<on>` handlers fired on the engine-derived
//!   transitions (`questActive`/`questComplete`/`questFailed`) plus mock
//!   `events:`.
//!
//! Output: a human transcript by default; `--json` emits a stable machine
//! transcript `{ kind, irVersion, exit, commands, state, facts, quests }`.
//!
//! Exit codes: `0` a complete walk, `2` an I/O / usage failure (unreadable
//! artifact/mock, malformed artifact, an `irVersion` outside the implemented
//! major.minor line, or an unknown command `kind`), `3` an incomplete walk (a
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
use lute_trace::{eval, EffectiveState, EvalEnv, FactStore, Value};
use serde_json::{json, Value as Json};

/// The IR major.minor line this reference runner implements, derived from
/// [`lute_compile::LUTE_IR_VERSION`] so the gate follows the compiler's IR
/// version forever. Parsing gates on major.minor (execution-model.md): an
/// artifact from a different major.minor is refused (exit 2); the PATCH
/// component is advisory and never gates.
fn impl_ir_line() -> (u64, u64) {
    parse_major_minor(lute_compile::LUTE_IR_VERSION)
        .expect("LUTE_IR_VERSION must carry a major.minor prefix")
}

/// A ground fact: `(relation, args)`.
type Fact = (String, Vec<String>);

/// Execute a compiled artifact against a mock playthrough. See [`crate::Command::Run`].
pub fn run_artifact(artifact: &Path, mock: Option<&Path>, json_out: bool) -> ExitCode {
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

    // ── Version negotiation (execution-model.md): gate on major.minor. ──
    let (impl_major, impl_minor) = impl_ir_line();
    let ir_version = art.get("irVersion").and_then(Json::as_str).unwrap_or("");
    match parse_major_minor(ir_version) {
        Some((maj, min)) if maj == impl_major && min == impl_minor => {}
        _ => {
            eprintln!(
                "lute run: unsupported irVersion {ir_version:?}: this runner implements the \
                 {impl_major}.{impl_minor} line (engines gate on major.minor)"
            );
            return ExitCode::from(2);
        }
    }

    if !art.get("commands").map(Json::is_array).unwrap_or(false) {
        eprintln!("lute run: artifact has no `commands` array");
        return ExitCode::from(2);
    }

    // ── Mock playthrough (same surfaces as `lute trace --mock`). ──
    let mock_set = match mock {
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

    let mut runner = Runner::new(&art, mock_set);
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

/// Parse `"0.7.0"` → `(0, 7)`; `None` when it lacks a `major.minor` prefix.
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
}

struct Obj {
    id: String,
    done: String,
    optional: bool,
    /// `addr` of the completion body segment, or `None` (empty body).
    body: Option<String>,
}

/// A parsed `<on>` handler.
struct Handler {
    event: String,
    when: Option<String>,
    body: String,
}

/// The bounded step outcome of the dispatcher.
enum Step {
    /// Continue at this command index (`>= commands.len()` ⇒ end).
    Next(usize),
    /// Halt the walk (incomplete decision or a hard error already recorded).
    Halt,
}

/// The reference engine over one artifact.
struct Runner {
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
    /// An unknown command `kind` or malformed record (exit 2).
    fatal: Option<String>,
}

impl Runner {
    fn new(art: &Json, mock: lute_trace::MockSet) -> Self {
        let kind = art.get("kind").and_then(Json::as_str).unwrap_or("scene").to_string();
        let commands = art.get("commands").and_then(Json::as_array).cloned().unwrap_or_default();

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

        let mut runner = Runner {
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
            fatal: None,
        };

        // Apply mock state seeds (override defaults), coerced by declared type.
        let seeds: Vec<(String, String)> =
            runner.mock.state.iter().map(|(p, lit, _)| (p.clone(), lit.clone())).collect();
        for (path, lit) in seeds {
            let v = runner.coerce_literal(&path, &lit);
            runner.state.insert(path, v);
        }
        // Mock facts join the base set (they are supplied ground answers).
        let mock_facts: Vec<String> = runner.mock.facts.clone();
        for f in mock_facts {
            if let Some(fact) = parse_ground_fact(&f) {
                runner.base_facts.insert(fact);
            }
        }
        runner.recompute_facts();
        runner
    }

    /// Coerce a raw mock literal against a path's declared value-type.
    fn coerce_literal(&self, path: &str, lit: &str) -> Value {
        match self.types.get(path).map(String::as_str) {
            Some("bool") => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Str(lit.to_string()),
            },
            Some("number") => lit.parse::<f64>().map(Value::Num).unwrap_or(Value::Str(lit.to_string())),
            // enum / string / reserved / unknown: keep verbatim, but recognize
            // an obvious bool/number so an un-typed seed still evaluates.
            _ => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => lit.parse::<f64>().map(Value::Num).unwrap_or(Value::Str(lit.to_string())),
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
    fn eval_raw(&self, raw: &str) -> Value {
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
        let env = EvalEnv { state: &eff, facts: &fs };
        let mut unresolved = Vec::new();
        eval(&ided.expr, &env, &mut unresolved)
    }

    /// `Some(bool)` for a decided guard, `None` when unknown.
    fn truthy(&self, raw: &str) -> Option<bool> {
        match self.eval_raw(raw) {
            Value::Bool(b) => Some(b),
            _ => None,
        }
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

    fn run(&mut self) -> Result<(), String> {
        if self.kind == "quest" {
            self.run_quest();
        } else {
            self.run_range(0, self.commands.len());
        }
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// Drive the dispatcher over `[start, stop)`. Used for the whole scene
    /// (`0..len`) and for bounded hub-option / quest-body segments.
    fn run_range(&mut self, start: usize, stop: usize) {
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
                    if self.fatal.is_some() || self.incomplete {
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
            "plugin" => {
                self.exec_plugin(&cmd);
                Step::Next(pc + 1)
            }
            // Declarations — inert in a linear walk (a quest artifact is driven
            // by `run_quest`, never linearly).
            "quest" | "on" => Step::Next(pc + 1),
            other => {
                self.fatal = Some(format!("unknown command kind {other:?} (a new capability the runner cannot fake)"));
                Step::Halt
            }
        }
    }

    // ── content & staging ──────────────────────────────────────────────

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
        let path = cmd.get("path").and_then(Json::as_str).unwrap_or("").to_string();
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
        let rel = cmd.get("relation").and_then(Json::as_str).unwrap_or("").to_string();
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
        let rel = cmd.get("relation").and_then(Json::as_str).unwrap_or("").to_string();
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

    // ── control flow ───────────────────────────────────────────────────

    fn do_choice(&mut self, cmd: &Json) -> Step {
        let branch = cmd.get("branchId").and_then(Json::as_str).unwrap_or("").to_string();
        let record_key = cmd.get("recordKey").and_then(Json::as_str).map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let options = cmd.get("options").and_then(Json::as_array).cloned().unwrap_or_default();

        let forced = match self.mock.choose.get(&branch).and_then(|v| v.first()).cloned() {
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
        let opt = match options.iter().find(|o| o.get("id").and_then(Json::as_str) == Some(&forced)) {
            Some(o) => o.clone(),
            None => {
                self.fatal = Some(format!("choice `{branch}` has no option `{forced}`"));
                return Step::Halt;
            }
        };
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

    fn do_hub(&mut self, cmd: &Json) -> Step {
        let id = cmd.get("id").and_then(Json::as_str).unwrap_or("").to_string();
        let record_key = cmd.get("recordKey").and_then(Json::as_str).map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let converge_idx = self.resolve(converge);
        let options = cmd.get("options").and_then(Json::as_array).cloned().unwrap_or_default();

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

        let forced = match self.mock.choose.get(&id).cloned() {
            Some(list) => list,
            None => {
                self.incomplete = true;
                self.transcript.push(json!({
                    "addr": addr(cmd),
                    "kind": "hub",
                    "hub": id,
                    "chose": Json::Null,
                    "note": "no mock decision — incomplete",
                }));
                return Step::Halt;
            }
        };

        let mut visited_once: BTreeSet<String> = BTreeSet::new();
        for choice_id in forced {
            let opt = match options.iter().find(|o| o.get("id").and_then(Json::as_str) == Some(&choice_id)) {
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
            if let Some(key) = &record_key {
                self.state.insert(key.clone(), Value::Str(choice_id.clone()));
            }
            // hub visit record slot (scene.visited.<hub>.<opt>, state-lifecycle.md).
            self.state.insert(format!("scene.visited.{id}.{choice_id}"), Value::Bool(true));
            self.transcript.push(json!({
                "addr": addr(cmd),
                "kind": "hub",
                "hub": id,
                "chose": choice_id,
            }));
            let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
            let start = self.resolve(target);
            let stop = boundaries.iter().find(|&&b| b > start).copied().unwrap_or(self.commands.len());
            self.run_range(start, stop);
            if self.fatal.is_some() || self.incomplete {
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
        let arms = cmd.get("arms").and_then(Json::as_array).cloned().unwrap_or_default();
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        for (i, arm) in arms.iter().enumerate() {
            let test = arm.get("test").and_then(Json::as_str).unwrap_or("");
            if self.truthy(test) == Some(true) {
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

    fn exec_plugin(&mut self, cmd: &Json) {
        let tag = cmd.get("tag").and_then(Json::as_str).unwrap_or("").to_string();
        // Apply effects that need no bridge result; record a `bridgeResult`
        // effect as unresolved (no host bridge is invoked).
        let mut unresolved = Vec::new();
        if let Some(effects) = cmd.get("effects").and_then(Json::as_array) {
            for e in effects {
                let path = e.get("path").and_then(Json::as_str).unwrap_or("").to_string();
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
                        event: cmd.get("event").and_then(Json::as_str).unwrap_or("").to_string(),
                        when: cel_raw(cmd.get("when")),
                        body: cmd.get("body").and_then(Json::as_str).unwrap_or("").to_string(),
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

        for q in &quests {
            self.quest_status.insert(q.id.clone(), "unset".to_string());
        }

        // Activation (quest-lifecycle.md §Activation).
        let quest_ids: Vec<String> = quests.iter().map(|q| q.id.clone()).collect();
        for (qi, q) in quests.iter().enumerate() {
            let activate = match &q.start {
                None => true, // no `start`: activates at the start of the walk / accept.
                Some(raw) => self.truthy(raw) == Some(true),
            } || self.mock.accepts.contains(&quest_ids[qi]);
            if activate {
                self.set_quest_state(&q.id, "active");
                self.fire_event("questActive", &handlers, &seg_starts);
            }
        }

        // Track which objectives have completed (monotone).
        let mut done: BTreeSet<(usize, usize)> = BTreeSet::new();
        self.reevaluate(&quests, &handlers, &seg_starts, &mut done);

        // Mock events fire in order; each re-evaluates the lifecycle.
        let events: Vec<String> = self.mock.events.clone();
        for ev in events {
            self.fire_event(&ev, &handlers, &seg_starts);
            self.reevaluate(&quests, &handlers, &seg_starts, &mut done);
        }

        // Incomplete if an active quest is stuck on an undecidable required
        // objective (a missing mock left the `done` predicate unknown).
        for q in &quests {
            if self.quest_status.get(&q.id).map(String::as_str) == Some("active") {
                for o in &q.objectives {
                    if !o.optional && self.eval_raw(&o.done) == Value::Unknown {
                        self.incomplete = true;
                    }
                }
            }
        }
    }

    /// Re-evaluate objectives (monotone), then `fail` before derived completion
    /// (quest-lifecycle.md §Re-evaluation cadence), to a fixpoint.
    fn reevaluate(
        &mut self,
        quests: &[QuestDecl],
        handlers: &[Handler],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        let mut changed = true;
        let mut rounds = 0;
        while changed && rounds < quests.len() * 8 + 16 {
            changed = false;
            rounds += 1;
            for (qi, q) in quests.iter().enumerate() {
                if self.quest_status.get(&q.id).map(String::as_str) != Some("active") {
                    continue;
                }
                // 1. objectives (monotone; body plays once).
                for (oi, o) in q.objectives.iter().enumerate() {
                    if done.contains(&(qi, oi)) {
                        continue;
                    }
                    if self.truthy(&o.done) == Some(true) {
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
                        if let Some(body) = &o.body {
                            self.run_segment(body, seg_starts);
                        }
                        changed = true;
                    }
                }
                // 2. fail BEFORE derived completion (§6.3 precedence).
                if let Some(fail) = &q.fail {
                    if self.truthy(fail) == Some(true) {
                        self.set_quest_state(&q.id, "failed");
                        self.fire_event("questFailed", handlers, seg_starts);
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
                    self.fire_event("questComplete", handlers, seg_starts);
                    changed = true;
                }
            }
        }
    }

    fn set_quest_state(&mut self, id: &str, state: &str) {
        self.state.insert(format!("quest.{id}.state"), Value::Str(state.to_string()));
        self.quest_status.insert(id.to_string(), state.to_string());
        self.transcript.push(json!({
            "kind": "quest",
            "quest": id,
            "state": state,
        }));
    }

    /// Fire every handler matching `event` whose `when` holds over the current
    /// (pre-event) state snapshot, running each body once.
    fn fire_event(&mut self, event: &str, handlers: &[Handler], seg_starts: &[usize]) {
        let matching: Vec<usize> = handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.event == event)
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
        let stop = seg_starts.iter().find(|&&s| s > start).copied().unwrap_or(self.commands.len());
        self.run_range(start, stop);
    }

    // ── output ─────────────────────────────────────────────────────────

    fn output_value(&self) -> Json {
        let state: serde_json::Map<String, Json> =
            self.state.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect();
        let facts: Vec<Json> =
            self.all_facts.iter().map(|(r, a)| Json::String(render_fact(r, a))).collect();
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
        println!("{}", serde_json::to_string_pretty(&self.output_value()).unwrap_or_default());
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
                "assert" => format!("  {a}  assert {}", e.get("fact").and_then(Json::as_str).unwrap_or("")),
                "retract" => format!("  {a}  retract {}", e.get("pattern").and_then(Json::as_str).unwrap_or("")),
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
                "match" => format!("  {a}  match  -> {}", e.get("result").and_then(Json::as_str).unwrap_or("")),
                "barrier" => format!("  {a}  barrier (no real clock)"),
                "plugin" => format!(
                    "  {a}  plugin {} (external call, not invoked)",
                    e.get("tag").and_then(Json::as_str).unwrap_or("")
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
        println!("run {}", if self.incomplete { "incomplete" } else { "complete" });
    }
}

// ── free helpers ────────────────────────────────────────────────────────

fn addr(cmd: &Json) -> &str {
    cmd.get("addr").and_then(Json::as_str).unwrap_or("")
}

/// The `raw` of a `{raw, expr}` CEL pair, when present and non-empty.
fn cel_raw(pair: Option<&Json>) -> Option<String> {
    pair.and_then(|p| p.get("raw")).and_then(Json::as_str).filter(|s| !s.trim().is_empty()).map(str::to_string)
}

fn parse_quest(cmd: &Json) -> QuestDecl {
    let id = cmd.get("id").and_then(Json::as_str).unwrap_or("").to_string();
    let objectives = cmd
        .get("objectives")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .map(|o| Obj {
                    id: o.get("id").and_then(Json::as_str).unwrap_or("").to_string(),
                    done: o.get("done").and_then(|d| d.get("raw")).and_then(Json::as_str).unwrap_or("").to_string(),
                    optional: o.get("optional").and_then(Json::as_bool).unwrap_or(false),
                    body: o.get("body").and_then(Json::as_str).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    QuestDecl {
        id,
        start: cel_raw(cmd.get("start")),
        fail: cel_raw(cmd.get("fail")),
        objectives,
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
        "const" => Some(Term::Const(t.get("value").and_then(Json::as_str)?.to_string())),
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
        if let Lit::Atom { atom, negated: false } = lit {
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
            Lit::Atom { atom, negated: true } => {
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
    let env = EvalEnv { state: &eff, facts: &fs };
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
fn unify(terms: &[Term], args: &[String], binding: &BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
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
