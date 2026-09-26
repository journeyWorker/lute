//! Fact provenance (dsl 0.24.0 T3-2): what last asserted each base fact of
//! a playthrough, for `--explain`'s asserted leaves.

use std::collections::{BTreeMap, BTreeSet};

use lute_trace::exec::session::{kind_label, parse_ground_fact, Played, Presented, StepBody};
use serde_json::Value as Json;

use lute_trace::datalog::Fact;

use super::run::Playthrough;

/// dsl 0.24.0 T3-2: what last asserted each base fact of the playthrough —
/// ``entry `keeperLog2`, step 4``, `engine step 7`, the script's `facts:` —
/// for `--explain`'s asserted leaves. `initial` is the world's fact set
/// before the first step (seed facts are labelled by the caller).
pub(super) fn fact_origins(play: &Playthrough, initial: &BTreeSet<Fact>) -> BTreeMap<Fact, String> {
    let mut out: BTreeMap<Fact, String> = initial
        .iter()
        .map(|f| (f.clone(), "the script's `facts:`".to_string()))
        .collect();
    let mut take = |records: &[Json], who: String| {
        for rec in records {
            if rec.get("kind").and_then(Json::as_str) == Some("assert") {
                if let Some(f) = rec
                    .get("fact")
                    .and_then(Json::as_str)
                    .and_then(parse_ground_fact)
                {
                    out.insert(f, who.clone());
                }
            }
        }
    };
    for q in &play.start {
        take(
            &q.transcript,
            format!("a quest handler in {}, before step 1", q.document),
        );
    }
    for s in &play.steps {
        let step = match s.iteration {
            Some((k, of)) => format!("step {} ({k}/{of})", s.n),
            None => format!("step {}", s.n),
        };
        // dsl 0.27.0 §3: a kind / `for` beat names the member it ran for.
        let beat = |p: &Presented| {
            let member = p
                .member
                .as_deref()
                .map(|m| format!(" for {m}"))
                .unwrap_or_default();
            format!("{} `{}`{member}, {step}", kind_label(p.kind), p.id)
        };
        match &s.body {
            StepBody::Engine { writes } => take(writes, format!("engine {step}")),
            StepBody::NewRun { writes, .. } => take(writes, format!("newRun {step}")),
            _ => {}
        }
        for played in s.body.days_played() {
            match played {
                Played::Quest(q) => take(
                    &q.transcript,
                    format!("a quest handler in {}, {step}", q.document),
                ),
                Played::Beat(p) => take(&p.transcript, beat(p)),
            }
        }
        for q in s.body.settled() {
            take(
                &q.transcript,
                format!("a quest handler in {}, {step}", q.document),
            );
        }
        if let Some(StepBody::Occasion { presented, .. }) = s.body.occasion() {
            for p in presented {
                take(&p.transcript, beat(p));
            }
        }
        for q in &s.quests {
            take(
                &q.transcript,
                format!("a quest handler in {}, {step}", q.document),
            );
        }
    }
    out
}
