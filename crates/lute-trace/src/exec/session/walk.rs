//! One walk through the [`Machine`]: the session's [`PlayDriver`], the
//! finished [`Walked`] and its fold back into the world, and the honesty
//! gate every walk passes ([`PlayHalt`]).

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value as Json;

use super::project::{decision_options, state_entry_type, ExecProject};
use super::world::World;
use crate::exec::{
    BridgeCall, BridgeQueues, BridgeReply, Carry, Driver, Forced, Machine, Menu, OnUnknown,
    Pick as MenuPick, ScriptedChoices, Seed, UnknownSite, Verdict as OptionVerdict,
};
use crate::UnresolvedAtom;

/// The session's [`Driver`] (`lute play`, `lute calendar`, the play files of
/// `lute test`): the script's `choose:` over the playthrough's cursor, the
/// playthrough's bridge answers (the running step's, then the top level's),
/// every scripted pick of an option that is not offered refused
/// (`E-TRACE-CHOICE`, as `lute trace` refuses it — a silent skip would let
/// the script drift out of step with what the player was really offered),
/// and a halt AT every site whose value the walk cannot decide (§6 R4: a
/// play knows every value). Records are collected verbatim for the
/// renderers.
#[derive(Default)]
pub struct PlayDriver {
    pub choices: ScriptedChoices,
    pub bridges: BridgeQueues,
    pub transcript: Vec<Json>,
    /// What a refused pick's closed guard names ([`Driver::premise_hint`]):
    /// the project's producers and the play's decisions.
    pub premises: super::producers::Premises,
}

impl PlayDriver {
    /// A walk scripted by `choose`, resuming `w`'s choice cursor and bridge
    /// queues.
    pub fn new(choose: &BTreeMap<String, Vec<String>>, w: &World) -> Self {
        PlayDriver {
            choices: ScriptedChoices::new(choose.clone(), w.choice_cursor.clone()),
            bridges: w.bridges.clone(),
            transcript: Vec::new(),
            premises: Default::default(),
        }
    }
}

impl Driver for PlayDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> MenuPick {
        self.choices.pick(menu)
    }

    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, verdict: &OptionVerdict) -> Forced {
        match verdict {
            OptionVerdict::Spent | OptionVerdict::Closed(_) => Forced::Refuse,
            OptionVerdict::Open | OptionVerdict::Unknown(_) => Forced::Take,
        }
    }

    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        match self.bridges.next(call.tag) {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        // The walk halts where the value is needed; the post-walk honesty
        // gate ([`outcome_halt`] over [`Carry::unresolved`]) names it.
        OnUnknown::Halt
    }

    fn emit(&mut self, rec: Json) {
        self.transcript.push(rec);
    }

    /// A play has no mocks: a fact the guard misses names what in the
    /// project asserts it and what the play chose there so far — this
    /// walk's own decisions included.
    fn premise_hint(&self, read: &crate::exec::GuardRead) -> String {
        let pr = &self.premises;
        let Some(producers) = &pr.producers else {
            return String::new();
        };
        let mut decisions = pr.decisions.clone();
        decisions.extend(super::producers::decisions_of(
            &pr.document,
            pr.step,
            &self.transcript,
        ));
        producers.hint(read, &decisions)
    }
}

/// A finished walk: the machine's carry plus what its [`PlayDriver`]
/// collected — the transcript play's renderer reuses verbatim, and the
/// choice cursor and bridge answers the next walk resumes from.
pub struct Walked {
    pub carry: Carry,
    pub transcript: Vec<Json>,
    pub choice_cursor: BTreeMap<String, usize>,
    pub bridges: BridgeQueues,
}

impl Walked {
    pub fn of(m: Machine<PlayDriver>) -> Self {
        let (carry, d) = m.into_carry();
        Walked {
            carry,
            transcript: d.transcript,
            choice_cursor: d.choices.cursor,
            bridges: d.bridges,
        }
    }
}

/// Fold a finished walk back into the world: persistent tiers only
/// (`scene.*` never carries), facts, quest statuses, the accepts it made
/// (dsl 0.21.0 §7a.3) for the next quest advance, and how far it consumed
/// the script's multi-decision `choose:` lists and the bridge answers.
pub fn absorb(w: &mut World, outcome: &Walked) {
    let carry = &outcome.carry;
    for (k, v) in &carry.state {
        // dsl 0.26.0 §5: `occasion.target` lives only while its beat runs.
        if !k.starts_with("scene.") && k != lute_check::beats::OCCASION_TARGET {
            w.state.insert(k.clone(), v.clone());
        }
    }
    w.facts = carry.base_facts.clone();
    w.quests = carry.quest_status.clone();
    w.quest_instances = carry.quest_instances.clone();
    for id in &carry.accepted {
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    for id in &carry.accepted_next_run {
        if !w.next_run_accepts.contains(id) {
            w.next_run_accepts.push(id.clone());
        }
    }
    w.choice_cursor = outcome.choice_cursor.clone();
    w.failed_objectives
        .extend(carry.failed_objectives.iter().cloned());
    w.bridges = outcome.bridges.clone();
}

/// Why the playthrough stopped short of its last step.
pub enum PlayHalt {
    /// exit 1 — a `pick` that is not eligible, or a scripted `choose:`
    /// decision the runner refused (`E-TRACE-CHOICE`: guard false, or a
    /// spent `once` option) at its presentation point.
    Error(String),
    /// exit 2 — the runner refused a malformed artifact / unknown command.
    Fatal(String),
    /// exit 3 — something the reference runner cannot decide.
    Incomplete(String),
}

impl PlayHalt {
    /// The process exit this halt maps to (1, 2 or 3).
    pub fn exit_code(&self) -> u8 {
        match self {
            PlayHalt::Error(_) => 1,
            PlayHalt::Fatal(_) => 2,
            PlayHalt::Incomplete(_) => 3,
        }
    }

    /// The halt as every surface prints it: plain text, no spec citations
    /// ([`lute_core_span::plain_message`]).
    pub fn message(&self) -> std::borrow::Cow<'_, str> {
        match self {
            PlayHalt::Error(m) | PlayHalt::Fatal(m) | PlayHalt::Incomplete(m) => {
                lute_core_span::plain_message(m)
            }
        }
    }

    pub fn exit_label(&self) -> &'static str {
        match self {
            PlayHalt::Incomplete(_) => "incomplete",
            PlayHalt::Error(_) | PlayHalt::Fatal(_) => "error",
        }
    }
}

pub fn describe_atoms(atoms: &[UnresolvedAtom]) -> String {
    let mut parts: Vec<String> = atoms
        .iter()
        .map(|a| match a {
            UnresolvedAtom::Path(p) => format!("state path `{p}` has no value"),
            UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => {
                format!("fact `{f}` is undetermined")
            }
            UnresolvedAtom::Time => {
                "now()/validAt(...) has no reference-runtime resolution".to_string()
            }
        })
        .collect();
    parts.dedup();
    if parts.is_empty() {
        "it does not evaluate to a bool".to_string()
    } else {
        parts.join("; ")
    }
}

/// The honesty gate every finished walk passes (`what` names the
/// presentation or quest document): an unscripted decision, a plugin call
/// with no bridge answer (dsl 0.24.0 §5 — the walk halted AT the call),
/// an undecidable quest objective, `now()`/`validAt(...)`.
pub fn outcome_halt(outcome: &Walked, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    if outcome.carry.incomplete {
        if let Some(rec) =
            outcome.transcript.iter().rev().find(|c| {
                c.get("note").and_then(Json::as_str) == Some(crate::exec::NOTE_NO_DECISION)
            })
        {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("choice");
            let id = rec
                .get("branch")
                .or_else(|| rec.get("hub"))
                .and_then(Json::as_str)
                .unwrap_or("?");
            let options = decision_options(doc_json, id);
            let used_up = match (kind, rec.get("scripted").and_then(Json::as_u64)) {
                ("hub", Some(n)) => format!(
                    " — the hub is still open after the {n} scripted pick(s) of its `choose:` list"
                ),
                (_, Some(n)) => format!(
                    " — all {n} decisions of its `choose:` list were used by earlier presentations"
                ),
                (_, None) => String::new(),
            };
            return Some(PlayHalt::Incomplete(format!(
                "{what} reached {kind} `{id}` with no scripted `choose:` decision{used_up} (options: {})",
                if options.is_empty() {
                    "none".to_string()
                } else {
                    options.join(", ")
                }
            )));
        }
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("plugin") && c.get("unanswered").is_some()
        }) {
            let tag = rec.get("tag").and_then(Json::as_str).unwrap_or("?");
            let strs = |key: &str| -> Vec<&str> {
                let list = rec.get(key).and_then(Json::as_array).into_iter().flatten();
                list.filter_map(Json::as_str).collect()
            };
            // `unanswered` and `unresolvedEffects` pair each field with the
            // result slot it writes; the slot's declared type is the
            // placeholder, as in `lute trace`'s hint.
            let types: Vec<Option<lute_manifest::types::Type>> = strs("unresolvedEffects")
                .into_iter()
                .map(|p| state_entry_type(doc_json, p))
                .collect();
            let shape = crate::bridge_answer_shape(
                strs("unanswered")
                    .into_iter()
                    .zip(types.iter().map(Option::as_ref)),
            );
            return Some(PlayHalt::Incomplete(format!(
                "{what}: plugin call `{tag}` reads a bridge result and has no answer — give one \
                 with `bridges: {{ {tag}: [ {shape} ] }}` (top level or on the step)"
            )));
        }
        if let Some(rec) = outcome.transcript.iter().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("objective")
                && (c.get("done").is_some_and(Json::is_null)
                    || c.get("failed").is_some_and(Json::is_null))
        }) {
            let slot = if rec.get("failed").is_some_and(Json::is_null) {
                "`by` condition"
            } else {
                "`done` condition"
            };
            return Some(PlayHalt::Incomplete(format!(
                "{what}: required objective `{}.{}` has a {slot} that evaluates unknown",
                rec.get("quest").and_then(Json::as_str).unwrap_or("?"),
                rec.get("objective").and_then(Json::as_str).unwrap_or("?"),
            )));
        }
        return Some(PlayHalt::Incomplete(format!("{what} is incomplete")));
    }
    if outcome
        .carry
        .unresolved
        .iter()
        .any(|a| matches!(a, UnresolvedAtom::Time))
    {
        return Some(PlayHalt::Incomplete(format!(
            "{what} depends on now()/validAt(...), which the reference runner cannot resolve"
        )));
    }
    None
}

/// How a finished runner walk halts the playthrough, if it does: a refused
/// scripted decision (`E-TRACE-CHOICE`) is an error like an ineligible
/// `pick:` (exit 1); any other runner failure is fatal (exit 2); then the
/// honesty gate. A `::end` is not a halt: it ends the walk it ran in (one
/// presentation, or one quest document's advance) and the playthrough
/// goes on with the next step (0.23.1) — only a script `end: true` ends it.
pub fn walk_stop(
    result: Result<(), String>,
    outcome: &Walked,
    what: &str,
    doc_json: &Json,
) -> Option<PlayHalt> {
    match result {
        Err(msg) if outcome.carry.refused => Some(PlayHalt::Error(format!("{what}: {msg}"))),
        Err(msg) => Some(PlayHalt::Fatal(format!("{what}: {msg}"))),
        Ok(()) => outcome_halt(outcome, what, doc_json),
    }
}

/// A presentation or quest walk of the project document `doc`, resumed from
/// `carry`, scripted by `choose` over `w`'s choice cursor and bridge
/// answers, reading `w`'s presented scenes and the project's bridge-result
/// readers.
pub fn play_machine(
    p: &ExecProject,
    w: &World,
    doc: &str,
    seed: Seed,
    carry: Carry,
    choose: &BTreeMap<String, Vec<String>>,
) -> Machine<PlayDriver> {
    let machine = Machine::resume_with_project_schema(
        &p.artifacts[doc],
        std::sync::Arc::clone(&p.codes[doc]),
        seed.clone(),
        carry,
        PlayDriver::new(choose, w),
        p.store_schemas[seed.derive as usize].clone(),
    )
    .with_visited(&w.visited)
    .with_bridge_reads(p.bridge_reads.clone());
    w.observe_machine(machine)
}

/// The bridge answers a walk consumed: per tag, the head of `before`'s step
/// queue then of its top queue, as many as `after` no longer holds.
pub fn consumed_bridges(
    before: &BridgeQueues,
    after: &BridgeQueues,
) -> BTreeMap<String, Vec<crate::BridgeAnswer>> {
    let mut out: BTreeMap<String, Vec<crate::BridgeAnswer>> = BTreeMap::new();
    for (was, now) in [(&before.step, &after.step), (&before.top, &after.top)] {
        for (tag, queue) in was {
            let left = now.get(tag).map_or(0, VecDeque::len);
            let taken = queue.len().saturating_sub(left);
            if taken > 0 {
                out.entry(tag.clone())
                    .or_default()
                    .extend(queue.iter().take(taken).cloned());
            }
        }
    }
    out
}
