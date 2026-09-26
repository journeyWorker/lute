//! Presenting a beat — a scene, an entry or a bundle beat walked from the
//! world — and the `once` / `share` spending a presentation makes.

use std::collections::{BTreeMap, BTreeSet};

use lute_compile::index::{BeatKind, IndexBeat};
use serde_json::Value as Json;

use super::eligibility::kind_label;
use super::project::ExecProject;
use super::resolve::ever_read_path;
use super::walk::{absorb, consumed_bridges, play_machine, walk_stop, PlayHalt, Walked};
use super::world::{json_to_value, World};
use crate::datalog::Fact;
use crate::exec::{Carry, Seed};
use crate::{MockSet, Value};

/// One presented beat. `pub(crate)` with the `*_before` / `facts_after` /
/// `bridges` / `member` captures for the differential harness
/// (`crate::differential`, docs/design/runtime-unification.md §4.3), which
/// replays each presentation through trace from exactly this world.
#[derive(Clone)]
pub struct Presented {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub transcript: Vec<Json>,
    pub state_before: BTreeMap<String, Value>,
    pub state_after: BTreeMap<String, Value>,
    /// The member a kind-target beat was raised for (`occasion.target`).
    pub member: Option<String>,
    /// The world's base facts before and after the presentation.
    pub facts_before: BTreeSet<Fact>,
    pub facts_after: BTreeSet<Fact>,
    /// The presented scenes `visited(…)` read before the presentation.
    pub visited_before: BTreeSet<String>,
    /// Quest id -> state before the presentation.
    pub quests_before: BTreeMap<String, String>,
    /// The bridge answers the presentation consumed, per tag, in call order.
    pub bridges: BTreeMap<String, Vec<crate::BridgeAnswer>>,
}

/// A scene's fresh starting state: its OWN `scene.*` defaults (never the
/// union's, which may carry another document's same-named scene path), with
/// the world's persistent tiers overlaid.
pub fn scene_initial_state(
    doc_json: &Json,
    live: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    for e in doc_json
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = e.get("path").and_then(Json::as_str) else {
            continue;
        };
        if path.starts_with("scene.") {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                state.insert(path.to_string(), v);
            }
        }
    }
    for (k, v) in live {
        state.insert(k.clone(), v.clone());
    }
    state
}

/// dsl 0.24.0 §1: record where on the declared clock beat `id` was just
/// presented — `once: day` / `once: slot` are spent until the clock leaves
/// that day / slot — for every beat its presentation spends (dsl 0.25.0
/// §2, [`spend_group`]). Nothing without a clock (such a beat is
/// `E-BEAT-ATTR`).
pub fn spend_at_clock(p: &ExecProject, w: &mut World, id: &str) {
    if let Some(at) = p
        .index
        .clock
        .as_ref()
        .and_then(|c| crate::clock::position(c, &w.state))
    {
        for m in spend_group(p, id) {
            w.spent_at.insert(m.to_string(), at.clone());
        }
    }
}

/// dsl 0.25.0 §2: beat `id`'s `share` key, when it declares one.
pub fn share_of<'p>(p: &'p ExecProject, id: &str) -> Option<&'p str> {
    p.index.beats.iter().find(|b| b.id == id)?.share.as_deref()
}

/// dsl 0.25.0 §2: the beats one presentation of `id` spends — every beat of
/// its `share` key (itself included), else `id` alone.
pub fn spend_group<'p>(p: &'p ExecProject, id: &'p str) -> Vec<&'p str> {
    match share_of(p, id) {
        Some(key) => p
            .index
            .beats
            .iter()
            .filter(|b| b.share.as_deref() == Some(key))
            .map(|b| b.id.as_str())
            .collect(),
        None => vec![id],
    }
}

/// Spend the `once: run` (when `run`) and `once: user` of every beat a
/// presentation of `id` spends ([`spend_group`]), and remember who spent a
/// `share` key.
pub fn spend_shared(p: &ExecProject, w: &mut World, id: &str, run: bool) {
    for m in spend_group(p, id) {
        if run {
            w.spent_run.insert(m.to_string());
        }
        w.spent_user.insert(m.to_string());
    }
    if let Some(key) = share_of(p, id) {
        w.share_spent_by.insert(key.to_string(), id.to_string());
    }
}

/// Present `beat`: a scene through the Machine (`scene.*` fresh), an entry
/// through its entry path (first-read effects, `entry.<id>.read`), a bundle
/// beat through its `beat` record's body (dsl 0.23.0 §4). `member` is the
/// raised member a kind beat reads as `occasion.target` (dsl 0.26.0 §5).
pub fn present(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    mock: &MockSet,
) -> (Presented, Option<PlayHalt>) {
    let doc_json = &p.artifacts[&beat.document];
    let state_before = w.state.clone();
    let (facts_before, visited_before, quests_before, bridges_before) = (
        w.facts.clone(),
        w.visited.clone(),
        w.quests.clone(),
        w.bridges.clone(),
    );
    let carry = Carry::world(
        scene_initial_state(doc_json, &w.state),
        w.facts.clone(),
        w.quests.clone(),
    );
    let mut m = play_machine(p, w, doc_json, Seed::from(mock), carry, &mock.choose)
        .with_display_names(&p.display_names);
    m.bind_occasion_target(member);
    let mut m = match beat.kind {
        BeatKind::Entry => m.with_entry(&beat.id),
        BeatKind::Scene => m,
        BeatKind::Bundle => m.with_bundle_beat(&beat.id),
    };
    let result = m.run();
    let outcome = Walked::of(m);
    absorb(w, &outcome);
    match beat.kind {
        BeatKind::Scene | BeatKind::Bundle => {
            w.visited.insert(beat.id.clone());
            spend_shared(p, w, &beat.id, true);
            spend_at_clock(p, w, &beat.id);
            crate::exec::cadence::spend_season(p, w, &beat.id);
        }
        // dsl 0.22.0 §7: a completed first read sets the user-tier
        // `everRead` beside the runner's run-tier `read`; never reset. dsl
        // 0.25.0 §2: a shared entry's read spends its key's other beats.
        BeatKind::Entry => {
            if w.state.get(&format!("entry.{}.read", beat.id)) == Some(&Value::Bool(true)) {
                w.state.insert(ever_read_path(&beat.id), Value::Bool(true));
                if beat.share.is_some() {
                    spend_shared(p, w, &beat.id, true);
                }
                spend_at_clock(p, w, &beat.id);
                crate::exec::cadence::spend_season(p, w, &beat.id);
            }
        }
    }
    let what = format!(
        "{} `{}` ({})",
        kind_label(beat.kind),
        beat.id,
        beat.document
    );
    let stop = walk_stop(result, &outcome, &what, doc_json);
    let presented = Presented {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        transcript: outcome.transcript,
        state_before,
        state_after: w.state.clone(),
        member: member.map(str::to_string),
        facts_before,
        facts_after: w.facts.clone(),
        visited_before,
        quests_before,
        bridges: consumed_bridges(&bridges_before, &w.bridges),
    };
    (presented, stop)
}

/// [`present`] with the script's `choose:`, the step's own `choose:`
/// replacing it key by key (dsl 0.22.0 §2). A step-local decision list is
/// consumed from its start and leaves the script-wide list's consumption
/// where it was.
pub fn present_with_choose(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    step_choose: &BTreeMap<String, Vec<String>>,
) -> (Presented, Option<PlayHalt>) {
    let mut mock = w.mock();
    mock.choose = w.choose.clone();
    mock.choose
        .extend(step_choose.iter().map(|(k, v)| (k.clone(), v.clone())));
    let saved: Vec<(String, Option<usize>)> = step_choose
        .keys()
        .map(|k| (k.clone(), w.choice_cursor.remove(k)))
        .collect();
    let out = present(p, w, beat, member, &mock);
    for (k, cursor) in saved {
        match cursor {
            Some(c) => w.choice_cursor.insert(k, c),
            None => w.choice_cursor.remove(&k),
        };
    }
    out
}
