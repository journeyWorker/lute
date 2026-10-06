//! The quest lifecycle across the project's quest documents (dsl 0.21.0
//! §6, D-H): the settle to a fixpoint, a raised occasion or world event,
//! and the deferred `<on>` handler bodies of a `judge: before` raise.

use std::collections::BTreeMap;

use serde_json::{json, Value as Json};

use super::project::ExecProject;
use super::walk::{absorb, play_machine, walk_stop, PlayHalt, Walked};
use super::world::World;
use crate::exec::record::str_of;
use crate::exec::Seed;

/// One quest document's lifecycle transitions from one advance.
pub struct QuestAdvance {
    pub document: String,
    pub transcript: Vec<Json>,
}

/// Advance every quest lifecycle to a fixpoint (dsl 0.21.0 §6, D-H): each
/// quest document's [`Machine::advance_quests`], repeated in path order until
/// a whole pass transitions nothing — a quest in one document may gate on
/// another's state. The pending accepts (§7a.3) ride every pass and are
/// spent once the lifecycle settles.
///
/// [`Machine::advance_quests`]: crate::exec::Machine::advance_quests
pub fn advance_quests(p: &ExecProject, w: &mut World) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    // dsl 0.27.0 §5: seasons opening and quests rearming since the last
    // settle apply first, so the fixpoint below starts from them.
    let mut out = crate::exec::cadence::observe(p, w);
    let passes = p.quest_docs.len() * 8 + 8;
    for _ in 0..passes {
        let (moved, stop) = advance_pass(p, w, None, &mut out);
        if stop.is_some() {
            return (out, stop);
        }
        if !moved {
            break;
        }
        // A pass that moved a quest may flip a `rearm` or a season's `live`
        // reading it: observed before the next pass.
        out.extend(crate::exec::cadence::observe(p, w));
    }
    // `spentBy` latches see the settled world only: a quest its `start`
    // activates in this settle is `active`, never its `unset` before.
    crate::exec::cadence::observe_latches(p, w);
    // dsl 0.24.0 §2 (ER N15): an accept of an `activate="accept"` child
    // while its parent is not active is spent — the transcript says so.
    for id in std::mem::take(&mut w.accepts) {
        let Some((parent, doc)) = p.accept_children.get(&id) else {
            continue;
        };
        let status = |q: &str| w.quests.get(q).map_or("unset", String::as_str);
        if status(&id) != "unset" || status(parent) == "active" {
            continue;
        }
        out.push(QuestAdvance {
            document: doc.clone(),
            transcript: vec![json!({
                "kind": "acceptSpent",
                "quest": id,
                "parent": parent,
                "parentStatus": status(parent),
            })],
        });
    }
    (out, None)
}

/// A moment raised for the quest lifecycles.
#[derive(Clone, Copy)]
pub enum Raise<'a> {
    /// dsl 0.21.0 §7a.2: judges active quests' `on="<occasion>"` objectives;
    /// the target it was raised for, if any, judges only the objectives
    /// without a `target` or with that one (dsl 0.23.0 §2).
    Occasion(&'a str, Option<&'a str>),
    /// dsl 0.22.0 §9: a world event — active quests' `<on event>` handlers
    /// run, exactly as trace `events:` fires it.
    Event(&'a str),
}

/// Raise an occasion or a world event — ONE pass in which every quest
/// document answers it (the moment is never re-raised by the fixpoint),
/// then the ordinary settle to a fixpoint.
pub fn raise(
    p: &ExecProject,
    w: &mut World,
    moment: Raise<'_>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    let mut out = Vec::new();
    let (_, stop) = advance_pass(p, w, Some(moment), &mut out);
    // dsl 0.24.0 §2.1: the raise judged the `done`s it deferred `by` for.
    if matches!(moment, Raise::Occasion(..)) {
        w.defer_by = None;
    }
    if stop.is_some() {
        return (out, stop);
    }
    let (more, stop) = advance_quests(p, w);
    out.extend(more);
    (out, stop)
}

/// dsl 0.24.0 §2: run the `<on>` handler bodies a `judge: before` raise
/// answered (`(quest document, body addr)`, firing order) — after the
/// occasion's beats — then settle every quest to a fixpoint, as after any
/// presentation. Consecutive bodies of one document share one walk.
pub fn run_deferred_handlers(
    p: &ExecProject,
    w: &mut World,
    handlers: Vec<(String, String)>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    let mut out = Vec::new();
    let mut rest = handlers.as_slice();
    while let Some((doc, _)) = rest.first() {
        let len = rest.iter().take_while(|(d, _)| d == doc).count();
        let bodies: Vec<String> = rest[..len].iter().map(|(_, b)| b.clone()).collect();
        rest = &rest[len..];
        let doc_json = &p.artifacts[doc];
        let no_script = BTreeMap::new();
        let mut m = play_machine(p, w, doc, Seed::from(&w.mock()), w.carry(), &no_script)
            .with_failed_objectives(&w.failed_objectives);
        let result = m.run_deferred_handlers(&bodies);
        let outcome = Walked::of(m);
        absorb(w, &outcome);
        let stop = walk_stop(
            result,
            &outcome,
            &format!("quest document `{doc}`"),
            doc_json,
        );
        if !outcome.transcript.is_empty() {
            out.push(QuestAdvance {
                document: doc.clone(),
                transcript: outcome.transcript,
            });
        }
        if stop.is_some() {
            return (out, stop);
        }
    }
    let (more, stop) = advance_quests(p, w);
    out.extend(more);
    (out, stop)
}

/// One pass over every quest document, path order: its runner resumes the
/// carried lifecycle with the pending accepts and, when given, the moment
/// raised. `true` when any document transitioned.
pub fn advance_pass(
    p: &ExecProject,
    w: &mut World,
    moment: Option<Raise<'_>>,
    out: &mut Vec<QuestAdvance>,
) -> (bool, Option<PlayHalt>) {
    let mut moved = false;
    for doc in &p.quest_docs {
        let doc_json = &p.artifacts[doc];
        let mut mock = w.mock();
        mock.accepts = w.accepts.clone();
        match moment {
            // The runner reads a targeted raise as `name@target`.
            Some(Raise::Occasion(o, t)) => mock
                .occasions
                .push(t.map_or_else(|| o.to_string(), |t| format!("{o}@{t}"))),
            Some(Raise::Event(e)) => mock.events.push(e.to_string()),
            None => {}
        }
        let skipped = handlers_skipped(doc_json, &w.quests, moment);
        let no_script = BTreeMap::new();
        let mut m = play_machine(p, w, doc, Seed::from(&mock), w.carry(), &no_script)
            .with_failed_objectives(&w.failed_objectives)
            .with_deferred_by(w.defer_by.as_deref())
            .with_deferred_handlers(w.defer_handlers);
        let result = m.advance_quests();
        let outcome = Walked::of(m);
        absorb(w, &outcome);
        w.deferred_handlers.extend(
            outcome
                .carry
                .deferred_handlers
                .iter()
                .map(|b| (doc.clone(), b.clone())),
        );
        let what = format!("quest document `{doc}`");
        let stop = walk_stop(result, &outcome, &what, doc_json);
        let mut transcript = skipped;
        transcript.extend(outcome.transcript.into_iter().filter(|c| {
            !c.get("done").is_some_and(Json::is_null) && !c.get("failed").is_some_and(Json::is_null)
        }));
        if !transcript.is_empty() {
            moved = true;
            out.push(QuestAdvance {
                document: doc.clone(),
                transcript,
            });
        }
        if stop.is_some() {
            return (moved, stop);
        }
    }
    (moved, None)
}

/// dsl 0.24.0 (T3-11): the `<on event>` handlers `moment` names whose quest
/// already left `active` (complete or failed — e.g. an engine write settled
/// it first) — they do not run, so the transcript says so instead of
/// staying silent. One synthetic `handlerSkipped` record each, statuses read
/// at the raise.
pub fn handlers_skipped(
    doc_json: &Json,
    quests: &BTreeMap<String, String>,
    moment: Option<Raise<'_>>,
) -> Vec<Json> {
    let (name, target) = match moment {
        Some(Raise::Occasion(o, t)) => (o, t),
        Some(Raise::Event(e)) => (e, None),
        None => return Vec::new(),
    };
    let mut owner: Option<&str> = None;
    let mut out = Vec::new();
    for cmd in doc_json
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        match str_of(cmd, "kind") {
            // Stream order recovers the enclosing quest (as the runner does).
            "quest" => owner = cmd.get("id").and_then(Json::as_str),
            "on" if str_of(cmd, "event") == name => {
                let aimed = cmd.get("target").and_then(Json::as_str);
                if aimed.is_some() && aimed != target {
                    continue;
                }
                let Some(q) = owner else { continue };
                if let Some(status) = quests
                    .get(q)
                    .filter(|s| matches!(s.as_str(), "complete" | "failed"))
                {
                    out.push(json!({
                        "addr": str_of(cmd, "addr"),
                        "kind": "handlerSkipped",
                        "quest": q,
                        "event": name,
                        "status": status,
                    }));
                }
            }
            _ => {}
        }
    }
    out
}

/// Settle every quest after the clock moved; the settle before a raise
/// defers the `by` of the `on=` objectives that raise judges (dsl 0.24.0
/// §2.1).
pub fn settle_before(
    p: &ExecProject,
    w: &mut World,
    next: Option<&String>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    if let Some(occasion) = next.filter(|o| p.objective_occasions.contains(*o)) {
        w.defer_by = Some(occasion.clone());
    }
    let (settled, stop) = advance_quests(p, w);
    if stop.is_some() {
        w.defer_by = None;
    }
    (settled, stop)
}
