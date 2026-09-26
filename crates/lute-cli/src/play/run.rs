//! The execution loop: the start settle, then every planned step (and
//! repetition) through the [`Session`] — the `include:` segments' scopes,
//! the `terminal:` and clock notes — and the world the playthrough ended
//! in.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use lute_trace::exec::seam::Closed;
use lute_trace::exec::session::{PlayHalt, QuestAdvance, Session, StepBody, World};

use super::plan::{Action, Scope, Step};
use super::script::PlayScript;
use crate::play_expect::WorldView;

pub(super) struct StepRecord {
    pub(super) n: usize,
    pub(super) label: Option<String>,
    /// `(k, of)` for the k-th run of a `repeat: of` step (of > 1).
    pub(super) iteration: Option<(usize, usize)>,
    pub(super) body: StepBody,
    pub(super) quests: Vec<QuestAdvance>,
    /// The world right after the step settled — captured only when the
    /// step's `expect:` judges it (0.23.1).
    pub(super) world: Option<WorldView>,
    /// Usage notes on the step as written — e.g. an `occasion:` step raising
    /// the `dayEnd` / `dayStart` its clock's `advance:` already raises
    /// ([`clock_raised_note`]).
    pub(super) notes: Vec<String>,
    /// dsl 0.25.0 §1: exclusive relations that both hold after the step
    /// (`seenAfter(elias) and fell(elias) both hold`) — each fails the play.
    pub(super) exclusive: Vec<String>,
}

/// The whole playthrough: the initial quest settle, then every step, and
/// the world it ended in.
pub(super) struct Playthrough {
    pub(super) start: Vec<QuestAdvance>,
    pub(super) steps: Vec<StepRecord>,
    /// The steps an `end: true` step left unplayed: `(n, label)`.
    pub(super) skipped: Vec<(usize, Option<String>)>,
    pub(super) outcome: Result<String, PlayHalt>,
    /// dsl 0.27.0 §4: the playthrough ended in the project's terminal state
    /// (`end: terminal`) — every step played and `terminal:` holds.
    pub(super) terminal: bool,
    pub(super) world: World,
}

pub(super) fn execute(script: &PlayScript, plan: &[Step], mut s: Session<'_>) -> Playthrough {
    // dsl 0.25.0 §1 (LH N16): the script's seeded world (`state:` /
    // `facts:`, the project's seeds, and what the rules derive) must not
    // already hold exclusive relations together.
    let seeded = s.exclusive();
    if !seeded.is_empty() {
        return Playthrough {
            start: Vec::new(),
            steps: Vec::new(),
            skipped: Vec::new(),
            terminal: false,
            outcome: Err(PlayHalt::Error(format!(
                "the script's seeded world holds exclusive relations together before step 1 — \
                 {}; fix the script's `facts:` (or the `excludes:` declaration)",
                seeded.join("; ")
            ))),
            world: s.world,
        };
    }
    let (start, halt) = s.settle();
    let mut steps = Vec::new();
    let finish = |start, steps, h: PlayHalt, world| Playthrough {
        start,
        steps,
        skipped: Vec::new(),
        terminal: false,
        outcome: Err(h),
        world,
    };
    if let Some(h) = halt {
        return finish(start, steps, h, s.world);
    }
    let mut open: Vec<OpenScope> = Vec::new();
    for (i, step) in plan.iter().enumerate() {
        enter_scopes(&mut s.world, &mut open, &step.segments);
        if matches!(step.action, Action::End) {
            enter_scopes(&mut s.world, &mut open, &[]);
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: None,
                body: StepBody::End,
                quests: Vec::new(),
                world: None,
                notes: Vec::new(),
                exclusive: Vec::new(),
            });
            let skipped: Vec<(usize, Option<String>)> = plan[i + 1..]
                .iter()
                .map(|s| (s.n, s.label.clone()))
                .collect();
            let reason = match skipped.len() {
                0 => format!("`end: true` at step {}", step.n),
                k => format!(
                    "`end: true` at step {} ({k} later step{} skipped)",
                    step.n,
                    if k == 1 { "" } else { "s" }
                ),
            };
            return Playthrough {
                start,
                steps,
                skipped,
                terminal: false,
                outcome: Ok(reason),
                world: s.world,
            };
        }
        let wants = script
            .step_expects
            .iter()
            .find(|(i, _, _)| *i == step.n)
            .and_then(|(_, _, e)| crate::play_expect::wants_world(e));
        for k in 1..=step.repeat {
            // dsl 0.24.0 §5: the step's own answers ride before the top
            // level's for exactly this run of the step.
            s.world.bridges.step = step.bridges.clone();
            // dsl 0.27.0 §4: an `occasion:` step's `engine:` writes land
            // first (their own record, then the settle), so the raise — its
            // `raisedWhen` gate included — sees them.
            if let Action::Occasion {
                writes: Some(writes),
                ..
            } = &step.action
            {
                let (body, quests, halt) = s.engine(step.n, writes);
                steps.push(StepRecord {
                    n: step.n,
                    label: step.label.clone(),
                    iteration: (step.repeat > 1).then_some((k, step.repeat)),
                    body,
                    quests,
                    world: None,
                    notes: Vec::new(),
                    exclusive: Vec::new(),
                });
                if let Some(h) = halt {
                    return finish(start, steps, h, s.world);
                }
            }
            let was_terminal = s.terminal();
            let (body, quests, halt) = run_step(&mut s, step);
            let leftover = std::mem::take(&mut s.world.bridges.step);
            let halt = halt.or_else(|| unconsumed_step_bridges(step.n, &leftover));
            // A halted step (a write-time exclusive refusal included, whose
            // `✗ exclusive` already sits in the transcript) is not checked again.
            let exclusive = if halt.is_none() {
                s.exclusive()
            } else {
                Vec::new()
            };
            let halt = halt.or_else(|| {
                (!exclusive.is_empty()).then(|| {
                    PlayHalt::Error(format!(
                        "step {}: exclusive relations hold together — {}",
                        step.n,
                        exclusive.join("; ")
                    ))
                })
            });
            let mut notes: Vec<String> = clock_raised_note(&s, &step.action, &plan[i + 1..])
                .into_iter()
                .chain(closed_raise_notes(&body))
                .collect();
            // dsl 0.27.0 §4: the step that ended the game says so.
            if halt.is_none() && !was_terminal && s.terminal() {
                notes.push(terminal_note(&s));
            }
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: (step.repeat > 1).then_some((k, step.repeat)),
                body,
                quests,
                world: wants.map(|wants| s.view(wants.facts)),
                notes,
                exclusive,
            });
            if let Some(h) = halt {
                return finish(start, steps, h, s.world);
            }
        }
    }
    enter_scopes(&mut s.world, &mut open, &[]);
    // dsl 0.27.0 §4: a playthrough whose last step left the game over ends
    // in the terminal state (a later raising step was refused above).
    if s.terminal() {
        let reason = format!("terminal — `terminal: {}` holds", terminal_text(&s));
        return Playthrough {
            start,
            steps,
            skipped: Vec::new(),
            outcome: Ok(reason),
            terminal: true,
            world: s.world,
        };
    }
    let n = steps.len();
    Playthrough {
        start,
        steps,
        skipped: Vec::new(),
        outcome: Ok(format!(
            "complete ({n} step{})",
            if n == 1 { "" } else { "s" }
        )),
        terminal: false,
        world: s.world,
    }
}

/// The project's `terminal:` condition as written.
fn terminal_text<'s>(s: &'s Session<'_>) -> &'s str {
    s.project()
        .index
        .terminal
        .as_ref()
        .map_or("", |t| t.raw.as_str())
}

/// dsl 0.27.0 §4: the note on the step after which `terminal:` holds.
fn terminal_note(s: &Session<'_>) -> String {
    format!(
        "the game is over — `terminal: {}` holds, so the engine raises no occasion from here \
         (`occasion:` / `advance:` steps are refused; `newRun: true` starts a new run)",
        terminal_text(s)
    )
}

/// dsl 0.27.0 §4: one note per raise an `advance:` did not make because the
/// seam was closed — the clock still moved and settled.
fn closed_raise_notes(body: &StepBody) -> Vec<String> {
    let StepBody::Advance { closed, .. } = body else {
        return Vec::new();
    };
    closed
        .iter()
        .map(|c| {
            let why = match &c.why {
                Closed::Gate(g) => format!("its `raisedWhen: {g}` is false there"),
                Closed::Terminal(t) => format!("the game is over (`terminal: {t}` holds)"),
                Closed::Unknown(u) => format!("whether the engine may raise it is undecided: {u}"),
            };
            format!(
                "`{}` was not raised at {} — {why}; the clock moved on without it",
                c.occasion, c.at
            )
        })
        .collect()
}

/// dsl 0.27.0 (T3-22): a segment open around the running step, with what
/// it replaced in the world — per `choose:` key the script's list and its
/// consumption, per `bridges:` tag the queue — so leaving it restores them.
struct OpenScope {
    scope: Arc<Scope>,
    choose: Vec<(String, Option<Vec<String>>, Option<usize>)>,
    bridges: Vec<(String, Option<VecDeque<lute_trace::BridgeAnswer>>)>,
}

/// Make `want` (outermost first) the open segments: leave every open one
/// that is not a prefix of it, innermost first, then enter the rest. A
/// segment's `choose:` lists replace the script's key by key and are
/// consumed from their start; its `bridges:` queues replace the script's
/// top-level ones tag by tag. Answers and decisions left over when it
/// closes are dropped with it.
fn enter_scopes(w: &mut World, open: &mut Vec<OpenScope>, want: &[Arc<Scope>]) {
    let keep = open
        .iter()
        .zip(want)
        .take_while(|(o, s)| Arc::ptr_eq(&o.scope, s))
        .count();
    while open.len() > keep {
        let Some(o) = open.pop() else { break };
        for (k, list, cursor) in o.choose.into_iter().rev() {
            match list {
                Some(list) => w.choose.insert(k.clone(), list),
                None => w.choose.remove(&k),
            };
            match cursor {
                Some(c) => w.choice_cursor.insert(k, c),
                None => w.choice_cursor.remove(&k),
            };
        }
        for (tag, queue) in o.bridges.into_iter().rev() {
            match queue {
                Some(q) => w.bridges.top.insert(tag, q),
                None => w.bridges.top.remove(&tag),
            };
        }
    }
    for scope in &want[keep..] {
        let choose = scope
            .choose
            .iter()
            .map(|(k, list)| {
                let prev = w.choose.insert(k.clone(), list.clone());
                (k.clone(), prev, w.choice_cursor.remove(k))
            })
            .collect();
        let bridges = scope
            .bridges
            .iter()
            .map(|(tag, q)| (tag.clone(), w.bridges.top.insert(tag.clone(), q.clone())))
            .collect();
        open.push(OpenScope {
            scope: scope.clone(),
            choose,
            bridges,
        });
    }
}

/// Summer R1: an `occasion:` step raising the `dayEnd` / `dayStart` the
/// clock's `raise:` map declares — the next `advance:` crossing that
/// midnight raises it again, so its content runs twice for one day. A note,
/// not an error: a script may mean it. Ember F7 (round-5 T3-24): only when
/// such an `advance:` comes — some `later` step, before a `newRun` / `end`,
/// moves the clock (from where it stands now, each advance from where the
/// one before it left it) across a midnight while raising that moment.
fn clock_raised_note(s: &Session<'_>, action: &Action, later: &[Step]) -> Option<String> {
    let Action::Occasion { occasion, .. } = action else {
        return None;
    };
    let clock = s.project().index.clock.as_ref()?;
    let moments = clock.raise.as_ref()?.moments();
    let (moment, when) = if moments.day_end.as_ref() == Some(occasion) {
        (
            "dayEnd",
            "at each midnight it crosses, before the clock leaves the day",
        )
    } else if moments.day_start.as_ref() == Some(occasion) {
        ("dayStart", "on each day it enters")
    } else {
        return None;
    };
    let mut at = s.clock_at()?;
    // dsl 0.27.0 §4: a finite clock stops at its last position, raising
    // its last `dayEnd` once; an advance after that moves nothing.
    let last = clock.last_at();
    let mut ended = s.world.clock_ended;
    let passed = later
        .iter()
        .take_while(|s| !matches!(s.action, Action::NewRun(_) | Action::End))
        .any(|s| {
            let Action::Advance { by, raise, .. } = &s.action else {
                return false;
            };
            let raises = match moment {
                "dayEnd" => raise.day_end.as_ref(),
                _ => raise.day_start.as_ref(),
            } == Some(occasion);
            (0..s.repeat).any(|_| {
                if ended {
                    return false;
                }
                let mut to = clock.advance(at, *by);
                let ends = last.is_some_and(|end| to > end);
                if let Some(end) = last.filter(|_| ends) {
                    to = end;
                }
                let crossed = to.day > at.day || (ends && moment == "dayEnd");
                at = to;
                ended = ends;
                raises && crossed
            })
        });
    passed.then(|| {
        format!(
            "`{occasion}` is the clock's `raise: {{ {moment}: {occasion} }}` — an `advance:` \
             raises it {when}; this step raises it again, so the same day's `{occasion}` runs \
             twice once an `advance:` passes it (drop the step and let `advance:` raise it)"
        )
    })
}

/// dsl 0.24.0 §5: answers a step's own `bridges:` gave that no plugin call
/// of the step consumed fail the step (exit 1), named — an answer the script
/// expected to decide something decided nothing.
fn unconsumed_step_bridges(
    n: usize,
    leftover: &BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
) -> Option<PlayHalt> {
    if leftover.is_empty() {
        return None;
    }
    let named: Vec<String> = leftover
        .iter()
        .map(|(tag, q)| {
            let answers: Vec<String> = q
                .iter()
                .map(|a| {
                    let fields: Vec<String> = a.iter().map(|(f, v)| format!("{f}: {v}")).collect();
                    format!("{{{}}}", fields.join(", "))
                })
                .collect();
            format!("`{tag}` {}", answers.join(" "))
        })
        .collect();
    Some(PlayHalt::Error(format!(
        "step {n}: its `bridges:` answers were not all consumed — no plugin call of the step \
         took {}",
        named.join("; ")
    )))
}

/// Run one step (one repetition): its body record, the quest advances it
/// made, and what ends the playthrough, if anything. Every step that
/// changes the world settles the quest lifecycle after it — a presentation,
/// an `engine:` write, a new run (dsl 0.22.0 §1.1) — and a raised occasion
/// or event is then answered by the quests.
fn run_step(s: &mut Session<'_>, step: &Step) -> lute_trace::exec::session::StepOutcome {
    let n = step.n;
    match &step.action {
        Action::Occasion {
            occasion,
            target,
            pick,
            choose,
            payload,
            ..
        } => {
            s.bind_payload(payload);
            s.occasion(n, occasion, target, pick, choose)
        }
        Action::NewRun(seed) => s.new_run(n, seed),
        Action::Engine(writes) => s.engine(n, writes),
        Action::Event(event) => s.event(event),
        Action::Advance {
            by,
            writes,
            raise,
            pick,
            choose,
        } => s.advance(n, *by, writes, raise, pick, choose),
        Action::End => unreachable!("`execute` ends the playthrough at an `end` step"),
    }
}
