//! The `--json` transcript: one object, reusing the runner's own records.

use std::collections::BTreeMap;

use lute_trace::exec::session::{
    kind_label, value_to_json, Presented, QuestAdvance, StepBody, Verdict,
};
use lute_trace::Value;
use serde_json::{json, Value as Json};

use super::human::pick_label;
use super::run::Playthrough;

fn state_delta(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Json {
    let mut delta = serde_json::Map::new();
    for (k, v) in after {
        if before.get(k) != Some(v) {
            delta.insert(k.clone(), value_to_json(v));
        }
    }
    Json::Object(delta)
}

fn quests_json(quests: &[QuestAdvance]) -> Json {
    Json::Array(
        quests
            .iter()
            .map(|q| json!({ "document": q.document, "commands": q.transcript }))
            .collect(),
    )
}

pub(super) fn render_json(play: &Playthrough) -> Json {
    let steps: Vec<Json> = play
        .steps
        .iter()
        .map(|s| {
            let mut o = serde_json::Map::new();
            o.insert("step".into(), json!(s.n));
            if let Some(label) = &s.label {
                o.insert("label".into(), json!(label));
            }
            if let Some((k, of)) = s.iteration {
                o.insert("iteration".into(), json!(k));
                o.insert("repeat".into(), json!(of));
            }
            if !s.notes.is_empty() {
                o.insert("notes".into(), json!(s.notes));
            }
            if !s.exclusive.is_empty() {
                o.insert("exclusive".into(), json!(s.exclusive));
            }
            // dsl 0.27.0 §4: an occasion step's `engine:` write, landed
            // before its raise (the next object, the same `step`).
            if s.before_raise {
                o.insert("beforeRaise".into(), json!(true));
            }
            match &s.body {
                StepBody::NewRun {
                    writes,
                    reset_quests,
                    accepted,
                    prev_run,
                    unjudged,
                } => {
                    o.insert("newRun".into(), json!(true));
                    o.insert("seed".into(), json!(writes));
                    if !reset_quests.is_empty() {
                        let reset: serde_json::Map<String, Json> = reset_quests
                            .iter()
                            .map(|(id, was)| (id.clone(), json!(was)))
                            .collect();
                        o.insert("resetQuests".into(), Json::Object(reset));
                    }
                    if !accepted.is_empty() {
                        o.insert("accepted".into(), json!(accepted));
                    }
                    if !prev_run.is_empty() {
                        let prev: serde_json::Map<String, Json> = prev_run
                            .iter()
                            .map(|(path, v)| (path.clone(), value_to_json(v)))
                            .collect();
                        o.insert("prevRun".into(), Json::Object(prev));
                    }
                    if !unjudged.is_empty() {
                        o.insert("resetUnjudged".into(), json!(unjudged));
                    }
                }
                StepBody::Engine { writes } => {
                    o.insert("engine".into(), json!(writes));
                }
                StepBody::Event { event } => {
                    o.insert("event".into(), json!(event));
                }
                StepBody::End => {
                    o.insert("end".into(), json!(true));
                }
                // dsl 0.24.0 §1: the move and its settle under `advance`,
                // each midnight raise (`dayEnd` / `dayStart`) under
                // `advance.days` in occasion-step fields; the occasion it
                // raised where it stopped as an occasion step's own fields.
                StepBody::Advance {
                    by,
                    from,
                    to,
                    writes,
                    settled,
                    days,
                    raised,
                    ended,
                    closed: _,
                } => {
                    let days: Vec<Json> = days
                        .iter()
                        .map(|d| {
                            let mut m = serde_json::Map::new();
                            m.insert("at".into(), json!(d.at));
                            m.insert("writes".into(), json!(d.writes));
                            m.insert("settled".into(), quests_json(&d.settled));
                            render_occasion_json(&mut m, &d.occasion);
                            m.insert("quests".into(), quests_json(&d.quests));
                            Json::Object(m)
                        })
                        .collect();
                    let mut advance = json!({
                        "by": by,
                        "from": from,
                        "to": to,
                        "writes": writes,
                        "quests": quests_json(settled),
                    });
                    if !days.is_empty() {
                        advance["days"] = Json::Array(days);
                    }
                    // dsl 0.27.0 §4: only when the clock ended here.
                    if *ended {
                        advance["ended"] = json!(true);
                    }
                    o.insert("advance".into(), advance);
                    if let Some(body) = raised {
                        render_occasion_json(&mut o, body);
                    }
                }
                body @ StepBody::Occasion { .. } => render_occasion_json(&mut o, body),
            }
            o.insert("quests".into(), quests_json(&s.quests));
            Json::Object(o)
        })
        .collect();
    let mut root = serde_json::Map::new();
    match &play.outcome {
        Ok(reason) => {
            root.insert("exit".into(), json!("complete"));
            root.insert("endReason".into(), json!(reason));
            // dsl 0.27.0 §4: the game ended in the project's terminal state.
            if play.terminal {
                root.insert("end".into(), json!("terminal"));
            }
        }
        Err(h) => {
            root.insert("exit".into(), json!(h.exit_label()));
            root.insert("error".into(), json!({ "message": h.message() }));
        }
    }
    root.insert(
        "start".into(),
        json!({ "quests": quests_json(&play.start) }),
    );
    if !play.skipped.is_empty() {
        root.insert(
            "skipped".into(),
            Json::Array(
                play.skipped
                    .iter()
                    .map(|(n, label)| match label {
                        Some(l) => json!({ "step": n, "label": l }),
                        None => json!({ "step": n }),
                    })
                    .collect(),
            ),
        );
    }
    root.insert("steps".into(), Json::Array(steps));
    Json::Object(root)
}

/// An occasion step's `--json` fields.
fn render_occasion_json(o: &mut serde_json::Map<String, Json>, body: &StepBody) {
    let StepBody::Occasion {
        occasion,
        target,
        select,
        pick,
        candidates,
        winner,
        decided: _,
        presented,
        judged,
    } = body
    else {
        return;
    };
    o.insert("occasion".into(), json!(occasion));
    if let Some(t) = target {
        o.insert("target".into(), json!(t));
    }
    o.insert("select".into(), json!(select.as_str()));
    if let Some(pk) = pick {
        o.insert("pick".into(), json!(pick_label(pk)));
    }
    let cands: Vec<Json> = candidates
        .iter()
        .map(|c| {
            let mut m = serde_json::Map::new();
            m.insert("id".into(), json!(c.id));
            m.insert("kind".into(), json!(kind_label(c.kind)));
            m.insert("document".into(), json!(c.document));
            m.insert("priority".into(), json!(c.priority));
            if let Some(member) = &c.for_member {
                m.insert("for".into(), json!(member));
            }
            let (eligible, reason) = match &c.verdict {
                Verdict::Eligible => (json!(true), None),
                Verdict::Ineligible(r) => (json!(false), Some(r.to_string())),
                Verdict::Unknown(d) => (Json::Null, Some(format!("when: unknown ({d})"))),
            };
            m.insert("eligible".into(), eligible);
            if c.read {
                m.insert("read".into(), json!(true));
            }
            if c.also {
                m.insert("also".into(), json!(true));
            }
            if let Some(r) = reason {
                m.insert("reason".into(), json!(r));
            }
            Json::Object(m)
        })
        .collect();
    // dsl 0.24.0 §2: a `judge: before` raise's quest advances, made before
    // the candidates were decided (the step's `quests` are the ones after).
    if !judged.is_empty() {
        o.insert("judgedBefore".into(), quests_json(judged));
    }
    o.insert("candidates".into(), Json::Array(cands));
    o.insert("winner".into(), json!(winner));
    // `presented` is the first presentation (the 0.22 shape); every later one
    // — a winner's `also` riders, the rest of a `select: sequence` — follows
    // in `then`, in order (dsl 0.23.0 §3).
    let pr_json = |pr: &Presented| {
        let mut m = json!({
            "id": pr.id,
            "kind": kind_label(pr.kind),
            "document": pr.document,
            "commands": pr.transcript,
            "stateDelta": state_delta(&pr.state_before, &pr.state_after),
        });
        if candidates.iter().any(|c| c.also && c.id == pr.id) {
            m["also"] = json!(true);
        }
        m
    };
    if let Some((first, rest)) = presented.split_first() {
        o.insert("presented".into(), pr_json(first));
        if !rest.is_empty() {
            o.insert(
                "then".into(),
                Json::Array(rest.iter().map(pr_json).collect()),
            );
        }
    }
}
