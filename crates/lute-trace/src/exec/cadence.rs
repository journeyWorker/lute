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
//!   `tier="season:<name>"` quests return to `unset`.
//! - Rearm (`<quest rearm>`): when the condition goes false→true the quest
//!   returns to `unset`.
//!
//! [`observe`] runs at the head of every quest settle and after every
//! settle pass that moved a quest (a `rearm` or a `live` may read quest
//! states), so a transition is applied before the lifecycle fixpoint goes on
//! (a rearmed quest whose `start` holds activates in the same settle) and
//! before the next eligibility judgment. An `advance:` also observes every
//! position the clock crosses ([`walk_clock`]): a window that opens and
//! closes inside one advance is still seen. The first observation is the
//! baseline: a condition already true when the playthrough starts opens
//! nothing.
//!
//! wasm-clean: no filesystem, process or threads.

use std::collections::{BTreeMap, BTreeSet};

use lute_compile::index::{BeatKind, IndexBeat, ProjectIndex};
use lute_compile::BeatOnce;
use serde_json::{json, Value as Json};

use crate::exec::session::{
    ever_read_path, json_to_value, move_clock, spend_group, value_to_json, ExecProject,
    QuestAdvance, World,
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
        CadencePlan { seasons, rearms }
    }

    /// `true` when the project declares no season and no rearm — nothing
    /// to observe.
    pub fn is_empty(&self) -> bool {
        self.seasons.is_empty() && self.rearms.is_empty()
    }
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
}

/// Why `beat` is spent in `w`, if it is: its `once` policy (a scene or
/// bundle beat by presentation, an entry by its read flags), a clock period
/// it was already presented in, its season's current window — naming the
/// sibling of its `share` key that spent it (dsl 0.25.0 §2).
pub fn once_spent(p: &ExecProject, w: &World, beat: &IndexBeat) -> Option<String> {
    let flag = |path: String| w.state.get(&path) == Some(&Value::Bool(true));
    let clock = p.index.clock.as_ref();
    let now = clock.and_then(|c| crate::clock::position(c, &w.state));
    let at = w.spent_at.get(&beat.id);
    let once = beat.once.as_ref()?;
    let reason: String = match (beat.kind, once) {
        (BeatKind::Scene | BeatKind::Bundle, BeatOnce::Run) if w.spent_run.contains(&beat.id) => {
            "once: run — already presented this run".into()
        }
        (BeatKind::Scene | BeatKind::Bundle, BeatOnce::User) if w.spent_user.contains(&beat.id) => {
            "once: user — already presented".into()
        }
        (BeatKind::Entry, BeatOnce::Run)
            if flag(format!("entry.{}.read", beat.id)) || w.spent_run.contains(&beat.id) =>
        {
            "once: run — already read this run".into()
        }
        (BeatKind::Entry, BeatOnce::User)
            if flag(ever_read_path(&beat.id)) || w.spent_user.contains(&beat.id) =>
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
        (_, BeatOnce::Season(name)) if w.cadence.spent_season.contains(&beat.id) => {
            format!("once: season:{name} — already presented this {name} window")
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
                BeatOnce::Season(name) => format!(" this {name} window"),
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

/// dsl 0.27.0 §5: whether the beat's `spentBy` condition holds — `Ok(Some)`
/// with the reason when it does, `Ok(None)` when it does not (or the beat
/// declares none), `Err` with the atoms when it is undecided.
pub fn spent_by<D: crate::exec::Driver>(
    eval: &mut crate::exec::Machine<D>,
    beat: &IndexBeat,
) -> Result<Option<String>, Vec<crate::UnresolvedAtom>> {
    let Some(raw) = beat.spent_by.as_deref() else {
        return Ok(None);
    };
    Ok(eval
        .eval_guard(raw)?
        .then(|| format!("spentBy: `{raw}` holds")))
}

/// Record that a presentation of `id` spent the season-scoped beats of its
/// spend group (dsl 0.27.0 §5; `share` keys spend every member).
pub fn spend_season(p: &ExecProject, w: &mut World, id: &str) {
    for m in spend_group(p, id) {
        let seasonal = p
            .index
            .beats
            .iter()
            .any(|b| b.id == m && matches!(b.once, Some(BeatOnce::Season(_))));
        if seasonal {
            w.cadence.spent_season.insert(m.to_string());
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
/// the false→true transitions (see the module doc). The transcript records:
/// `{kind: "season", season, state: "open" | "closed"}` (an opening also
/// carries `prev`: the paths the last window's values moved to) and
/// `{kind: "quest", quest, state: "unset", reset: "rearm" | "season:<name>",
/// was}` for every quest that left `unset`.
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
                push(
                    "",
                    json!({ "kind": "season", "season": s.name, "state": "open", "prev": prev }),
                );
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

/// Move the clock from `from` to `to` ([`move_clock`]), observing every
/// position strictly between — each day, and each slot on a slotted clock
/// — so a season window or a rearm condition that opens and closes inside
/// one `advance:` is still seen; the observations append to `observed`.
/// `to` itself is observed by the settle that follows the move. The `set`
/// records are the whole move's, as [`move_clock`] writes them.
pub fn walk_clock(
    p: &ExecProject,
    w: &mut World,
    clock: &lute_manifest::clock::ClockDecl,
    from: lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
    observed: &mut Vec<QuestAdvance>,
) -> Vec<Json> {
    if !p.cadence.is_empty() {
        let mut at = from;
        while at < to {
            let next = clock
                .advance(at, lute_manifest::clock::Advance::Slots(1))
                .min(to);
            move_clock(p, w, clock, at, next);
            at = next;
            if at < to {
                observed.extend(observe(p, w));
            }
        }
    }
    // Already at `to` after a walk: the rewrite only yields the records.
    move_clock(p, w, clock, from, to)
}

/// Open season `s`: the last window's values (when `had_window`) move to
/// `prev.season.*`, the season's state goes back to its defaults, its
/// season-scoped beats are spendable again. The mirrored paths with their
/// values, as JSON.
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
    let name = s.name.as_str();
    w.cadence.spent_season.retain(|id| {
        !p.index
            .beats
            .iter()
            .any(|b| b.id == *id && b.once.as_ref().and_then(BeatOnce::season) == Some(name))
    });
    Json::Object(prev)
}
