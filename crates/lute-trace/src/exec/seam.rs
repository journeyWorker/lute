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

use lute_check::gates::E_OCCASION_GATE;

/// Why the engine would not raise an occasion now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Closed {
    /// The project's `terminal:` holds (its raw condition).
    Terminal(String),
    /// The occasion's `raisedWhen` gate is false (its raw condition).
    Gate(String),
    /// The gate or the terminal condition could not be decided: what was
    /// unknown.
    Unknown(String),
}

/// Evaluate `raw` in `w`, `member` bound as `occasion.target`.
fn holds(p: &ExecProject, w: &World, raw: &str, member: Option<&str>) -> Result<bool, String> {
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    eval.bind_occasion_target(member);
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
        Some(t) => holds(p, w, &t.raw, None),
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
    match terminal_holds(p, w) {
        Ok(true) => {
            let raw = p
                .index
                .terminal
                .as_ref()
                .map_or_else(String::new, |t| t.raw.clone());
            return Some(Closed::Terminal(raw));
        }
        Ok(false) => {}
        Err(unknown) => return Some(Closed::Unknown(unknown)),
    }
    let gate = p.index.gates.iter().find(|g| g.occasion == occasion)?;
    let member = target.map(|t| member(p, occasion, t));
    match holds(p, w, &gate.raised_when.raw, member.as_deref()) {
        Ok(true) => None,
        Ok(false) => Some(Closed::Gate(gate.raised_when.raw.clone())),
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
        Closed::Gate(g) => PlayHalt::Error(format!(
            "step {n}: {E_OCCASION_GATE}: the engine raises {raised} only when `{g}` (its \
             `raisedWhen`), which is false here — make it hold first (an `engine:` write, an \
             earlier step), or drop the step"
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
