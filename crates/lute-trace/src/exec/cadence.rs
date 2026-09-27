//! Cadence (dsl 0.27.0 §5): how a beat's `once` is spent and respawns, and
//! the world transitions a playthrough observes between steps — seasons
//! opening and closing, quests rearming.
//!
//! - `once` spending: [`once_spent`] is the one judgment of every policy
//!   (`run` / `user` / `day` / `slot` / `week` / `season:<name>`, entries by
//!   their read flags, `share` keys naming the sibling that spent them);
//!   [`spend_season`] records a season-scoped spend.
//! - Seasons (`seasons: { <name>: { live } }`): when `live` goes
//!   false→true the season opens — `prev.season.<name>.*` takes the values
//!   the last window ended with, `season.<name>.*` goes back to the declared
//!   defaults, its `once: season:<name>` beats are spendable again and its
//!   `tier="season:<name>"` quests return to `unset`, and the facts of its
//!   `tier: season:<name>` relations go back to the seed facts.
//! - Rearm (`<quest rearm>`): when the condition goes false→true the quest
//!   returns to `unset`.
//!
//! [`observe`] runs at the head of every quest settle and after every
//! settle pass that moved a quest (a `rearm` or a `live` may read quest
//! states), so a transition is applied before the lifecycle fixpoint goes on
//! (a rearmed quest whose `start` holds activates in the same settle) and
//! before the next eligibility judgment. An `advance:` settles the quests —
//! and so observes — at every position the clock crosses
//! ([`walk_clock`](crate::exec::session::walk_clock)): a window that opens
//! and closes inside one advance is still seen, and its quests start, fail
//! and reset there. The first observation is the baseline: a condition
//! already true when the playthrough starts opens nothing.
//!
//! `spentBy` latches ([`observe_latches`]) are observed only where a settle
//! reaches its fixpoint — never at its head or between its passes — so a
//! latch sees settled worlds only: a `start="true"` quest is already
//! `active` when the first latch looks at it, never its pre-activation
//! `unset`.
//!
//! wasm-clean: no filesystem, process or threads.

use std::collections::{BTreeMap, BTreeSet};

use lute_compile::index::{BeatKind, IndexBeat, ProjectIndex};
use lute_compile::BeatOnce;
use serde_json::{json, Value as Json};

use crate::exec::session::{
    ever_read_path, json_to_value, spend_group, value_to_json, ExecProject, QuestAdvance, World,
};
use crate::Value;

/// One declared season, resolved against the project.
pub struct SeasonPlan {
    pub name: String,
    /// The `live` condition, `@def`-expanded.
    pub live: String,
    /// Every declared `season.<name>.*` path with its declared default.
    pub paths: Vec<(String, Option<Value>)>,
    /// `tier="season:<name>"` quests -> their objective ids.
    pub quests: Vec<(String, Vec<String>)>,
    /// The quest document of each of those quests.
    pub quest_docs: BTreeMap<String, String>,
    /// dsl 0.28.0 §5: `tier: season:<name>` relations.
    pub relations: BTreeSet<String>,
}

/// One `<quest rearm>`.
pub struct RearmPlan {
    pub quest: String,
    pub document: String,
    pub raw: String,
    pub objectives: Vec<String>,
}

/// What the project declares about cadence, read once at assembly.
#[derive(Default)]
pub struct CadencePlan {
    pub seasons: Vec<SeasonPlan>,
    pub rearms: Vec<RearmPlan>,
    pub latches: Vec<LatchPlan>,
}

impl CadencePlan {
    /// The plan of a project: the index's seasons, the state table's season
    /// paths, and every quest record's `tier` / `rearm`.
    pub fn of(
        index: &ProjectIndex,
        artifacts: &BTreeMap<String, Json>,
        quest_docs: &[String],
        state_table: &BTreeMap<String, Json>,
    ) -> Self {
        let quests: Vec<(&String, &Json)> = quest_docs
            .iter()
            .filter_map(|rel| artifacts.get(rel).map(|a| (rel, a)))
            .flat_map(|(rel, a)| {
                a.get("commands")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|c| c.get("kind").and_then(Json::as_str) == Some("quest"))
                    .map(move |c| (rel, c))
            })
            .collect();
        let objectives = |c: &Json| -> Vec<String> {
            c.get("objectives")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
                .collect()
        };
        let id_of = |c: &Json| c.get("id").and_then(Json::as_str).map(str::to_string);
        let seasons = index
            .seasons
            .iter()
            .map(|s| {
                let mine = |c: &&(&String, &Json)| {
                    c.1.get("tier")
                        .and_then(Json::as_str)
                        .and_then(lute_manifest::season::season_ref)
                        == Some(s.name.as_str())
                };
                SeasonPlan {
                    name: s.name.clone(),
                    live: s.live.raw.clone(),
                    paths: state_table
                        .iter()
                        .filter(|(p, _)| {
                            lute_manifest::season::season_of_path(p) == Some(s.name.as_str())
                        })
                        .map(|(p, e)| (p.clone(), e.get("default").and_then(json_to_value)))
                        .collect(),
                    quests: quests
                        .iter()
                        .filter(mine)
                        .filter_map(|(_, c)| Some((id_of(c)?, objectives(c))))
                        .collect(),
                    quest_docs: quests
                        .iter()
                        .filter(mine)
                        .filter_map(|(rel, c)| Some((id_of(c)?, (*rel).clone())))
                        .collect(),
                    relations: index
                        .relations
                        .iter()
                        .filter(|r| {
                            r.tier
                                .as_deref()
                                .and_then(lute_manifest::season::season_ref)
                                == Some(s.name.as_str())
                        })
                        .map(|r| r.name.clone())
                        .collect(),
                }
            })
            .collect();
        let rearms = quests
            .iter()
            .filter_map(|(rel, c)| {
                let raw = c.get("rearm")?.get("raw")?.as_str()?;
                Some(RearmPlan {
                    quest: id_of(c)?,
                    document: (*rel).clone(),
                    raw: raw.to_string(),
                    objectives: objectives(c),
                })
            })
            .collect();
        let latches = index
            .beats
            .iter()
            .filter_map(|b| {
                let raw = b.spent_by.as_ref()?;
                let members: Vec<Option<String>> = match (&b.for_kind, &b.target_kind) {
                    (Some(k), _) => k.members.iter().cloned().map(Some).collect(),
                    (None, Some(k)) => k.members.iter().cloned().map(Some).collect(),
                    (None, None) => vec![None],
                };
                Some(LatchPlan {
                    beat: b.id.clone(),
                    raw: raw.clone(),
                    members,
                })
            })
            .collect();
        CadencePlan {
            seasons,
            rearms,
            latches,
        }
    }

    /// `true` when the project declares no season, no rearm and no
    /// `spentBy` — nothing to observe.
    pub fn is_empty(&self) -> bool {
        self.seasons.is_empty() && self.rearms.is_empty() && self.latches.is_empty()
    }
}

/// One `spentBy` beat: observed at every settle for each member it is
/// judged for (a kind beat's members, else once), so the beat is spent
/// from the first moment its condition held.
pub struct LatchPlan {
    pub beat: String,
    /// The `spentBy` condition, `@def`-expanded.
    pub raw: String,
    /// `Some(member)` for a `for=` / `target="kind:…"` beat (bound as
    /// `occasion.target`), else one `None`.
    pub members: Vec<Option<String>>,
}

/// The cadence memory a playthrough carries between steps.
#[derive(Clone, Default)]
pub struct Cadence {
    /// Each season's `live` as last observed (absent before the first).
    pub live: BTreeMap<String, bool>,
    /// Each rearming quest's condition as last observed.
    pub rearm: BTreeMap<String, bool>,
    /// Beats (`once: season:<name>`) presented in their season's current
    /// window.
    pub spent_season: BTreeSet<String>,
    /// Seasons observed live at least once: an opening mirrors the last
    /// window into `prev.season.*` only when there was one.
    pub windows: BTreeSet<String>,
    /// `spentBy` beats whose condition has held, keyed like a spend
    /// ([`latch_key`]), with the clock position it was first seen holding
    /// (what a `once: day | slot | week` latch counts from). Cleared per
    /// scope: a new run drops the run-scoped ones, a season opening its
    /// own.
    pub latched: BTreeMap<String, Option<lute_manifest::clock::ClockAt>>,
}

/// dsl 0.28.0 (T1-6, T2-10): the key a beat's `once` is spent under — a
/// `for="kind:<kind>"` beat's per member (`<id>@<member>`: each member is
/// its own candidate), any other beat's its id.
pub fn spend_key(beat: &IndexBeat, member: Option<&str>) -> String {
    match (&beat.for_kind, member) {
        (Some(_), Some(m)) => format!("{}@{m}", beat.id),
        _ => beat.id.clone(),
    }
}

/// dsl 0.28.0 (T1-6): the run-tier flag an entry's first read is decided
/// by — `entry.<id>.read`, or a `for` entry's per member
/// `entry.<id>.readFor.<member>` (engine-internal; `entry.<id>.read` holds
/// once any member was read).
pub fn entry_read_flag(beat: &IndexBeat, member: Option<&str>) -> String {
    match (&beat.for_kind, member) {
        (Some(_), Some(m)) => entry_member_read_path(&beat.id, m),
        _ => format!("entry.{}.read", beat.id),
    }
}

/// `entry.<id>.readFor.<member>` ([`entry_read_flag`]).
pub fn entry_member_read_path(id: &str, member: &str) -> String {
    format!("entry.{id}.readFor.{member}")
}

/// Why `beat` (for `member`, a `for` beat's) is spent in `w`, if it is: its
/// `once` policy (a scene or bundle beat by presentation, an entry by its
/// read flags), a clock period it was already presented in, its season's
/// current window — naming the sibling of its `share` key that spent it
/// (dsl 0.25.0 §2).
pub fn once_spent(
    p: &ExecProject,
    w: &World,
    beat: &IndexBeat,
    member: Option<&str>,
) -> Option<String> {
    let flag = |path: String| w.state.get(&path) == Some(&Value::Bool(true));
    // A `spentBy` beat is spent by its condition ([`latched`]), never by
    // being presented or read.
    if beat.spent_by.is_some() {
        return None;
    }
    let key = spend_key(beat, member);
    let clock = p.index.clock.as_ref();
    let now = clock.and_then(|c| crate::clock::position(c, &w.state));
    let at = w.spent_at.get(&key);
    let once = beat.once.as_ref()?;
    let window = |name: &str| season_window(w, name);
    let reason: String = match (beat.kind, once) {
        (BeatKind::Scene | BeatKind::Bundle, BeatOnce::Run) if w.spent_run.contains(&key) => {
            "once: run — already presented this run".into()
        }
        (BeatKind::Scene | BeatKind::Bundle, BeatOnce::User) if w.spent_user.contains(&key) => {
            "once: user — already presented".into()
        }
        (BeatKind::Entry, BeatOnce::Run)
            if flag(entry_read_flag(beat, member)) || w.spent_run.contains(&key) =>
        {
            "once: run — already read this run".into()
        }
        (BeatKind::Entry, BeatOnce::User)
            if (beat.for_kind.is_none() && flag(ever_read_path(&beat.id)))
                || w.spent_user.contains(&key) =>
        {
            "once: user — already read".into()
        }
        // dsl 0.24.0 §1: spent until the clock's day (slot) moves on.
        (_, BeatOnce::Day) if now.zip(at).is_some_and(|(now, at)| at.day == now.day) => {
            "once: day — already presented today".into()
        }
        (_, BeatOnce::Slot) if now.is_some() && at == now.as_ref() => {
            "once: slot — already presented this slot".into()
        }
        // dsl 0.27.0 §5: spent until `clock.weekday` returns to `week.first`.
        (_, BeatOnce::Week)
            if clock.zip(now).zip(at).is_some_and(|((c, now), at)| {
                c.week_of(at.day).is_some() && c.week_of(at.day) == c.week_of(now.day)
            }) =>
        {
            "once: week — already presented this week".into()
        }
        // dsl 0.27.0 §5: spent until the season opens again.
        (_, BeatOnce::Season(name)) if w.cadence.spent_season.contains(&key) => {
            format!(
                "once: season:{name} — already presented in {}",
                window(name)
            )
        }
        _ => return None,
    };
    let by = beat
        .share
        .as_ref()
        .and_then(|k| Some((k, w.share_spent_by.get(k)?)))
        .filter(|(_, by)| **by != beat.id);
    Some(match by {
        Some((key, by)) => {
            let period = match once {
                BeatOnce::Run => " this run".to_string(),
                BeatOnce::Day => " today".to_string(),
                BeatOnce::Slot => " this slot".to_string(),
                BeatOnce::Week => " this week".to_string(),
                BeatOnce::Season(name) => format!(" in {}", window(name)),
                BeatOnce::User | BeatOnce::None => String::new(),
            };
            format!(
                "once: {} — `share: {key}` already spent{period} by {by}",
                once_word(once)
            )
        }
        None => reason,
    })
}

/// The authored spelling of a `once` policy, for a reason.
pub fn once_word(once: &BeatOnce) -> std::borrow::Cow<'static, str> {
    match once {
        BeatOnce::None => "false".into(),
        other => other.as_str(),
    }
}

/// Whether the beat (for `member`, the kind member bound as
/// `occasion.target`) is spent by its `spentBy`: `Ok(Some)` with the reason
/// when its condition has held within its `once` period ([`latched`]) or
/// holds now, `Ok(None)` when not (or the beat declares none), `Err` with
/// the atoms when it is undecided now.
pub fn spent_by<D: crate::exec::Driver>(
    p: &ExecProject,
    w: &World,
    eval: &mut crate::exec::Machine<D>,
    beat: &IndexBeat,
    member: Option<&str>,
) -> Result<Option<String>, Vec<crate::UnresolvedAtom>> {
    let Some(raw) = beat.spent_by.as_deref() else {
        return Ok(None);
    };
    if let Some(reason) = latched(p, w, beat, member) {
        return Ok(Some(reason));
    }
    let who = member.map(|m| format!(" for {m}")).unwrap_or_default();
    Ok(eval
        .eval_guard(raw)?
        .then(|| format!("spentBy: `{raw}` holds{who}")))
}

/// The key a `spentBy` beat is latched under: `<id>@<member>` for a kind
/// beat's member, else its id.
pub fn latch_key(id: &str, member: Option<&str>) -> String {
    match member {
        Some(m) => format!("{id}@{m}"),
        None => id.to_string(),
    }
}

/// How long a `spentBy` beat stays spent once its condition has held: its
/// `once` (`run` unless written).
pub fn latch_once(beat: &IndexBeat) -> BeatOnce {
    match &beat.once {
        None | Some(BeatOnce::None) => BeatOnce::Run,
        Some(once) => once.clone(),
    }
}

/// Why `beat` (for `member`) is still spent by a condition that held
/// earlier, if it is: a latch recorded by [`observe`] and not yet expired —
/// a `day` / `slot` / `week` one lasts while the clock stays in the period
/// it was recorded in; `run`, `user` and `season:<name>` ones until a new
/// run, never, or the season's next opening clears them.
pub fn latched(
    p: &ExecProject,
    w: &World,
    beat: &IndexBeat,
    member: Option<&str>,
) -> Option<String> {
    let raw = beat.spent_by.as_deref()?;
    let at = w.cadence.latched.get(&latch_key(&beat.id, member))?;
    let once = latch_once(beat);
    if !latch_live(p, w, &once, at.as_ref()) {
        return None;
    }
    let who = member.map(|m| format!(" for {m}")).unwrap_or_default();
    let period = match &once {
        BeatOnce::Run | BeatOnce::None => "this run".to_string(),
        BeatOnce::User => "for good (`once: user`)".to_string(),
        BeatOnce::Day => "today".to_string(),
        BeatOnce::Slot => "this slot".to_string(),
        BeatOnce::Week => "this week".to_string(),
        BeatOnce::Season(name) => format!("for {}", season_window(w, name)),
    };
    Some(format!("spentBy: `{raw}` held{who} — spent {period}"))
}

/// A season's window as a spend reason names it: "this" one while it is
/// live, "the last" one once it has closed (spent until it opens again).
fn season_window(w: &World, name: &str) -> String {
    match w.cadence.live.get(name) {
        Some(false) => format!("the last {name} window (closed; spendable when it opens again)"),
        _ => format!("this {name} window"),
    }
}

/// Whether a latch recorded at `at` still spends a beat of policy `once`.
fn latch_live(
    p: &ExecProject,
    w: &World,
    once: &BeatOnce,
    at: Option<&lute_manifest::clock::ClockAt>,
) -> bool {
    let clock = p.index.clock.as_ref();
    let now = clock.and_then(|c| crate::clock::position(c, &w.state));
    match (once, at, now) {
        (BeatOnce::Day, Some(at), Some(now)) => at.day == now.day,
        (BeatOnce::Slot, Some(at), Some(now)) => *at == now,
        (BeatOnce::Week, Some(at), Some(now)) => clock.is_some_and(|c| {
            c.week_of(at.day).is_some() && c.week_of(at.day) == c.week_of(now.day)
        }),
        _ => true,
    }
}

/// A new run: the run-scoped latches go — and, when the run starts its clock
/// over (`clock_restarts`), the clock-period ones with it, like the
/// `once: day | slot | week` spends.
pub fn new_run_latches(p: &ExecProject, w: &mut World, clock_restarts: bool) {
    let once_of = |key: &str| {
        let id = key.split_once('@').map_or(key, |(id, _)| id);
        p.index
            .beats
            .iter()
            .find(|b| b.id == id && b.spent_by.is_some())
            .map(latch_once)
    };
    w.cadence.latched.retain(|key, _| match once_of(key) {
        Some(BeatOnce::User | BeatOnce::Season(_)) => true,
        Some(BeatOnce::Day | BeatOnce::Slot | BeatOnce::Week) => !clock_restarts,
        _ => false,
    });
}

/// Record that a presentation of `id` (for `member`) spent the
/// season-scoped beats of its spend group (dsl 0.27.0 §5; `share` keys
/// spend every member).
pub fn spend_season(p: &ExecProject, w: &mut World, id: &str, member: Option<&str>) {
    for m in spend_group(p, id) {
        if let Some(b) = p
            .index
            .beats
            .iter()
            .find(|b| b.id == m && matches!(b.once, Some(BeatOnce::Season(_))))
        {
            w.cadence.spent_season.insert(spend_key(b, member));
        }
    }
}

/// Return quest `id` to `unset`: its status and reserved reads, objectives
/// undone, failure reasons and missed deadlines forgotten. The status it
/// left, when it was not already `unset`.
pub fn reset_quest(w: &mut World, id: &str, objectives: &[String]) -> Option<String> {
    let was = w.quests.insert(id.to_string(), "unset".to_string());
    w.state
        .insert(format!("quest.{id}.state"), Value::Str("unset".to_string()));
    w.state.remove(&format!("quest.{id}.activatedAt"));
    // dsl 0.24.0 §2: the failure reasons reset with the quest.
    w.state.remove(&format!("quest.{id}.failedBy"));
    for oid in objectives {
        w.state.insert(
            format!("quest.{id}.objectives.{oid}.done"),
            Value::Bool(false),
        );
        w.state
            .remove(&format!("quest.{id}.objectives.{oid}.failed"));
    }
    // dsl 0.23.0 §2: its missed deadlines reset with it.
    let prefix = format!("{id}.");
    w.failed_objectives.retain(|k| !k.starts_with(&prefix));
    was.filter(|s| s != "unset")
}

/// Observe every season's `live` and every quest's `rearm` in `w` and apply
/// the false→true transitions (see the module doc). The
/// transcript records: `{kind: "season", season, state: "open" |
/// "closed"}` (an opening also carries `prev`: the paths the last window's
/// values moved to) and `{kind: "quest", quest, state: "unset", reset:
/// "rearm" | "season:<name>", was}` for every quest that left `unset`.
pub fn observe(p: &ExecProject, w: &mut World) -> Vec<QuestAdvance> {
    let plan = &p.cadence;
    if plan.is_empty() {
        return Vec::new();
    }
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    let seasons: Vec<Option<bool>> = plan
        .seasons
        .iter()
        .map(|s| eval.eval_guard(&s.live).ok())
        .collect();
    let rearms: Vec<Option<bool>> = plan
        .rearms
        .iter()
        .map(|r| eval.eval_guard(&r.raw).ok())
        .collect();
    drop(eval);
    let mut out: Vec<QuestAdvance> = Vec::new();
    let mut push = |document: &str, rec: Json| match out.last_mut() {
        Some(q) if q.document == document => q.transcript.push(rec),
        _ => out.push(QuestAdvance {
            document: document.to_string(),
            transcript: vec![rec],
        }),
    };
    for (s, now) in plan.seasons.iter().zip(seasons) {
        // Undecided: nothing observed, nothing moves.
        let Some(now) = now else { continue };
        let before = w.cadence.live.insert(s.name.clone(), now);
        // An opening mirrors the last window only when there was one.
        let had_window = now && !w.cadence.windows.insert(s.name.clone());
        match (before, now) {
            (Some(false), true) => {
                let prev = open_season(p, w, s, had_window);
                let mut rec =
                    json!({ "kind": "season", "season": s.name, "state": "open", "prev": prev });
                if !s.relations.is_empty() {
                    rec["relations"] = json!(s.relations);
                }
                push("", rec);
                for (id, objectives) in &s.quests {
                    if let Some(was) = reset_quest(w, id, objectives) {
                        push(
                            &s.quest_docs[id],
                            json!({
                                "kind": "quest",
                                "quest": id,
                                "state": "unset",
                                "reset": format!("season:{}", s.name),
                                "was": was,
                            }),
                        );
                    }
                }
            }
            (Some(true), false) => push(
                "",
                json!({ "kind": "season", "season": s.name, "state": "closed" }),
            ),
            _ => {}
        }
    }
    for (r, now) in plan.rearms.iter().zip(rearms) {
        let Some(now) = now else { continue };
        let before = w.cadence.rearm.insert(r.quest.clone(), now);
        if before == Some(false) && now {
            if let Some(was) = reset_quest(w, &r.quest, &r.objectives) {
                push(
                    &r.document,
                    json!({
                        "kind": "quest",
                        "quest": r.quest,
                        "state": "unset",
                        "reset": "rearm",
                        "was": was,
                    }),
                );
            }
        }
    }
    out
}

/// A `spentBy` beat is spent from the first settled world its condition
/// holds in (for a kind beat: per member, bound as `occasion.target`) until
/// its `once` period ends — the condition turning false again does not
/// bring it back. Run once per settle, at its fixpoint (see the module
/// doc): a world mid-settle (a quest its `start` is about to activate) is
/// never latched on. Its clock position is recorded for a `day` / `slot` /
/// `week` period; an undecided condition latches nothing.
pub fn observe_latches(p: &ExecProject, w: &mut World) {
    if p.cadence.latches.is_empty() {
        return;
    }
    let now = p
        .index
        .clock
        .as_ref()
        .and_then(|c| crate::clock::position(c, &w.state));
    let mut held = Vec::new();
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    for l in &p.cadence.latches {
        let Some(beat) = p.index.beats.iter().find(|b| b.id == l.beat) else {
            continue;
        };
        for m in &l.members {
            let member = m.as_deref();
            if latched(p, w, beat, member).is_some() {
                continue;
            }
            eval.bind_occasion_target(member);
            if matches!(eval.eval_guard(&l.raw), Ok(true)) {
                held.push(latch_key(&l.beat, member));
            }
        }
    }
    drop(eval);
    for key in held {
        w.cadence.latched.insert(key, now.clone());
    }
}

/// Open season `s`: the last window's values (when `had_window`) move to
/// `prev.season.*`, the season's state goes back to its defaults, its
/// relations back to their seed facts, its season-scoped beats are
/// spendable again. The mirrored paths with their values, as JSON.
fn open_season(p: &ExecProject, w: &mut World, s: &SeasonPlan, had_window: bool) -> Json {
    let mut prev = serde_json::Map::new();
    for (path, default) in &s.paths {
        let mirror = format!("prev.{path}");
        match w.state.get(path).cloned().filter(|_| had_window) {
            Some(v) => {
                prev.insert(mirror.clone(), value_to_json(&v));
                w.state.insert(mirror, v);
            }
            None => {
                w.state.remove(&mirror);
            }
        }
        match default {
            Some(v) => w.state.insert(path.clone(), v.clone()),
            None => w.state.remove(path),
        };
    }
    if !s.relations.is_empty() {
        w.facts.retain(|(rel, _)| !s.relations.contains(rel));
        w.facts.extend(
            p.seed_facts
                .iter()
                .filter(|(rel, _)| s.relations.contains(rel))
                .cloned(),
        );
    }
    let name = s.name.as_str();
    // A `for` beat's key is `<id>@<member>` ([`spend_key`]).
    w.cadence.spent_season.retain(|key| {
        let id = key.split_once('@').map_or(key.as_str(), |(id, _)| id);
        !p.index
            .beats
            .iter()
            .any(|b| b.id == id && b.once.as_ref().and_then(BeatOnce::season) == Some(name))
    });
    // Its `spentBy` beats with `once: season:<name>` too.
    w.cadence.latched.retain(|key, _| {
        let id = key.split_once('@').map_or(key.as_str(), |(id, _)| id);
        !p.index
            .beats
            .iter()
            .any(|b| b.id == id && b.spent_by.is_some() && latch_once(b).season() == Some(name))
    });
    Json::Object(prev)
}
