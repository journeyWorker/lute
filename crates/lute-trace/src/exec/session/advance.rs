//! dsl 0.24.0 §1: an `advance:` of the declared clock — moving it, the
//! midnight `dayEnd` / `dayStart` raises it crosses, then its slot raise.

use std::collections::BTreeMap;

use serde_json::Value as Json;

use super::lifecycle::{settle_before, QuestAdvance};
use super::present::Presented;
use super::project::ExecProject;
use super::resolve::{Write, Writes};
use super::step::{apply_writes, run_occasion, Pick, StepBody};
use super::walk::PlayHalt;
use super::world::{clock_at, refresh_clock, World};
use crate::Value;

/// One transcript an `advance:` played at a midnight.
pub enum Played<'a> {
    Quest(&'a QuestAdvance),
    Beat(&'a Presented),
}

/// dsl 0.24.0 §1: a `dayEnd` / `dayStart` an `advance:` raised at a
/// midnight: the move that brought the clock there (its `set` records) and
/// the settle after it, then the occasion raised at `at` and the quests'
/// answer to it.
pub struct DayRaise {
    pub at: String,
    pub writes: Vec<Json>,
    pub settled: Vec<QuestAdvance>,
    pub occasion: Box<StepBody>,
    pub quests: Vec<QuestAdvance>,
}

/// dsl 0.24.0 §1: move the clock to `to`, writing its day (and slot) paths
/// where they change; the `set` records.
pub fn move_clock(
    p: &ExecProject,
    w: &mut World,
    clock: &lute_manifest::clock::ClockDecl,
    from: lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
) -> Vec<Json> {
    let mut moved = Writes::default();
    if to.day != from.day {
        moved
            .state
            .push((clock.day.clone(), Write::Set(Value::Num(to.day as f64))));
    }
    if let (Some(path), Some(name), true) =
        (&clock.slot, clock.slot_name(to.slot), to.slot != from.slot)
    {
        moved
            .state
            .push((path.clone(), Write::Set(Value::Str(name.to_string()))));
    }
    let writes = apply_writes(w, &moved).expect("literal writes never fail");
    refresh_clock(p, w);
    writes
}

/// dsl 0.24.0 §1: one `advance:` — write the clock's day/slot paths `by`
/// slots (or to the next day's first slot) forward, apply the step's
/// `engine:` writes where the clock arrives, settle every quest (a `by`
/// deadline the new time passes fails here), then raise the clock's
/// `raise.slot` occasion, exactly as an `occasion:` step raises it. With
/// `raise.dayEnd` / `raise.dayStart`, every midnight the advance crosses is
/// a stop of its own, before the `engine:` writes (ember R3: that evening's
/// `dayEnd` still reads the day it closes): `dayEnd` is raised at the day's
/// last slot (`advance: <n>` walks there — it never skips the close of a
/// day; `advance: day` closes the day where the clock stands), then the
/// clock crosses to the next day's first slot and `dayStart` is raised
/// there; each move settles the quests first.
#[allow(clippy::too_many_arguments)]
pub fn run_advance(
    p: &ExecProject,
    w: &mut World,
    n: usize,
    by: lute_manifest::clock::Advance,
    engine: &Writes,
    raise: &lute_manifest::clock::RaiseMoments,
    pick: &Option<Pick>,
    choose: &BTreeMap<String, Vec<String>>,
) -> (StepBody, Vec<QuestAdvance>, Option<PlayHalt>) {
    use lute_manifest::clock::{Advance, ClockAt};
    let clock = p
        .index
        .clock
        .as_ref()
        .expect("the plan requires a clock for `advance:`");
    let by_text = match by {
        Advance::Day => "day".to_string(),
        Advance::Slots(1) => "slot".to_string(),
        Advance::Slots(k) => k.to_string(),
        Advance::To { weekday, slot } => {
            let wd = weekday.map(|wd| {
                clock
                    .week
                    .as_ref()
                    .and_then(|w| w.labels.get(wd as usize).cloned())
                    .unwrap_or_else(|| format!("weekday {wd}"))
            });
            let sl = slot.and_then(|s| clock.slot_name(s)).map(str::to_string);
            format!(
                "to {}",
                wd.into_iter().chain(sl).collect::<Vec<_>>().join(" ")
            )
        }
    };
    let body = |from: String, to: String, writes, settled, days, raised, ended, closed| {
        StepBody::Advance {
            by: by_text.clone(),
            from,
            to,
            writes,
            settled,
            days,
            raised,
            ended,
            closed,
        }
    };
    let Some(from) = clock_at(p, w) else {
        let names = match &clock.slot {
            Some(slot) => format!(
                "`{}` / `{slot}` name no position on it (a whole day number and one of: {})",
                clock.day,
                clock.slots.join(", ")
            ),
            None => format!("`{}` is no whole day number", clock.day),
        };
        let halt = PlayHalt::Error(format!(
            "step {n}: `advance:` cannot move the clock — {names}"
        ));
        return (
            body(
                String::new(),
                String::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                Vec::new(),
            ),
            Vec::new(),
            Some(halt),
        );
    };
    // dsl 0.27.0 §4 (T2-5): a finite clock stops at its last position. An
    // advance once it ended — or from past it (an `engine:` write moved the
    // day on) — is a usage error.
    let last = clock.last_at();
    if let Some(end) = last.filter(|end| w.clock_ended || from > *end) {
        let why = if w.clock_ended {
            "the clock ended".to_string()
        } else {
            format!("the clock stands at {}", clock.describe(from))
        };
        let halt = PlayHalt::Error(format!(
            "step {n}: `advance:` past the clock's last position ({}) — {why}; a `newRun` \
             starts it over ({})",
            clock.describe(end),
            lute_manifest::clock::E_CLOCK_END
        ));
        let at = clock.describe(from);
        return (
            body(
                at.clone(),
                at,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                Vec::new(),
            ),
            Vec::new(),
            Some(halt),
        );
    }
    let mut writes = Vec::new();
    let mut to = clock.advance(from, by);
    // An advance whose destination lies past the end walks to the last
    // position, raising what it crosses on the way, then ends the clock.
    let ends = last.is_some_and(|end| to > end);
    if let Some(end) = last.filter(|_| ends) {
        to = end;
    }
    let mut at = from;
    let mut settled = Vec::new();
    let mut days = Vec::new();
    let mut stop = None;
    let mut closed = Vec::new();
    // Each midnight crossed, while the clock raises something there.
    while at.day < to.day && (raise.day_end.is_some() || raise.day_start.is_some()) {
        if let Some(end) = &raise.day_end {
            let last = if by == Advance::Day {
                at
            } else {
                clock.day_end(at)
            };
            writes.extend(crate::exec::cadence::walk_clock(
                p,
                w,
                clock,
                at,
                last,
                &mut settled,
            ));
            at = last;
            let (s, halt) = settle_before(p, w, Some(end));
            settled.extend(s);
            if halt.is_some() {
                stop = halt;
                break;
            }
            // dsl 0.27.0 §4: a closed seam (terminal, a false gate) raises
            // nothing; the clock still moves and settles.
            if crate::exec::seam::clock_raise_open(p, w, end, || clock.describe(at), &mut closed) {
                let (occasion, quests, halt) = run_occasion(p, w, n, end, &None, &None, choose);
                days.push(DayRaise {
                    at: clock.describe(at),
                    writes: std::mem::take(&mut writes),
                    settled: std::mem::take(&mut settled),
                    occasion: Box::new(occasion),
                    quests,
                });
                if halt.is_some() {
                    stop = halt;
                    break;
                }
            } else {
                // The settle deferred the `by`s this raise would judge.
                w.defer_by = None;
            }
        }
        let next = ClockAt {
            day: at.day + 1,
            slot: 0,
        };
        // dsl 0.27.0 §5: an `advance: day` sleeps through the rest of the
        // day; any other advance crosses its slots.
        writes.extend(if by == Advance::Day {
            move_clock(p, w, clock, at, next)
        } else {
            crate::exec::cadence::walk_clock(p, w, clock, at, next, &mut settled)
        });
        at = next;
        let Some(start) = &raise.day_start else {
            continue;
        };
        let (s, halt) = settle_before(p, w, Some(start));
        settled.extend(s);
        if halt.is_some() {
            stop = halt;
            break;
        }
        if !crate::exec::seam::clock_raise_open(p, w, start, || clock.describe(at), &mut closed) {
            w.defer_by = None;
            continue;
        }
        let (occasion, quests, halt) = run_occasion(p, w, n, start, &None, &None, choose);
        days.push(DayRaise {
            at: clock.describe(at),
            writes: std::mem::take(&mut writes),
            settled: std::mem::take(&mut settled),
            occasion: Box::new(occasion),
            quests,
        });
        if halt.is_some() {
            stop = halt;
            break;
        }
    }
    let mut raised = None;
    let mut quests = Vec::new();
    if stop.is_none() {
        writes.extend(if by == Advance::Day {
            move_clock(p, w, clock, at, to)
        } else {
            crate::exec::cadence::walk_clock(p, w, clock, at, to, &mut settled)
        });
        at = to;
    }
    // dsl 0.27.0 §4: at the end the clock raises the last day's `dayEnd`
    // (never its `raise.slot`) and stops; the step's `engine:` writes land
    // after it, where the clock stays.
    if ends && stop.is_none() {
        w.clock_ended = true;
        if let Some(end) = &raise.day_end {
            let (s, halt) = settle_before(p, w, Some(end));
            settled.extend(s);
            stop = halt;
            if stop.is_none()
                && crate::exec::seam::clock_raise_open(
                    p,
                    w,
                    end,
                    || clock.describe(at),
                    &mut closed,
                )
            {
                let (occasion, q, halt) = run_occasion(p, w, n, end, &None, &None, choose);
                days.push(DayRaise {
                    at: clock.describe(at),
                    writes: std::mem::take(&mut writes),
                    settled: std::mem::take(&mut settled),
                    occasion: Box::new(occasion),
                    quests: q,
                });
                stop = halt;
            } else {
                w.defer_by = None;
            }
        }
    }
    if stop.is_none() {
        match apply_writes(w, engine) {
            Ok(engine) => writes.extend(engine),
            Err(e) => stop = Some(PlayHalt::Error(format!("step {n}: {e}"))),
        }
    }
    if stop.is_none() {
        let next = if ends { None } else { raise.slot.as_ref() };
        let (s, halt) = settle_before(p, w, next);
        settled.extend(s);
        stop = halt;
        let open = next.filter(|o| {
            stop.is_none()
                && crate::exec::seam::clock_raise_open(p, w, o, || clock.describe(at), &mut closed)
        });
        if open.is_none() {
            w.defer_by = None;
        }
        if let (Some(occasion), None) = (open, &stop) {
            let (b, q, halt) = run_occasion(p, w, n, occasion, &None, pick, choose);
            raised = Some(Box::new(b));
            quests = q;
            stop = halt;
        }
    }
    (
        body(
            clock.describe(from),
            clock.describe(at),
            writes,
            settled,
            days,
            raised,
            ends,
            closed,
        ),
        quests,
        stop,
    )
}
