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
    use super::{Arc, Deserialize, Serialize};
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
    clock_at, resolve_fact, resolve_state, ExecProject, Pick as SessionPick, PlayHalt,
    ResolveError, SaveSeed, Session, StepBody, WalkDriver, Walked, World, WorldSeed, Write, Writes,
};
use crate::Machine;

/// The event contract version outputs and snapshots are stamped with.
pub const EVENT_VERSION: &str = "0.51.0";
/// The execution IR version this runtime executes (major.minor gate).
pub const IR_VERSION: &str = "0.51.0";

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
    #[serde(rename = "timeout")]
    Timeout { request: u64 },
    #[serde(rename = "choose")]
    Choose { request: u64, option: String },
    /// dsl 0.41.0 §5.3: the answer to the pending `awaitBridge` — the
    /// result `fields` content reads, and the host's own `writes`, applied
    /// at the call after its declared effects.
    #[serde(rename = "bridgeResult")]
    BridgeResult {
        request: u64,
        fields: BTreeMap<String, Json>,
        #[serde(default, skip_serializing_if = "WritesInput::is_empty")]
        writes: WritesInput,
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
    /// dsl 0.41.0 §5.5: a plugin call suspended for its bridge results.
    #[serde(rename = "awaitBridge")]
    Bridge {
        request: u64,
        tag: String,
        document: String,
        position: String,
        /// The call's attributes as the runtime executes it: the IR
        /// command's `fields`, `occasion.target` bound to the raise's
        /// member (`{}` when none).
        attrs: BTreeMap<String, Json>,
        /// 0.49.0 §3.1: the IR command's plugin stamps (its `stamps`
        /// object), beside and never merged into `attrs`; omitted when the
        /// command carries none.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        stamps: BTreeMap<String, Json>,
        /// Each result field content reads, with its declared type.
        results: BTreeMap<String, lute_manifest::types::Type>,
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
    pub timeout: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
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
        serde_json::from_value(value).map_err(|e| {
            reject(
                "E-RUNTIME-SNAPSHOT-VERSION",
                format!("invalid snapshot: {e}"),
            )
        })
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

/// The answer of a read-only condition query, `evaluate` or `def`
/// (dsl 0.41.0 §5.1).
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    /// The typed value: `bool`, `int`, `double`, `str` or `unknown` — never
    /// `error`, which is the rejection `E-RUNTIME-EVAL`.
    pub value: crate::Value,
    /// Why an `unknown` value is unknown, one reason per unresolved read;
    /// omitted for a decided value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
}

/// dsl 0.41.0 §5.2: the answer of the `gate` query — whether a
/// `raiseOccasion` of the occasion (for its target) with no writes would
/// pass the seam now.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// The raise would be made.
    pub open: bool,
    /// Why the gate is undecided, one reason per unresolved read (as
    /// [`Evaluation::unresolved`]); omitted when it is decided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
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
    /// The stage at the pending await; the `world` of a suspended state is
    /// the world before its input (dsl 0.40.0 §4.2).
    pub stage: crate::Stage,
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
        /// dsl 0.41.0 §5.3: the host's writes with the answer, re-applied
        /// on replay.
        #[serde(default, skip_serializing_if = "WritesInput::is_empty")]
        writes: WritesInput,
    },
}

/// The loaded executable project and its host-facing operations.
#[derive(Debug)]
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

    /// Load a whole-project `lute compile` bundle (spec 0.38.0 §4.1). Rejected with
    /// `E-RUNTIME-IR-VERSION` when the index or an artifact is stamped with
    /// another IR major.minor than [`IR_VERSION`], when a command has a
    /// `kind` this runtime does not execute, when the bundle does not
    /// decode, or when a document is malformed (a loop that can cycle,
    /// dsl 0.51.0 §3.7; a `dynamic` raise that is not its occasion's
    /// `occasion.target` reference, §4.6).
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
            world: Arc::clone(&state.world),
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

    #[expect(
        clippy::result_large_err,
        reason = "rejection returns the pre-step state required for deterministic recovery"
    )]
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
            // dsl 0.44.0 §3.2: a host write cannot follow the ending; the
            // refusal says where such a write belongs.
            let message = if matches!(input, Input::HostWrite { .. }) {
                "the runtime has ended; only newRun or an outsideRun raise is accepted \
                 \u{2014} a write that belongs to the ending rides as `writes` on the input \
                 that ends the run"
            } else {
                "the runtime has ended; only newRun or an outsideRun raise is accepted"
            };
            return Err((unchanged.clone(), reject("E-RUNTIME-BUSY", message)));
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
                    Input::Timeout { request },
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
                                "timeout request does not match the pending await",
                            ),
                        ));
                    }
                    let Some(default) = menu.default.clone() else {
                        return Err((
                            unchanged.clone(),
                            reject(
                                "E-RUNTIME-TIMEOUT",
                                "the pending await is not a timed choice",
                            ),
                        ));
                    };
                    if menu.timeout.is_none() {
                        return Err((
                            unchanged.clone(),
                            reject(
                                "E-RUNTIME-TIMEOUT",
                                "the pending await is not a timed choice",
                            ),
                        ));
                    }
                    let mut a = c.answers;
                    a.push(Answer::Choice {
                        id: menu.id.clone(),
                        option: default,
                    });
                    ((*c.before).clone(), c.input, a, c.delivered)
                }
                (Input::Timeout { .. }, _) => {
                    return Err((
                        unchanged.clone(),
                        reject(
                            "E-RUNTIME-TIMEOUT",
                            "the pending await is not a timed choice",
                        ),
                    ))
                }
                (
                    Input::BridgeResult {
                        request,
                        fields,
                        writes,
                    },
                    Await::Bridge {
                        request: expected,
                        tag,
                        results,
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
                    if let Err(m) = validate_bridge(results, fields) {
                        return Err((unchanged.clone(), reject("E-RUNTIME-BRIDGE-SHAPE", m)));
                    }
                    // dsl 0.41.0 §5.3: the writes resolve when the input
                    // arrives, as `hostWrite`'s do.
                    if let Err(m) = resolve_bridge_writes(&self.project, writes) {
                        return Err((unchanged.clone(), reject("E-RUNTIME-INPUT", m)));
                    }
                    let mut a = c.answers;
                    a.push(Answer::Bridge {
                        tag: tag.clone(),
                        fields: fields.clone(),
                        writes: writes.clone(),
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
                if matches!(input, Input::Timeout { .. }) {
                    return Err((
                        unchanged.clone(),
                        reject(
                            "E-RUNTIME-TIMEOUT",
                            "the pending await is not a timed choice",
                        ),
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
        let mut f = ReplayFactory::new(Arc::clone(&self.project), Vec::new());
        let s = Session::resume(&self.project, (*state.world).clone(), &mut f);
        s.candidates(occasion, target)
    }
    pub fn eligibility(
        &self,
        state: &State,
        beat: &str,
        member: Option<&str>,
    ) -> Option<crate::session::Candidate> {
        let mut f = ReplayFactory::new(Arc::clone(&self.project), Vec::new());
        let s = Session::resume(&self.project, (*state.world).clone(), &mut f);
        s.eligibility(beat, member)
    }
    pub fn clock(&self, state: &State) -> Option<lute_manifest::clock::ClockAt> {
        clock_at(&self.project, &state.world)
    }
    /// The persistent presentation stage of a state: at a pending await,
    /// the stage at that await; otherwise the world's.
    pub fn stage(&self, state: &State) -> crate::Stage {
        match &state.continuation {
            Some(continuation) => continuation.stage.clone(),
            None => state.world.stage.clone(),
        }
    }
    pub fn terminal(&self, state: &State) -> bool {
        let mut f = ReplayFactory::new(Arc::clone(&self.project), Vec::new());
        Session::resume(&self.project, (*state.world).clone(), &mut f).terminal()
    }
    pub fn view(&self, state: &State, with_facts: bool) -> crate::session::WorldView {
        let mut f = ReplayFactory::new(Arc::clone(&self.project), Vec::new());
        Session::resume(&self.project, (*state.world).clone(), &mut f).view(with_facts)
    }

    /// dsl 0.41.0 §5.1: evaluate an IR `exprNode` over `state`'s world as a
    /// walk's guard is. `E-RUNTIME-INPUT` when `expr` is not an `exprNode`;
    /// `E-RUNTIME-EVAL` when it evaluates to an error.
    pub fn evaluate(&self, state: &State, expr: &Json) -> Result<Evaluation, Rejected> {
        let slot = crate::Slot::of_expr(expr)
            .ok_or_else(|| reject("E-RUNTIME-INPUT", "`expr` is not an IR exprNode"))?;
        self.evaluated(state, &slot)
    }

    /// dsl 0.41.0 §5.1: evaluate the index def `name` over `state`'s world,
    /// each `param` node bound to the positional argument of the same index
    /// in `args` (one JSON scalar per declared parameter). `E-RUNTIME-INPUT`
    /// when the bundle carries no such def, on an arity mismatch or an
    /// argument that does not fit its parameter type; `E-RUNTIME-EVAL` when
    /// it evaluates to an error.
    pub fn def(&self, state: &State, name: &str, args: &[Json]) -> Result<Evaluation, Rejected> {
        let def = self.project.index.defs.get(name).ok_or_else(|| {
            reject(
                "E-RUNTIME-INPUT",
                format!("the bundle carries no def `{name}`"),
            )
        })?;
        let bound = def_args(&self.project, name, &def.params, args)
            .map_err(|m| reject("E-RUNTIME-INPUT", m))?;
        let expr = serde_json::to_value(&def.expr).map_err(|e| {
            reject(
                "E-RUNTIME-INPUT",
                format!("def `{name}` does not encode: {e}"),
            )
        })?;
        let expr = crate::expr::bind_params(&expr, &bound);
        let slot = crate::Slot::of(&serde_json::json!({ "cel": def.cel, "expr": expr }))
            .ok_or_else(|| reject("E-RUNTIME-INPUT", format!("def `{name}` does not decode")))?;
        self.evaluated(state, &slot)
    }

    /// dsl 0.41.0 §5.2: whether a `raiseOccasion` of `occasion` (for
    /// `target`) with no writes would pass the seam now — the raise rule of
    /// `step` exactly. `E-RUNTIME-INPUT` when the bundle declares no such
    /// occasion. The state is read as `view` reads it and not changed.
    pub fn gate(
        &self,
        state: &State,
        occasion: &str,
        target: Option<&str>,
    ) -> Result<Gate, Rejected> {
        if !self.project.occasions.contains_key(occasion) {
            return Err(reject(
                "E-RUNTIME-INPUT",
                format!("the bundle declares no occasion `{occasion}`"),
            ));
        }
        Ok(
            match crate::seam::gate(&self.project, &state.world, occasion, target) {
                Ok(open) => Gate {
                    open,
                    unresolved: Vec::new(),
                },
                Err(unresolved) => Gate {
                    open: false,
                    unresolved,
                },
            },
        )
    }

    fn evaluated(&self, state: &State, slot: &crate::Slot) -> Result<Evaluation, Rejected> {
        match crate::session::evaluate(&self.project, &state.world, slot, None) {
            (crate::Value::Error(error), _) => Err(reject(
                "E-RUNTIME-EVAL",
                format!("`{}` evaluates to an error: {error}", slot.raw()),
            )),
            (value, atoms) => {
                let mut unresolved = Vec::new();
                if value == crate::Value::Unknown {
                    for atom in &atoms {
                        let reason = crate::session::describe_atom(atom);
                        if !unresolved.contains(&reason) {
                            unresolved.push(reason);
                        }
                    }
                }
                Ok(Evaluation { value, unresolved })
            }
        }
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
        let mut factory = ReplayFactory::replaying(Arc::clone(&self.project), answers.clone())
            .map_err(|m| reject("E-RUNTIME-INPUT", m))?;
        let (stop, final_world, ended) = {
            let mut session = Session::resume(&self.project, world.clone(), &mut factory);
            let outcome =
                execute_input(&mut session, &input).map_err(|m| reject("E-RUNTIME-INPUT", m))?;
            let (_, _, stop) = outcome;
            let ended = stop.is_none() && session.terminal();
            (stop, session.world.clone(), ended)
        };
        // dsl 0.41.0 §5.2: a raise the engine would not make is rejected —
        // the state stays as it was, the input's own writes and payload
        // included — never a halt.
        if let Some(refused @ PlayHalt::Refused(_)) = &stop {
            return Err(reject("E-OCCASION-GATE", refused.message()));
        }
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
                    stage: factory.pending_stage.take().unwrap_or_default(),
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
fn reject(code: impl Into<String>, message: impl std::fmt::Display) -> Rejected {
    Rejected {
        code: code.into(),
        message: message.to_string(),
    }
}
fn set_request(await_: Await, request: u64) -> Await {
    match await_ {
        Await::Choice { menu, .. } => Await::Choice { request, menu },
        Await::Bridge {
            tag,
            document,
            position,
            attrs,
            stamps,
            results,
            ..
        } => Await::Bridge {
            request,
            tag,
            document,
            position,
            attrs,
            stamps,
            results,
        },
        other => other,
    }
}

/// dsl 0.41.0 §5.1 rule 8: `args` against def `name`'s declared `params`,
/// keyed by parameter name — one JSON scalar per parameter, each fitting
/// its type (`int` a JSON integer within `±2^53`, `double` a number, `bool`
/// a boolean, `string` a string, `enum` a member string, `domain` /
/// `entity` a string, a member when the kind is closed).
fn def_args<'d>(
    p: &ExecProject,
    name: &str,
    params: &'d [lute_manifest::schema::DefParam],
    args: &[Json],
) -> Result<BTreeMap<&'d str, crate::Value>, String> {
    use crate::Value;
    use lute_manifest::types::Type;
    if args.len() != params.len() {
        let declared: Vec<&str> = params.iter().map(|d| d.name.as_str()).collect();
        return Err(format!(
            "def `{name}` takes {} argument(s) ({}), not {}",
            params.len(),
            declared.join(", "),
            args.len()
        ));
    }
    let mut bound = BTreeMap::new();
    for (param, arg) in params.iter().zip(args) {
        let misfit = |what: &str| {
            format!(
                "argument `{}` of def `{name}` must be {what}, not `{arg}`",
                param.name
            )
        };
        let member = |kind: &str, members: Option<&[String]>| -> Result<Value, String> {
            let s = arg
                .as_str()
                .ok_or_else(|| misfit(&format!("a `{kind}` member string")))?;
            if let Some(members) = members {
                crate::session::member_of(kind, members, s)
                    .map_err(|m| format!("argument `{}` of def `{name}`: {m}", param.name))?;
            }
            Ok(Value::Str(s.to_string()))
        };
        let value = match &param.ty {
            Type::Int => crate::expr::host_int(arg)
                .map(Value::Int)
                .ok_or_else(|| misfit("a JSON integer within ±2^53"))?,
            Type::Double => arg
                .as_f64()
                .map(Value::Double)
                .ok_or_else(|| misfit("a number"))?,
            Type::Bool => arg
                .as_bool()
                .map(Value::Bool)
                .ok_or_else(|| misfit("a boolean"))?,
            Type::Str => arg
                .as_str()
                .map(|s| Value::Str(s.to_string()))
                .ok_or_else(|| misfit("a string"))?,
            Type::Enum(members) => member("enum", Some(members))?,
            Type::Domain(kind) | Type::Entity(kind) => {
                member(kind, crate::session::domain_members(p, kind))?
            }
            other => {
                return Err(format!(
                    "parameter `{}` of def `{name}` has type `{}`, which a host cannot pass",
                    param.name,
                    serde_json::to_string(other).unwrap_or_default()
                ))
            }
        };
        bound.insert(param.name.as_str(), value);
    }
    Ok(bound)
}

/// Why a host's `writes` (`hostWrite`, `advanceClock`, `newRun`,
/// `raiseOccasion`, `bridgeResult`) do not resolve against the project.
/// `Display` is the runtime's `E-RUNTIME-INPUT` message.
#[derive(Clone, Debug, PartialEq)]
pub enum HostWriteError {
    /// A `bridgeResult` writes the clock's `day` / `slot` path.
    ClockPath { path: String },
    /// A state `value` that is not a JSON scalar.
    NotScalar { path: String },
    /// An `int` path written (`value` or `add`) outside `±2^53`, or a
    /// fractional `add`; `value` is the number as written.
    OutOfBound { path: String, value: String },
    /// A state write without exactly one of `value` / `add`.
    ValueOrAdd { path: String },
    /// A state value or fact the project refuses — [`resolve_state`]'s /
    /// [`resolve_fact`]'s reason, unprefixed.
    Resolve(ResolveError),
    /// An `accept` naming no declared quest.
    UnknownQuest { quest: String },
}

impl std::fmt::Display for HostWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClockPath { path } => write!(
                f,
                "bridgeResult writes `{path}`, the clock's path — the clock cannot move inside a \
                 walk; move it with `advanceClock` after the answer"
            ),
            Self::NotScalar { path } => write!(f, "state `{path}` must be a scalar"),
            Self::OutOfBound { path, value } => write!(
                f,
                "state `{path}` is an `int`: it takes an integer within ±2^53, not `{value}`"
            ),
            Self::ValueOrAdd { path } => write!(
                f,
                "state write `{path}` must have exactly one of value or add"
            ),
            Self::Resolve(source) => source.fmt(f),
            Self::UnknownQuest { quest } => write!(f, "unknown quest `{quest}`"),
        }
    }
}

impl std::error::Error for HostWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resolve(source) => Some(source),
            Self::ClockPath { .. }
            | Self::NotScalar { .. }
            | Self::OutOfBound { .. }
            | Self::ValueOrAdd { .. }
            | Self::UnknownQuest { .. } => None,
        }
    }
}

impl From<ResolveError> for HostWriteError {
    fn from(source: ResolveError) -> Self {
        Self::Resolve(source)
    }
}

/// dsl 0.41.0 §5.3 rule 2: a `bridgeResult`'s `writes`, resolved as
/// `hostWrite`'s are; a write to the clock's `day` / `slot` path is refused
/// (the clock cannot move inside a walk).
pub fn resolve_bridge_writes(p: &ExecProject, raw: &WritesInput) -> Result<Writes, HostWriteError> {
    if let Some(clock) = &p.index.clock {
        if let Some(w) = raw
            .state
            .iter()
            .find(|w| w.path == clock.day || clock.slot.as_deref() == Some(w.path.as_str()))
        {
            return Err(HostWriteError::ClockPath {
                path: w.path.clone(),
            });
        }
    }
    resolve_writes(p, raw)
}

fn scalar_text(v: Option<&Json>) -> Option<String> {
    match v? {
        Json::String(s) => Some(s.clone()),
        Json::Bool(b) => Some(b.to_string()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `hostWrite`'s (and `advanceClock` / `newRun` / `bridgeResult`'s) `writes`
/// against the project. An `int` path takes, as `value` or `add`, an integer
/// within `±2^53` — the bound every host-passed `int` has ([`def_args`],
/// [`validate_bridge`]) — so no write is accepted that the walk then cannot
/// apply.
fn resolve_writes(p: &ExecProject, raw: &WritesInput) -> Result<Writes, HostWriteError> {
    use crate::expr::INT_BOUND;
    let mut out = Writes::default();
    for w in &raw.state {
        let out_of_bound = |n: &dyn std::fmt::Display| HostWriteError::OutOfBound {
            path: w.path.clone(),
            value: n.to_string(),
        };
        let write = match (&w.value, w.add) {
            (Some(v), None) => {
                let value = resolve_state(
                    p,
                    &w.path,
                    &scalar_text(Some(v)).ok_or_else(|| HostWriteError::NotScalar {
                        path: w.path.clone(),
                    })?,
                )?;
                if let crate::Value::Int(n) = value {
                    if !(-INT_BOUND..=INT_BOUND).contains(&n) {
                        return Err(out_of_bound(&n));
                    }
                }
                Write::Set(value)
            }
            (None, Some(n)) => {
                let int = p
                    .state_table
                    .get(&w.path)
                    .and_then(|e| e.get("type"))
                    .and_then(Json::as_str)
                    == Some("int");
                // Exact: every integer within ±2^53 is an `f64`.
                let bound = INT_BOUND as f64;
                if int && (n.fract() != 0.0 || !(-bound..=bound).contains(&n)) {
                    return Err(out_of_bound(&n));
                }
                Write::Add(n)
            }
            _ => {
                return Err(HostWriteError::ValueOrAdd {
                    path: w.path.clone(),
                })
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
            return Err(HostWriteError::UnknownQuest { quest: q.clone() });
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
                        Some(i) => {
                            Some(i64::try_from(i).expect("a position is at most isize::MAX"))
                        }
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
                let writes = resolve_writes(s.project(), writes).map_err(|e| e.to_string())?;
                let landed = s.engine(0, &writes);
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
                let vals = crate::session::typed_payload(s.project(), occasion, &payload)
                    .map_err(|e| e.to_string())?;
                s.bind_payload(&vals);
            }
            let p = pick.as_ref().map(PickInput::session).transpose()?;
            Ok(s.occasion(0, occasion, target, &p, &BTreeMap::new()))
        }
        Input::AdvanceClock { by, writes, pick } => {
            let w = resolve_writes(s.project(), writes).map_err(|e| e.to_string())?;
            let by = resolve_advance(s.project(), by)?;
            let raise = s
                .project()
                .index
                .clock
                .as_ref()
                .and_then(|c| c.raise.as_ref())
                .map_or_else(Default::default, lute_manifest::clock::ClockRaise::moments);
            let p = pick.as_ref().map(PickInput::session).transpose()?;
            Ok(s.advance(0, by, &w, &raise, &p, &BTreeMap::new()))
        }
        Input::HostWrite { writes } => {
            let w = resolve_writes(s.project(), writes).map_err(|e| e.to_string())?;
            Ok(s.engine(0, &w))
        }
        Input::WorldEvent { name } => Ok(s.event(name)),
        Input::NewRun { writes } => {
            let w = resolve_writes(s.project(), writes).map_err(|e| e.to_string())?;
            Ok(s.new_run(0, &w))
        }
        Input::Choose { .. } | Input::Timeout { .. } | Input::BridgeResult { .. } => {
            Err("answer input cannot execute without a continuation".into())
        }
    }
}

struct ReplayFactory {
    project: Arc<ExecProject>,
    answers: Vec<Answer>,
    /// dsl 0.41.0 §5.3: each answer's resolved `writes`, by answer index
    /// (empty for a choice).
    writes: Arc<Vec<Writes>>,
    answer_cursor: usize,
    events: Vec<Event>,
    pending: Option<Await>,
    pending_cut: Option<usize>,
    /// The stage at the suspension point, taken from the suspended walk.
    pending_stage: Option<crate::Stage>,
    document: String,
}
impl ReplayFactory {
    /// A factory for a query: no answers to replay.
    fn new(project: Arc<ExecProject>, answers: Vec<Answer>) -> Self {
        Self {
            project,
            answers,
            writes: Arc::default(),
            answer_cursor: 0,
            events: Vec::new(),
            pending: None,
            pending_cut: None,
            pending_stage: None,
            document: String::new(),
        }
    }

    /// A factory replaying `answers`, each bridge answer's `writes`
    /// resolved ([`resolve_answer_writes`]); `Err` names a write that does
    /// not resolve.
    fn replaying(project: Arc<ExecProject>, answers: Vec<Answer>) -> Result<Self, HostWriteError> {
        let writes = answers
            .iter()
            .map(|answer| match answer {
                Answer::Bridge { writes, .. } => resolve_bridge_writes(&project, writes),
                Answer::Choice { .. } => Ok(Writes::default()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut factory = Self::new(project, answers);
        factory.writes = Arc::new(writes);
        Ok(factory)
    }
}
struct ReplayDriver {
    choices: Vec<Answer>,
    writes: Arc<Vec<Writes>>,
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
                timeout: menu.timeout,
                default: menu.default.map(str::to_owned),
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
            if let Answer::Bridge { tag, fields, .. } = &self.choices[self.cursor] {
                if tag == call.tag {
                    let writes = self.writes.get(self.cursor).cloned().unwrap_or_default();
                    self.cursor += 1;
                    return BridgeReply::Answer {
                        fields: fields
                            .iter()
                            .filter_map(|(k, v)| scalar_text(Some(v)).map(|s| (k.clone(), s)))
                            .collect(),
                        writes,
                    };
                }
            }
        }
        // dsl 0.41.0 §5.5: each result field content reads, typed by the
        // capability's `result:` (index `bridgeResults`), else by the state
        // slot it lands on.
        let results = call
            .reads
            .iter()
            .filter(|(field, _)| self.project.bridge_reads.reads(call.tag, field))
            .map(|(field, path)| {
                let declared = self
                    .project
                    .bridge_reads
                    .result_types
                    .get(call.tag)
                    .and_then(|fields| fields.get(field))
                    .cloned();
                let ty = declared.unwrap_or_else(|| {
                    self.project
                        .state_table
                        .get(path)
                        .map_or(lute_manifest::types::Type::Str, state_result_type)
                });
                (field.clone(), ty)
            })
            .collect();
        self.pending = Some(Await::Bridge {
            request: 0,
            tag: call.tag.into(),
            document: self.document.clone(),
            position: call.addr.into(),
            attrs: call
                .attrs
                .map(|attrs| attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            stamps: call
                .stamps
                .map(|stamps| stamps.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            results,
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
            writes: Arc::clone(&self.writes),
            cursor: self.answer_cursor,
            document: document.into(),
            events: Vec::new(),
            pending: None,
            pending_cut: None,
            project: Arc::clone(&self.project),
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
        if driver.pending.is_some() {
            // The walk stops at the unanswered pick, so its carry holds the
            // stage at the await (dsl 0.40.0 §4.2).
            self.pending_stage = Some(carry.stage.clone());
        }
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
        result: Result<(), crate::WalkError>,
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
        result.err().map(|e| PlayHalt::Fatal(e.message))
    }
}

/// The type a result field landing on a state slot (its state-table
/// `entry`) answers in, when no capability declares one.
fn state_result_type(entry: &Json) -> lute_manifest::types::Type {
    use lute_manifest::types::Type;
    match entry.get("type").and_then(Json::as_str) {
        Some("bool") => Type::Bool,
        Some("int") => Type::Int,
        Some("double" | "number") => Type::Double,
        Some("enum") => Type::Enum(
            entry
                .get("domain")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
        ),
        _ => Type::Str,
    }
}

/// dsl 0.41.0 §5.5 rule 2: a `bridgeResult`'s `fields` against the await's
/// `results` types — `int` an integer within `±2^53` (the bound of every
/// host-passed `int`, as [`def_args`]), `double` a number, `bool` a
/// boolean, `string` a string, `enum` a member string, `domain` / `entity`
/// a string.
fn validate_bridge(
    results: &BTreeMap<String, lute_manifest::types::Type>,
    fields: &BTreeMap<String, Json>,
) -> Result<(), String> {
    use lute_manifest::types::Type;
    for (name, ty) in results {
        let Some(v) = fields.get(name) else {
            return Err(format!("bridge field `{name}` is required"));
        };
        let (good, what) = match ty {
            Type::Bool => (v.is_boolean(), "a boolean".to_string()),
            Type::Int => (
                crate::expr::host_int(v).is_some(),
                "an integer within ±2^53".to_string(),
            ),
            Type::Double => (v.is_number(), "a number".to_string()),
            Type::Enum(members) => (
                v.as_str().is_some_and(|s| members.iter().any(|m| m == s)),
                format!("one of {}", members.join(", ")),
            ),
            _ => (v.is_string(), "a string".to_string()),
        };
        if !good {
            return Err(format!("bridge field `{name}` must be {what}, not `{v}`"));
        }
    }
    Ok(())
}
