//! dsl 0.27.0 §4 (T2-3, T2-4): what the engine honours before it raises an
//! occasion — the occasion's `raisedWhen` gate and the project's
//! `terminal:` condition, both carried in the compiled project
//! (`ProjectIndex.gates` / `.terminal`) and evaluated here against the
//! world, by the one evaluator every `when` is decided by.
//!
//! A `lute play` step that raises an occasion the engine would not raise is
//! refused (`E-OCCASION-GATE`); a raise the clock makes (`raise.slot`,
//! `dayStart`, `dayEnd`) is simply not made; once the terminal condition
//! holds the playthrough ends (`end: terminal`) and a later `advance:` or
//! `occasion:` step is a usage error.

use super::session::{ExecProject, PlayHalt, World};
use crate::exec::{Driver, GuardRead, Machine};

use lute_check::gates::E_OCCASION_GATE;

/// Why the engine would not raise an occasion now.
#[derive(Clone, Debug, PartialEq)]
pub enum Closed {
    /// The project's `terminal:` holds (its raw condition).
    Terminal(String),
    /// The occasion's `raisedWhen` gate is false: its raw condition and
    /// the reads it is false over ([`Machine::false_reads`], HW27-10).
    Gate { raw: String, reads: Vec<GuardRead> },
    /// The gate or the terminal condition could not be decided: what was
    /// unknown.
    Unknown(String),
}

impl Closed {
    /// The false reads of a closed gate, `; `-joined after " since " — ``
    /// since `run.stalker` is morgue`` — empty when there are none.
    pub fn reads_text(reads: &[GuardRead]) -> String {
        if reads.is_empty() {
            return String::new();
        }
        let found: Vec<String> = reads.iter().map(GuardRead::found).collect();
        format!(" since {}", found.join("; "))
    }
}

/// Decide `raw` in `eval`; `Err` names what was unknown.
fn decide<D: Driver>(eval: &mut Machine<D>, raw: &str) -> Result<bool, String> {
    eval.eval_guard(raw).map_err(|atoms| {
        format!(
            "`{raw}` evaluates unknown: {}",
            super::session::describe_atoms(&atoms)
        )
    })
}

/// Whether the project's `terminal:` holds in `w` (`Ok(false)` without
/// one); `Err` names what was unknown.
pub fn terminal_holds(p: &ExecProject, w: &World) -> Result<bool, String> {
    match &p.index.terminal {
        None => Ok(false),
        Some(t) => decide(
            &mut w.evaluator(&p.eval_json).with_visited(&w.visited),
            &t.raw,
        ),
    }
}

/// The member `occasion.target` reads for a raise of `occasion` for
/// `target`: a domain target's member (`room.office` → `office`), else the
/// target as raised.
fn member<'t>(p: &ExecProject, occasion: &str, target: &'t str) -> std::borrow::Cow<'t, str> {
    p.occasions
        .get(occasion)
        .and_then(|d| lute_check::gates::target_member(d, target))
        .map_or(std::borrow::Cow::Borrowed(target), std::borrow::Cow::Owned)
}

/// Why the engine would not raise `occasion` (for `target`) in `w` — the
/// terminal condition holds, or the occasion's gate is false — or `None`
/// when it would.
pub fn closed(p: &ExecProject, w: &World, occasion: &str, target: Option<&str>) -> Option<Closed> {
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    closed_in(p, &mut eval, occasion, target)
}

/// [`closed`] decided by `eval` — the world's evaluator in `lute play`, the
/// walk's own Machine over the mocks in `lute trace` / `lute test` — so
/// every tool judges the seam by one rule (HW27-04). `target` is the raise
/// target (`room.office`, or a bare member); `occasion.target` is left
/// bound to its member.
pub fn closed_in<D: Driver>(
    p: &ExecProject,
    eval: &mut Machine<D>,
    occasion: &str,
    target: Option<&str>,
) -> Option<Closed> {
    let gate = p.index.gates.iter().find(|g| g.occasion == occasion);
    if p.index.terminal.is_none() && gate.is_none() {
        return None;
    }
    let member = target.map(|t| member(p, occasion, t));
    eval.bind_occasion_target(member.as_deref());
    if let Some(t) = &p.index.terminal {
        match decide(eval, &t.raw) {
            Ok(true) => return Some(Closed::Terminal(t.raw.clone())),
            Ok(false) => {}
            Err(unknown) => return Some(Closed::Unknown(unknown)),
        }
    }
    let raw = &gate?.raised_when.raw;
    match decide(eval, raw) {
        Ok(true) => None,
        Ok(false) => Some(Closed::Gate {
            raw: raw.clone(),
            reads: eval.false_reads(raw),
        }),
        Err(unknown) => Some(Closed::Unknown(unknown)),
    }
}

/// The halt of a `lute play` step (`n`) raising `occasion` (for `target`)
/// while it is [`Closed`].
pub fn refusal(n: usize, occasion: &str, target: Option<&str>, why: &Closed) -> PlayHalt {
    let raised = match target {
        Some(t) => format!("`{occasion}` for `{t}`"),
        None => format!("`{occasion}`"),
    };
    match why {
        Closed::Terminal(t) => PlayHalt::Error(format!(
            "step {n}: {E_OCCASION_GATE}: the game is over — `terminal: {t}` holds, so the engine \
             raises no occasion ({raised} included); start a new run (`newRun: true`) to play on"
        )),
        Closed::Gate { raw, reads } => PlayHalt::Error(format!(
            "step {n}: {E_OCCASION_GATE}: the engine raises {raised} only when `{raw}` (its \
             `raisedWhen`), which is false here{} — make it hold first (an `engine:` write, an \
             earlier step), or drop the step",
            Closed::reads_text(reads)
        )),
        Closed::Unknown(u) => PlayHalt::Incomplete(format!(
            "step {n}: whether the engine may raise {raised} is undecided: {u}"
        )),
    }
}

/// The halt of an `advance:` step (`n`) once the terminal condition holds.
pub fn advance_after_terminal(n: usize, terminal: &str) -> PlayHalt {
    PlayHalt::Error(format!(
        "step {n}: {E_OCCASION_GATE}: `advance:` after the game is over — `terminal: {terminal}` \
         holds, so the engine raises no occasion and the clock does not move on; start a new run \
         (`newRun: true`) to play on"
    ))
}

/// A raise the clock did not make during an `advance:` because the seam
/// was closed: where the clock stood, the occasion, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedRaise {
    pub at: String,
    pub occasion: String,
    pub why: Closed,
}

/// Whether the clock may raise `occasion` now; when it may not, the raise
/// is recorded in `closed` (at `at()`, the clock's position) and not made.
pub fn clock_raise_open(
    p: &ExecProject,
    w: &World,
    occasion: &str,
    at: impl FnOnce() -> String,
    closed: &mut Vec<ClosedRaise>,
) -> bool {
    match self::closed(p, w, occasion, None) {
        None => true,
        Some(why) => {
            closed.push(ClosedRaise {
                at: at(),
                occasion: occasion.to_string(),
                why,
            });
            false
        }
    }
}
