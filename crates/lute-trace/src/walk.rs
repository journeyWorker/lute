//! §4.4 walk semantics: the deterministic, document-ordered walk over
//! `doc.shots` (the scene walk, Task 19) and `doc.quests` (the quest walk —
//! events, monotonic objectives, fail precedence, Task 20) plus the §4.3
//! pipeline that gets a document there — check gate, mock validation, D14's
//! `normalize_document` + `expand_document` reuse, then the walk itself.
//!
//! ## Pipeline (§4.3, verbatim order)
//! 1. `lute_check::check(input)` — any `Error` -> [`TraceExit::Refused`]
//!    (run `check` first).
//! 2. Parse + `lute_cel::fill_document` + `lute_check::fold_env` (the SAME
//!    re-derivation `lute_compile::compile` performs after its own gate).
//! 3. [`crate::mock::validate`] — any `E-TRACE-*` -> `Refused`.
//! 4. `lute_compile::normalize::normalize_document` (components bound, §6.4
//!    folds, `when=`/`persist=` desugared) then
//!    `lute_compile::expand::expand_document` (`@`/`$`-free) — D14.
//! 5. Walk `doc.shots` (scene) then `doc.quests` (quest) linearly.
//!
//! ## Why every CEL slot is RE-PARSED, never read via `slot.ast`
//! `expand_document`'s `expand_slot` rewrites `slot.raw` (text substitution)
//! but never touches `slot.ast` — a handle filled by step 2 points at the
//! ARENA ENTRY FOR THE PRE-EXPANSION TEXT, which is now stale. `normalize`
//! also synthesizes brand-new slots (`synth_when_match`'s `"$"` test) that
//! were never filled at all. `lute-compile`'s own `expr.rs` (`lower`) and
//! `match_check.rs`'s `test_expr` hit the identical situation and both
//! re-parse `slot.raw` fresh from a scratch [`CelArena`] rather than trust
//! `.ast` — [`slot_expr`] is that same house idiom, carried here (D1: the
//! evaluator lives ONLY in this crate).

use std::collections::BTreeMap;

use cel_parser::ast::Expr;
use lute_cel::CelArena;
use lute_check::cel_expand::{expand_cel, DefTable};
use lute_check::ctx::Env as CheckEnv;
use lute_check::meta::StateSchema;
use lute_check::{CheckInput, Ctx};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{
    Arm, Assert, AttrValue, Branch, CelSlot, Choice, ClipNode, Directive, Document, Hub, Interp,
    InterpKind, IsPattern, Line, Match, Node, Objective, Quest, Retract, Set, Timeline,
};
use lute_syntax::datalog::FactTerm;

use crate::eval::{
    eval, eval_path_read, expr_path, literal_to_value, EffectiveState, EvalEnv, FactStore, Pat, Read,
};
use crate::mock::{self, MockSet};
use crate::report::{self, ComponentBoundary, Coverage, CoverageCount, Decision, Seeds, Step, TraceExit, TraceReport, UnresolvedEntry};
use crate::value::{UnresolvedAtom, Value};

/// Walk control flow: `Continue` past this node/construct; `Incomplete`
/// propagates all the way up to [`trace_document`] as an `unknown` guard
/// that HALTED the walk (§4.4, exit 3); `Refused` propagates a walk-time
/// `E-TRACE-CHOICE` (a forced choice ineligible at its presentation point,
/// §4.4, exit 1). Every `walk_*` function returns this and every caller
/// stops immediately on a non-`Continue` value — the walk truly halts, it
/// never merely skips the offending construct.
enum Flow {
    Continue,
    Incomplete,
    Refused(Vec<Diagnostic>),
}

/// Mutable walk state threaded through every `walk_*` function.
struct Walk<'a> {
    mocks: &'a MockSet,
    defs: &'a DefTable<'a>,
    check_env: &'a CheckEnv,
    snapshot: &'a CapabilitySnapshot,
    state: EffectiveState<'a>,
    facts: FactStore<'a>,
    steps: Vec<Step>,
    decisions: Vec<Decision>,
    unresolved: Vec<UnresolvedEntry>,
    coverage_choices: BTreeMap<String, CoverageCount>,
    coverage_arms: BTreeMap<String, CoverageCount>,
}

impl<'a> Walk<'a> {
    fn env(&self) -> EvalEnv<'_> {
        EvalEnv { state: &self.state, facts: &self.facts }
    }

    fn push_decision(
        &mut self,
        construct: &str,
        id: &str,
        span: Span,
        outcome: String,
        guard: Option<String>,
        forced: bool,
        auto: bool,
        eligible: Vec<String>,
    ) {
        let d = Decision {
            construct: construct.to_string(),
            id: id.to_string(),
            span,
            outcome,
            guard,
            forced,
            auto,
            eligible,
        };
        self.decisions.push(d.clone());
        self.steps.push(Step::Decision(d));
    }

    fn record_choice_decision(
        &mut self,
        construct: &str,
        id: &str,
        span: Span,
        outcome: String,
        guard: Option<String>,
        forced: bool,
        auto: bool,
        eligible: Vec<String>,
        total: usize,
    ) {
        self.push_decision(construct, id, span, outcome, guard, forced, auto, eligible);
        self.coverage_choices
            .entry(id.to_string())
            .and_modify(|c| c.visited += 1)
            .or_insert(CoverageCount { visited: 1, total });
    }

    fn record_match_decision(&mut self, id: &str, span: Span, outcome: String, guard: Option<String>, total: usize) {
        self.push_decision("match", id, span, outcome, guard, false, false, Vec::new());
        self.coverage_arms.insert(id.to_string(), CoverageCount { visited: 1, total });
    }

    fn record_unresolved(&mut self, construct: &str, id: &str, span: Span, expression: String, atoms: Vec<UnresolvedAtom>) {
        let rendered = atoms.iter().map(render_atom).collect();
        self.unresolved.push(UnresolvedEntry {
            construct: construct.to_string(),
            id: id.to_string(),
            span,
            expression,
            atoms: rendered,
        });
    }
}

/// One `UnresolvedAtom` -> its §4.6 "supply it as a mock" hint text.
fn render_atom(a: &UnresolvedAtom) -> String {
    match a {
        UnresolvedAtom::Path(p) => format!("--state {p}=<value>"),
        UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => format!("--fact \"{f}\""),
        UnresolvedAtom::Time => "no mock surface for narrative time".to_string(),
    }
}

/// Parse `raw` fresh into a scratch [`CelArena`] and hand back its `Expr`
/// (see the module doc for why — `slot.ast` is unreliable post-normalize/
/// expand). An empty/whitespace-only `raw` (a structural gap, e.g. an
/// absent `test=`) has no CEL to parse and yields `None`; a parse failure
/// (gate-proven unreachable post-`check()`) also yields `None` rather than
/// panicking.
fn slot_expr(raw: &str) -> Option<Expr> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, raw, 0).ok()?;
    arena.get(handle).map(|r| r.expr.clone())
}

/// A `<choice when=…>`/`<hub choice when=…>` guard: `None` (no `when`) is
/// trivially eligible; otherwise evaluate it.
/// A `Condition`-kind slot's decided value is gate-proven boolean; a stray
/// `Num`/`Str` (unreachable post-`check()`) degrades to `Unknown` here
/// rather than forcing every guard call site to guess at a non-boolean
/// outcome.
fn as_guard_value(v: Value) -> Value {
    match v {
        Value::Bool(_) | Value::Unknown => v,
        Value::Num(_) | Value::Str(_) => Value::Unknown,
    }
}

fn eval_choice_guard(when: Option<&CelSlot>, env: &EvalEnv<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    let Some(slot) = when else {
        return Value::Bool(true);
    };
    let v = match slot_expr(&slot.raw) {
        Some(expr) => eval(&expr, env, unresolved),
        None => Value::Bool(true),
    };
    as_guard_value(v)
}

fn render_choice_guard(when: Option<&CelSlot>) -> Option<String> {
    when.and_then(|s| {
        let t = s.raw.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

/// `lit` (a `<when is="…">` alternative, already trimmed, never `"unset"` —
/// callers filter that member out) against a DECIDED subject `v`.
fn literal_matches(lit: &str, v: &Value) -> bool {
    match v {
        Value::Bool(b) => match lit {
            "true" => *b,
            "false" => !*b,
            _ => false,
        },
        Value::Num(n) => lit.parse::<f64>().map(|l| l == *n).unwrap_or(false),
        Value::Str(s) => s == lit,
        Value::Unknown => false,
    }
}

/// `is="a|b|unset"` pattern matching against the match SUBJECT (§7.3.1,
/// §4.4: "an `Unset` subject matches `is=\"unset\"`"). Unlike a general
/// value-read ([`eval_path_read`], which reports `Unset` as `Unknown` — a
/// path with no effective value has genuinely never been decided at the
/// VALUE level), `unset`-MEMBERSHIP is definite when the subject is a bare
/// state path (mirrors `isSet()`/`has()`, D19): read directly via
/// [`EffectiveState::read`], never through the atom-recording value path,
/// so a subject that IS unset never spuriously halts an `is="unset"` arm.
/// A non-path subject (e.g. a `holds(...)` fact query) has no notion of
/// "unset"; `unset` never matches it.
fn eval_is_pattern(pat: &IsPattern, subject_raw: &str, env: &EvalEnv<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    let alts: Vec<&str> = pat.raw.split('|').map(str::trim).collect();
    let wants_unset = alts.iter().any(|a| *a == "unset");
    let subject_path = slot_expr(subject_raw).and_then(|e| expr_path(&e));
    if let Some(path) = &subject_path {
        if matches!(env.state.read(path), Read::Unset) {
            return Value::Bool(wants_unset);
        }
    }
    let literals: Vec<&str> = alts.iter().copied().filter(|a| *a != "unset").collect();
    if literals.is_empty() {
        // The pattern was ONLY "unset" and the subject IS set (or is not a
        // bare path at all) -> definitely not a member.
        return Value::Bool(false);
    }
    let Some(expr) = slot_expr(subject_raw) else {
        return Value::Unknown;
    };
    let v = eval(&expr, env, unresolved);
    if v == Value::Unknown {
        return Value::Unknown;
    }
    Value::Bool(literals.iter().any(|lit| literal_matches(lit, &v)))
}

/// K3 AND over two already-evaluated [`Value`]s (§7.3.1: `is` + `test` =
/// pattern AND guard).
fn k3_and(a: Value, b: Value) -> Value {
    if a == Value::Bool(false) || b == Value::Bool(false) {
        Value::Bool(false)
    } else if a == Value::Unknown || b == Value::Unknown {
        Value::Unknown
    } else {
        Value::Bool(true)
    }
}

/// One `<when is test>` arm's combined guard (§7.3.1). Neither present is
/// `E-WHEN-PATTERN`-gated unreachable; degrades to `Bool(true)` rather than
/// panicking.
fn eval_arm_guard(is: Option<&IsPattern>, test: &CelSlot, subject_raw: &str, env: &EvalEnv<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    let is_val = match is {
        None => Value::Bool(true),
        Some(pat) => eval_is_pattern(pat, subject_raw, env, unresolved),
    };
    if is_val == Value::Bool(false) {
        return Value::Bool(false);
    }
    let test_val = match slot_expr(&test.raw) {
        Some(expr) => as_guard_value(eval(&expr, env, unresolved)),
        None => Value::Bool(true), // no `test=` -> `is` alone gates
    };
    k3_and(is_val, test_val)
}

fn render_guard_text(is: Option<&IsPattern>, test_raw: &str) -> Option<String> {
    let test_trimmed = test_raw.trim();
    match (is, test_trimmed.is_empty()) {
        (Some(p), true) => Some(format!("is=\"{}\"", p.raw)),
        (Some(p), false) => Some(format!("is=\"{}\" && {test_trimmed}", p.raw)),
        (None, true) => None,
        (None, false) => Some(test_trimmed.to_string()),
    }
}

/// A `<choice persist="run" into="run.<path>" [value=...]>`-synthesized
/// `::set` (D8's `synth_persist`, normalize.rs) carries NO sentinel — it is
/// structurally identical to a hand-authored `::set`. It IS identifiable:
/// `synth_persist` stamps the appended `Set`'s span from the `into` attr's
/// span (falling back to the choice's own span only when `into` is
/// missing — gate-proven unreachable). Recomputing that exact match is the
/// only reliable "is this the sugar write" test.
fn is_persist_sugar_set(choice: &Choice, set: &Set) -> bool {
    let persists = choice
        .attrs
        .iter()
        .any(|a| a.key == "persist" && matches!(&a.value, AttrValue::Str(s) if s == "run"));
    if !persists {
        return false;
    }
    choice.attrs.iter().any(|a| {
        a.key == "into" && matches!(&a.value, AttrValue::Str(p) if p == &set.path) && a.span == set.span
    })
}

fn has_bool_attr(attrs: &[lute_syntax::ast::Attr], key: &str) -> bool {
    attrs.iter().any(|a| a.key == key && matches!(a.value, AttrValue::BoolTrue))
}

fn is_exit_choice(c: &Choice) -> bool {
    has_bool_attr(&c.attrs, "exit")
}

fn is_once_choice(c: &Choice) -> bool {
    has_bool_attr(&c.attrs, "once")
}

fn choice_diag(span: Span, id: &str, choice_id: &str, reason: &str) -> Diagnostic {
    Diagnostic {
        code: mock::E_TRACE_CHOICE.to_string(),
        severity: Severity::Error,
        message: format!(
            "`--choose {id}={choice_id}` is ineligible at its presentation point: {reason} (dsl 0.4.0 §4.4)"
        ),
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
    }
}

fn fact_term_text(t: &FactTerm) -> String {
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
    }
}

fn fmt_fact(rel: &str, args: &[String]) -> String {
    format!("{rel}({})", args.join(", "))
}

fn resolve_interp(interp: &Interp, w: &Walk<'_>) -> Option<String> {
    match interp.kind {
        // No mock surface (e.g. `userName`) — always kept verbatim.
        InterpKind::Reserved => None,
        InterpKind::Path => {
            let mut atoms = Vec::new();
            let v = eval_path_read(&interp.raw, &w.env(), &mut atoms);
            report::value_text(&v)
        }
        InterpKind::Ref => {
            let expanded = expand_cel(&interp.raw, w.defs, None, &mut Vec::new()).ok()?;
            let expr = slot_expr(&expanded)?;
            let mut atoms = Vec::new();
            let v = eval(&expr, &w.env(), &mut atoms);
            report::value_text(&v)
        }
    }
}

/// Line: emit; interpolations substituted where decided, kept verbatim
/// `{{…}}` where unknown (§4.4/§4.5).
fn render_line_text(l: &Line, w: &Walk<'_>) -> String {
    if l.interps.is_empty() {
        return l.text.clone();
    }
    let base = l.text_span.byte_start;
    let mut out = String::with_capacity(l.text.len());
    let mut cursor = base;
    for interp in &l.interps {
        if interp.span.byte_start < cursor || interp.span.byte_end > base + l.text.len() {
            continue; // defensive: a malformed span never panics the walk
        }
        out.push_str(&l.text[(cursor - base)..(interp.span.byte_start - base)]);
        match resolve_interp(interp, w) {
            Some(text) => out.push_str(&text),
            None => out.push_str(&l.text[(interp.span.byte_start - base)..(interp.span.byte_end - base)]),
        }
        cursor = interp.span.byte_end;
    }
    out.push_str(&l.text[(cursor - base)..]);
    out
}

fn walk_line(l: &Line, w: &mut Walk<'_>) {
    let text = render_line_text(l, w);
    w.steps.push(Step::Line { speaker: l.speaker.clone(), text });
}

fn walk_directive(d: &Directive, w: &mut Walk<'_>) {
    let boundary = if d.tag == lute_compile::normalize::COMPONENT_BEGIN {
        Some(ComponentBoundary::Begin)
    } else if d.tag == lute_compile::normalize::COMPONENT_END {
        Some(ComponentBoundary::End)
    } else {
        None
    };
    w.steps.push(Step::Directive { tag: d.tag.clone(), component_boundary: boundary });
}

fn combine_numeric(op: &str, a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => Value::Num(match op {
            "+=" => x + y,
            "-=" => x - y,
            "*=" => x * y,
            _ => y,
        }),
        _ => Value::Unknown,
    }
}

/// Set: eval RHS -> [`EffectiveState::write`] (sequential in-flow
/// visibility, `0.3 §8.1`). Compound ops (`+=`/`-=`/`*=`) combine against
/// the CURRENT effective value; an `Unknown` operand on either side
/// propagates (never guesses a numeric result).
fn apply_set_op(op: &str, path: &str, state: &EffectiveState<'_>, rhs: Value) -> Value {
    match op {
        "+=" | "-=" | "*=" => {
            let current = match state.read(path) {
                Read::Value(v) => v,
                Read::Unset => Value::Unknown,
            };
            combine_numeric(op, current, rhs)
        }
        _ => rhs, // "=" (the common case) or a malformed op (checker-gated unreachable)
    }
}

fn walk_set(s: &Set, w: &mut Walk<'_>, sugar_ctx: Option<&Choice>) {
    let mut atoms = Vec::new();
    let rhs = match slot_expr(&s.expr.raw) {
        Some(expr) => eval(&expr, &w.env(), &mut atoms),
        None => Value::Unknown,
    };
    let new_val = apply_set_op(&s.op, &s.path, &w.state, rhs);
    w.state.write(&s.path, new_val.clone());
    let sugar = sugar_ctx.is_some_and(|c| is_persist_sugar_set(c, s));
    let value = report::value_text(&new_val).unwrap_or_else(|| "unknown".to_string());
    w.steps.push(Step::Set { path: s.path.clone(), value, sugar });
}

fn walk_assert(a: &Assert, w: &mut Walk<'_>) {
    let args: Vec<String> = a.pattern.args.iter().map(|arg| fact_term_text(&arg.term)).collect();
    w.facts.assert(&a.pattern.relation, &args);
    w.steps.push(Step::Assert { text: fmt_fact(&a.pattern.relation, &args) });
}

fn walk_retract(r: &Retract, w: &mut Walk<'_>) {
    let pat: Vec<Pat> = r
        .pattern
        .args
        .iter()
        .map(|arg| match &arg.term {
            FactTerm::Wildcard => Pat::Wildcard,
            FactTerm::Ident(s) => Pat::Ground(s.clone()),
            FactTerm::Bool(b) => Pat::Ground(b.to_string()),
        })
        .collect();
    w.facts.retract(&r.pattern.relation, &pat);
    let args: Vec<String> = r.pattern.args.iter().map(|arg| fact_term_text(&arg.term)).collect();
    w.steps.push(Step::Retract { text: fmt_fact(&r.pattern.relation, &args) });
}

/// Match: arms top-to-bottom (§4.4); a `true` arm fires, `false` skips, an
/// `unknown` arm HALTS the trace at THIS match. A fourth outcome (§4.4):
/// every arm decides `false` (none `unknown`) and there is no
/// `<otherwise>` — every arm was DEFINITELY ruled out, so there is nothing
/// to halt on. Trace never guesses here either: nothing fires, coverage
/// records `0/total`, the match is annotated `"no arm"` in the transcript,
/// and the walk continues (this is knowledge, not absence of it).
fn walk_match(m: &Match, w: &mut Walk<'_>) -> Flow {
    let subject_raw = m.subject.raw.clone();
    let total_arms = m.arms.len();
    for (idx, arm) in m.arms.iter().enumerate() {
        match arm {
            Arm::Otherwise { body, .. } => {
                w.record_match_decision(&subject_raw, m.span, "otherwise".to_string(), None, total_arms);
                return walk_nodes(body, w, None);
            }
            Arm::When { is, test, body, .. } => {
                let mut atoms = Vec::new();
                let v = eval_arm_guard(is.as_ref(), test, &subject_raw, &w.env(), &mut atoms);
                match v {
                    Value::Bool(true) => {
                        let guard = render_guard_text(is.as_ref(), &test.raw);
                        w.record_match_decision(&subject_raw, m.span, format!("arm {}", idx + 1), guard, total_arms);
                        return walk_nodes(body, w, None);
                    }
                    Value::Bool(false) => continue,
                    // A `Condition`-kind slot is gate-proven boolean; a
                    // decided `Num`/`Str` here is unreachable — degrade to
                    // the SAME halt an `Unknown` guard takes (never guess).
                    Value::Unknown | Value::Num(_) | Value::Str(_) => {
                        let expr = render_guard_text(is.as_ref(), &test.raw).unwrap_or_default();
                        w.record_unresolved("match", &subject_raw, m.span, expr, atoms);
                        w.coverage_arms
                            .entry(subject_raw.clone())
                            .or_insert(CoverageCount { visited: 0, total: total_arms });
                        return Flow::Incomplete;
                    }
                }
            }
        }
    }
    w.push_decision("match", &subject_raw, m.span, "no arm".to_string(), None, false, false, Vec::new());
    w.coverage_arms
        .entry(subject_raw)
        .or_insert(CoverageCount { visited: 0, total: total_arms });
    Flow::Continue
}

/// Branch: eligibility = each `choice.when` evaluated AT THE PRESENTATION
/// POINT (§4.4) against the CURRENT effective state — the SAME
/// `EffectiveState` every earlier in-flow `::set`/`::assert` in this shot
/// already wrote through, so a choice enabled only by an earlier write is
/// correctly eligible. `E-BRANCH-ALL-GUARDED` guarantees >=1 unguarded
/// (trivially `Bool(true)`) choice in a check-clean document, so the
/// "no --choose, nothing true" arm below is gate-proven unreachable —
/// degraded total (D20) rather than assumed away.
fn walk_branch(b: &Branch, w: &mut Walk<'_>) -> Flow {
    let total = b.choices.len();
    let checked: Vec<(Value, Vec<UnresolvedAtom>)> = b
        .choices
        .iter()
        .map(|c| {
            let mut atoms = Vec::new();
            let v = eval_choice_guard(c.when.as_ref(), &w.env(), &mut atoms);
            (v, atoms)
        })
        .collect();
    let eligible: Vec<String> = checked
        .iter()
        .enumerate()
        .filter(|(_, (v, _))| *v == Value::Bool(true))
        .map(|(i, _)| b.choices[i].id.clone())
        .collect();

    let forced_id = w.mocks.choose.get(&b.id).and_then(|v| v.first().cloned());
    let (winner, forced, auto) = if let Some(cid) = &forced_id {
        let Some(idx) = b.choices.iter().position(|c| &c.id == cid) else {
            return Flow::Refused(vec![choice_diag(b.span, &b.id, cid, "names no choice in this branch")]);
        };
        match checked[idx].0.clone() {
            Value::Bool(false) => {
                return Flow::Refused(vec![choice_diag(
                    b.choices[idx].span,
                    &b.id,
                    cid,
                    "its guard decided false at this presentation point",
                )]);
            }
            Value::Bool(true) => (idx, false, false),
            // Gate-proven boolean; a decided `Num`/`Str` degrades to the
            // same "forced past an unresolved guard" treatment as `Unknown`.
            Value::Unknown | Value::Num(_) | Value::Str(_) => (idx, true, false),
        }
    } else if let Some(idx) = checked.iter().position(|(v, _)| *v == Value::Bool(true)) {
        (idx, false, true)
    } else {
        let atoms: Vec<UnresolvedAtom> = checked
            .iter()
            .filter(|(v, _)| *v == Value::Unknown)
            .flat_map(|(_, a)| a.clone())
            .collect();
        w.record_unresolved("branch", &b.id, b.span, "eligibility".to_string(), atoms);
        w.coverage_choices
            .entry(b.id.clone())
            .or_insert(CoverageCount { visited: 0, total });
        return Flow::Incomplete;
    };

    let choice = &b.choices[winner];
    let guard = render_choice_guard(choice.when.as_ref());
    w.record_choice_decision("branch", &b.id, choice.span, choice.id.clone(), guard, forced, auto, eligible, total);
    w.state.write(&format!("scene.choices.{}", b.id), Value::Str(choice.id.clone()));
    walk_nodes(&choice.body, w, Some(choice))
}

/// `once` + `scene.visited.<hub>.<choice>` (already `false`-defaulted by
/// the checker's implicit fold, `HubRecord`) is DEFINITE — read directly,
/// never through the atom-recording value path (mirrors `is="unset"`'s own
/// treatment).
fn eval_hub_choice_eligibility(choice: &Choice, hub_id: &str, w: &Walk<'_>, unresolved: &mut Vec<UnresolvedAtom>) -> Value {
    if is_once_choice(choice) {
        let visited_path = format!("scene.visited.{hub_id}.{}", choice.id);
        if matches!(w.state.read(&visited_path), Read::Value(Value::Bool(true))) {
            return Value::Bool(false);
        }
    }
    eval_choice_guard(choice.when.as_ref(), &w.env(), unresolved)
}

fn record_hub_pick(w: &mut Walk<'_>, id: &str, choice: &Choice, forced: bool, auto: bool, total: usize) {
    let guard = render_choice_guard(choice.when.as_ref());
    w.record_choice_decision("hub", id, choice.span, choice.id.clone(), guard, forced, auto, Vec::new(), total);
    w.state.write(&format!("scene.visited.{id}.{}", choice.id), Value::Bool(true));
    w.state.write(&format!("scene.choices.{id}"), Value::Str(choice.id.clone()));
}

/// Hub: with a `--choose` list, selections are taken in order, eligibility
/// re-evaluated (`when`/`once`/`exit`) BEFORE each — ineligible ->
/// `E-TRACE-CHOICE` (Refused). Without one, a single doc-order pass visits
/// each eligible non-`exit` choice once, re-evaluating after each arm (so a
/// later arm's `when` sees an earlier arm's write), then takes the first
/// eligible `exit`. `scene.choices.<id>`/`scene.visited.<id>.*` are applied
/// to the effective state as the engine would (§4.4's own text).
fn walk_hub(h: &Hub, w: &mut Walk<'_>) -> Flow {
    let id = mock::hub_id(h).unwrap_or_default();
    let total = h.choices.len();

    if let Some(seq) = w.mocks.choose.get(&id).cloned() {
        for cid in &seq {
            let Some(choice) = h.choices.iter().find(|c| &c.id == cid) else {
                return Flow::Refused(vec![choice_diag(h.span, &id, cid, "names no choice in this hub")]);
            };
            let mut atoms = Vec::new();
            let elig = eval_hub_choice_eligibility(choice, &id, w, &mut atoms);
            match elig {
                Value::Bool(false) => {
                    return Flow::Refused(vec![choice_diag(
                        choice.span,
                        &id,
                        cid,
                        "its guard decided false, or it is `once` and already visited, at this presentation point",
                    )]);
                }
                Value::Bool(true) | Value::Unknown | Value::Num(_) | Value::Str(_) => {
                    let forced = elig == Value::Unknown;
                    record_hub_pick(w, &id, choice, forced, false, total);
                    let flow = walk_nodes(&choice.body, w, Some(choice));
                    if !matches!(flow, Flow::Continue) {
                        return flow;
                    }
                    if is_exit_choice(choice) {
                        break;
                    }
                }
            }
        }
        return Flow::Continue;
    }

    // Auto (no `--choose` entry for this hub): one doc-order pass over the
    // non-`exit` choices, re-evaluating eligibility fresh at each — a
    // choice's own guard is read AFTER every earlier choice's write in this
    // pass, honoring §4.4's "an arm enabled by a prior arm's write".
    for choice in h.choices.iter().filter(|c| !is_exit_choice(c)) {
        let mut atoms = Vec::new();
        let elig = eval_hub_choice_eligibility(choice, &id, w, &mut atoms);
        if elig == Value::Bool(true) {
            record_hub_pick(w, &id, choice, false, true, total);
            let flow = walk_nodes(&choice.body, w, Some(choice));
            if !matches!(flow, Flow::Continue) {
                return flow;
            }
        }
    }
    // First eligible `exit` in document order (D20: none true + some
    // unknown -> Incomplete, never guessed).
    let mut chosen: Option<&Choice> = None;
    let mut unknown_atoms: Vec<UnresolvedAtom> = Vec::new();
    for choice in h.choices.iter().filter(|c| is_exit_choice(c)) {
        let mut atoms = Vec::new();
        let elig = eval_hub_choice_eligibility(choice, &id, w, &mut atoms);
        match elig {
            Value::Bool(true) => {
                chosen = Some(choice);
                break;
            }
            Value::Unknown | Value::Num(_) | Value::Str(_) => unknown_atoms.extend(atoms),
            Value::Bool(false) => {}
        }
    }
    match chosen {
        Some(choice) => {
            record_hub_pick(w, &id, choice, false, true, total);
            walk_nodes(&choice.body, w, Some(choice))
        }
        None => {
            w.record_unresolved("hub", &id, h.span, "exit eligibility".to_string(), unknown_atoms);
            w.coverage_choices
                .entry(id.clone())
                .or_insert(CoverageCount { visited: 0, total });
            Flow::Incomplete
        }
    }
}

/// Timeline: clips reported in resolved `(at, track)` order — REUSES
/// `lute_compile::schedule::schedule_timeline` (the exact math
/// `stage.rs:117-126` runs at compile time) so trace never re-derives
/// cursor/barrier semantics; no clock is simulated, each resolved clip's
/// underlying `Set`/`Directive` walks exactly like a top-level one.
fn walk_timeline(t: &Timeline, w: &mut Walk<'_>) -> Flow {
    let ctx = Ctx { env: w.check_env, in_match: false, match_subject: None };
    let (clips, _barrier_at) = lute_compile::schedule::schedule_timeline(t, &ctx, w.snapshot);
    for sc in &clips {
        let node = match sc.node {
            ClipNode::Directive(d) => Node::Directive(d.clone()),
            ClipNode::Set(s) => Node::Set(s.clone()),
        };
        let flow = walk_node(&node, w, None);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
    }
    Flow::Continue
}

fn walk_node(node: &Node, w: &mut Walk<'_>, sugar_ctx: Option<&Choice>) -> Flow {
    match node {
        Node::Line(l) => {
            walk_line(l, w);
            Flow::Continue
        }
        Node::Directive(d) => {
            walk_directive(d, w);
            Flow::Continue
        }
        Node::Set(s) => {
            walk_set(s, w, sugar_ctx);
            Flow::Continue
        }
        Node::Assert(a) => {
            walk_assert(a, w);
            Flow::Continue
        }
        Node::Retract(r) => {
            walk_retract(r, w);
            Flow::Continue
        }
        Node::Match(m) => walk_match(m, w),
        Node::Branch(b) => walk_branch(b, w),
        Node::Hub(h) => walk_hub(h, w),
        Node::Timeline(t) => walk_timeline(t, w),
        // `<on>`/`<objective>` are grammar-admitted only inside `<quest>`
        // bodies (dsl 0.2.0 §6.7); Task 19 walks `doc.shots` only (Task 20
        // owns quests), so these never actually occur here — kept total.
        Node::On(_) | Node::Objective(_) => Flow::Continue,
    }
}

fn walk_nodes(nodes: &[Node], w: &mut Walk<'_>, sugar_ctx: Option<&Choice>) -> Flow {
    for node in nodes {
        let flow = walk_node(node, w, sugar_ctx);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
    }
    Flow::Continue
}

fn walk_document(doc: &Document, w: &mut Walk<'_>) -> Flow {
    let mut prev_shot = 0i64;
    for (i, shot) in doc.shots.iter().enumerate() {
        let authored = shot.number.unwrap_or(i as i64 + 1);
        let shot_no = authored.max(prev_shot + 1);
        prev_shot = shot_no;
        w.steps.push(Step::Shot { number: shot_no });
        let flow = walk_nodes(&shot.body, w, None);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
    }
    Flow::Continue
}

// ---------------------------------------------------------------------
// Quest walk (Task 20, dsl 0.4.0 §4.4): events, monotonic objectives,
// fail precedence. `quest.<id>.state`/`quest.<id>.objectives.<oid>.done`
// are RESERVED paths trace derives from its OWN walk here — never the
// schema `default:` ([`crate::eval::is_reserved_quest_path`],
// `EffectiveState::read`) — so a quest's `state` and every objective's
// `done` genuinely start `Unset` until this section decides and writes
// them.
// ---------------------------------------------------------------------

/// A quest's lifecycle position, tracked locally through [`walk_quest`]
/// (mirrored into `quest.<id>.state` via [`EffectiveState::write`] on every
/// transition — the reserved-path write IS the report-visible record).
/// `start` deciding `false`/`unknown` never produces a live [`QuestState`]
/// at all (`walk_quest` returns before this type is ever constructed) — a
/// quest that never activates has no lifecycle to track.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QuestState {
    Active,
    Complete,
    Failed,
}

fn quest_state_path(quest_id: &str) -> String {
    format!("quest.{quest_id}.state")
}

fn objective_done_path(quest_id: &str, objective_id: &str) -> String {
    format!("quest.{quest_id}.objectives.{objective_id}.done")
}

/// An `<objective done="…">` slot's rendered guard text — mirrors
/// [`render_choice_guard`] for a non-optional [`CelSlot`] (`done` is never
/// `Option`; a missing `done=` still yields a syntactically valid, possibly
/// empty, slot — `parse_objective`).
fn render_done_guard(done: &CelSlot) -> Option<String> {
    let t = done.raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn is_objective_done(w: &Walk<'_>, quest_id: &str, objective_id: &str) -> bool {
    matches!(
        w.state.read(&objective_done_path(quest_id, objective_id)),
        Read::Value(Value::Bool(true))
    )
}

/// Re-evaluate every `<objective>` in `quest.body` (document order) whose
/// `done` isn't ALREADY recorded true — monotonic (§4.4: "once `true`,
/// recorded"): a done objective is never re-evaluated, so nothing it
/// decided can un-decide on a later pass. A fresh `true` is written to the
/// reserved `objectives.<oid>.done` path (visible to any later read/
/// interp, D19-style); `false`/`unknown` write nothing (the path stays
/// `Unset` until a pass decides `true` — §4.3's own "starts Unset"
/// exception). `unknown` records an [`UnresolvedEntry`] (never halts —
/// unlike `<match>`, an objective is a lifecycle FACT the report tables,
/// not a control-flow gate the walk must stop on).
fn reevaluate_objectives(quest: &Quest, w: &mut Walk<'_>) {
    for node in &quest.body {
        let Node::Objective(o) = node else { continue };
        if o.id.is_empty() || is_objective_done(w, &quest.id, &o.id) {
            continue;
        }
        let mut atoms = Vec::new();
        let v = match slot_expr(&o.done.raw) {
            Some(expr) => as_guard_value(eval(&expr, &w.env(), &mut atoms)),
            None => Value::Unknown, // gate-proven unreachable post-check (E-OBJECTIVE-MISSING-DONE)
        };
        let guard = render_done_guard(&o.done);
        match v {
            Value::Bool(true) => {
                w.state.write(&objective_done_path(&quest.id, &o.id), Value::Bool(true));
                w.push_decision("objective", &o.id, o.span, "done".to_string(), guard, false, false, Vec::new());
            }
            Value::Bool(false) => {
                w.push_decision("objective", &o.id, o.span, "pending".to_string(), guard, false, false, Vec::new());
            }
            Value::Unknown | Value::Num(_) | Value::Str(_) => {
                w.record_unresolved("objective", &o.id, o.span, guard.unwrap_or_default(), atoms);
            }
        }
    }
}

fn quest_required_objectives(quest: &Quest) -> impl Iterator<Item = &Objective> {
    quest.body.iter().filter_map(|n| match n {
        Node::Objective(o) if !o.optional && !o.id.is_empty() => Some(o),
        _ => None,
    })
}

/// "All required objectives done" (§4.4 completion) — vacuously `false`
/// for a quest with zero required objectives: a quest can never complete
/// by having nothing to complete (only `fail`, or an unending `active`,
/// applies to such a quest).
fn quest_complete(quest: &Quest, w: &Walk<'_>) -> bool {
    let mut any = false;
    for o in quest_required_objectives(quest) {
        any = true;
        if !is_objective_done(w, &quest.id, &o.id) {
            return false;
        }
    }
    any
}

/// One event's `<on>` dispatch (§4.4): ALL matching `<on event>` guards in
/// `quest.body` (document order) evaluate against the SAME PRE-EVENT
/// SNAPSHOT — `state`/`facts` cloned ONCE before any of this event's arms
/// run (`0.2 §4.2`) — so an earlier-firing handler's write is invisible to
/// a LATER handler's guard for this SAME event (only a SUBSEQUENT event's
/// snapshot would see it). A firing handler's BODY then walks against the
/// LIVE state/facts via the ordinary [`walk_nodes`] — sequential in-flow
/// visibility resumes inside the body, only the guard is snapshotted.
/// `unknown` records unresolved and does not fire (trace never guesses);
/// only a NESTED `<match>`/`<branch>`/`<hub>` inside a firing handler's
/// body can propagate a non-`Continue` [`Flow`] out of this function.
fn dispatch_event(quest: &Quest, event_name: &str, w: &mut Walk<'_>) -> Flow {
    let snap_state = w.state.clone();
    let snap_facts = w.facts.clone();
    let snap_env = EvalEnv { state: &snap_state, facts: &snap_facts };
    for node in &quest.body {
        let Node::On(on) = node else { continue };
        if on.event != event_name {
            continue;
        }
        let mut atoms = Vec::new();
        let guard_v = eval_choice_guard(on.when.as_ref(), &snap_env, &mut atoms);
        let guard_text = render_choice_guard(on.when.as_ref());
        match guard_v {
            Value::Bool(true) => {
                w.push_decision("on", event_name, on.span, "fires".to_string(), guard_text, false, false, Vec::new());
                let flow = walk_nodes(&on.body, w, None);
                if !matches!(flow, Flow::Continue) {
                    return flow;
                }
            }
            Value::Bool(false) => {
                w.push_decision("on", event_name, on.span, "skipped".to_string(), guard_text, false, false, Vec::new());
            }
            Value::Unknown | Value::Num(_) | Value::Str(_) => {
                w.record_unresolved("on", event_name, on.span, guard_text.unwrap_or_default(), atoms);
            }
        }
    }
    Flow::Continue
}

/// After activation and after every event (§4.4): re-evaluate objectives
/// (monotonic), evaluate `fail` BEFORE derived completion (`0.2 §6.3`
/// precedence — a quest whose objectives would ALL be done still fails if
/// `fail` decides true THIS pass), then check completion. Fires exactly
/// one of `questFailed`/`questComplete` on a fresh transition; a quest
/// already `Complete`/`Failed` never reaches this function again
/// (`walk_quest`'s own `Active`-only loop guard).
fn settle_quest(quest: &Quest, state: &mut QuestState, w: &mut Walk<'_>) -> Flow {
    reevaluate_objectives(quest, w);

    let mut fail_atoms = Vec::new();
    let fail_v = match quest.fail.as_ref().and_then(|f| slot_expr(&f.raw)) {
        Some(expr) => as_guard_value(eval(&expr, &w.env(), &mut fail_atoms)),
        None => Value::Bool(false),
    };
    if matches!(fail_v, Value::Bool(true)) {
        *state = QuestState::Failed;
        w.state.write(&quest_state_path(&quest.id), Value::Str("failed".to_string()));
        let guard = render_choice_guard(quest.fail.as_ref());
        w.push_decision("quest", &quest.id, quest.span, "failed".to_string(), guard, false, false, Vec::new());
        return dispatch_event(quest, "questFailed", w);
    }

    if quest_complete(quest, w) {
        *state = QuestState::Complete;
        w.state.write(&quest_state_path(&quest.id), Value::Str("complete".to_string()));
        w.push_decision("quest", &quest.id, quest.span, "complete".to_string(), None, false, false, Vec::new());
        return dispatch_event(quest, "questComplete", w);
    }

    Flow::Continue
}

/// One `<quest>`'s full lifecycle (§4.4). **Two distinct paths reach the
/// same `unset -> active` transition:** a `start`-having quest activates
/// **declaratively** — evaluate `start`; `true` transitions it, deciding
/// `false` never activates (reported via a `"never"` [`Decision`]),
/// `unknown` leaves the quest (and every objective — nothing below this
/// point ever runs) unresolved. A `start`-less quest is **accept-driven**:
/// it stays inactive — reported `"awaiting accept"`, the walk CONTINUES
/// (never unresolved/exit-3) — until `quest.id` appears in
/// [`MockSet::accepts`] (`--accept`, pre-walk validated E-TRACE-ACCEPT-clean
/// by [`crate::mock::validate`]: an unknown id or a `start`-having quest
/// never reaches here). Either path activates AT MOST once, so
/// `questActive` fires from this ONE call site — never twice (the
/// historical double-fire: `--event questActive` used to re-dispatch it a
/// second time through the loop below; lifecycle names are now rejected
/// pre-walk, E-TRACE-EVENT, so `events` can never carry one). Once active,
/// `questActive` fires as the quest's OWN first event, settled once, then
/// every `--event` in CLI order — each re-dispatches and re-settles; a
/// transition to `Complete`/`Failed` is terminal, stopping further event
/// processing for THIS quest (the `Active`-only loop guard below).
fn walk_quest(quest: &Quest, events: &[String], w: &mut Walk<'_>) -> Flow {
    match quest.start.as_ref() {
        Some(start_slot) => {
            // Declarative path: `start` decides the transition.
            let mut atoms = Vec::new();
            let start_v = eval_choice_guard(Some(start_slot), &w.env(), &mut atoms);
            let start_guard = render_choice_guard(Some(start_slot));
            match start_v {
                Value::Bool(false) => {
                    w.push_decision("quest", &quest.id, quest.span, "never".to_string(), start_guard, false, false, Vec::new());
                    return Flow::Continue;
                }
                Value::Unknown | Value::Num(_) | Value::Str(_) => {
                    w.record_unresolved("quest", &quest.id, quest.span, start_guard.unwrap_or_default(), atoms);
                    return Flow::Continue;
                }
                Value::Bool(true) => {}
            }
            w.state.write(&quest_state_path(&quest.id), Value::Str("active".to_string()));
            w.push_decision("quest", &quest.id, quest.span, "active".to_string(), start_guard, false, false, Vec::new());
        }
        None => {
            // Accept-driven path: no `start` predicate — stays inactive
            // until `--accept <questId>` names this quest (§4.4). NOT an
            // unknown/halt condition: the walk simply continues past this
            // quest, exactly like `start` deciding false, except the
            // report says "awaiting accept" rather than "never" (the quest
            // MAY still activate later via a different trace invocation
            // with `--accept`, unlike a `start=false` quest).
            if !w.mocks.accepts.iter().any(|id| id == &quest.id) {
                w.push_decision("quest", &quest.id, quest.span, "awaiting accept".to_string(), None, false, false, Vec::new());
                return Flow::Continue;
            }
            w.state.write(&quest_state_path(&quest.id), Value::Str("active".to_string()));
            w.push_decision("quest", &quest.id, quest.span, "active".to_string(), None, true, false, Vec::new());
        }
    }

    let mut state = QuestState::Active;

    let flow = dispatch_event(quest, "questActive", w);
    if !matches!(flow, Flow::Continue) {
        return flow;
    }
    let flow = settle_quest(quest, &mut state, w);
    if !matches!(flow, Flow::Continue) {
        return flow;
    }

    for event in events {
        if !matches!(state, QuestState::Active) {
            break;
        }
        let flow = dispatch_event(quest, event, w);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
        let flow = settle_quest(quest, &mut state, w);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
    }

    Flow::Continue
}

/// `doc.quests` linearly, document order (mirrors [`walk_document`]'s own
/// shape) — admission (§3.3/§6.7) guarantees a check-clean document never
/// populates both `doc.shots` and `doc.quests`, so [`trace_document`]
/// calling both this and [`walk_document`] unconditionally is safe.
fn walk_quests(doc: &Document, events: &[String], w: &mut Walk<'_>) -> Flow {
    for quest in &doc.quests {
        let flow = walk_quest(quest, events, w);
        if !matches!(flow, Flow::Continue) {
            return flow;
        }
    }
    Flow::Continue
}

fn seed_state(mocks: &MockSet, schema: &StateSchema) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (path, raw, _span) in &mocks.state {
        // Already schema-validated by `mock::validate` (run before this);
        // a miss here is defensive-total, never a silent wrong value.
        if let Some(decl) = schema.decls.get(path) {
            if let Some(lit) = mock::coerce_state_literal(&decl.ty, raw) {
                out.insert(path.clone(), literal_to_value(&lit));
            }
        }
    }
    out
}

fn seed_facts(mocks: &MockSet, facts: &mut FactStore<'_>) {
    for raw in &mocks.facts {
        // Already parse/schema-validated by `mock::validate`.
        if let Ok(pat) = lute_syntax::datalog::parse_fact(raw) {
            let args: Vec<String> = pat.args.iter().map(|a| fact_term_text(&a.term)).collect();
            facts.assert(&pat.relation, &args);
        }
    }
}

fn seeds_summary(mocks: &MockSet) -> Seeds {
    Seeds {
        state_paths: mocks.state.len(),
        facts: mocks.facts.len(),
        choices: mocks.choose.values().map(Vec::len).sum(),
    }
}

fn empty_report(uri: &str, mocks: &MockSet) -> TraceReport {
    TraceReport {
        file: uri.to_string(),
        seeds: seeds_summary(mocks),
        steps: Vec::new(),
        decisions: Vec::new(),
        unresolved: Vec::new(),
        coverage: Coverage::default(),
    }
}

/// The §4.3/§4.4/§4.5 pipeline, end to end: check gate -> mock validation
/// -> D14's normalize+expand reuse -> the deterministic walk -> the §4.5
/// report + exit code. Never panics — every degrade path returns a
/// (possibly empty) report alongside the appropriate [`TraceExit`].
pub fn trace_document(input: &CheckInput, mocks: MockSet) -> (TraceReport, TraceExit) {
    // 1. `check` gate (§4.3): any Error -> Refused, run check first.
    let result = lute_check::check(input);
    if !result.ok {
        return (empty_report(&input.uri, &mocks), TraceExit::Refused(result.diagnostics));
    }

    // 2. Re-derive the parsed, CEL-filled document + folded environment
    //    (mirrors `lute_compile::compile`'s own re-derivation after ITS
    //    gate — check's own diagnostics were already reported above).
    let (mut doc, _parse_diags) = lute_syntax::parse(&input.text);
    let mut arena = CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _fd1, _fd2) = lute_check::fold_env(&doc, input);

    // 3. Mock validation (§4.3): any E-TRACE-* -> Refused.
    let mock_diags = mock::validate(&mocks, &folded, &doc);
    if !mock_diags.is_empty() {
        return (empty_report(&input.uri, &mocks), TraceExit::Refused(mock_diags));
    }

    // 4. D14: normalize (components bound, §6.4 folds, when=/persist=
    //    desugared) then expand (@/$-free) — the SAME lute-compile passes,
    //    in the SAME order, `compile` itself runs.
    let mut diags = lute_compile::normalize::normalize_document(&mut doc, &input.components, &folded.env.state);
    let table = DefTable { bodies: &folded.def_bodies, params: &folded.env.def_params };
    diags.extend(lute_compile::expand::expand_document(&mut doc, &table));
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return (empty_report(&input.uri, &mocks), TraceExit::Refused(diags));
    }

    // 5. Walk `doc.shots` (the scene walk, Task 19) then `doc.quests` (the
    //    quest walk, Task 20) — admission guarantees a check-clean document
    //    never populates both, so running both unconditionally is safe.
    let seed = seed_state(&mocks, &folded.env.state);
    let state = EffectiveState::new(&folded.env.state, seed);
    let mut facts = FactStore::new(&folded.env.rel_vocab);
    seed_facts(&mocks, &mut facts);

    let mut w = Walk {
        mocks: &mocks,
        defs: &table,
        check_env: &folded.env,
        snapshot: &input.snapshot,
        state,
        facts,
        steps: Vec::new(),
        decisions: Vec::new(),
        unresolved: Vec::new(),
        coverage_choices: BTreeMap::new(),
        coverage_arms: BTreeMap::new(),
    };

    let mut flow = walk_document(&doc, &mut w);
    if matches!(flow, Flow::Continue) {
        flow = walk_quests(&doc, &mocks.events, &mut w);
    }

    let report = TraceReport {
        file: input.uri.clone(),
        seeds: seeds_summary(&mocks),
        steps: w.steps,
        decisions: w.decisions,
        unresolved: w.unresolved,
        coverage: Coverage { choices: w.coverage_choices, arms: w.coverage_arms },
    };
    // An objective/quest-`start` `unknown` (Task 20) records an unresolved
    // atom WITHOUT returning `Flow::Incomplete` (it never halts the walk,
    // unlike an unknown `<match>` guard) — so `Incomplete` is driven by
    // EITHER signal, matching §4.5's exit-3 contract for both halted and
    // merely-unresolved-but-otherwise-complete walks.
    let exit = match flow {
        Flow::Continue if report.unresolved.is_empty() => TraceExit::Complete,
        Flow::Continue | Flow::Incomplete => TraceExit::Incomplete,
        Flow::Refused(ds) => TraceExit::Refused(ds),
    };
    (report, exit)
}
