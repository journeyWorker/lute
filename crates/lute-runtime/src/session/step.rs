//! The step operations and the [`StepBody`] each records: an occasion
//! raise ([`run_occasion`]), a `newRun` and an `engine:` step's writes.

use std::collections::BTreeMap;

use lute_manifest::schema::OccasionSelect;
use serde_json::{json, Value as Json};

use super::advance::{DayRaise, Played};
use super::eligibility::{
    candidates, deciding_unknown, kind_label, presented, rejudge, Candidate, RaiseSeam, Verdict,
};
use super::lifecycle::{advance_quests, raise, run_deferred_handlers, QuestAdvance, Raise};
use super::present::{present_with_choose, Presented};
use super::project::ExecProject;
use super::resolve::{entry_flag, Write, Writes};
use super::walk::PlayHalt;
use super::world::{json_to_value, render_fact, value_to_json, World};
use crate::Value;

/// A `pick:` on a `select: all` occasion.
#[derive(Clone, PartialEq)]
pub enum Pick {
    /// Present this beat.
    Beat(String),
    /// `pick: none` (dsl 0.22.0 §10): close the list — nothing presented or
    /// spent; `on=` objectives are still judged.
    Pass,
}

/// One script step's record.
pub enum StepBody {
    Occasion {
        occasion: String,
        target: Option<String>,
        select: OccasionSelect,
        pick: Option<Pick>,
        candidates: Vec<Candidate>,
        /// The main beat — `None` with `decided: true` when the occasion
        /// passed with no main story (no eligible non-`also` beat, or
        /// `pick: none`).
        winner: Option<String>,
        decided: bool,
        /// Every presentation, in order (dsl 0.23.0 §3): the winner, then
        /// its `also` riders; or a `select: sequence`'s eligible beats.
        presented: Vec<Presented>,
        /// dsl 0.24.0 §2: under `judge: before`, the quest advances the
        /// occasion's raise made before its candidates were decided — shown
        /// and transcribed ahead of the presentations. Empty otherwise.
        judged: Vec<QuestAdvance>,
        /// dsl 0.27.0 §4: the engine would not make this raise — why, in
        /// a few words (`gate false`, `the game is over`, `undecided`);
        /// the step then has no candidates. `None` for a raise made.
        not_raised: Option<&'static str>,
    },
    /// `writes`: the long form's seed records; `reset_quests`/`prev_run`:
    /// [`NewRunReport`].
    NewRun {
        writes: Vec<Json>,
        reset_quests: Vec<(String, String)>,
        prev_run: Vec<(String, Value)>,
        unjudged: Vec<String>,
        /// dsl 0.24.0 §2: the `at="nextRun"` accepts applied at this start.
        accepted: Vec<String>,
    },
    Engine {
        writes: Vec<Json>,
    },
    Event {
        event: String,
    },
    /// dsl 0.24.0 §1: an `advance:` — `by` as written (`slot`, `day`, `3`),
    /// the clock `from` → `to` (described, `day 2 (Tue) morning`), each
    /// `dayEnd` / `dayStart` the clock raised at a midnight it crossed,
    /// then the `set` records of the last move and the step's `engine:`
    /// writes (applied where the clock arrives), the quest settle right
    /// after, then the clock's `raise.slot` occasion as an `Occasion` body.
    /// `ended` (dsl 0.27.0 §4): the advance reached a finite clock's end —
    /// it stopped at the last position, raised the last `dayEnd` (in
    /// `days`) and no `raise.slot`. `closed` (dsl 0.27.0 §4): the raises it
    /// did not make because the seam was closed (a false `raisedWhen`, the
    /// terminal state). `passed`: the positions the clock stood at on the
    /// way, between `from` and `to`, where its `raise.slot` occasion was not
    /// raised (it is raised once, where the clock stops).
    Advance {
        by: String,
        from: String,
        to: String,
        writes: Vec<Json>,
        settled: Vec<QuestAdvance>,
        days: Vec<DayRaise>,
        raised: Option<Box<StepBody>>,
        ended: bool,
        closed: Vec<crate::seam::ClosedRaise>,
        passed: Option<super::advance::PassedRaise>,
    },
    /// `end: true` — the playthrough ends here.
    End,
}

impl StepBody {
    /// The occasion this step raised: its own, or the one an `advance:`
    /// raised after moving the clock.
    pub fn occasion(&self) -> Option<&StepBody> {
        match self {
            StepBody::Occasion { .. } => Some(self),
            StepBody::Advance { raised, .. } => raised.as_deref(),
            _ => None,
        }
    }

    /// The quest advances that ran before this step's presentations, in
    /// order: the settle an `advance:` ran before raising its occasion, then
    /// the raise of a `judge: before` occasion (dsl 0.24.0 §2).
    pub fn settled(&self) -> impl Iterator<Item = &QuestAdvance> {
        let own: &[QuestAdvance] = match self {
            StepBody::Advance { settled, .. } => settled,
            _ => &[],
        };
        let judged: &[QuestAdvance] = match self.occasion() {
            Some(StepBody::Occasion { judged, .. }) => judged,
            _ => &[],
        };
        own.iter().chain(judged)
    }

    /// What an `advance:` played at the midnights it crossed, in order —
    /// before [`Self::settled`]: each move's settle, the `dayEnd` /
    /// `dayStart` raise's quest advances and presentations, the quests'
    /// answer. Empty for any other step.
    pub fn days_played(&self) -> Vec<Played<'_>> {
        let StepBody::Advance { days, .. } = self else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for d in days {
            out.extend(d.settled.iter().map(Played::Quest));
            if let StepBody::Occasion {
                judged, presented, ..
            } = &*d.occasion
            {
                out.extend(judged.iter().map(Played::Quest));
                out.extend(presented.iter().map(Played::Beat));
            }
            out.extend(d.quests.iter().map(Played::Quest));
        }
        out
    }
}

/// Raise `occasion` (for `target`) as one step: its candidates' verdicts,
/// what it presents (`pick` on a `select: all` occasion; `choose` over the
/// script's), every quest settle after each presentation, and the quests'
/// answer to the raise.
#[allow(clippy::too_many_arguments)]
pub fn run_occasion<F: super::WalkDriver>(
    p: &ExecProject,
    w: &mut World,
    n: usize,
    occasion: &String,
    target: &Option<String>,
    pick: &Option<Pick>,
    choose: &BTreeMap<String, Vec<String>>,
    factory: &mut F,
) -> (StepBody, Vec<QuestAdvance>, Option<PlayHalt>) {
    // dsl 0.21.0 §7a.2: the occasion judges the `on=` objectives of every
    // active quest — for the step's target (dsl 0.23.0 §2) — and fires the
    // `<on event>` handlers of a same-named world event (0.23.1). By default
    // after the presentations (or none — `pick: none` included, dsl 0.22.0
    // §10); under `judge: before` (dsl 0.24.0 §2) first, so the beats'
    // `when` and bodies read the judged quests.
    let judged_here = p.objective_occasions.contains(occasion) || p.world_events.contains(occasion);
    let before = judged_here && p.judges_before(occasion);
    // dsl 0.24.0 §2.1: until the raise, this step's settles defer the `by`
    // of the `on=` objectives it judges — their `done` is judged first.
    if judged_here {
        w.defer_by = Some(
            target
                .as_ref()
                .map_or_else(|| occasion.clone(), |t| format!("{occasion}@{t}")),
        );
    }
    let mut quests = Vec::new();
    let mut judged = Vec::new();
    // dsl 0.28.0 (T1-22): the seam (`terminal:`, the gate) is decided when
    // the occasion is raised — before a `judge: before` judgement — and the
    // raise's own beats are judged under it.
    let seam = RaiseSeam::decide(
        p,
        w,
        occasion,
        target.as_deref(),
        factory.observer(),
    );
    if before {
        // dsl 0.24.0 §2: the quests are judged and settled before the beats;
        // the `<on>` handlers that answer run after them.
        w.defer_handlers = true;
        let (more, s) = raise(
            p,
            w,
            Raise::Occasion(occasion, target.as_deref()),
            factory,
        );
        w.defer_handlers = false;
        judged = more;
        if s.is_some() {
            w.deferred_handlers.clear();
            let body = StepBody::Occasion {
                occasion: occasion.clone(),
                target: target.clone(),
                select: p.select_of(occasion),
                pick: pick.clone(),
                candidates: Vec::new(),
                winner: None,
                decided: false,
                presented: Vec::new(),
                judged,
                not_raised: None,
            };
            return (body, quests, s);
        }
    }
    let select = p.select_of(occasion);
    let mut cands = candidates(
        p,
        w,
        occasion,
        target.as_deref(),
        Some(&seam),
        factory.observer(),
    );
    let halt = if let Some(c) = deciding_unknown(&cands, select) {
        let Verdict::Unknown(detail) = &c.verdict else {
            unreachable!("deciding_unknown returns only unknown verdicts")
        };
        Some(PlayHalt::Incomplete(format!(
            "step {n}: the `when` of {} `{}` ({}) decides the {occasion} outcome but {detail}",
            kind_label(c.kind),
            c.id,
            c.document
        )))
    } else if let Some(Pick::Beat(pk)) = pick {
        match cands.iter().find(|c| &c.id == pk).map(|c| &c.verdict) {
            Some(Verdict::Eligible) => None,
            Some(Verdict::Ineligible(reason)) => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not eligible — {reason}"
            ))),
            _ => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not a candidate of {occasion}"
            ))),
        }
    } else if select == OccasionSelect::All && pick.is_none() {
        let offered: Vec<&str> = cands
            .iter()
            .filter(|c| matches!(c.verdict, Verdict::Eligible))
            .map(|c| c.id.as_str())
            .collect();
        (!offered.is_empty()).then(|| {
            PlayHalt::Error(format!(
                "step {n}: occasion `{occasion}` is `select: all` and offers [{}] — name the \
                 beat the player takes with `pick:` (or `pick: none` to close the list)",
                offered.join(", ")
            ))
        })
    } else {
        None
    };
    let decided = halt.is_none();
    // What the step presents, in order (dsl 0.23.0 §3): the `pick` on
    // `select: all`; otherwise [`presented`] — the `select: first` winner
    // then its eligible `also` beats. dsl 0.28.0 (T2-10): a `select:
    // sequence` raise judges each beat again just before its turn, once an
    // earlier beat of the raise has played.
    let sequence = decided && pick.is_none() && select == OccasionSelect::Sequence;
    let order: Vec<usize> = match (decided, pick) {
        (false, _) | (true, Some(Pick::Pass)) => Vec::new(),
        (true, Some(Pick::Beat(pk))) => {
            cands.iter().position(|c| &c.id == pk).into_iter().collect()
        }
        (true, None) if sequence => (0..cands.len()).collect(),
        (true, None) => presented(select, &cands),
    };
    // Each presentation that plays through settles every quest before the
    // next one (so a `by` deadline is judged after each). One that halts
    // presents and advances nothing further. A `::end` ends only its own
    // presentation (0.23.1): its settle runs, the step's other
    // presentations (`also` riders, a `sequence`) still play, the occasion
    // still judges, and the playthrough goes on with the next step.
    let mut presented_beats: Vec<Presented> = Vec::new();
    // The winner is the main beat: never an `also` rider.
    let mut winner = None;
    let mut stop = halt;
    for i in order {
        if stop.is_some() {
            break;
        }
        if sequence {
            if !presented_beats.is_empty() {
                let now = rejudge(
                    p,
                    w,
                    occasion,
                    target.as_deref(),
                    &cands[i],
                    &seam,
                    factory.observer(),
                );
                if now != cands[i].verdict {
                    cands[i].verdict = now;
                    cands[i].rejudged = true;
                }
            }
            match &cands[i].verdict {
                Verdict::Eligible => {}
                Verdict::Ineligible(_) => continue,
                Verdict::Unknown(detail) => {
                    let c = &cands[i];
                    stop = Some(PlayHalt::Incomplete(format!(
                        "step {n}: the `when` of {} `{}` ({}) decides the {occasion} outcome but \
                         {detail} (judged at its turn, after an earlier beat of this raise)",
                        kind_label(c.kind),
                        c.id,
                        c.document
                    )));
                    break;
                }
            }
        }
        let c = &cands[i];
        let Some(b) = p
            .index
            .beats
            .iter()
            .find(|b| b.id == c.id && b.document == c.document && b.kind == c.kind)
        else {
            continue;
        };
        // dsl 0.27.0 §3: a `for` beat presents for its candidate's member.
        let member = match &c.for_member {
            Some(m) => Some(m.as_str()),
            None => b.answers(occasion, target.as_deref()).flatten(),
        };
        if !c.also && winner.is_none() {
            winner = Some(c.id.clone());
        }
        let (pr, s) = present_with_choose(p, w, b, member, choose, factory);
        presented_beats.push(pr);
        stop = s;
        if stop.is_none() {
            let (more, s) = advance_quests(p, w, factory);
            quests.extend(more);
            stop = s;
        }
    }
    if stop.is_none() && judged_here && !before {
        let (more, s) = raise(p, w, Raise::Occasion(occasion, target.as_deref()), factory);
        quests.extend(more);
        stop = s;
    }
    // dsl 0.24.0 §2: a `judge: before` raise's handlers run after the beats.
    let handlers = std::mem::take(&mut w.deferred_handlers);
    if stop.is_none() && !handlers.is_empty() {
        let (more, s) = run_deferred_handlers(p, w, handlers, factory);
        quests.extend(more);
        stop = s;
    }
    // A step that halted before its raise defers nothing past itself.
    w.defer_by = None;
    let body = StepBody::Occasion {
        occasion: occasion.clone(),
        target: target.clone(),
        select,
        pick: pick.clone(),
        candidates: cands,
        winner,
        decided,
        presented: presented_beats,
        judged,
        not_raised: None,
    };
    (body, quests, stop)
}

/// What a `newRun` did beyond its seed writes, for the transcript.
pub struct NewRunReport {
    /// The seed's write records.
    pub writes: Vec<Json>,
    /// `(id, status)` of every `<quest tier="run">` the new run reset —
    /// only those that had left `unset`.
    pub reset_quests: Vec<(String, String)>,
    /// dsl 0.24.0 §2: the queued `at="nextRun"` accepts this run start
    /// applies (the settle after the reset activates them).
    pub accepted: Vec<String>,
    /// The `prev.run.*` snapshot the ended run left (path, value), in path
    /// order — printed value by value (dsl 0.24.0, T3-10).
    pub prev_run: Vec<(String, Value)>,
    /// Accept-driven run-tier quests (CR N3: only those can be queued with
    /// `at="nextRun"`) the reset found `active` with no objective done or
    /// failed — taken this run, and discarded by the reset.
    pub unjudged: Vec<String>,
}

/// `newRun`: `run.*` state back to its declared defaults, `entry.<id>.read`
/// flags too (run-tier, dsl 0.19.0 §5; a `for` entry's per-member
/// `entry.<id>.readFor.<member>`, dsl 0.28.0), run-tier facts back to the
/// project's seed facts, `<quest tier="run">` quests back to `unset` with
/// their objectives undone (dsl 0.22.0 §7), and `once: run` spending
/// cleared; then the long form's seed (§1.1). `user.*`/`app.*`, user-tier
/// quests, `entry.<id>.everRead`, other facts, `visited` and `once: user`
/// spending persist.
pub fn new_run(p: &ExecProject, w: &mut World, seed: &Writes) -> Result<NewRunReport, String> {
    let run_tier = |path: &str| {
        path.starts_with("run.")
            || entry_flag(path).is_some_and(|(_, flag)| flag == "read")
            || (path.starts_with("entry.") && path.contains(".readFor."))
    };
    // dsl 0.23.0 §6: the ending run's `run.*` values become `prev.run.*`
    // (a path unset at run end stays unset in the mirror).
    let ended: Vec<(String, Value)> = w
        .state
        .iter()
        .filter_map(|(k, v)| lute_manifest::semantics::cel_paths::prev_run_path(k).map(|prev| (prev, v.clone())))
        .collect();
    let prev_run = ended.clone();
    w.state
        .retain(|k, _| !run_tier(k) && !lute_manifest::semantics::cel_paths::is_prev_path(k));
    w.state.extend(ended);
    for (path, e) in &p.state_table {
        if run_tier(path) {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                w.state.insert(path.clone(), v);
            }
        }
    }
    w.facts.retain(|(rel, _)| !p.run_relations.contains(rel));
    for f in &p.seed_facts {
        if p.run_relations.contains(&f.0) {
            w.facts.insert(f.clone());
        }
    }
    let unjudged: Vec<String> = p
        .run_quests
        .iter()
        .filter(|(id, objectives)| {
            p.accept_driven.contains(*id)
                && w.quests.get(*id).map(String::as_str) == Some("active")
                && objectives.iter().all(|oid| {
                    let judged = |flag: &str| {
                        w.state.get(&format!("quest.{id}.objectives.{oid}.{flag}"))
                            == Some(&Value::Bool(true))
                    };
                    !judged("done") && !judged("failed")
                })
                && !w
                    .failed_objectives
                    .iter()
                    .any(|k| k.starts_with(&format!("{id}.")))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut reset_quests = Vec::new();
    for (id, objectives) in &p.run_quests {
        if let Some(status) = crate::cadence::reset_quest(w, id, objectives) {
            reset_quests.push((id.clone(), status));
        }
    }
    w.spent_run.clear();
    // dsl 0.27.0 §4: a new run starts a run-tier clock over — its day back
    // to the default, so its `once: day|slot|week` spends and its end go
    // too. A clock whose day path outlives the run (`user.*`, `app.*`) keeps
    // its position, so it keeps both.
    let clock_restarts = p
        .index
        .clock
        .as_ref()
        .filter(|c| crate::clock::restarts_each_run(c));
    if let Some(clock) = clock_restarts {
        w.spent_at.clear();
        crate::clock::set_ended(clock, &mut w.state, false);
    }
    // A `spentBy` beat spent this run is spendable again (its `user` /
    // `season:<name>` latches stay; its clock-period ones go with the clock).
    crate::cadence::new_run_latches(p, w, clock_restarts.is_some());
    // dsl 0.24.0 §2: acceptances queued for the next run apply now, after
    // the reset, so a run-tier quest taken between runs survives it.
    let accepted = std::mem::take(&mut w.next_run_accepts);
    for id in &accepted {
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    Ok(NewRunReport {
        writes: apply_writes(w, seed)?,
        reset_quests,
        accepted,
        prev_run,
        unjudged,
    })
}

/// Apply an `engine:` step's (or a `newRun` seed's) writes — `state:`, then
/// `facts:`, then `retract:` — returning one transcript record per write in
/// the runner's own record shapes (`set` / `assert` / `retract`).
pub fn apply_writes(w: &mut World, writes: &Writes) -> Result<Vec<Json>, String> {
    let mut records = Vec::new();
    for (path, write) in &writes.state {
        let v = match write {
            Write::Set(v) => v.clone(),
            Write::Add(d) => match w.state.get(path) {
                Some(Value::Int(n)) => n.checked_add(*d as i64).map(Value::Int),
                Some(Value::Double(n)) => Some(Value::Double(n + d)),
                _ => None,
            }
            .ok_or_else(|| format!("`{path}` has no number value to add {d} to"))?,
        };
        records.push(json!({ "kind": "set", "path": path, "value": value_to_json(&v) }));
        w.state.insert(path.clone(), v);
    }
    for f in &writes.facts {
        records.push(json!({ "kind": "assert", "fact": render_fact(f) }));
        w.facts.insert(f.clone());
    }
    for f in &writes.retract {
        let held = w.facts.remove(f);
        records.push(json!({ "kind": "retract", "pattern": render_fact(f), "held": held }));
    }
    // dsl 0.26.0 §7 (T2-9): the next quest settle activates them. A quest
    // already active or settled is not taken again: the transcript says so
    // and nothing changes.
    for id in &writes.accept {
        if let Some(status) = w.quests.get(id).filter(|s| *s != "unset") {
            records.push(json!({ "kind": "acceptIgnored", "quest": id, "status": status }));
            continue;
        }
        records.push(json!({ "kind": "accept", "quest": id, "by": "engine" }));
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    Ok(records)
}
