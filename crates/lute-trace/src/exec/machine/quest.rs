//! The quest lifecycle (quest-lifecycle.md): quest and `<on>` handler
//! declarations, activation and accepts, objective judging (`done`, `on=`
//! occasions, `by` / `until` deadlines), failure cascades, reward grants
//! and handler firing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{json, Value as Json};

use super::format::value_to_json;
use super::{addr, fold_op, Code, Machine, Site};
use crate::eval::Read;
use crate::exec::driver::{Driver, SiteKind};
use crate::exec::expr::{Expr, Slot};
use crate::Value;

/// A parsed quest declaration head (quest-lifecycle.md).
struct QuestDecl {
    id: String,
    /// Activation predicate; `None` ⇒ activates at start / accept-driven.
    start: Option<Arc<Slot>>,
    /// Failure predicate, evaluated before derived completion.
    fail: Option<Arc<Slot>>,
    objectives: Vec<Obj>,
    /// dsl 0.16.0 §3 D-D: owner-declared `<reward/>` entries in
    /// declaration order. Grants fire at fresh `complete`/`failed`
    /// transitions (spec §3 D-D) — [`Machine::emit_quest_grants`] filters
    /// by the per-entry `on=` marker.
    rewards: Vec<RewardRec>,
    /// dsl 0.24.0 §2: `QuestCmd.activate == "accept"` — a subquest child
    /// that activates only once accepted while its parent is active.
    accept_activated: bool,
    /// dsl 0.24.0 §2: `QuestCmd.complete == "any"` — ANY required
    /// objective done completes the quest.
    complete_any: bool,
}

struct Obj {
    id: String,
    /// Blank (unknown, no atom) when the objective declares none.
    done: Arc<Slot>,
    optional: bool,
    /// `addr` of the completion body segment, or `None` (empty body).
    body: Option<String>,
    /// `ObjectiveEntry.quest` — the referenced child quest id (subquest
    /// design 2026-08-31 §3), or `None` for an authored-`done` objective.
    quest: Option<String>,
    /// dsl 0.16.0 §3 D-D: owner-declared `<reward/>` entries in
    /// declaration order. Fires ONCE at fresh `done` (spec §3 D-D),
    /// BEFORE any quest-level grant fires.
    rewards: Vec<RewardRec>,
    /// dsl 0.21.0 §7a.2: `ObjectiveEntry.on` — the occasion at which this
    /// objective's `done` is judged; `None` ⇒ judged continuously.
    on: Option<String>,
    /// dsl 0.23.0 §2: `ObjectiveEntry.by` — while not done, the first time
    /// it is true the objective fails.
    by: Option<Arc<Slot>>,
    /// dsl 0.24.0 §2.1: `ObjectiveEntry.until` — judged only when the
    /// objective's occasion (and target) is raised, after its `done`.
    until: Option<Arc<Slot>>,
    /// dsl 0.23.0 §2: `ObjectiveEntry.target` — with `on`, judged only by a
    /// raise for this target.
    target: Option<String>,
}

/// One `RewardEntry` (`ir.rs`, dsl 0.16.0 §3) parsed straight off the
/// artifact JSON, with `outcome` normalized to `Option<String>` (only ever
/// `Some("failed")` after the checker's `E-REWARD-ATTR` gate — the runner
/// filters strictly on that value). `when` is the raw CEL fragment
/// evaluated at the grant instant via [`Machine::truthy`]; `None` here
/// means an unconditional grant.
struct RewardRec {
    kind: String,
    target: Option<String>,
    amount: Option<i64>,
    amount_min: Option<i64>,
    amount_max: Option<i64>,
    when: Option<Arc<Slot>>,
    outcome: Option<String>,
    /// dsl 0.23.0 §8: `RewardEntry.credits` — the state path a grant adds
    /// its (scalar) amount to.
    credits: Option<String>,
}

/// dsl 0.16.0 §3 D-D: which lifecycle transition is firing declarative
/// rewards. [`GrantEvent::Objective`] fires every objective-level reward
/// (objective entries never carry `outcome=`, spec §2); [`GrantEvent::Complete`]
/// fires quest-level rewards whose `outcome=` is unset (the default
/// "on complete"); [`GrantEvent::Failed`] fires quest-level rewards whose
/// `outcome == "failed"` (both authored-`fail` and §2.3 cascade paths hit this
/// arm, with the transcript's `onFailed: true` marking the transition kind).
#[derive(Clone, Copy)]
enum GrantEvent {
    Objective,
    Complete,
    Failed,
}

/// A parsed `<on>` handler. `quest` is the ENCLOSING quest's id, recovered
/// from stream order (an `on` record is emitted inside its quest's walk, so
/// it follows its own quest record and precedes the next one). Lifecycle
/// events (`questActive`/`questComplete`/`questFailed`) fire only for their
/// own enclosing quest (quest-lifecycle.md); world events are unscoped.
struct Handler {
    /// The `on` record's `addr`.
    addr: String,
    event: String,
    when: Option<Arc<Slot>>,
    body: String,
    quest: Option<String>,
    /// dsl 0.24.0 §2: `OnCmd.target` — fires only for a raise of the
    /// same-named occasion for this target.
    target: Option<String>,
}

impl<D: Driver> Machine<D> {
    /// The quest artifact's declarations: its quests, its `<on>` handlers
    /// (each with its enclosing quest), and the body-segment boundaries —
    /// every objective body and every `<on>` body.
    fn quest_program(&self) -> (Vec<QuestDecl>, Vec<Handler>, Vec<usize>) {
        let mut quests: Vec<QuestDecl> = Vec::new();
        let mut handlers: Vec<Handler> = Vec::new();
        for cmd in &self.code.commands {
            match cmd.get("kind").and_then(Json::as_str) {
                Some("quest") => quests.push(parse_quest(&self.code, cmd)),
                Some("on") => {
                    handlers.push(Handler {
                        event: cmd
                            .get("event")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        when: self.code.slot(cmd.get("when")),
                        body: cmd
                            .get("body")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        // Stream order recovers the enclosing quest: the `on`
                        // record is emitted inside its quest's walk, after the
                        // quest declaration head (stage.rs `walk_quest`).
                        quest: quests.last().map(|q| q.id.clone()),
                        addr: addr(cmd).to_string(),
                        target: cmd.get("target").and_then(Json::as_str).map(str::to_string),
                    });
                }
                _ => {}
            }
        }
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
        (quests, handlers, seg_starts)
    }

    /// `lute play` (dsl 0.24.0 §2): run the `<on>` handler bodies a `judge:
    /// before` raise answered ([`Machine::with_deferred_handlers`]) — after
    /// the occasion's beats, in the order they fired. Their `when` was
    /// decided when they fired; a `::end` in one ends the rest.
    pub fn run_deferred_handlers(&mut self, bodies: &[String]) -> Result<(), String> {
        self.quest_resume = true;
        let (_, _, seg_starts) = self.quest_program();
        for body in bodies {
            if self.stopped() {
                break;
            }
            self.run_segment(body, &seg_starts);
        }
        self.store.derive();
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    pub(super) fn run_quest(&mut self) {
        let (quests, handlers, seg_starts) = self.quest_program();

        // A fresh walk (`lute run`, `lute trace`) starts every quest `unset`
        // — unless the seed holds it `active` / `complete` / `failed` (a
        // mock's `quests:`, dsl 0.26.0 §7 T3-5): it was there before the
        // walk, as a play's save holds it, so it starts there and no
        // `questActive` fires (D9). A resumed one (`lute play`) keeps each
        // carried status and registers only a quest it has never seen —
        // populating its `quest.<id>.state` as `unset` so a beat `when` over
        // it decides instead of reading an unset path.
        for q in &quests {
            if !self.quest_resume {
                let seeded = match self.store.values.get(&format!("quest.{}.state", q.id)) {
                    Some(Value::Str(s))
                        if matches!(s.as_str(), "active" | "complete" | "failed") =>
                    {
                        Some(s.clone())
                    }
                    _ => None,
                };
                match seeded {
                    Some(state) => {
                        self.driver.observe(json!({
                            "kind": "quest", "quest": q.id, "outcome": state, "guard": "seeded",
                        }));
                        self.quest_status.insert(q.id.clone(), state);
                    }
                    None => {
                        self.quest_status.insert(q.id.clone(), "unset".to_string());
                    }
                }
            } else if !self.quest_status.contains_key(&q.id) {
                self.quest_status.insert(q.id.clone(), "unset".to_string());
                self.write(&format!("quest.{}.state", q.id), Value::Str("unset".into()));
            }
        }

        // Parent→child edges (subquest design 2026-08-31 §2.4/§3): a child is
        // any quest some objective references via `ObjectiveEntry.quest`.
        let parent_of: BTreeMap<String, String> = quests
            .iter()
            .flat_map(|q| {
                q.objectives
                    .iter()
                    .filter_map(|o| o.quest.clone().map(|c| (c, q.id.clone())))
            })
            .collect();

        // Activation (quest-lifecycle.md §Activation). A REFERENCED child is
        // parent-activation-driven (§2.4): it never activates at walk start
        // and is not accept-driven — `reevaluate` activates it once its parent
        // is `active` (no `start`: immediately; with `start`: when the
        // predicate holds while the parent is active). An unreferenced quest
        // with no `start` is ACCEPT-DRIVEN (dsl 0.21.0 §7a.3): it stays
        // `unset` until a mock `accepts:` entry or an `accept` record names
        // it — the rule `lute trace` (dsl 0.4.0 §4.4) always applied.
        for q in &quests {
            if parent_of.contains_key(&q.id)
                || self.quest_status.get(&q.id).map(String::as_str) != Some("unset")
            {
                continue;
            }
            let started = match &q.start {
                None => None,
                Some(cond) => Some(self.judge(cond, Site::quest(SiteKind::QuestStart, &q.id, &q.id))),
            };
            if self.stopped() {
                return;
            }
            let start_raw = q.start.as_deref().map(Slot::raw);
            match started {
                Some(Some(true)) => {
                    self.activate_quest(&q.id, start_raw, false, &handlers, &seg_starts)
                }
                _ if self.is_accepted(&q.id) => {
                    self.activate_quest(&q.id, None, true, &handlers, &seg_starts)
                }
                Some(Some(false)) => self.observe_waiting(&q.id, "never", start_raw),
                None => self.observe_waiting(&q.id, "awaiting accept", None),
                Some(None) => {}
            }
        }

        // Track which objectives have completed (monotone). A resumed walk
        // seeds this from the carried `quest.<id>.objectives.<oid>.done`, so an
        // objective completed by an earlier advance never completes again.
        let mut done: BTreeSet<(usize, usize)> = BTreeSet::new();
        if self.quest_resume {
            for (qi, q) in quests.iter().enumerate() {
                for (oi, o) in q.objectives.iter().enumerate() {
                    let path = format!("quest.{}.objectives.{}.done", q.id, o.id);
                    if self.store.values.get(&path) == Some(&Value::Bool(true)) {
                        done.insert((qi, oi));
                    }
                }
            }
        }
        // dsl 0.24.0 §2.1: the settles before this walk's raises defer the
        // `by` of the `on=` objectives those raises judge.
        self.defer_by.extend(self.seed.occasions.iter().cloned());
        self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);

        // Mock events fire in order; each re-evaluates the lifecycle. An `end`
        // inside a handler/objective body ends the WALK (dsl 0.8.0), so no
        // later event is delivered — nothing downstream of the terminator runs.
        let events: Vec<String> = self.seed.events.clone();
        for ev in events {
            if self.stopped() {
                break;
            }
            self.fire_event(&ev, None, None, &handlers, &seg_starts);
            self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);
        }

        // dsl 0.21.0 §7a.2: occasions are raised in order after the walk
        // settles; each judges the `on="<occasion>"` objectives of every
        // active quest, then the lifecycle settles again. `lute run` records
        // the raise; `lute play`'s step header already names it. 0.23.1: a
        // raise also fires the same-named world event — the `<on event>`
        // handlers run first, then the occasion judges. (A lifecycle event
        // name is never raised this way: the runner fires those itself.)
        let occasions: Vec<String> = self.seed.occasions.clone();
        for occasion in &occasions {
            if self.stopped() {
                break;
            }
            let (name, target) = crate::split_occasion(occasion);
            self.bind_occasion_target(target);
            if !self.quest_resume {
                let mut rec = json!({ "kind": "occasion", "occasion": name });
                if let Some(t) = target {
                    rec["target"] = json!(t);
                }
                self.driver.emit(rec);
            }
            if !lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name) {
                // dsl 0.24.0 §2: an `<on target>` answers only a raise for
                // its target.
                self.fire_event(name, None, target, &handlers, &seg_starts);
                if self.stopped() {
                    self.bind_occasion_target(None);
                    break;
                }
            }
            self.judge_occasion(occasion, &quests, &seg_starts, &mut done);
            self.reevaluate(&quests, &parent_of, &handlers, &seg_starts, &mut done);
            self.bind_occasion_target(None);
        }

        // Incomplete if an active quest is stuck on an undecidable required
        // objective (a missing mock left the `done` predicate unknown). An
        // `end` record makes this moot: the author declared the walk finished,
        // so an unsettled objective is a deliberate outcome, not a missing mock.
        if self.stopped() {
            if !self.quest_resume {
                self.observe_spent_accepts(&quests, &parent_of);
            }
            return;
        }
        if !self.quest_resume {
            self.observe_spent_accepts(&quests, &parent_of);
        }
        for (qi, q) in quests.iter().enumerate() {
            if self.quest_status.get(&q.id).map(String::as_str) == Some("active") {
                for (oi, o) in q.objectives.iter().enumerate() {
                    if o.optional
                        || done.contains(&(qi, oi))
                        || self
                            .failed_objectives
                            .contains(&format!("{}.{}", q.id, o.id))
                    {
                        continue;
                    }
                    // An `on=` objective is judged only at its occasion (for
                    // its target, dsl 0.23.0 §2); one this walk never raised
                    // is not stuck, it is waiting.
                    let judged = o.on.as_ref().is_none_or(|on| {
                        occasions
                            .iter()
                            .any(|r| crate::raise_judges(r, on, o.target.as_deref()))
                    });
                    // dsl 0.23.0 §2 / 0.24.0 §2.1: an undecidable deadline
                    // could still fail the quest — as stuck as an undecidable
                    // `done`. `by` is judged at every settle; `until` only
                    // where the objective is judged.
                    let unknown = |this: &mut Self, slot: &Option<Arc<Slot>>| {
                        slot.as_ref()
                            .is_some_and(|c| this.eval_value(c) == Value::Unknown)
                    };
                    let (key, stuck) = if judged && self.eval_value(&o.done) == Value::Unknown {
                        ("done", true)
                    } else if unknown(self, &o.by) || (judged && unknown(self, &o.until)) {
                        ("failed", true)
                    } else {
                        ("done", false)
                    };
                    if stuck {
                        self.incomplete = true;
                        // `lute play` names the stuck objective in its halt;
                        // `lute run`'s transcript is unchanged.
                        if self.quest_resume {
                            self.driver.emit(json!({
                                "kind": "objective",
                                "quest": q.id,
                                "objective": o.id,
                                key: Json::Null,
                            }));
                        }
                    }
                }
            }
        }
    }

    /// `lute play` (dsl 0.21.0 §6, D-H): advance this quest artifact's
    /// lifecycle over carried-over state, facts and statuses exactly as
    /// [`Machine::run`] settles it for `lute run` — activation, objectives,
    /// `fail` before completion, `<on>` handlers, `<reward>` grants — but
    /// RESUMED ([`Machine::quest_resume`]): nothing already active, terminal or
    /// done transitions again. Called once per quest artifact after every
    /// presentation, so later beat conditions see real quest progress.
    pub fn advance_quests(&mut self) -> Result<(), String> {
        self.quest_resume = true;
        self.run_quest();
        self.store.derive();
        match self.fatal.take() {
            Some(msg) => Err(msg),
            None => Ok(()),
        }
    }

    /// Re-evaluate the quest lifecycle to a fixpoint: referenced-child
    /// activation (subquest §2.4), then per active quest objectives
    /// (monotone), then `fail` before derived completion (quest-lifecycle.md
    /// §Re-evaluation cadence). A terminal transition cascades every
    /// still-`active` child to `failed` (subquest §2.3, recursive) — the one
    /// downward rule a per-artifact compile cannot synthesize.
    fn reevaluate(
        &mut self,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
        handlers: &[Handler],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        let mut changed = true;
        let mut rounds = 0;
        while changed && !self.stopped() && rounds < quests.len() * 8 + 16 {
            changed = false;
            rounds += 1;
            // 0. referenced-child activation (§2.4): a pending child whose
            // parent is `active` activates — immediately without `start`, or
            // when its `start` predicate holds (evaluated only while the
            // parent is active). dsl 0.24.0 §2: an `activate="accept"`
            // child additionally waits for an accept naming it. An
            // unreferenced accept-driven quest activates once an `accept`
            // record this walk ran names it (dsl 0.21.0 §7a.3 — an accept
            // inside a handler/objective body).
            for q in quests {
                if self.quest_status.get(&q.id).map(String::as_str) != Some("unset") {
                    continue;
                }
                let activate = match parent_of.get(&q.id) {
                    Some(parent) => {
                        if self.quest_status.get(parent).map(String::as_str) != Some("active") {
                            continue;
                        }
                        match &q.start {
                            None if q.accept_activated => {
                                if self.is_accepted(&q.id) {
                                    Some((None, true))
                                } else {
                                    self.observe_waiting(&q.id, "awaiting accept", None);
                                    None
                                }
                            }
                            None => Some((None, false)),
                            Some(cond) => {
                                let site = Site::quest(SiteKind::QuestStart, &q.id, &q.id);
                                match self.judge(cond, site) {
                                    Some(true) => Some((Some(cond.raw()), false)),
                                    Some(false) => {
                                        self.observe_waiting(&q.id, "never", Some(cond.raw()));
                                        None
                                    }
                                    None => None,
                                }
                            }
                        }
                    }
                    None => (q.start.is_none() && self.is_accepted(&q.id)).then_some((None, true)),
                };
                if self.stopped() {
                    return;
                }
                if let Some((guard, by_accept)) = activate {
                    self.activate_quest(&q.id, guard, by_accept, handlers, seg_starts);
                    changed = true;
                }
            }
            for (qi, q) in quests.iter().enumerate() {
                if self.stopped() {
                    return;
                }
                if self.quest_status.get(&q.id).map(String::as_str) != Some("active") {
                    continue;
                }
                // 1. objectives (monotone; body plays once). An `on=`
                // objective is judged only at its occasion
                // ([`Machine::judge_occasion`]), never continuously; a failed
                // one never again.
                for (oi, o) in q.objectives.iter().enumerate() {
                    if o.on.is_some()
                        || o.id.is_empty()
                        || done.contains(&(qi, oi))
                        || self
                            .failed_objectives
                            .contains(&format!("{}.{}", q.id, o.id))
                    {
                        continue;
                    }
                    if self.judge_done(q, qi, oi, seg_starts, done) {
                        changed = true;
                    }
                    if self.stopped() {
                        return;
                    }
                }
                // 1b. dsl 0.23.0 §2, 0.24.0 §2.1: deadlines, after the
                // objectives were judged (`done` wins a tie) — a not-done
                // objective whose `by` is true fails the first time, `on=`
                // or not: a deadline is a moment, not a place. (`until` is
                // judged only at the occasion, [`Machine::judge_occasion`].)
                // An `on=` objective a raise still to come in this step
                // judges waits for it: its `done` is judged first.
                for (oi, o) in q.objectives.iter().enumerate() {
                    let deferred = o.on.as_deref().is_some_and(|on| {
                        self.defer_by
                            .iter()
                            .any(|r| crate::raise_judges(r, on, o.target.as_deref()))
                    });
                    if !deferred {
                        changed |= self.judge_deadline(q, qi, oi, done, "by");
                    }
                    if self.stopped() {
                        return;
                    }
                }
                // 2. fail BEFORE derived completion (§6.3 precedence): a
                // required objective whose `by` failed it (in this settle or
                // at the raise before it), a required subquest that failed
                // (dsl 0.28.0: `failedBy: subquest`), or an authored `fail`.
                // dsl 0.24.0 §2: a `complete="any"` quest is not failed by
                // one missed alternative — its synthesized `fail` fails it
                // once every required objective has failed.
                let missed = if q.complete_any {
                    None
                } else {
                    q.objectives.iter().find_map(|o| {
                        if o.optional {
                            return None;
                        }
                        // The objective's own miss (`by` / `until`) first —
                        // a subquest objective has deadlines too.
                        let key = format!("{}.{}", q.id, o.id);
                        if self.failed_objectives.contains(&key) {
                            let kind = self.objective_failed_by.get(&key).copied().unwrap_or("by");
                            let text = if kind == "until" { &o.until } else { &o.by };
                            return Some((kind, text.as_ref().map(|s| s.raw.clone())));
                        }
                        let child = o.quest.as_ref()?;
                        let failed =
                            self.quest_status.get(child).map(String::as_str) == Some("failed");
                        failed
                            .then(|| ("subquest", Some(format!("quest.{child}.state == 'failed'"))))
                    })
                };
                let failed_by = match missed {
                    Some((kind, text)) => Some((kind, text)),
                    None => match &q.fail {
                        Some(fail) => {
                            let site = Site::quest(SiteKind::QuestFail, &q.id, &q.id);
                            (self.judge(fail, site) == Some(true))
                                .then(|| ("fail", Some(fail.raw.clone())))
                        }
                        None => None,
                    },
                };
                if self.stopped() {
                    return;
                }
                if let Some((reason, guard)) = failed_by {
                    self.observe_quest(&q.id, "failed", guard.as_deref());
                    self.set_quest_failed(&q.id, reason);
                    // dsl 0.16.0 §3 D-D: fresh `failed` → grant
                    // `outcome="failed"` quest rewards BEFORE `questFailed`
                    // handlers and BEFORE the §2.3 downward cascade.
                    self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Failed);
                    self.fire_event("questFailed", Some(&q.id), None, handlers, seg_starts);
                    self.cascade_children(
                        &q.id, "cascade", quests, parent_of, handlers, seg_starts,
                    );
                    changed = true;
                    continue;
                }
                // 3. derived completion: all non-optional objectives done —
                // or, for `complete="any"` (dsl 0.24.0 §2), any one of them.
                let complete = if q.complete_any {
                    q.objectives
                        .iter()
                        .enumerate()
                        .any(|(oi, o)| !o.optional && done.contains(&(qi, oi)))
                } else {
                    q.objectives
                        .iter()
                        .enumerate()
                        .all(|(oi, o)| o.optional || done.contains(&(qi, oi)))
                };
                if complete {
                    self.observe_quest(&q.id, "complete", None);
                    self.set_quest_state(&q.id, "complete", None);
                    // dsl 0.16.0 §3 D-D: fresh `complete` → grant this
                    // quest's outcome-less rewards BEFORE `questComplete`
                    // handlers play.
                    self.emit_grants(&q.id, None, &q.rewards, GrantEvent::Complete);
                    self.fire_event("questComplete", Some(&q.id), None, handlers, seg_starts);
                    // dsl 0.24.0 §2: the alternatives an `any` quest did not
                    // take are superseded, not cascaded.
                    let reason = if q.complete_any {
                        "superseded"
                    } else {
                        "cascade"
                    };
                    self.cascade_children(&q.id, reason, quests, parent_of, handlers, seg_starts);
                    changed = true;
                }
            }
        }
    }

    /// `→ active`: the transition (reserved state, status, `quest` record),
    /// the observation `lute trace` reports (`guard`: the `start` that held;
    /// `forced`: activated by an accept), then the quest's `questActive`
    /// handlers.
    fn activate_quest(
        &mut self,
        id: &str,
        guard: Option<&str>,
        by_accept: bool,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        // The first activation in a save is instance 1. Reset paths advance
        // this value before returning the quest to `unset`.
        self.quest_instances.entry(id.to_string()).or_insert(1);
        self.driver.observe(json!({
            "kind": "quest", "quest": id, "outcome": "active",
            "guard": guard.map(str::trim), "forced": by_accept,
        }));
        self.set_quest_state(id, "active", None);
        self.fire_event("questActive", Some(id), None, handlers, seg_starts);
    }

    /// A quest transition `lute trace` reports as a decision.
    fn observe_quest(&mut self, id: &str, outcome: &str, guard: Option<&str>) {
        self.driver.observe(json!({
            "kind": "quest", "quest": id, "outcome": outcome, "guard": guard.map(str::trim),
        }));
    }

    /// A quest that did not activate — `never` (its `start` decided false)
    /// or `awaiting accept` — observed once per walk.
    fn observe_waiting(&mut self, id: &str, outcome: &str, guard: Option<&str>) {
        if self.observed_waiting.insert(id.to_string()) {
            self.observe_quest(id, outcome, guard);
        }
    }

    /// dsl 0.24.0 §2 (ER N15): an accept of an `activate="accept"` child
    /// that the walk left `unset` because its parent was not active when
    /// it came — spent without effect, observed so `lute trace` can say so.
    fn observe_spent_accepts(
        &mut self,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
    ) {
        for q in quests.iter().filter(|q| q.accept_activated) {
            let Some(parent) = parent_of.get(&q.id) else {
                continue;
            };
            if !self.is_accepted(&q.id)
                || self.quest_status.get(&q.id).map(String::as_str) != Some("unset")
            {
                continue;
            }
            let why = match self.quest_status.get(parent).map(String::as_str) {
                Some("complete") => "already complete",
                Some("failed") => "already failed",
                Some("active") => continue,
                _ => "never active",
            };
            self.driver.observe(json!({
                "kind": "acceptSpent", "quest": q.id, "parent": parent, "why": why,
            }));
        }
    }

    /// Judge objective `oi`'s `done` (continuous or at its occasion): a
    /// fresh `true` completes it ([`Machine::complete_objective`]); `false`
    /// is observed pending; undecided is an [`SiteKind::ObjectiveDone`] site.
    /// `true` when it completed now.
    fn judge_done(
        &mut self,
        q: &QuestDecl,
        qi: usize,
        oi: usize,
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) -> bool {
        let o = &q.objectives[oi];
        let site = Site::quest(SiteKind::ObjectiveDone, &o.id, &q.id);
        match self.judge(&o.done, site) {
            Some(true) => {
                self.complete_objective(q, qi, oi, seg_starts, done);
                true
            }
            Some(false) => {
                self.driver.observe(json!({
                    "kind": "objective", "quest": q.id, "objective": o.id,
                    "outcome": "pending", "guard": o.done.raw.trim(),
                }));
                false
            }
            None => false,
        }
    }

    /// A fresh `done` (monotone — `done` records it; the body plays once):
    /// write `quest.<id>.objectives.<oid>.done`, record it, fire the
    /// objective's rewards (dsl 0.16.0 §3 D-D: BEFORE the body runs, and
    /// before any quest-level grant or `questComplete` handler), then play
    /// the completion body.
    fn complete_objective(
        &mut self,
        q: &QuestDecl,
        qi: usize,
        oi: usize,
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        let o = &q.objectives[oi];
        done.insert((qi, oi));
        self.driver.observe(json!({
            "kind": "objective", "quest": q.id, "objective": o.id,
            "outcome": "done", "guard": o.done.raw.trim(),
        }));
        self.write(
            &format!("quest.{}.objectives.{}.done", q.id, o.id),
            Value::Bool(true),
        );
        self.driver.emit(json!({
            "kind": "objective",
            "quest": q.id,
            "objective": o.id,
            "done": true,
        }));
        self.emit_grants(&q.id, Some(&o.id), &o.rewards, GrantEvent::Objective);
        if let Some(body) = &o.body {
            self.run_segment(body, seg_starts);
        }
    }

    /// dsl 0.21.0 §7a.2: raise `occasion` — every ACTIVE quest judges its
    /// not-yet-done `on="<occasion>"` objectives, document order. The raise
    /// is `name` or `name@target` (dsl 0.23.0 §2): an objective with a
    /// `target` is judged only by a raise for it; a failed one never. Its
    /// `until` is judged here, after its `done`; its `by` — deferred by the
    /// settles of the step until this raise ([`Machine::defer_by`]) — at the
    /// caller's settle right after (`fail` before completion), so `done`
    /// wins over a deadline that came true in this step.
    fn judge_occasion(
        &mut self,
        occasion: &str,
        quests: &[QuestDecl],
        seg_starts: &[usize],
        done: &mut BTreeSet<(usize, usize)>,
    ) {
        self.defer_by.retain(|r| r != occasion);
        for (qi, q) in quests.iter().enumerate() {
            let judged: Vec<usize> = q
                .objectives
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.on.as_deref()
                        .is_some_and(|on| crate::raise_judges(occasion, on, o.target.as_deref()))
                })
                .map(|(oi, _)| oi)
                .collect();
            for &oi in &judged {
                if self.stopped()
                    || self.quest_status.get(&q.id).map(String::as_str) != Some("active")
                {
                    break;
                }
                let o = &q.objectives[oi];
                if done.contains(&(qi, oi))
                    || self
                        .failed_objectives
                        .contains(&format!("{}.{}", q.id, o.id))
                {
                    continue;
                }
                self.judge_done(q, qi, oi, seg_starts, done);
            }
            for &oi in &judged {
                if self.stopped()
                    || self.quest_status.get(&q.id).map(String::as_str) != Some("active")
                {
                    break;
                }
                self.judge_deadline(q, qi, oi, done, "until");
            }
        }
    }

    /// dsl 0.23.0 §2, 0.24.0 §2.1: judge objective `oi`'s deadline `kind` —
    /// `by` (every settle) or `until` (at its occasion's raise) — skipped
    /// when it has none, is done, or already failed. The first time it is
    /// true the objective fails (recorded with the kind as its `failedBy`);
    /// a failed required objective fails its quest at the next settle.
    /// `true` when it failed now.
    fn judge_deadline(
        &mut self,
        q: &QuestDecl,
        qi: usize,
        oi: usize,
        done: &BTreeSet<(usize, usize)>,
        kind: &'static str,
    ) -> bool {
        let o = &q.objectives[oi];
        let slot = if kind == "until" { &o.until } else { &o.by };
        let Some(cond) = slot else { return false };
        let key = format!("{}.{}", q.id, o.id);
        if done.contains(&(qi, oi)) || self.failed_objectives.contains(&key) {
            return false;
        }
        let site = Site::quest(
            if kind == "until" {
                SiteKind::ObjectiveUntil
            } else {
                SiteKind::ObjectiveBy
            },
            &o.id,
            &q.id,
        );
        if self.judge(cond, site) != Some(true) {
            return false;
        }
        self.driver.observe(json!({
            "kind": "objective", "quest": q.id, "objective": o.id,
            "outcome": "failed", "guard": cond.raw.trim(),
        }));
        self.record_objective_failure(&q.id, &o.id, kind);
        self.driver.emit(json!({
            "kind": "objective",
            "quest": q.id,
            "objective": o.id,
            "failed": true,
            "failedBy": kind,
        }));
        true
    }

    /// dsl 0.24.0 §2: objective `objective` of `quest` failed for `kind`
    /// (`by` / `until`): never judged again, the reserved
    /// `quest.<id>.objectives.<oid>.failed` reads `true`, and a required
    /// one fails its quest with that `failedBy`.
    fn record_objective_failure(&mut self, quest: &str, objective: &str, kind: &'static str) {
        let key = format!("{quest}.{objective}");
        self.write(
            &format!("quest.{quest}.objectives.{objective}.failed"),
            Value::Bool(true),
        );
        self.objective_failed_by.insert(key.clone(), kind);
        self.failed_objectives.insert(key);
    }

    /// Downward cascade (subquest design 2026-08-31 §2.3): on `terminal`'s
    /// terminal transition, every still-`active` child transitions to
    /// `failed` and fires ITS OWN `questFailed` handlers; recursive (a
    /// cascaded failure is itself a terminal transition). A required child
    /// cannot be `active` when its parent completes (its completion is part
    /// of the parent's derived completion), so the `complete` arm only ever
    /// fails running optionals. dsl 0.24.0 §2: `reason` is the direct
    /// children's `failedBy` — `cascade`, or `superseded` when a
    /// `complete="any"` parent completed; deeper levels are `cascade`.
    fn cascade_children(
        &mut self,
        terminal: &str,
        reason: &'static str,
        quests: &[QuestDecl],
        parent_of: &BTreeMap<String, String>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let mut stack = vec![(terminal.to_string(), reason)];
        while let Some((parent, reason)) = stack.pop() {
            if self.stopped() {
                return;
            }
            let children: Vec<String> = quests
                .iter()
                .filter(|q| {
                    parent_of.get(&q.id) == Some(&parent)
                        && self.quest_status.get(&q.id).map(String::as_str) == Some("active")
                })
                .map(|q| q.id.clone())
                .collect();
            for child in children {
                // dsl 0.16.0 §3 D-D: cascade-fail IS a fresh `failed`
                // transition (§2.3). Grant `outcome="failed"` rewards on the
                // cascaded child before firing its own `questFailed` — same
                // ordering as an authored `fail`, so a consumer reads no
                // structural difference.
                let child_rewards = quests
                    .iter()
                    .find(|q| q.id == child)
                    .map(|q| q.rewards.as_slice())
                    .unwrap_or(&[]);
                self.observe_quest(
                    &child,
                    "failed",
                    Some(&format!("{reason} from quest.{parent}")),
                );
                self.set_quest_failed(&child, reason);
                self.emit_grants(&child, None, child_rewards, GrantEvent::Failed);
                self.fire_event("questFailed", Some(&child), None, handlers, seg_starts);
                stack.push((child, "cascade"));
            }
        }
    }

    /// The `→ failed` transition with its reason (dsl 0.24.0 §2): the
    /// reserved `quest.<id>.failedBy` reads `fail`, `by`, `until`,
    /// `cascade` or `superseded` from here on; the transcript record
    /// carries it as `failedBy`.
    fn set_quest_failed(&mut self, id: &str, reason: &str) {
        self.set_quest_state(id, "failed", Some(reason));
    }

    /// The `→ state` transition: the reserved `quest.<id>.state`, the
    /// status, and the `quest` record (with `failedBy` for a failure, whose
    /// reserved path is written after the state's).
    fn set_quest_state(&mut self, id: &str, state: &str, failed_by: Option<&str>) {
        self.write(&format!("quest.{id}.state"), Value::Str(state.to_string()));
        self.quest_status.insert(id.to_string(), state.to_string());
        let mut rec = json!({
            "kind": "quest",
            "quest": id,
            "state": state,
        });
        if let Some(reason) = failed_by {
            self.write(
                &format!("quest.{id}.failedBy"),
                Value::Str(reason.to_string()),
            );
            rec["failedBy"] = json!(reason);
        }
        self.driver.emit(rec);
    }

    /// dsl 0.16.0 §3 D-D: emit a `grant` transcript record for every
    /// entry in `rewards` (declaration order) whose `outcome=` filter matches
    /// `event` AND whose `when=` gate is `Some(true)` against the LIVE
    /// state at the grant instant. `objective_id: Some(oid)` marks an
    /// objective-level grant (the checker rejects `on=` on those, so the
    /// filter is a no-op here — passed as [`GrantEvent::Objective`]).
    /// Ranges are passed through verbatim on the wire (spec D-C: never
    /// pre-rolled — `amountMin`/`amountMax` land on the transcript entry
    /// exactly as the artifact carried them).
    fn emit_grants(
        &mut self,
        quest_id: &str,
        objective_id: Option<&str>,
        rewards: &[RewardRec],
        event: GrantEvent,
    ) {
        let instance = *self.quest_instances.entry(quest_id.to_string()).or_insert(1);
        for (index, r) in rewards.iter().enumerate() {
            if r.kind.trim().is_empty() {
                continue;
            }
            let on_failed = matches!(r.outcome.as_deref(), Some("failed"));
            let matches = match event {
                GrantEvent::Objective => true,
                GrantEvent::Complete => !on_failed,
                GrantEvent::Failed => on_failed,
            };
            if !matches {
                continue;
            }
            if let Some(raw) = &r.when {
                let site = Site::quest(SiteKind::Reward, &r.kind, quest_id);
                if self.judge(raw, site) != Some(true) {
                    continue;
                }
            }
            let mut reward = serde_json::Map::new();
            reward.insert("kind".into(), Json::String(r.kind.clone()));
            if let Some(t) = &r.target {
                reward.insert("target".into(), Json::String(t.clone()));
            }
            if let Some(n) = r.amount {
                reward.insert("amount".into(), json!(n));
            }
            if let Some(lo) = r.amount_min {
                reward.insert("amountMin".into(), json!(lo));
            }
            if let Some(hi) = r.amount_max {
                reward.insert("amountMax".into(), json!(hi));
            }
            let mut rec = serde_json::Map::new();
            rec.insert("kind".into(), Json::String("grant".into()));
            rec.insert("quest".into(), Json::String(quest_id.to_string()));
            rec.insert("instance".into(), json!(instance));
            if let Some(oid) = objective_id {
                rec.insert("objective".into(), Json::String(oid.to_string()));
            }
            rec.insert("index".into(), json!(index));
            rec.insert("reward".into(), Json::Object(reward));
            // dsl 0.23.0 §8: a kind that credits a path adds the amount there
            // (an unknown or absent current value stays unknown, as `+=`
            // folds it). A range is the engine's roll (D-C), so the reference
            // runner credits scalar amounts only.
            if let (Some(path), Some(n)) = (&r.credits, r.amount) {
                let before = match self.store.read(path) {
                    Read::Value(v) => v,
                    Read::Unset => Value::Unknown,
                };
                let after = fold_op("+=", &before, &Value::Int(n));
                rec.insert(
                    "credited".into(),
                    json!({ "path": path, "value": value_to_json(&after) }),
                );
                self.write(path, after);
            }
            if matches!(event, GrantEvent::Failed) {
                rec.insert("onFailed".into(), Json::Bool(true));
            }
            self.driver.emit(Json::Object(rec));
        }
    }

    /// Fire every handler matching `event` whose `when` holds over the
    /// PRE-EVENT state: every matching handler's `when` is judged before any
    /// of their bodies runs, so a sibling's write never decides another
    /// sibling of the same event (D10, quest-lifecycle.md) — then each firing
    /// body runs once, in document order. `scope` is the transitioning
    /// quest's id for the engine-derived lifecycle events — those fire ONLY
    /// for their own enclosing quest (quest-lifecycle.md); `None` (a mock
    /// world event) fires every matching handler — under `lute play` (dsl
    /// 0.22.0 §9) only those of an ACTIVE quest, as `lute trace` delivers
    /// `events:`. `target` is the target an occasion was raised for (dsl
    /// 0.24.0 §2): a handler with a `target` fires only for that target,
    /// never for a plain event or a lifecycle transition.
    fn fire_event(
        &mut self,
        event: &str,
        scope: Option<&str>,
        target: Option<&str>,
        handlers: &[Handler],
        seg_starts: &[usize],
    ) {
        let matching: Vec<usize> = handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                h.event == event
                    && h.target.as_deref().is_none_or(|t| Some(t) == target)
                    && match scope {
                        Some(s) => h.quest.as_deref() == Some(s),
                        None => {
                            !self.quest_resume
                                || h.quest.as_ref().is_some_and(|q| {
                                    self.quest_status.get(q).map(String::as_str) == Some("active")
                                })
                        }
                    }
            })
            .map(|(i, _)| i)
            .collect();
        let mut verdicts: Vec<(usize, Option<bool>)> = Vec::with_capacity(matching.len());
        for i in matching {
            let h = &handlers[i];
            let verdict = match &h.when {
                None => Some(true),
                Some(raw) => {
                    let site = Site {
                        quest: h.quest.as_deref(),
                        ..Site::new(SiteKind::Handler, event, &h.addr)
                    };
                    self.judge(raw, site)
                }
            };
            if self.stopped() {
                return;
            }
            verdicts.push((i, verdict));
        }
        for (i, verdict) in verdicts {
            let h = &handlers[i];
            let outcome = match verdict {
                Some(true) => "fires",
                Some(false) => "skipped",
                None => continue,
            };
            self.driver.observe(json!({
                "kind": "on", "event": event, "quest": h.quest, "position": h.addr,
                "outcome": outcome, "guard": h.when.as_deref().map(|s| s.raw.trim()),
            }));
            if verdict != Some(true) {
                continue;
            }
            let body = h.body.clone();
            match &mut self.deferred_handlers {
                Some(later) => later.push(body),
                None => self.run_segment(&body, seg_starts),
            }
            if self.stopped() {
                return;
            }
        }
    }

    /// Run a quest body segment: from `body_addr` up to the next segment start
    /// (or end of the stream). Bodies are forward-only (quest-lifecycle.md).
    fn run_segment(&mut self, body_addr: &str, seg_starts: &[usize]) {
        let start = self.resolve(body_addr);
        let stop = seg_starts
            .iter()
            .find(|&&s| s > start)
            .copied()
            .unwrap_or(self.code.commands.len());
        self.run_range(start, stop);
    }
}

fn parse_quest(code: &Code, cmd: &Json) -> QuestDecl {
    let id = cmd
        .get("id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let objectives = cmd
        .get("objectives")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .map(|o| Obj {
                    id: o.get("id").and_then(Json::as_str).unwrap_or("").to_string(),
                    done: code.slot(o.get("done")).unwrap_or_else(|| {
                        Arc::new(Slot::synthetic(String::new(), Expr::Invalid))
                    }),
                    optional: o.get("optional").and_then(Json::as_bool).unwrap_or(false),
                    body: o.get("body").and_then(Json::as_str).map(str::to_string),
                    quest: o.get("quest").and_then(Json::as_str).map(str::to_string),
                    rewards: parse_rewards(code, o),
                    on: o.get("on").and_then(Json::as_str).map(str::to_string),
                    by: code.slot(o.get("by")),
                    target: o.get("target").and_then(Json::as_str).map(str::to_string),
                    until: code.slot(o.get("until")),
                })
                .collect()
        })
        .unwrap_or_default();
    QuestDecl {
        id,
        start: code.slot(cmd.get("start")),
        fail: code.slot(cmd.get("fail")),
        objectives,
        rewards: parse_rewards(code, cmd),
        accept_activated: cmd.get("activate").and_then(Json::as_str) == Some("accept"),
        complete_any: cmd.get("complete").and_then(Json::as_str) == Some("any"),
    }
}

/// dsl 0.16.0 §3: parse the `rewards:` array off a `QuestCmd`/
/// `ObjectiveEntry` JSON record into the runner's [`RewardRec`] shape.
/// The array is `skip_serializing_if = "Vec::is_empty"` on the compile
/// side, so a rewardless owner has no `rewards` key at all; this
/// gracefully returns an empty vector in that case. Fields map directly
/// from the wire (`kind`/`target`/`amount`/`amountMin`/`amountMax`/
/// `when.raw`/`outcome`); a malformed entry keeps default values (empty
/// `kind` filters at grant time via [`Machine::emit_grants`]).
fn parse_rewards(code: &Code, owner: &Json) -> Vec<RewardRec> {
    owner
        .get("rewards")
        .and_then(Json::as_array)
        .map(|arr| arr.iter().map(|r| parse_reward(code, r)).collect())
        .unwrap_or_default()
}

fn parse_reward(code: &Code, r: &Json) -> RewardRec {
    RewardRec {
        kind: r
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        target: r.get("target").and_then(Json::as_str).map(str::to_string),
        amount: r.get("amount").and_then(Json::as_i64),
        amount_min: r.get("amountMin").and_then(Json::as_i64),
        amount_max: r.get("amountMax").and_then(Json::as_i64),
        when: code.slot(r.get("when")),
        outcome: r.get("outcome").and_then(Json::as_str).map(str::to_string),
        credits: r.get("credits").and_then(Json::as_str).map(str::to_string),
    }
}
