//! The execution loop: the start settle, then every planned step (and
//! repetition) through the [`Session`] — the `include:` segments' scopes,
//! the `terminal:` and clock notes — and the world the playthrough ended
//! in.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use serde_json::Value as Json;

use lute_trace::exec::seam::Closed;
use lute_trace::exec::session::{PlayHalt, Played, QuestAdvance, Session, StepBody, World};

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
    /// dsl 0.27.0 §4: this record is the `engine:` write an `occasion:` step
    /// lands before its raise; the step itself is the next record. It is
    /// part of the step, not a step (or repetition) of its own: the step's
    /// `expect:` judges the raise, and the end count skips it.
    pub(super) before_raise: bool,
}

/// The whole playthrough: the initial quest settle, then every step, and
/// the world it ended in.
pub(super) struct Playthrough {
    pub(super) start: Vec<QuestAdvance>,
    pub(super) steps: Vec<StepRecord>,
    /// The steps an `end: true` step left unplayed: `(n, label)`.
    pub(super) skipped: Vec<(usize, Option<String>)>,
    pub(super) outcome: Result<String, PlayHalt>,
    /// The playthrough ran to its end (every step, or an `end: true` step)
    /// with the project's `terminal:` holding (`end: terminal`).
    pub(super) terminal: bool,
    pub(super) world: World,
}

impl Playthrough {
    /// How the playthrough ended — the value `expect.end` judges
    /// ([`crate::play_expect::ENDS`]): `complete`, `terminal`, or the halt's
    /// `incomplete` / `error`.
    pub(super) fn ended(&self) -> &'static str {
        match &self.outcome {
            Ok(_) if self.terminal => "terminal",
            Ok(_) => "complete",
            Err(h) => h.exit_label(),
        }
    }
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
    // HW27-10: a refused step (`E-OCCASION-GATE`, a `pick:` that is not
    // eligible, …) is located at the step as written — its play, or the
    // steps file an `include:` spliced it from — like a usage error.
    let finish = |start, steps, h: PlayHalt, world| Playthrough {
        start,
        steps,
        skipped: Vec::new(),
        terminal: false,
        outcome: Err(match h {
            PlayHalt::Error(m) => {
                PlayHalt::Error(super::plan::locate_step_error(&script.steps, &m).unwrap_or(m))
            }
            h => h,
        }),
        world,
    };
    if let Some(h) = halt {
        return finish(start, steps, h, s.world);
    }
    let mut open: Vec<OpenScope> = Vec::new();
    // T3-56: the `choose:` keys of `include:` items that closed without
    // presenting them, with the include — what a later halt at that
    // choice names.
    let mut ended: Vec<(String, String)> = Vec::new();
    for (i, step) in plan.iter().enumerate() {
        let closed = enter_scopes(&mut s.world, &mut open, &step.segments, &mut ended);
        note_on_last(&mut steps, closed);
        if matches!(step.action, Action::End) {
            let closed = enter_scopes(&mut s.world, &mut open, &[], &mut ended);
            note_on_last(&mut steps, closed);
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: None,
                body: StepBody::End,
                quests: Vec::new(),
                world: None,
                notes: Vec::new(),
                exclusive: Vec::new(),
                before_raise: false,
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
                terminal: s.terminal(),
                outcome: Ok(reason),
                world: s.world,
            };
        }
        let wants = script
            .step_expects
            .iter()
            .find(|(i, _, _)| *i == step.n)
            .and_then(|(_, _, e)| crate::play_expect::wants_world(e));
        let first = steps.len();
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
                    before_raise: true,
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
                .chain(passed_raise_note(&s, &body))
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
                before_raise: false,
            });
            for o in &mut open {
                menus_presented(steps.last().expect("just pushed"), &mut o.used);
            }
            if let Some(h) = halt {
                let h = name_ended_include(h, steps.last(), &ended);
                return finish(start, steps, h, s.world);
            }
        }
        if let Some(note) = step_choose_note(step, &steps[first..]) {
            note_on_last(&mut steps, vec![note]);
        }
    }
    let closed = enter_scopes(&mut s.world, &mut open, &[], &mut ended);
    note_on_last(&mut steps, closed);
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
    let n = steps.iter().filter(|s| !s.before_raise).count();
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
                Closed::Gate { raw, reads } => format!(
                    "its `raisedWhen: {raw}` is false there{}",
                    Closed::reads_text(reads)
                ),
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

/// The note on an `advance:` that passed positions without raising the
/// clock's slot occasion (raised once, where the clock stops), naming them
/// by day — a run of days passed whole as one range — when some beat
/// answers that occasion.
fn passed_raise_note(s: &Session<'_>, body: &StepBody) -> Option<String> {
    let StepBody::Advance {
        passed: Some(pr), ..
    } = body
    else {
        return None;
    };
    if pr.beats == 0 {
        return None;
    }
    let clock = s.project().index.clock.as_ref()?;
    let day = |d: i64| match clock.weekday_label(d) {
        Some(label) => format!("day {d} ({label})"),
        None => format!("day {d}"),
    };
    let days = |from: i64, to: i64| match to - from {
        0 => day(from),
        _ => format!("{} to {}", day(from), day(to)),
    };
    let mut groups: Vec<(i64, Vec<&str>)> = Vec::new();
    for at in &pr.at {
        match groups.last_mut() {
            Some((d, slots)) if *d == at.day => slots.extend(at.slot.as_deref()),
            _ => groups.push((at.day, at.slot.as_deref().into_iter().collect())),
        }
    }
    // Runs of consecutive days passed whole: `(first, last)`; a day passed
    // in part is its own entry with its slots.
    let whole = |slots: &[&str]| slots.len() == clock.slot_count();
    let mut named: Vec<String> = Vec::new();
    let mut run: Option<(i64, i64)> = None;
    let flush = |run: &mut Option<(i64, i64)>, named: &mut Vec<String>| {
        if let Some((a, b)) = run.take() {
            if clock.slot.is_some() {
                named.push(format!("{}, every slot", days(a, b)));
            } else if b - a >= 2 {
                named.push(days(a, b));
            } else {
                named.extend((a..=b).map(day));
            }
        }
    };
    for (d, slots) in &groups {
        if whole(slots) || clock.slot.is_none() {
            match &mut run {
                Some((_, b)) if *b + 1 == *d => *b = *d,
                _ => {
                    flush(&mut run, &mut named);
                    run = Some((*d, *d));
                }
            }
        } else {
            flush(&mut run, &mut named);
            named.push(format!("{} {}", day(*d), slots.join(", ")));
        }
    }
    flush(&mut run, &mut named);
    let sep = if clock.slot.is_some() { "; " } else { ", " };
    Some(format!(
        "passed {} without raising `{}` ({} beat{} answer{} it; an `advance:` raises it only \
         where the clock stops)",
        named.join(sep),
        pr.occasion,
        pr.beats,
        if pr.beats == 1 { "" } else { "s" },
        if pr.beats == 1 { "s" } else { "" },
    ))
}

/// dsl 0.27.0 (T3-22): a segment open around the running step, with what
/// it replaced in the world — per `choose:` key the script's list and its
/// consumption, per `bridges:` tag the queue — so leaving it restores them,
/// and what its steps used of its own decisions and answers (OT-F-1).
struct OpenScope {
    scope: Arc<Scope>,
    choose: Vec<(String, Option<Vec<String>>, Option<usize>)>,
    bridges: Vec<(String, Option<VecDeque<lute_trace::BridgeAnswer>>)>,
    used: Used,
}

/// OT-F-1: what the steps of one `include:` item — every repetition of it
/// — used of its own `choose:` / `bridges:`: the branches/hubs they
/// presented, the tags a call took an answer of.
#[derive(Default)]
struct Used {
    menus: BTreeSet<String>,
    bridged: BTreeSet<String>,
}

/// Make `want` (outermost first) the open segments: leave every open one
/// that is not a prefix of it, innermost first, then enter the rest. A
/// segment's `choose:` lists replace the script's key by key and are
/// consumed from their start; its `bridges:` queues replace the script's
/// top-level ones tag by tag. Answers and decisions left over when it
/// closes are dropped with it. OT-F-1: an `include:` item whose steps —
/// over all its repetitions — never presented a `choose:` key of its own,
/// or never took an answer of a `bridges:` tag of its own, says so as its
/// last repetition closes (a decision written on the wrong `include:`);
/// the notes are returned, and each such key is pushed onto `ended` with
/// its include (T3-56: a later halt at that choice names it).
fn enter_scopes(
    w: &mut World,
    open: &mut Vec<OpenScope>,
    want: &[Arc<Scope>],
    ended: &mut Vec<(String, String)>,
) -> Vec<String> {
    let keep = open
        .iter()
        .zip(want)
        .take_while(|(o, s)| Arc::ptr_eq(&o.scope, s))
        .count();
    let mut closed: Vec<(usize, Arc<Scope>, Used)> = Vec::new();
    while open.len() > keep {
        let Some(mut o) = open.pop() else { break };
        for (tag, answers) in &o.scope.bridges {
            if w.bridges.top.get(tag).map_or(0, VecDeque::len) < answers.len() {
                o.used.bridged.insert(tag.clone());
            }
        }
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
        closed.push((open.len(), o.scope, o.used));
    }
    for (depth, scope) in want.iter().enumerate().skip(keep) {
        // The next repetition of the same `include:` item carries on.
        let used = closed
            .iter()
            .position(|(d, s, _)| *d == depth && s.include == scope.include)
            .map(|i| closed.remove(i).2)
            .unwrap_or_default();
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
            used,
        });
    }
    for (_, scope, used) in &closed {
        ended.extend(
            scope
                .choose
                .keys()
                .filter(|k| !used.menus.contains(*k))
                .map(|k| (k.clone(), scope.include.clone())),
        );
    }
    closed
        .iter()
        .rev()
        .filter_map(|(_, scope, used)| leftover_note(scope, used))
        .collect()
}

/// T3-56: a halt at a choice no decision was scripted for, when an
/// `include:` that already ended carried that decision unused — the halt
/// names it (its "never used" note sits steps above).
fn name_ended_include(
    h: PlayHalt,
    last: Option<&StepRecord>,
    ended: &[(String, String)],
) -> PlayHalt {
    let PlayHalt::Incomplete(msg) = h else {
        return h;
    };
    let unscripted = last.and_then(|r| {
        step_transcript(r).find_map(|rec| {
            (rec.get("note").and_then(|n| n.as_str()) == Some(lute_trace::exec::NOTE_NO_DECISION))
                .then(|| menu_id(rec))
                .flatten()
        })
    });
    match unscripted.and_then(|id| ended.iter().rev().find(|(k, _)| k == id)) {
        Some((_, include)) => PlayHalt::Incomplete(format!(
            "{msg} — its decision was scripted on {include}, which ended before this step"
        )),
        None => PlayHalt::Incomplete(msg),
    }
}

/// T3-56: a step's own `choose:` keys none of its presentations (over all
/// its repetitions) presented — dropped with the step, like an
/// `include:`'s ([`leftover_note`]).
fn step_choose_note(step: &Step, records: &[StepRecord]) -> Option<String> {
    let choose = match &step.action {
        Action::Occasion { choose, .. } | Action::Advance { choose, .. } => choose,
        _ => return None,
    };
    let mut used = Used::default();
    for r in records {
        menus_presented(r, &mut used);
    }
    let unused: Vec<String> = choose
        .keys()
        .filter(|k| !used.menus.contains(*k))
        .map(|k| format!("`choose: {k}` (no presentation of the step presented `{k}`)"))
        .collect();
    (!unused.is_empty()).then(|| {
        format!(
            "step {} never used its own {} — a decision for a later step belongs on that step, \
             or on the script",
            step.n,
            unused.join(", ")
        )
    })
}

/// Notes a segment left as it closed ([`leftover_note`]), on the last step
/// it ran — the record the reader sees them under.
fn note_on_last(steps: &mut [StepRecord], notes: Vec<String>) {
    if let Some(last) = steps.last_mut() {
        last.notes.extend(notes);
    }
}

/// OT-F-1: what an `include:` item's own `choose:` / `bridges:` left
/// unused over all its repetitions — a `choose:` key none of its steps
/// presented, a `bridges:` tag none of its calls took an answer of. `None`
/// when it used every one.
fn leftover_note(scope: &Scope, used: &Used) -> Option<String> {
    let unused: Vec<String> = scope
        .choose
        .keys()
        .filter(|k| !used.menus.contains(*k))
        .map(|k| format!("`choose: {k}` (no step of the include presented `{k}`)"))
        .chain(
            scope
                .bridges
                .keys()
                .filter(|t| !used.bridged.contains(*t))
                .map(|t| format!("`bridges: {t}` (no call of the include took an answer)")),
        )
        .collect();
    (!unused.is_empty()).then(|| {
        format!(
            "{} never used its own {} — a decision or answer for a step outside the include \
             belongs on that step, or on the script",
            scope.include,
            unused.join(", ")
        )
    })
}

/// Every record a step record's walks wrote — its beats (an `advance:`'s
/// midnight raises included) and its quest handlers.
fn step_transcript(r: &StepRecord) -> impl Iterator<Item = &Json> {
    let beats = r
        .body
        .occasion()
        .into_iter()
        .flat_map(|b| match b {
            StepBody::Occasion { presented, .. } => presented.iter().collect(),
            _ => Vec::new(),
        })
        .map(|pr| pr.transcript.as_slice());
    let days: Vec<&[Json]> = r
        .body
        .days_played()
        .into_iter()
        .map(|p| match p {
            Played::Beat(pr) => pr.transcript.as_slice(),
            Played::Quest(q) => q.transcript.as_slice(),
        })
        .collect();
    let quests = r
        .body
        .settled()
        .chain(&r.quests)
        .map(|q| q.transcript.as_slice());
    beats.chain(days).chain(quests).flatten()
}

/// The branch/hub id a `choice` / `hub` record presented.
fn menu_id(rec: &Json) -> Option<&str> {
    match rec.get("kind").and_then(|k| k.as_str()) {
        Some("choice") => rec.get("branch"),
        Some("hub") => rec.get("hub"),
        _ => None,
    }
    .and_then(|v| v.as_str())
}

/// Every branch/hub id a step record's walks presented.
fn menus_presented(r: &StepRecord, into: &mut Used) {
    into.menus
        .extend(step_transcript(r).filter_map(menu_id).map(str::to_string));
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
    let mut ended = lute_trace::clock::ended(&s.world.state);
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
