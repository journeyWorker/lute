use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use lute_runtime::session::{
    decision_options, state_entry_type, ExecProject, PlayHalt, SessionEvalObserver, WalkDriver,
    Walked, World,
};
use lute_runtime::{
    BridgeAnswer, BridgeCall, BridgeQueues, Driver, Forced, GuardRead, Machine, Menu, OnUnknown,
    Pick, ScriptedChoices, UnknownSite, Verdict,
};
use serde_json::Value as Json;

use super::events::ScriptAnswer;
use super::producers::{decisions_of, Decision, Premises, Producers};

/// The CLI-owned state used to drive and resume play walks.
///
/// Choices, bridge queues, and condition-dump observation are deliberately
/// kept here rather than in the runtime world. A fresh [`PlayDriver`] is made
/// for every machine walk; [`WalkDriver::finish`] folds its cursor and bridge
/// consumption back into this state.
#[derive(Default)]
pub(crate) struct PlayDriverState {
    pub(crate) choose: BTreeMap<String, Vec<String>>,
    pub(crate) choice_cursor: BTreeMap<String, usize>,
    pub(crate) bridges: BridgeQueues,
    pub(crate) decisions: Vec<Decision>,
    pub(crate) step: usize,
    pub(crate) document: String,
    pub(crate) records: Vec<(String, Json)>,
    /// Every decision and bridge answer the walks gave, in order — what
    /// `lute play --events` answers the runtime's awaits with.
    pub(crate) answers: Vec<ScriptAnswer>,
    override_cursors: Vec<Vec<(String, Option<usize>)>>,
    observer: Option<SessionEvalObserver>,
    producers: Option<Arc<Producers>>,
}

#[allow(dead_code)]
impl PlayDriverState {
    pub(crate) fn new(
        choose: BTreeMap<String, Vec<String>>,
        bridges: BridgeQueues,
        producers: Option<Arc<Producers>>,
    ) -> Self {
        Self { choose, bridges, producers, ..Self::default() }
    }
    pub(crate) fn for_project(
        project: &ExecProject,
        choose: BTreeMap<String, Vec<String>>,
        bridges: BridgeQueues,
    ) -> Self {
        let reserved = project
            .index
            .relations
            .iter()
            .filter(|r| r.reserved)
            .map(|r| r.name.clone())
            .collect();
        Self::new(
            choose,
            bridges,
            Some(Arc::new(Producers::of(&project.artifacts, reserved))),
        )
    }

    pub(crate) fn with_observer(mut self, observer: Option<SessionEvalObserver>) -> Self {
        self.observer = observer;
        self
    }

    pub(crate) fn set_observer(&mut self, observer: Option<SessionEvalObserver>) {
        self.observer = observer;
    }

    pub(crate) fn set_step(&mut self, step: usize, choose: &BTreeMap<String, Vec<String>>, bridges: &BTreeMap<String, Vec<BridgeAnswer>>) {
        self.step = step;
        self.choose.extend(choose.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.bridges.step = BridgeQueues::queue(bridges);
    }

    pub(crate) fn set_top_choose(&mut self, choose: BTreeMap<String, Vec<String>>) {
        self.choose = choose;
    }

    pub(crate) fn top_bridges_mut(&mut self) -> &mut BTreeMap<String, VecDeque<BridgeAnswer>> {
        &mut self.bridges.top
    }
}

pub(crate) struct PlayDriver {
    pub(crate) choices: ScriptedChoices,
    pub(crate) bridges: BridgeQueues,
    pub(crate) transcript: Vec<Json>,
    pub(crate) premises: Premises,
    /// The decisions and bridge answers this walk gave, in order.
    pub(crate) answers: Vec<ScriptAnswer>,
}

impl PlayDriver {
    fn new(
        choose: &BTreeMap<String, Vec<String>>,
        state: &PlayDriverState,
        document: &str,
        _world: &World,
    ) -> Self {
        Self {
            choices: ScriptedChoices::new(choose.clone(), state.choice_cursor.clone()),
            bridges: state.bridges.clone(),
            transcript: Vec::new(),
            premises: Premises {
                producers: state.producers.clone(),
                decisions: state.decisions.clone(),
                document: document.to_string(),
                step: state.step,
            },
            answers: Vec::new(),
        }
    }
}

impl Driver for PlayDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        let pick = self.choices.pick(menu);
        if let Pick::Option(option) = &pick {
            self.answers.push(ScriptAnswer::Choice { menu: menu.id.to_string(), option: option.clone() });
        }
        pick
    }

    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, verdict: &Verdict) -> Forced {
        match verdict {
            Verdict::Spent | Verdict::Closed(_) => Forced::Refuse,
            Verdict::Open | Verdict::Unknown(_) => Forced::Take,
        }
    }

    fn bridge(&mut self, call: &BridgeCall<'_>) -> lute_runtime::BridgeReply {
        match self.bridges.next(call.tag) {
            Some(answer) => {
                self.answers.push(ScriptAnswer::Bridge { tag: call.tag.to_string(), fields: answer.clone() });
                lute_runtime::BridgeReply::Answer(answer)
            }
            None => lute_runtime::BridgeReply::Unanswered,
        }
    }

    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown { OnUnknown::Halt }

    fn emit(&mut self, rec: Json) { self.transcript.push(rec); }

    fn premise_hint(&self, read: &GuardRead) -> String {
        let Some(producers) = &self.premises.producers else { return String::new() };
        let mut decisions = self.premises.decisions.clone();
        decisions.extend(decisions_of(&self.premises.document, self.premises.step, &self.transcript));
        producers.hint(read, &decisions)
    }
}

impl WalkDriver for PlayDriverState {
    type Driver = PlayDriver;

    fn new_driver(
        &mut self,
        document: &str,
        choose: &BTreeMap<String, Vec<String>>,
        world: &World,
    ) -> Self::Driver {
        self.document = document.to_string();
        let overrides = choose
            .iter()
            .filter(|(key, value)| self.choose.get(*key) != Some(*value))
            .map(|(key, _)| (key.clone(), self.choice_cursor.remove(key)))
            .collect();
        self.override_cursors.push(overrides);
        PlayDriver::new(choose, self, document, world)
    }

    fn finish(&mut self, machine: Machine<Self::Driver>) -> Walked {
        let before = self.bridges.clone();
        let (carry, driver) = machine.into_carry();
        self.choice_cursor = driver.choices.cursor.clone();
        self.bridges = driver.bridges.clone();
        if let Some(overrides) = self.override_cursors.pop() {
            for (key, cursor) in overrides {
                match cursor {
                    Some(cursor) => {
                        self.choice_cursor.insert(key, cursor);
                    }
                    None => {
                        self.choice_cursor.remove(&key);
                    }
                }
            }
        }
        self.decisions.extend(decisions_of(
            &driver.premises.document,
            driver.premises.step,
            &driver.transcript,
        ));
        self.records.extend(driver.transcript.iter().cloned().map(|record| (driver.premises.document.clone(), record)));
        self.answers.extend(driver.answers);
        Walked {
            carry,
            transcript: driver.transcript,
            bridges: consumed_bridges(&before, &self.bridges),
        }
    }
    fn halt(&self, result: Result<(), String>, outcome: &Walked, what: &str, doc_json: &Json) -> Option<PlayHalt> {
        walk_stop(result, outcome, what, doc_json)
    }

    fn observer(&self) -> Option<SessionEvalObserver> { self.observer.clone() }
}


pub(crate) fn outcome_halt(outcome: &Walked, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    if outcome.carry.incomplete {
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| c.get("note").and_then(Json::as_str) == Some(lute_runtime::NOTE_NO_DECISION)) {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("choice");
            let id = rec.get("branch").or_else(|| rec.get("hub")).and_then(Json::as_str).unwrap_or("?");
            let options = decision_options(doc_json, id);
            let used_up = match (kind, rec.get("scripted").and_then(Json::as_u64)) {
                ("hub", Some(n)) => format!(" — the hub is still open after the {n} scripted pick(s) of its `choose:` list"),
                (_, Some(n)) => format!(" — all {n} decisions of its `choose:` list were used by earlier presentations"),
                (_, None) => String::new(),
            };
            return Some(PlayHalt::Incomplete(format!("{what} reached {kind} `{id}` with no scripted `choose:` decision{used_up} (options: {})", if options.is_empty() { "none".to_string() } else { options.join(", ") })));
        }
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| c.get("kind").and_then(Json::as_str) == Some("plugin") && c.get("unanswered").is_some()) {
            let tag = rec.get("tag").and_then(Json::as_str).unwrap_or("?");
            let strs = |key: &str| -> Vec<&str> { rec.get(key).and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_str).collect() };
            let types: Vec<Option<lute_manifest::types::Type>> = strs("unresolvedEffects").into_iter().map(|p| state_entry_type(doc_json, p)).collect();
            let shape = lute_runtime::bridge_answer_shape(strs("unanswered").into_iter().zip(types.iter().map(Option::as_ref)));
            return Some(PlayHalt::Incomplete(format!("{what}: plugin call `{tag}` reads a bridge result and has no answer — give one with `bridges: {{ {tag}: [ {shape} ] }}` (top level or on the step)")));
        }
        if let Some(rec) = outcome.transcript.iter().find(|c| c.get("kind").and_then(Json::as_str) == Some("objective") && (c.get("done").is_some_and(Json::is_null) || c.get("failed").is_some_and(Json::is_null))) {
            let slot = if rec.get("failed").is_some_and(Json::is_null) { "`by` condition" } else { "`done` condition" };
            return Some(PlayHalt::Incomplete(format!("{what}: required objective `{}.{}` has a {slot} that evaluates unknown", rec.get("quest").and_then(Json::as_str).unwrap_or("?"), rec.get("objective").and_then(Json::as_str).unwrap_or("?"))));
        }
        return Some(PlayHalt::Incomplete(format!("{what} is incomplete")));
    }
    if outcome.carry.unresolved.iter().any(|a| matches!(a, lute_runtime::UnresolvedAtom::Time)) {
        return Some(PlayHalt::Incomplete(format!("{what} depends on now()/validAt(...), which the reference runner cannot resolve")));
    }
    None
}

pub(crate) fn walk_stop(result: Result<(), String>, outcome: &Walked, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    match result {
        Err(msg) if outcome.carry.refused => Some(PlayHalt::Error(format!("{what}: {msg}"))),
        Err(msg) => Some(PlayHalt::Fatal(format!("{what}: {msg}"))),
        Ok(()) => outcome_halt(outcome, what, doc_json),
    }
}

pub(crate) fn consumed_bridges(before: &BridgeQueues, after: &BridgeQueues) -> BTreeMap<String, Vec<BridgeAnswer>> {
    let mut out = BTreeMap::new();
    for (was, now) in [(&before.step, &after.step), (&before.top, &after.top)] {
        for (tag, queue) in was {
            let left = now.get(tag).map_or(0, VecDeque::len);
            let taken = queue.len().saturating_sub(left);
            if taken > 0 { out.entry(tag.clone()).or_insert_with(Vec::new).extend(queue.iter().take(taken).cloned()); }
        }
    }
    out
}
