//! Presenting a beat — a scene, an entry or a bundle beat walked from the
//! world — and the `once` / `share` spending a presentation makes.

use std::collections::{BTreeMap, BTreeSet};

use crate::index::{AdvanceSpec, BeatKind, IndexBeat};
use lute_manifest::clock::Advance;
use serde_json::Value as Json;

use super::eligibility::kind_label;
use super::project::ExecProject;
use super::resolve::ever_read_path;
use super::walk::{absorb, play_machine, walk_stop, PlayHalt};
use super::world::{clock_at, json_to_value, World};
use super::step::StepBody;
use crate::datalog::Fact;
use crate::{Carry, Seed};
use crate::{MockSet, Value};
/// Maximum depth of a declared-advance cascade before a likely cycle is
/// rejected. Ordinary plays have one or a small finite chain; this is only a
/// runtime guard against a beat raising itself (or a cycle of beats).
pub const MAX_ADVANCE_CASCADE: usize = 64;

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
    pub state_after_body: BTreeMap<String, Value>,
    pub facts_after_body: BTreeSet<Fact>,
    /// The member a kind-target beat was raised for (`occasion.target`).
    pub member: Option<String>,
    /// The world's base facts before and after the presentation.
    pub facts_before: BTreeSet<Fact>,
    pub facts_after: BTreeSet<Fact>,
    pub raised: Vec<Presented>,
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
/// presented (for `member`, a `for` beat's) — `once: day` / `once: slot`
/// are spent until the clock leaves that day / slot — for every beat its
/// presentation spends (dsl 0.25.0 §2, [`spend_keys`]). Nothing without a
/// clock (such a beat is `E-BEAT-ATTR`).
pub fn spend_at_clock(p: &ExecProject, w: &mut World, id: &str, member: Option<&str>) {
    if let Some(at) = p
        .index
        .clock
        .as_ref()
        .and_then(|c| crate::clock::position(c, &w.state))
    {
        for key in spend_keys(p, id, member) {
            w.spent_at.insert(key, at.clone());
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

/// The `once` keys one presentation of `id` (for `member`) spends: every
/// beat of its [`spend_group`], a `for` beat's per member (dsl 0.28.0,
/// [`spend_key`](crate::cadence::spend_key).
fn spend_keys(p: &ExecProject, id: &str, member: Option<&str>) -> Vec<String> {
    spend_group(p, id)
        .into_iter()
        .map(|m| match p.index.beats.iter().find(|b| b.id == m) {
            Some(b) => crate::cadence::spend_key(b, member),
            None => m.to_string(),
        })
        .collect()
}

/// Spend the `once: run` (when `run`) and `once: user` of every beat a
/// presentation of `id` (for `member`) spends ([`spend_keys`]), and
/// remember who spent a `share` key.
pub fn spend_shared(p: &ExecProject, w: &mut World, id: &str, member: Option<&str>, run: bool) {
    for key in spend_keys(p, id, member) {
        if run {
            w.spent_run.insert(key.clone());
        }
        w.spent_user.insert(key);
    }
    if let Some(key) = share_of(p, id) {
        w.share_spent_by.insert(key.to_string(), id.to_string());
    }
}

/// Present `beat`: a scene through the Machine (`scene.*` fresh), an entry
/// through its entry path (first-read effects, `entry.<id>.read`), a bundle
/// beat through its `beat` record's body (dsl 0.23.0 §4). `member` is the
/// raised member a kind beat reads as `occasion.target` (dsl 0.26.0 §5).
pub fn present<B, C, F: super::WalkDriver>(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    mock: &MockSet<B, C>,
    factory: &mut F,
) -> (Presented, Option<PlayHalt>) {
    factory.event(crate::runtime::Event::Presentation {
        occasion: (!beat.on.is_empty()).then(|| beat.on.clone()),
        target: member.map(str::to_string),
        beat: beat.id.clone(),
        document: beat.document.clone(),
        kind: match beat.kind { BeatKind::Scene => "scene", BeatKind::Entry => "entry", BeatKind::Bundle => "bundle" }.into(),
    });
    let doc_json = &p.artifacts[&beat.document];
    let state_before = w.state.clone();
    let (facts_before, visited_before, quests_before) =
        (w.facts.clone(), w.visited.clone(), w.quests.clone());
    let mut carry = Carry::world(
        scene_initial_state(doc_json, &w.state),
        w.facts.clone(),
        w.quests.clone(),
    );
    carry.quest_instances = w.quest_instances.clone();
    let mut m = play_machine(
        p,
        w,
        &beat.document,
        Seed::from(mock),
        carry,
        &mock.choose,
        factory,
    )
    .with_display_names(&p.display_names);
    m.bind_occasion_target(member);
    let mut m = match beat.kind {
        BeatKind::Entry => m.with_entry(&beat.id),
        BeatKind::Scene => m,
        BeatKind::Bundle => m.with_bundle_beat(&beat.id),
    };
    let result = m.run();
    let outcome = factory.finish(m);
    factory.event(crate::runtime::Event::PresentationEnd {
        beat: beat.id.clone(),
        document: beat.document.clone(),
        reason: None,
    });
    absorb(w, &outcome);
    match beat.kind {
        BeatKind::Scene | BeatKind::Bundle => {
            w.visited.insert(beat.id.clone());
            spend_shared(p, w, &beat.id, member, true);
            spend_at_clock(p, w, &beat.id, member);
            crate::cadence::spend_season(p, w, &beat.id, member);
        }
        // dsl 0.22.0 §7: a completed first read sets the user-tier
        // `everRead` beside the runner's run-tier `read`; never reset. dsl
        // 0.25.0 §2: a shared entry's read spends its key's other beats.
        // dsl 0.28.0 (T1-6): a `for` entry is read per member.
        BeatKind::Entry => {
            let read = crate::cadence::entry_read_flag(beat, member);
            if w.state.get(&read) == Some(&Value::Bool(true)) {
                w.state.insert(ever_read_path(&beat.id), Value::Bool(true));
                if beat.share.is_some() || beat.for_kind.is_some() {
                    spend_shared(p, w, &beat.id, member, true);
                }
                spend_at_clock(p, w, &beat.id, member);
                crate::cadence::spend_season(p, w, &beat.id, member);
            }
        }
    }
    let what = format!(
        "{} `{}` ({})",
        kind_label(beat.kind),
        beat.id,
        beat.document
    );
    let mut stop = walk_stop(factory, result, &outcome, &what, doc_json);
    let state_after_body = w.state.clone();
    let facts_after_body = w.facts.clone();
    let mut raised = Vec::new();
    // dsl 0.31.0 §1: a declared beat advance is the same clock movement as
    // an explicit `advance:` step.
    if stop.is_none() {
        if let Some(spec) = beat.advances {
            if w.advance_cascade_depth >= MAX_ADVANCE_CASCADE {
                stop = Some(PlayHalt::Error(format!(
                    "E-ADVANCE-CASCADE: beat `{}` exceeded the maximum nested \
                     `advances:` cascade depth ({MAX_ADVANCE_CASCADE}); the clock raise \
                     likely loops back to this beat",
                    beat.id
                )));
            } else if let Some(clock) = p.index.clock.as_ref() {
                let by = match spec {
                    AdvanceSpec::Slot => Advance::Slots(1),
                    AdvanceSpec::Day => Advance::Day,
                    AdvanceSpec::Slots(n) => Advance::Slots(n),
                };
                let raise = clock
                    .raise
                    .as_ref()
                    .map_or_else(Default::default, |r| r.moments());
                let clock_before = clock_at(p, w);
                w.advance_cascade_depth += 1;
                let (advance_body, _, raised_stop) = super::advance::run_advance(
                    p,
                    w,
                    0,
                    by,
                    &Default::default(),
                    &raise,
                    &None,
                    &BTreeMap::new(),
                    factory,
                );
                w.advance_cascade_depth -= 1;
                if let (Some(from), Some(to)) = (clock_before, clock_at(p, w)) {
                    if from != to {
                        factory.event(crate::runtime::Event::Clock { from: serde_json::json!({ "day": from.day, "slot": from.slot }), to: serde_json::json!({ "day": to.day, "slot": to.slot }), passed: None });
                    }
                }
                if let StepBody::Advance { raised: Some(occasion), .. } = advance_body {
                    if let StepBody::Occasion { presented, .. } = *occasion {
                        raised = presented;
                    }
                }
                w.clock_advanced_by_beat = true;
                stop = raised_stop;
            }
        }
    }
    let presented = Presented {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        transcript: outcome.transcript,
        state_before,
        state_after: w.state.clone(),
        state_after_body,
        member: member.map(str::to_string),
        facts_before,
        facts_after: w.facts.clone(),
        facts_after_body,
        raised,
        visited_before,
        quests_before,
        bridges: outcome.bridges,
    };
    (presented, stop)
}

/// [`present`] with the script's `choose:`, the step's own `choose:`
/// replacing it key by key (dsl 0.22.0 §2). A step-local decision list is
/// consumed from its start and leaves the script-wide list's consumption
/// where it was.
pub fn present_with_choose<F: super::WalkDriver>(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    step_choose: &BTreeMap<String, Vec<String>>,
    factory: &mut F,
) -> (Presented, Option<PlayHalt>) {
    let mut mock = w.mock();
    mock.choose = step_choose.clone();
    present(p, w, beat, member, &mock, factory)
}
