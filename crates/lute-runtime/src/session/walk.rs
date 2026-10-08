//! One walk through the [`Machine`], its folded carry and the runtime halt
//! kinds.  A caller-owned [`WalkDriver`] supplies the walk driver and builds
//! any play/report messages.

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value as Json;

use super::project::ExecProject;
use super::world::World;
use crate::{BridgeQueues, Carry, Machine, Seed};
use crate::UnresolvedAtom;

/// A finished walk: its carry and host-collected transcript.
pub struct Walked {
    pub carry: Carry,
    pub transcript: Vec<Json>,
    /// Bridge answers consumed by this walk, grouped by plugin tag.
    pub bridges: BTreeMap<String, Vec<crate::BridgeAnswer>>,
}

/// Fold a finished walk back into the world: persistent tiers only
/// (`scene.*` never carries), facts, quest statuses, the accepts it made
/// (dsl 0.21.0 §7a.3) for the next quest advance, and how far it consumed
/// the script's multi-decision `choose:` lists and the bridge answers.
pub fn absorb(w: &mut World, outcome: &Walked) {
    let carry = &outcome.carry;
    for (k, v) in &carry.state {
        // dsl 0.26.0 §5: `occasion.target` lives only while its beat runs.
        if !k.starts_with("scene.") && k != lute_manifest::semantics::beats::OCCASION_TARGET {
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
    w.failed_objectives
        .extend(carry.failed_objectives.iter().cloned());
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

/// How a finished runner walk halts the playthrough, if it does.
pub fn walk_stop<F: super::WalkDriver>(
    factory: &F,
    result: Result<(), String>,
    outcome: &Walked,
    what: &str,
    doc_json: &Json,
) -> Option<PlayHalt> {
    match result {
        Err(msg) => factory.halt(Err(msg), outcome, what, doc_json),
        Ok(()) => factory.halt(Ok(()), outcome, what, doc_json),
    }
}


/// Build one walk from a caller-owned driver factory.
pub fn play_machine<F: super::WalkDriver>(
    p: &ExecProject,
    w: &World,
    doc: &str,
    seed: Seed,
    carry: Carry,
    choose: &BTreeMap<String, Vec<String>>,
    factory: &mut F,
) -> Machine<F::Driver> {
    let machine = Machine::resume_with_project_schema(
        &p.artifacts[doc],
        std::sync::Arc::clone(&p.codes[doc]),
        seed.clone(),
        carry,
        factory.new_driver(doc, choose, w),
        p.store_schemas[seed.derive as usize].clone(),
    )
    .with_visited(&w.visited)
    .with_bridge_reads(p.bridge_reads.clone());
    let Some(observer) = factory.observer() else {
        return machine;
    };
    machine.with_eval_observer(move |raw, value, atoms, snapshot| {
        observer(raw, value, atoms, snapshot);
    })
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
