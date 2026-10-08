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
use crate::exec::{Driver, GuardRead, Machine, Slot};

use lute_check::gates::E_OCCASION_GATE;

/// Why the engine would not raise an occasion now.
#[derive(Clone, Debug, PartialEq)]
pub enum Closed {
    /// The project's `terminal:` holds (its raw condition).
    Terminal(String),
    /// The occasion's `raisedWhen` gate is false: its condition as the
    /// author wrote it (`@stageReleased`, not the expansion) and the reads
    /// it is false over — each false conjunct's, so a negated fact that
    /// holds is named too ([`Machine::false_conjuncts`]).
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

/// Decide `cond` in `eval`; `Err` names what was unknown.
fn decide<D: Driver>(eval: &mut Machine<D>, cond: &Slot) -> Result<bool, String> {
    eval.eval_guard(cond).map_err(|atoms| {
        format!(
            "`{}` evaluates unknown: {}",
            cond.raw(),
            super::session::describe_atoms(&atoms)
        )
    })
}

/// Whether the project's `terminal:` holds in `w` (`Ok(false)` without
/// one); `Err` names what was unknown.
pub fn terminal_holds(p: &ExecProject, w: &World) -> Result<bool, String> {
    match &p.conds.terminal {
        None => Ok(false),
        Some(t) => decide(
            &mut w
                .evaluator_with_schema(&p.eval_json, p.store_schemas[w.derive.unwrap_or(true) as usize].clone())
                .with_visited(&w.visited),
            t,
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
    let mut eval = w
        .evaluator_with_schema(&p.eval_json, p.store_schemas[w.derive.unwrap_or(true) as usize].clone())
        .with_visited(&w.visited);
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
    // dsl 0.28.0 (T2-9): an `outsideRun` occasion (a title screen, a
    // gallery) is raised after the game is over too.
    let terminal = p
        .index
        .terminal
        .as_ref()
        .zip(p.conds.terminal.as_ref())
        .filter(|_| !p.index.outside_run.iter().any(|o| o == occasion));
    if terminal.is_none() && gate.is_none() {
        return None;
    }
    let member = target.map(|t| member(p, occasion, t));
    eval.bind_occasion_target(member.as_deref());
    if let Some((t, cond)) = terminal {
        match decide(eval, cond) {
            Ok(true) => return Some(Closed::Terminal(t.shown().to_string())),
            Ok(false) => {}
            Err(unknown) => return Some(Closed::Unknown(unknown)),
        }
    }
    let gate = gate?;
    let cond = p.conds.gates.get(&gate.occasion)?;
    match decide(eval, cond) {
        Ok(true) => None,
        Ok(false) => {
            let mut reads: Vec<GuardRead> = Vec::new();
            for r in eval
                .false_conjuncts(cond)
                .into_iter()
                .flat_map(|(_, r)| r)
            {
                if !reads.contains(&r) {
                    reads.push(r);
                }
            }
            Some(Closed::Gate {
                raw: gate.raised_when.shown().to_string(),
                reads,
            })
        }
        Err(unknown) => Some(Closed::Unknown(unknown)),
    }
}

/// The halt of a `lute play` step (`n`) raising `occasion` (for `target`)
/// while it is [`Closed`]. The project's clock tells a gate over it to
/// advance the clock rather than write its paths; its `terminalPersists`
/// says whether a new run reopens a game that is over.
pub fn refusal(
    n: usize,
    occasion: &str,
    target: Option<&str>,
    why: &Closed,
    p: &ExecProject,
) -> PlayHalt {
    let clock = p.index.clock.as_ref();
    let raised = match target {
        Some(t) => format!("`{occasion}` for `{t}`"),
        None => format!("`{occasion}`"),
    };
    match why {
        Closed::Terminal(t) => PlayHalt::Error(format!(
            "step {n}: {E_OCCASION_GATE}: the game is over — `terminal: {t}` holds, so the engine \
             raises no occasion ({raised} included); {}, or, if the engine raises `{occasion}` \
             outside a run too (a title screen, a gallery), declare it `outsideRun: true`",
            play_on(t, p.index.terminal_persists)
        )),
        Closed::Gate { raw, reads } => {
            // A payload read is changed by this step's own `payload:`,
            // anything else before the raise.
            let payload = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
            let by_payload =
                |r: &GuardRead| matches!(r, GuardRead::Path(p, _) if p.starts_with(&payload));
            let by_clock = |r: &GuardRead| {
                matches!(r, GuardRead::Path(p, _) if lute_manifest::clock::is_clock_path(p)
                    || clock.is_some_and(|c| c.day == *p || c.slot.as_deref() == Some(p)))
            };
            let fix = if !reads.is_empty() && reads.iter().all(by_payload) {
                "raise it with a payload that satisfies it (`payload:` on this step)"
            } else if reads.iter().any(by_clock) {
                // An `engine:` write to the clock's paths jumps it without
                // raising what the clock raises on the way.
                "make it hold first (an `advance:` step to that moment, or an earlier step; an \
                 `engine:` write to the clock's paths moves it without raising anything on the \
                 way)"
            } else if reads.iter().any(by_payload) {
                "make it hold first (an `engine:` write, an earlier step, or `payload:` on this \
                 step)"
            } else {
                "make it hold first (an `engine:` write, an earlier step)"
            };
            PlayHalt::Error(format!(
                "step {n}: {E_OCCASION_GATE}: the engine raises {raised} only when `{raw}` (its \
                 `raisedWhen`), which is false here{} — {fix}, or drop the step",
                Closed::reads_text(reads)
            ))
        }
        Closed::Unknown(u) => PlayHalt::Incomplete(format!(
            "step {n}: whether the engine may raise {raised} is undecided: {u}"
        )),
    }
}

/// The halt of an `advance:` step (`n`) once the terminal condition holds.
pub fn advance_after_terminal(n: usize, p: &ExecProject) -> PlayHalt {
    let terminal = p.index.terminal.as_ref().map_or("", |t| t.raw.as_str());
    PlayHalt::Error(format!(
        "step {n}: {E_OCCASION_GATE}: `advance:` after the game is over — `terminal: {terminal}` \
         holds, so the engine raises no occasion and the clock does not move on; {}",
        play_on(terminal, p.index.terminal_persists)
    ))
}

/// dsl 0.28.0 (T3-19): what a script does once `terminal` holds — a new
/// run, unless the ending persists (`persists: true`: the game is over for
/// good) or the condition reads state a new run keeps ([`persistent_read`]),
/// when a new run does not help.
fn play_on(terminal: &str, persists: bool) -> String {
    if persists {
        return "the ending persists (`persists: true`), so the game is over for good and no new \
                run reopens it — drop the step"
            .to_string();
    }
    match persistent_read(terminal) {
        Some(read) => {
            format!("it still holds after a new run: it reads `{read}`, which a new run keeps")
        }
        None => "start a new run (`newRun: true`) to play on".to_string(),
    }
}

/// The first path `terminal` reads that a new run keeps
/// ([`lute_check::gates::persistent_reads`]) — once it holds, a new run
/// does not end the game over.
pub fn persistent_read(terminal: &str) -> Option<String> {
    let unknown = |_: &str| None;
    lute_check::gates::persistent_reads(terminal, &unknown, &unknown)
        .into_iter()
        .next()
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
