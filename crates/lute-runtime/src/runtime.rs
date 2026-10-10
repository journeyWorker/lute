//! The host-facing, resumable runtime API (scenario DSL 0.38.0 §§3, 5–9).
//!
//! A suspended input is resumed by replaying the input from its pre-input
//! world with the answers already supplied.  This deliberately keeps the
//! execution engine and the public state model independent of any host UI.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

mod arc_world {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &Arc<crate::session::World>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        v.as_ref().serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Arc<crate::session::World>, D::Error> {
        Ok(Arc::new(crate::session::World::deserialize(d)?))
    }
}
use crate::driver::{
    BridgeCall, BridgeReply, Driver, Forced, Menu, MenuKind, OnUnknown, Pick, UnknownSite, Verdict,
};
use crate::index::Bundle;
use crate::session::{
    clock_at, resolve_fact, resolve_state, ExecProject, Pick as SessionPick, PlayHalt, SaveSeed,
    Session, StepBody, WalkDriver, Walked, World, WorldSeed, Write, Writes,
};
use crate::Machine;

/// The event contract version outputs and snapshots are stamped with.
pub const EVENT_VERSION: &str = "0.39.0";
/// The execution IR version this runtime executes (major.minor gate).
pub const IR_VERSION: &str = "0.39.0";

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateWrite {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<f64>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritesInput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<StateWrite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retract: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accept: Vec<String>,
}
impl WritesInput {
    fn is_empty(&self) -> bool {
        self.state.is_empty()
            && self.facts.is_empty()
            && self.retract.is_empty()
            && self.accept.is_empty()
    }
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PickInput {
    Pass(String),
    Beat { beat: String },
}

impl PickInput {
    fn session(&self) -> Result<SessionPick, String> {
        match self {
            PickInput::Pass(s) if s == "pass" => Ok(SessionPick::Pass),
            PickInput::Beat { beat } => Ok(SessionPick::Beat(beat.clone())),
            PickInput::Pass(s) => Err(format!("pick must be `pass`, not `{s}`")),
        }
    }
}

/// The seven inputs in §5.1.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Input {
    #[serde(rename = "raiseOccasion")]
    RaiseOccasion {
        occasion: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        payload: BTreeMap<String, Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pick: Option<PickInput>,
        #[serde(default, skip_serializing_if = "WritesInput::is_empty")]
        writes: WritesInput,
    },
    #[serde(rename = "choose")]
    Choose { request: u64, option: String },
    #[serde(rename = "bridgeResult")]
    BridgeResult {
        request: u64,
        fields: BTreeMap<String, Json>,
    },
    #[serde(rename = "advanceClock")]
    AdvanceClock {
        by: AdvanceBy,
        #[serde(default, skip_serializing_if = "WritesInput::is_empty")]
        writes: WritesInput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pick: Option<PickInput>,
    },
    #[serde(rename = "hostWrite")]
    HostWrite { writes: WritesInput },
    #[serde(rename = "worldEvent")]
    WorldEvent { name: String },
    #[serde(rename = "newRun")]
    NewRun {
        #[serde(default, skip_serializing_if = "WritesInput::is_empty")]
        writes: WritesInput,
    },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdvanceBy {
    Named(String),
    Slots(u32),
    ToSlot { to: String },
    To { to: ClockPosition },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClockPosition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekday: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

/// The initial world seed (§5.1).
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<StateWrite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
    #[serde(default = "default_true")]
    pub derive: bool,
    #[serde(default, skip_serializing_if = "SaveInput::is_empty")]
    pub save: SaveInput,
}
impl Default for Seed {
    fn default() -> Self {
        Self {
            state: Vec::new(),
            facts: Vec::new(),
            derive: true,
            save: SaveInput::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInput {
    #[serde(default)]
    pub visited: Vec<String>,
    #[serde(default)]
    pub presented: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub quests: BTreeMap<String, String>,
    #[serde(default)]
    #[serde(rename = "questInstances")]
    pub quest_instances: BTreeMap<String, u64>,
    #[serde(default)]
    #[serde(rename = "entriesRead")]
    pub entries_read: BTreeMap<String, Vec<String>>,
}
impl SaveInput {
    fn is_empty(&self) -> bool {
        self.visited.is_empty()
            && self.presented.is_empty()
            && self.quests.is_empty()
            && self.quest_instances.is_empty()
            && self.entries_read.is_empty()
    }
}

/// An execution record or session-level notification.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Event {
    #[serde(rename = "record")]
    Record { document: String, record: Json },
    #[serde(rename = "presentation")]
    Presentation {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        occasion: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        beat: String,
        document: String,
        kind: String,
    },
    #[serde(rename = "presentationEnd")]
    PresentationEnd {
        beat: String,
        document: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "quest")]
    Quest {
        document: String,
        commands: Vec<Json>,
    },
    #[serde(rename = "clock")]
    Clock {
        from: Json,
        to: Json,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passed: Option<Json>,
    },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Await {
    #[serde(rename = "awaitChoice")]
    Choice { request: u64, menu: MenuAwait },
    #[serde(rename = "awaitBridge")]
    Bridge {
        request: u64,
        tag: String,
        document: String,
        position: String,
        fields: BTreeMap<String, String>,
    },
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "ended")]
    Ended { reason: String },
    #[serde(rename = "halted")]
    Halted {
        kind: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        site: Option<String>,
    },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuAwait {
    pub construct: String,
    pub id: String,
    pub document: String,
    pub position: String,
    pub presentation: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
    pub options: Vec<MenuOptionAwait>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuOptionAwait {
    pub id: String,
    pub verdict: String,
    pub exit: bool,
    pub once: bool,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    #[serde(rename = "eventVersion")]
    pub event_version: String,
    pub events: Vec<Event>,
    #[serde(rename = "await")]
    pub await_: Await,
}

/// A serialized, resumable runtime state.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    #[serde(rename = "snapshotVersion")]
    pub snapshot_version: String,
    pub project: String,
    #[serde(rename = "await")]
    pub await_: Await,
    pub request: u64,
    #[serde(with = "arc_world")]
    #[cfg_attr(feature = "json-schema", schemars(with = "crate::session::World"))]
    pub world: Arc<World>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<Continuation>,
}

/// A typed JSON input accepted by [`Snapshot::from_json`].
pub trait SnapshotJsonInput {
    fn into_json(self) -> Result<Json, Rejected>;
}

impl SnapshotJsonInput for Json {
    fn into_json(self) -> Result<Json, Rejected> {
        Ok(self)
    }
}

impl SnapshotJsonInput for &str {
    fn into_json(self) -> Result<Json, Rejected> {
        serde_json::from_str(self).map_err(|e| reject("E-RUNTIME-SNAPSHOT-VERSION", e.to_string()))
    }
}

impl SnapshotJsonInput for String {
    fn into_json(self) -> Result<Json, Rejected> {
        self.as_str().into_json()
    }
}

impl Snapshot {
    /// Decode a snapshot while preserving the version rejection semantics.
    pub fn from_json<T: SnapshotJsonInput>(value: T) -> Result<Self, Rejected> {
        let value = value.into_json()?;
        let version = value
            .get("snapshotVersion")
            .and_then(Json::as_str)
            .ok_or_else(|| reject("E-RUNTIME-SNAPSHOT-VERSION", "snapshotVersion is missing"))?;
        let major_minor = |s: &str| s.split('.').take(2).collect::<Vec<_>>().join(".");
        if major_minor(version) != major_minor(EVENT_VERSION) {
            return Err(reject(
                "E-RUNTIME-SNAPSHOT-VERSION",
                format!("snapshot version {version} is not {EVENT_VERSION}'s major.minor"),
            ));
        }
        serde_json::from_value(value)
            .map_err(|e| reject("E-RUNTIME-SNAPSHOT-VERSION", format!("invalid snapshot: {e}")))
    }
}

/// One line of the `--events` JSONL stream.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamLine {
    Seed { seed: Seed, output: Output },
    Input { input: Input, output: Output },
    InputRejected { input: Input, rejected: Rejected },
    SeedRejected { seed: Seed, rejected: Rejected },
}

impl Output {
    fn new(events: Vec<Event>, await_: Await) -> Self {
        Self {
            event_version: EVENT_VERSION.into(),
            events,
            await_,
        }
    }
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rejected {
    pub code: String,
    pub message: String,
}

/// A resumable runtime state.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[cfg_attr(feature = "json-schema", schemars(with = "crate::session::World"))]
    #[serde(with = "arc_world")]
    pub world: Arc<World>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<Continuation>,
    pub request: u64,
    pub phase: Phase,
}

impl State {
    /// What the state waits for: the pending await of a suspended input,
    /// else `idle`, `ended` or `halted` by its phase.
    pub fn current_await(&self) -> Await {
        if let Some(c) = &self.continuation {
            return c.pending.clone();
        }
        match &self.phase {
            Phase::Idle => Await::Idle,
            Phase::Ended => Await::Ended {
                reason: "terminal".into(),
            },
            Phase::Halted { kind, message } => Await::Halted {
                kind: kind.clone(),
                message: message.clone(),
                site: None,
            },
        }
    }
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Continuation {
    #[cfg_attr(feature = "json-schema", schemars(with = "crate::session::World"))]
    #[serde(with = "arc_world")]
    pub before: Arc<World>,
    pub input: Input,
    pub(crate) answers: Vec<Answer>,
    pub pending: Await,
    pub(crate) delivered: usize,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Ended,
    Halted { kind: String, message: String },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Answer {
    Choice {
        id: String,
        option: String,
    },
    Bridge {
        tag: String,
        fields: BTreeMap<String, Json>,
    },
}

/// The loaded executable project and its host-facing operations.
pub struct Runtime {
    project: Arc<ExecProject>,
    /// The bundle's identity (spec 0.38.0 §7.2), what snapshots carry.
    fingerprint: String,
}

impl Runtime {
    /// The stable identity of the loaded executable bundle.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn project(&self) -> &ExecProject {
        &self.project
    }

    /// Load a `lute compile --all` bundle (spec 0.38.0 §4.1). Rejected with
    /// `E-RUNTIME-IR-VERSION` when the index or an artifact is stamped with
    /// another IR major.minor than [`IR_VERSION`], when a command has a
    /// `kind` this runtime does not execute, or when the bundle does not
    /// decode.
    pub fn load(bundle: Bundle) -> Result<Self, Rejected> {
        let ir_line = |v: &str| v.split('.').take(2).collect::<Vec<_>>().join(".");
        let stamps = std::iter::once(("project.index.json", &bundle.index))
            .chain(bundle.artifacts.iter().map(|(path, a)| (path.as_str(), a)));
        for (path, json) in stamps {
            let stamp = json.get("irVersion").and_then(Json::as_str).unwrap_or("");
            if ir_line(stamp) != ir_line(IR_VERSION) {
                return Err(reject(
                    "E-RUNTIME-IR-VERSION",
                    format!(
                        "{path} is IR `{stamp}`; this runtime executes IR {}.x",
                        ir_line(IR_VERSION)
                    ),
                ));
            }
        }
        let fingerprint = crate::snapshot::fingerprint(&bundle.index, &bundle.artifacts);
        let project = ExecProject::load(bundle)
            .map_err(|(_, messages)| reject("E-RUNTIME-IR-VERSION", messages.join("; ")))?;
        for (path, code) in &project.codes {
            if let Some(kind) = code
                .commands
                .iter()
                .map(|c| c.get("kind").and_then(Json::as_str).unwrap_or(""))
                .find(|kind| !crate::machine::COMMAND_KINDS.contains(kind))
            {
                return Err(reject(
                    "E-RUNTIME-IR-VERSION",
                    format!("{path}: command kind `{kind}` is not one this runtime executes"),
                ));
            }
        }
        Ok(Self {
            project: Arc::new(project),
            fingerprint,
        })
    }

    /// The snapshot of `state` (spec §7.2): `await` is the state's current
    /// await — the pending choice or bridge, or `idle` / `ended` / `halted`.
    pub fn snapshot(&self, state: &State) -> Snapshot {
        Snapshot {
            snapshot_version: EVENT_VERSION.into(),
            project: self.fingerprint.clone(),
            await_: state.current_await(),
            request: state.request,
            world: state.world.clone(),
            continuation: state.continuation.clone(),
        }
    }

    /// Restore a typed snapshot taken by this bundle and contract minor.
    pub fn restore(&self, snapshot: Snapshot) -> Result<State, Rejected> {
        let major_minor = |s: &str| s.split('.').take(2).collect::<Vec<_>>().join(".");
        if major_minor(&snapshot.snapshot_version) != major_minor(EVENT_VERSION) {
            return Err(reject(
                "E-RUNTIME-SNAPSHOT-VERSION",
                format!(
                    "snapshot version {} is not {EVENT_VERSION}'s major.minor",
                    snapshot.snapshot_version
                ),
            ));
        }
        if snapshot.project != self.fingerprint {
            return Err(reject(
                "E-RUNTIME-SNAPSHOT-PROJECT",
                "snapshot belongs to a different project bundle",
            ));
        }
        let phase = match &snapshot.await_ {
            Await::Ended { .. } => Phase::Ended,
            Await::Halted { kind, message, .. } => Phase::Halted {
                kind: kind.clone(),
                message: message.clone(),
            },
            Await::Idle | Await::Choice { .. } | Await::Bridge { .. } => Phase::Idle,
        };
        Ok(State {
            world: snapshot.world,
            continuation: snapshot.continuation,
            request: snapshot.request,
            phase,
        })
    }

    pub fn begin(&self, seed: Seed) -> Result<(State, Output), Rejected> {
        let world = self.seed_world(&seed)?;
        self.run_input(world, seed_input(), Vec::new(), 0, 0)
    }

    pub fn step(
        &self,
        mut state: State,
        input: Input,
    ) -> Result<(State, Output), (State, Rejected)> {
        let unchanged = state.clone();
        if let Phase::Halted { message, .. } = state.phase.clone() {
            return Err((
                unchanged,
                Rejected {
                    code: "E-RUNTIME-HALTED".into(),
                    message,
                },
            ));
        }
        if matches!(state.phase, Phase::Ended)
            && !matches!(input, Input::NewRun { .. } | Input::RaiseOccasion { .. })
        {
            return Err((
                unchanged.clone(),
                reject(
                    "E-RUNTIME-BUSY",
                    "the runtime has ended; only newRun or an outsideRun raise is accepted",
                ),
            ));
        }
        let (base, original, answers, delivered) = match state.continuation.take() {
            Some(c) => match (&input, &c.pending) {
                (
                    Input::Choose { request, option },
                    Await::Choice {
                        request: expected,
                        menu,
                    },
                ) => {
                    if request != expected {
                        return Err((
                            unchanged.clone(),
                            reject(
                                "E-RUNTIME-REQUEST",
                                "choose request does not match the pending await",
                            ),
                        ));
                    }
                    let Some(item) = menu.options.iter().find(|o| o.id == *option) else {
                        return Err((
                            unchanged.clone(),
                            reject(
                                "E-RUNTIME-OPTION",
                                format!("option `{option}` is not in menu `{}`", menu.id),
                            ),
                        ));
                    };
                    if item.verdict != "open" {
                        return Err((
                            unchanged.clone(),
                            reject("E-RUNTIME-OPTION", format!("option `{option}` is not open")),
                        ));
                    }
                    let mut a = c.answers;
                    a.push(Answer::Choice {
                        id: menu.id.clone(),
                        option: option.clone(),
                    });
                    ((*c.before).clone(), c.input, a, c.delivered)
                }
                (
                    Input::BridgeResult { request, fields },
                    Await::Bridge {
                        request: expected,
                        tag,
                        fields: shape,
                        ..
                    },
                ) => {
                    if request != expected {
                        return Err((
                            unchanged.clone(),
                            reject(
                                "E-RUNTIME-REQUEST",
                                "bridge request does not match the pending await",
                            ),
                        ));
                    }
                    if let Err(m) = validate_bridge(shape, fields) {
                        return Err((unchanged.clone(), reject("E-RUNTIME-BRIDGE-SHAPE", m)));
                    }
                    let mut a = c.answers;
                    a.push(Answer::Bridge {
                        tag: tag.clone(),
                        fields: fields.clone(),
                    });
                    ((*c.before).clone(), c.input, a, c.delivered)
                }
                _ => {
                    return Err((
                        unchanged.clone(),
                        reject("E-RUNTIME-BUSY", "an input is already suspended"),
                    ))
                }
            },
            None => {
                if matches!(input, Input::Choose { .. } | Input::BridgeResult { .. }) {
                    return Err((
                        unchanged.clone(),
                        reject("E-RUNTIME-BUSY", "no choice or bridge await is pending"),
                    ));
                }
                ((*state.world).clone(), input.clone(), Vec::new(), 0)
            }
        };
        match self.run_input(base, original, answers, delivered, state.request) {
            Ok(next) => Ok(next),
            Err(rej) => Err((unchanged, rej)),
        }
    }

    pub fn candidates(
        &self,
        state: &State,
        occasion: &str,
        target: Option<&str>,
    ) -> Vec<crate::session::Candidate> {
        let mut f = ReplayFactory::new(self.project.clone(), Vec::new());
        let s = Session::resume(&self.project, (*state.world).clone(), &mut f);
        s.candidates(occasion, target)
    }
    pub fn eligibility(
        &self,
        state: &State,
        beat: &str,
        member: Option<&str>,
    ) -> Option<crate::session::Candidate> {
        let mut f = ReplayFactory::new(self.project.clone(), Vec::new());
        let s = Session::resume(&self.project, (*state.world).clone(), &mut f);
        s.eligibility(beat, member)
    }
    pub fn clock(&self, state: &State) -> Option<lute_manifest::clock::ClockAt> {
        clock_at(&self.project, &state.world)
    }
    pub fn terminal(&self, state: &State) -> bool {
        let mut f = ReplayFactory::new(self.project.clone(), Vec::new());
        Session::resume(&self.project, (*state.world).clone(), &mut f).terminal()
    }
    pub fn view(&self, state: &State, with_facts: bool) -> crate::session::WorldView {
        let mut f = ReplayFactory::new(self.project.clone(), Vec::new());
        Session::resume(&self.project, (*state.world).clone(), &mut f).view(with_facts)
    }

    fn seed_world(&self, seed: &Seed) -> Result<World, Rejected> {
        let state = seed
            .state
            .iter()
            .map(|w| {
                if w.add.is_some() || w.value.is_none() {
                    Err(format!(
                        "seed state `{}` must contain a scalar value",
                        w.path
                    ))
                } else {
                    scalar_text(w.value.as_ref())
                        .map(|v| (w.path.clone(), v, None))
                        .ok_or_else(|| {
                            format!("seed state `{}` must contain a scalar value", w.path)
                        })
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|m| reject("E-RUNTIME-INPUT", m))?;
        let surfaces: crate::MockSet<(), ()> = crate::MockSet {
            state,
            facts: seed.facts.clone(),
            ..Default::default()
        };
        let save = SaveSeed {
            visited: seed.save.visited.clone(),
            presented_user: seed.save.presented.get("user").cloned().unwrap_or_default(),
            presented_run: seed.save.presented.get("run").cloned().unwrap_or_default(),
            quests: seed
                .save
                .quests
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            quest_instances: seed
                .save
                .quest_instances
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            entries_run: seed
                .save
                .entries_read
                .get("run")
                .cloned()
                .unwrap_or_default(),
            entries_user: seed
                .save
                .entries_read
                .get("user")
                .cloned()
                .unwrap_or_default(),
        };
        let ws = WorldSeed {
            surfaces: &surfaces,
            save: &save,
            derive: (!seed.derive).then_some(false),
        };
        crate::session::seed_world(&self.project, &ws).map_err(|errs| {
            reject(
                "E-RUNTIME-INPUT",
                errs.into_iter()
                    .map(|e| e.msg)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }

    fn run_input(
        &self,
        world: World,
        input: Input,
        answers: Vec<Answer>,
        delivered: usize,
        request: u64,
    ) -> Result<(State, Output), Rejected> {
        let mut factory = ReplayFactory::new(self.project.clone(), answers.clone());
        let (stop, final_world, ended) = {
            let mut session = Session::resume(&self.project, world.clone(), &mut factory);
            let outcome =
                execute_input(&mut session, &input).map_err(|m| reject("E-RUNTIME-INPUT", m))?;
            let (_, _, stop) = outcome;
            let ended = stop.is_none() && session.terminal();
            (stop, session.world.clone(), ended)
        };
        let events = factory.events.clone();
        if let Some(await_) = factory.pending.take() {
            let cut = factory
                .pending_cut
                .unwrap_or(events.len())
                .min(events.len());
            let await_ = set_request(await_, request + 1);
            let state = State {
                world: Arc::new(world.clone()),
                continuation: Some(Continuation {
                    before: Arc::new(world),
                    input,
                    answers,
                    pending: await_.clone(),
                    delivered: cut,
                }),
                request: request + 1,
                phase: Phase::Idle,
            };
            return Ok((
                state,
                Output::new(events[delivered.min(cut)..cut].to_vec(), await_),
            ));
        }
        if let Some(halt) = stop {
            let message = halt.message().into_owned();
            let kind = halt.exit_label().to_string();
            let await_ = Await::Halted {
                kind: kind.clone(),
                message: message.clone(),
                site: None,
            };
            let state = State {
                world: Arc::new(final_world),
                continuation: None,
                request,
                phase: Phase::Halted {
                    kind,
                    message: message.clone(),
                },
            };
            return Ok((
                state,
                Output::new(events[delivered.min(events.len())..].to_vec(), await_),
            ));
        }
        let await_ = if ended {
            Await::Ended {
                reason: "terminal".into(),
            }
        } else {
            Await::Idle
        };
        let state = State {
            world: Arc::new(final_world),
            continuation: None,
            request,
            phase: if ended { Phase::Ended } else { Phase::Idle },
        };
        Ok((
            state,
            Output::new(events[delivered.min(events.len())..].to_vec(), await_),
        ))
    }
}
fn seed_input() -> Input {
    Input::HostWrite {
        writes: WritesInput::default(),
    }
}
fn reject(code: impl Into<String>, message: impl Into<String>) -> Rejected {
    Rejected {
        code: code.into(),
        message: message.into(),
    }
}
fn set_request(await_: Await, request: u64) -> Await {
    match await_ {
        Await::Choice { menu, .. } => Await::Choice { request, menu },
        Await::Bridge {
            tag,
            document,
            position,
            fields,
            ..
        } => Await::Bridge {
            request,
            tag,
            document,
            position,
            fields,
        },
        other => other,
    }
}

fn scalar_text(v: Option<&Json>) -> Option<String> {
    match v? {
        Json::String(s) => Some(s.clone()),
        Json::Bool(b) => Some(b.to_string()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn resolve_writes(p: &ExecProject, raw: &WritesInput) -> Result<Writes, String> {
    let mut out = Writes::default();
    for w in &raw.state {
        let write = match (&w.value, w.add) {
            (Some(v), None) => Write::Set(resolve_state(
                p,
                &w.path,
                &scalar_text(Some(v))
                    .ok_or_else(|| format!("state `{}` must be a scalar", w.path))?,
            )?),
            (None, Some(n)) => Write::Add(n),
            _ => {
                return Err(format!(
                    "state write `{}` must have exactly one of value or add",
                    w.path
                ))
            }
        };
        out.state.push((w.path.clone(), write));
    }
    for f in &raw.facts {
        out.facts.push(resolve_fact(p, f, None)?);
    }
    for f in &raw.retract {
        out.retract.push(resolve_fact(p, f, None)?);
    }
    for q in &raw.accept {
        if !p.quest_objectives.contains_key(q) {
            return Err(format!("unknown quest `{q}`"));
        }
        out.accept.push(q.clone());
    }
    Ok(out)
}

fn resolve_advance(
    p: &ExecProject,
    by: &AdvanceBy,
) -> Result<lute_manifest::clock::Advance, String> {
    match by {
        AdvanceBy::Named(s) => match s.as_str() {
            "slot" => Ok(lute_manifest::clock::Advance::Slots(1)),
            "day" => Ok(lute_manifest::clock::Advance::Day),
            _ => Err(format!(
                "advance `{s}` is not `slot` or `day` — a destination is `{{\"to\": …}}`"
            )),
        },
        AdvanceBy::Slots(n) => Ok(lute_manifest::clock::Advance::Slots(*n)),
        AdvanceBy::ToSlot { to } => {
            let c = p.index.clock.as_ref().ok_or("project declares no clock")?;
            let slot = c
                .slots
                .iter()
                .position(|name| name == to)
                .ok_or_else(|| format!("unknown clock slot `{to}`"))?;
            Ok(lute_manifest::clock::Advance::To {
                weekday: None,
                slot: Some(slot),
            })
        }
        AdvanceBy::To { to } => {
            if to.weekday.is_none() && to.slot.is_none() {
                return Err("advance destination must contain `weekday` or `slot`".into());
            }
            let c = p.index.clock.as_ref().ok_or("project declares no clock")?;
            let weekday = to
                .weekday
                .as_deref()
                .map(|weekday| {
                    let week = c.week.as_ref().ok_or("the clock declares no `week:`")?;
                    match week.labels.iter().position(|x| x == weekday) {
                        Some(i) => Some(i as i64),
                        None => weekday
                            .parse::<i64>()
                            .ok()
                            .filter(|i| (0..i64::from(week.length)).contains(i)),
                    }
                    .ok_or_else(|| format!("unknown weekday `{weekday}`"))
                })
                .transpose()?;
            let slot = to
                .slot
                .as_deref()
                .map(|slot| {
                    c.slots
                        .iter()
                        .position(|x| x == slot)
                        .ok_or_else(|| format!("unknown clock slot `{slot}`"))
                })
                .transpose()?;
            Ok(lute_manifest::clock::Advance::To { weekday, slot })
        }
    }
}

fn execute_input<F: WalkDriver>(
    s: &mut Session<'_, '_, F>,
    input: &Input,
) -> Result<
    (
        StepBody,
        Vec<crate::session::QuestAdvance>,
        Option<PlayHalt>,
    ),
    String,
> {
    match input {
        Input::RaiseOccasion {
            occasion,
            target,
            payload,
            pick,
            writes,
        } => {
            // dsl 0.27.0 §4: the writes land first — their own record, then
            // the settle — so the raise, its `raisedWhen` gate included,
            // sees them.
            if !writes.is_empty() {
                let landed = s.engine(0, &resolve_writes(s.project(), writes)?);
                if landed.2.is_some() {
                    return Ok(landed);
                }
            }
            let payload = payload
                .iter()
                .map(|(k, v)| {
                    scalar_text(Some(v))
                        .map(|value| (k.clone(), value))
                        .ok_or_else(|| format!("payload `{k}` must be scalar"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !payload.is_empty() {
                let vals = crate::session::typed_payload(s.project(), occasion, &payload)?;
                s.bind_payload(&vals);
            }
            let p = pick.as_ref().map(PickInput::session).transpose()?;
            Ok(s.occasion(0, occasion, target, &p, &BTreeMap::new()))
        }
        Input::AdvanceClock { by, writes, pick } => {
            let w = resolve_writes(s.project(), writes)?;
            let by = resolve_advance(s.project(), by)?;
            let raise = s
                .project()
                .index
                .clock
                .as_ref()
                .and_then(|c| c.raise.as_ref())
                .map_or_else(Default::default, |r| r.moments());
            let p = pick.as_ref().map(PickInput::session).transpose()?;
            Ok(s.advance(0, by, &w, &raise, &p, &BTreeMap::new()))
        }
        Input::HostWrite { writes } => {
            let w = resolve_writes(s.project(), writes)?;
            Ok(s.engine(0, &w))
        }
        Input::WorldEvent { name } => Ok(s.event(name)),
        Input::NewRun { writes } => {
            let w = resolve_writes(s.project(), writes)?;
            Ok(s.new_run(0, &w))
        }
        Input::Choose { .. } | Input::BridgeResult { .. } => {
            Err("answer input cannot execute without a continuation".into())
        }
    }
}

struct ReplayFactory {
    project: Arc<ExecProject>,
    answers: Vec<Answer>,
    answer_cursor: usize,
    events: Vec<Event>,
    pending: Option<Await>,
    pending_cut: Option<usize>,
    document: String,
}
impl ReplayFactory {
    fn new(project: Arc<ExecProject>, answers: Vec<Answer>) -> Self {
        Self {
            project,
            answers,
            answer_cursor: 0,
            events: Vec::new(),
            pending: None,
            pending_cut: None,
            document: String::new(),
        }
    }
}
struct ReplayDriver {
    choices: Vec<Answer>,
    cursor: usize,
    document: String,
    events: Vec<Event>,
    pending: Option<Await>,
    pending_cut: Option<usize>,
    project: Arc<ExecProject>,
}
impl Driver for ReplayDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        if self.cursor < self.choices.len() {
            if let Answer::Choice { id, option } = &self.choices[self.cursor] {
                if id == menu.id {
                    self.cursor += 1;
                    return Pick::Option(option.clone());
                }
            }
        }
        let open = menu
            .options
            .iter()
            .any(|o| matches!(o.verdict, Verdict::Open));
        if !open {
            return if menu.construct == MenuKind::Hub {
                Pick::HubAutoPass
            } else {
                Pick::AutoFirst
            };
        }
        self.pending = Some(Await::Choice {
            request: 0,
            menu: MenuAwait {
                construct: if menu.construct == MenuKind::Branch {
                    "branch".into()
                } else {
                    "hub".into()
                },
                id: menu.id.into(),
                document: self.document.clone(),
                position: menu.addr.into(),
                presentation: menu.presentation,
                prompt: None,
                timeout: None,
                options: menu
                    .options
                    .iter()
                    .map(|o| MenuOptionAwait {
                        id: o.id.clone(),
                        verdict: match o.verdict {
                            Verdict::Open => "open",
                            Verdict::Closed(_) => "closed",
                            Verdict::Spent => "spent",
                            Verdict::Unknown(_) => "closed",
                        }
                        .into(),
                        exit: o.exit,
                        once: o.once,
                    })
                    .collect(),
            },
        });
        self.pending_cut = Some(self.events.len());
        Pick::Unscripted { scripted: 0 }
    }
    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, _verdict: &Verdict) -> Forced {
        Forced::Refuse
    }
    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        if self.cursor < self.choices.len() {
            if let Answer::Bridge { tag, fields } = &self.choices[self.cursor] {
                if tag == call.tag {
                    self.cursor += 1;
                    return BridgeReply::Answer(
                        fields
                            .iter()
                            .filter_map(|(k, v)| scalar_text(Some(v)).map(|s| (k.clone(), s)))
                            .collect(),
                    );
                }
            }
        }
        let fields = call
            .reads
            .iter()
            .filter(|(field, _)| self.project.bridge_reads.reads(call.tag, field))
            .map(|(field, path)| {
                let ty = self
                    .project
                    .state_table
                    .get(path)
                    .and_then(|entry| entry.get("type"))
                    .and_then(Json::as_str)
                    .unwrap_or("string");
                (
                    field.clone(),
                    match ty {
                        "int" | "double" | "number" => "number",
                        "bool" => "bool",
                        _ => "string",
                    }
                    .to_string(),
                )
            })
            .collect();
        self.pending = Some(Await::Bridge {
            request: 0,
            tag: call.tag.into(),
            document: self.document.clone(),
            position: call.addr.into(),
            fields,
        });
        self.pending_cut = Some(self.events.len());
        BridgeReply::Unanswered
    }
    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        OnUnknown::Halt
    }
    fn emit(&mut self, rec: Json) {
        self.events.push(Event::Record {
            document: self.document.clone(),
            record: rec,
        });
    }
}
impl WalkDriver for ReplayFactory {
    type Driver = ReplayDriver;
    fn new_driver(
        &mut self,
        document: &str,
        _choose: &BTreeMap<String, Vec<String>>,
        _world: &World,
    ) -> Self::Driver {
        self.document = document.into();
        ReplayDriver {
            choices: self.answers.clone(),
            cursor: self.answer_cursor,
            document: document.into(),
            events: Vec::new(),
            pending: None,
            pending_cut: None,
            project: self.project.clone(),
        }
    }
    fn finish(&mut self, machine: Machine<Self::Driver>) -> Walked {
        let (carry, driver) = machine.into_carry();
        self.answer_cursor = driver.cursor;
        // The session reads the walk's records (a quest pass that moved,
        // a quest event's commands): the transcript is this walk's records.
        let transcript = driver
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Record { record, .. } => Some(record.clone()),
                _ => None,
            })
            .collect();
        let event_base = self.events.len();
        self.events.extend(driver.events);
        self.pending_cut = driver.pending_cut.map(|cut| event_base + cut);
        self.pending = driver.pending;
        Walked {
            carry,
            transcript,
            bridges: BTreeMap::new(),
        }
    }
    fn event(&mut self, event: Event) {
        self.events.push(event);
    }
    fn halt(
        &self,
        result: Result<(), String>,
        outcome: &Walked,
        what: &str,
        _doc_json: &Json,
    ) -> Option<PlayHalt> {
        if self.pending.is_some() {
            return Some(PlayHalt::Incomplete(format!("{what}: awaiting host input")));
        }
        if outcome.carry.incomplete {
            return Some(PlayHalt::Incomplete(format!("{what}: walk is incomplete")));
        }
        result.err().map(PlayHalt::Fatal)
    }
}

fn validate_bridge(
    shape: &BTreeMap<String, String>,
    fields: &BTreeMap<String, Json>,
) -> Result<(), String> {
    for (name, ty) in shape {
        let Some(v) = fields.get(name) else {
            return Err(format!("bridge field `{name}` is required"));
        };
        let good = match ty.as_str() {
            "bool" => v.is_boolean(),
            "number" => v.is_number(),
            _ => v.is_string(),
        };
        if !good {
            return Err(format!("bridge field `{name}` must be a {ty}"));
        }
    }
    Ok(())
}
