//! dsl 0.24.0 §1: an `advance:` of the declared clock — moving it, the
//! midnight `dayEnd` / `dayStart` raises it crosses, then its slot raise.

use std::collections::BTreeMap;

use serde_json::Value as Json;

use super::lifecycle::{advance_quests, settle_before, QuestAdvance};
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

/// The positions an `advance:` stood at on its way, strictly between where
/// it started and where it stopped, at which the clock's `raise.slot`
/// occasion was not raised — it is raised once, where the clock stops.
/// `beats`: how many beats answer that occasion.
pub struct PassedRaise {
    pub occasion: String,
    pub at: Vec<PassedAt>,
    pub beats: usize,
}

/// One position a [`PassedRaise`] names: its day and slot (`None` on a
/// clock without slots).
pub struct PassedAt {
    pub day: i64,
    pub slot: Option<String>,
}

/// The positions strictly between `from` and `to` the clock stood at: every
/// slot on the way, except that an `advance: day` sleeps through the rest
/// of its day and stands only at the next day's first slot.
fn passed_positions(
    clock: &lute_manifest::clock::ClockDecl,
    by: lute_manifest::clock::Advance,
    from: lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
) -> Vec<lute_manifest::clock::ClockAt> {
    use lute_manifest::clock::{Advance, ClockAt};
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        at = if by == Advance::Day && at.day == from.day {
            ClockAt {
                day: at.day + 1,
                slot: 0,
            }
        } else {
            clock.advance(at, Advance::Slots(1))
        };
        if at < to {
            out.push(at);
        }
    }
    out
}

/// The [`PassedRaise`] of an advance from `from` to `to`, when the clock
/// declares a `raise.slot` occasion and the advance passed a position.
fn passed_raise(
    p: &ExecProject,
    clock: &lute_manifest::clock::ClockDecl,
    by: lute_manifest::clock::Advance,
    raise: &lute_manifest::clock::RaiseMoments,
    from: lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
) -> Option<PassedRaise> {
    let occasion = raise.slot.as_ref()?;
    let at: Vec<PassedAt> = passed_positions(clock, by, from, to)
        .into_iter()
        .map(|at| PassedAt {
            day: at.day,
            slot: clock
                .slot
                .as_ref()
                .and(clock.slot_name(at.slot))
                .map(str::to_string),
        })
        .collect();
    if at.is_empty() {
        return None;
    }
    let beats = p
        .index
        .beats
        .iter()
        .filter(|b| b.answers(occasion, None).is_some())
        .count();
    Some(PassedRaise {
        occasion: occasion.clone(),
        at,
        beats,
    })
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
            .push((clock.day.clone(), Write::Set(Value::Int(to.day))));
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

/// dsl 0.27.0 §5 (G-3): move the clock from `*at` to `to` ([`move_clock`]),
/// settling every quest ([`advance_quests`]) at each position strictly
/// between — each day, and each slot on a slotted clock — as a stop of the
/// clock would: seasons open and close, rearms fire, quests start, complete
/// and fail (`by`) there, each judged in the world of that position (the
/// same snapshot the cadence observation reads). A window that opens and
/// closes inside one `advance:` behaves as it does when the clock raises
/// `dayStart` / `dayEnd` at every midnight. `to` itself is settled by the
/// caller's settle that follows.
///
/// A crossed position whose settle moved anything appends the clock's `set`
/// records of the move up to it (document `""`), then that settle's records,
/// to `settled`. The rest of the move's `set` records are returned — or,
/// when `settled` already holds records (which the caller shows after its
/// writes), appended to it too, so the transcript keeps the clock's order.
/// On a halt `*at` is the position the clock stopped at.
pub fn walk_clock(
    p: &ExecProject,
    w: &mut World,
    clock: &lute_manifest::clock::ClockDecl,
    at: &mut lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
    settled: &mut Vec<QuestAdvance>,
) -> (Vec<Json>, Option<PlayHalt>) {
    // Nothing to settle: one move.
    if p.cadence.is_empty() && p.quest_docs.is_empty() {
        let writes = move_clock(p, w, clock, *at, to);
        *at = to;
        return (writes, None);
    }
    // The last position whose move is already in the transcript.
    let mut shown = *at;
    while *at < to {
        let next = clock
            .advance(*at, lute_manifest::clock::Advance::Slots(1))
            .min(to);
        move_clock(p, w, clock, *at, next);
        *at = next;
        if *at == to {
            break;
        }
        let (s, halt) = advance_quests(p, w);
        if !s.is_empty() || halt.is_some() {
            // Already there: the rewrite only yields the records.
            let moved = move_clock(p, w, clock, shown, *at);
            settled.push(QuestAdvance {
                document: String::new(),
                transcript: moved,
            });
            settled.extend(s);
            shown = *at;
        }
        if halt.is_some() {
            return (Vec::new(), halt);
        }
    }
    let tail = move_clock(p, w, clock, shown, to);
    if settled.is_empty() {
        return (tail, None);
    }
    if !tail.is_empty() {
        settled.push(QuestAdvance {
            document: String::new(),
            transcript: tail,
        });
    }
    (Vec::new(), None)
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
            passed: None,
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
    // dsl 0.27.0 §4: once the game is over the clock does not move on — the
    // refused step still says where the clock stands (HW27-08).
    if let Ok(true) = crate::exec::seam::terminal_holds(p, w) {
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
            Some(crate::exec::seam::advance_after_terminal(n, p)),
        );
    }
    // dsl 0.27.0 §4 (T2-5): a finite clock stops at its last position. An
    // advance once it ended — or from past it (an `engine:` write moved the
    // day on) — is a usage error.
    let last = clock.last_at();
    let ended = crate::clock::ended(&w.state);
    if let Some(end) = last.filter(|end| ended || from > *end) {
        let why = if ended {
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
            let (moved, halt) = walk_clock(p, w, clock, &mut at, last, &mut settled);
            writes.extend(moved);
            if halt.is_some() {
                stop = halt;
                break;
            }
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
        if by == Advance::Day {
            writes.extend(move_clock(p, w, clock, at, next));
            at = next;
        } else {
            let (moved, halt) = walk_clock(p, w, clock, &mut at, next, &mut settled);
            writes.extend(moved);
            if halt.is_some() {
                stop = halt;
                break;
            }
        }
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
        if by == Advance::Day {
            writes.extend(move_clock(p, w, clock, at, to));
            at = to;
        } else {
            let (moved, halt) = walk_clock(p, w, clock, &mut at, to, &mut settled);
            writes.extend(moved);
            stop = halt;
        }
    }
    // dsl 0.27.0 §4: at the end the clock raises the last day's `dayEnd`
    // (never its `raise.slot`) and stops; the step's `engine:` writes land
    // after it, where the clock stays.
    if ends && stop.is_none() {
        crate::clock::set_ended(clock, &mut w.state, true);
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
    let mut out = body(
        clock.describe(from),
        clock.describe(at),
        writes,
        settled,
        days,
        raised,
        ends,
        closed,
    );
    if let StepBody::Advance { passed, .. } = &mut out {
        *passed = passed_raise(p, clock, by, raise, from, at);
    }
    (out, quests, stop)
}
