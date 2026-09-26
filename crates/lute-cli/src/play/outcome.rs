//! What a play's expectations are judged against (dsl 0.22.0 §4): the
//! [`PlayOutcome`] of every step, the menus the play offered and answered,
//! and the judgment.

use std::collections::{BTreeMap, BTreeSet};

use lute_trace::exec::session::{world_view, ExecProject, Played, StepBody, Verdict};
use serde_json::Value as Json;

use super::human::{said, str_of, DocCmds};
use super::run::Playthrough;
use super::script::PlayScript;
use crate::play_expect::{ExpectMiss, PlayOutcome, StepOutcome};

/// What a play's expectations are judged against (dsl 0.22.0 §4): one row
/// per executed step (with the world after it, when its `expect:` judges
/// it), and the world the play ended in.
pub(super) fn play_outcome(p: &ExecProject, play: &Playthrough) -> PlayOutcome {
    let steps = play
        .steps
        .iter()
        .filter_map(|s| {
            let mut row = StepOutcome {
                index: s.n,
                label: s.label.clone(),
                world: s.world.clone(),
                ..StepOutcome::default()
            };
            // dsl 0.27.0 (T3-8): `winner`, `offered` and `notOffered` judge
            // the step's last raise — an occasion step's own; an
            // `advance:`'s raise where the clock stops: the clock's slot
            // occasion, else the last midnight's `dayStart` / `dayEnd`.
            let stop = match &s.body {
                StepBody::Advance { days, raised, .. } => raised
                    .as_deref()
                    .or_else(|| days.last().map(|d| &*d.occasion)),
                body => body.occasion(),
            };
            if let Some(StepBody::Occasion {
                occasion,
                target,
                candidates,
                winner,
                presented,
                ..
            }) = stop
            {
                row.occasion = occasion.clone();
                row.target = target.clone();
                row.winner = winner.clone();
                row.offered = candidates
                    .iter()
                    .filter(|c| matches!(c.verdict, Verdict::Eligible))
                    .map(|c| c.id.clone())
                    .collect();
                row.presented = presented.iter().map(|pr| pr.id.clone()).collect();
                for pr in presented {
                    offered_options(p, &pr.document, &pr.transcript, &mut row.options);
                }
            }
            // Summer R2 / lighthouse N15, T3-8: an `advance:` step's
            // `presented` spans every raise it made — each midnight's
            // `dayEnd` / `dayStart`, then the slot raise — in order, each
            // beat tagged with the raise that presented it.
            if let StepBody::Advance {
                by,
                to,
                days,
                raised,
                ..
            } = &s.body
            {
                let raises = days
                    .iter()
                    .map(|d| (&*d.occasion, d.at.as_str()))
                    .chain(raised.as_deref().map(|b| (b, to.as_str())));
                (row.presented, row.presented_from) = raises
                    .flat_map(|(b, at)| match b {
                        StepBody::Occasion {
                            occasion,
                            presented,
                            ..
                        } => presented
                            .iter()
                            .map(|pr| (pr.id.clone(), format!("{occasion} at {at}")))
                            .collect::<Vec<_>>(),
                        _ => Vec::new(),
                    })
                    .unzip();
                row.occasion = match stop {
                    Some(StepBody::Occasion { occasion, .. }) => {
                        format!("advance {by} → {occasion}")
                    }
                    _ => format!("advance {by}"),
                };
            }
            match &s.body {
                StepBody::Occasion { .. } | StepBody::Advance { .. } => {}
                StepBody::NewRun { .. } => row.occasion = "newRun".to_string(),
                StepBody::Engine { .. } => row.occasion = "engine".to_string(),
                StepBody::Event { event } => row.occasion = format!("event {event}"),
                StepBody::End => return None,
            }
            for played in s.body.days_played() {
                let (document, transcript) = match played {
                    Played::Quest(q) => (&q.document, &q.transcript),
                    Played::Beat(pr) => (&pr.document, &pr.transcript),
                };
                offered_options(p, document, transcript, &mut row.options);
            }
            for q in s.body.settled().chain(&s.quests) {
                offered_options(p, &q.document, &q.transcript, &mut row.options);
            }
            Some(row)
        })
        .collect();
    let (said, said_steps) = said(p, play);
    PlayOutcome {
        steps,
        end: world_view(p, &play.world, true),
        said,
        said_steps,
        exit: match &play.outcome {
            Ok(_) => "complete",
            Err(h) => h.exit_label(),
        },
        entry_aliases: p.entry_aliases.clone(),
    }
}

/// Every branch/hub presentation in `records` (one document's runner
/// transcript) -> the options it offered: the IR command's options minus
/// the ones spent (`once`) or whose guard decided false, unioned per id into
/// `into` — what a play step's `expect.options` judges (dsl 0.24.0, T3-10).
fn offered_options(
    p: &ExecProject,
    document: &str,
    records: &[Json],
    into: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let cmds = DocCmds::new(p, document, false);
    for rec in records {
        let id = match str_of(rec, "kind") {
            "choice" => str_of(rec, "branch"),
            "hub" => str_of(rec, "hub"),
            _ => continue,
        };
        let listed = |key: &str, opt: &str| {
            rec.get(key)
                .and_then(Json::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(opt)))
        };
        let offered = cmds
            .get(str_of(rec, "addr"))
            .and_then(|c| c.get("options"))
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o.get("id").and_then(Json::as_str))
            .filter(|o| !listed("spent", o) && !listed("ineligible", o))
            .map(str::to_string);
        into.entry(id.to_string()).or_default().extend(offered);
    }
}

/// One branch/hub a play answered (T3-20): the document, the branch/hub
/// id, the option it chose, the options offered there (eligible and not
/// spent) and how many the construct declares — what `lute test
/// --coverage` folds into its chosen-vs-never-chosen table.
pub(crate) struct PlayedChoice {
    pub document: String,
    pub id: String,
    pub chose: Option<String>,
    pub offered: BTreeSet<String>,
    pub total: usize,
}

/// Every branch/hub record in `records` (one document's runner transcript)
/// as a [`PlayedChoice`], read as [`offered_options`] reads it.
pub(super) fn played_choices(
    p: &ExecProject,
    document: &str,
    records: &[Json],
    into: &mut Vec<PlayedChoice>,
) {
    let cmds = DocCmds::new(p, document, false);
    for rec in records {
        let id = match str_of(rec, "kind") {
            "choice" => str_of(rec, "branch"),
            "hub" => str_of(rec, "hub"),
            _ => continue,
        };
        let listed = |key: &str, opt: &str| {
            rec.get(key)
                .and_then(Json::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(opt)))
        };
        let options: Vec<&str> = cmds
            .get(str_of(rec, "addr"))
            .and_then(|c| c.get("options"))
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o.get("id").and_then(Json::as_str))
            .collect();
        into.push(PlayedChoice {
            document: document.to_string(),
            id: id.to_string(),
            chose: rec.get("chose").and_then(Json::as_str).map(str::to_string),
            offered: options
                .iter()
                .filter(|o| !listed("spent", o) && !listed("ineligible", o))
                .map(|o| o.to_string())
                .collect(),
            total: options.len(),
        });
    }
}

/// Judge the script's expectations; empty when it carries none.
pub(super) fn judge(script: &PlayScript, outcome: &PlayOutcome) -> Vec<ExpectMiss> {
    if !script.has_expect() {
        return Vec::new();
    }
    crate::play_expect::check(outcome, &script.step_expects, script.expect.as_ref())
}
