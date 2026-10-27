//! Shared execution runtime for concrete and preview walks.

pub mod cadence;
pub mod clock;
pub mod datalog;
pub mod driver;
pub mod eval;
pub mod expr;
pub mod index;
pub mod input;
pub mod machine;
pub mod runtime;
pub mod schema;
pub mod seam;
pub mod session;
pub mod snapshot;
pub mod stage;
pub mod store;
pub mod value;

pub use driver::{
    guard_premise, BridgeCall, BridgeQueues, BridgeReply, Driver, Forced, GuardRead, Menu,
    MenuKind, MenuOption, MenuText, OnUnknown, Pick, ScriptedChoices, SiteKind, UnknownSite,
    Verdict,
};
pub use eval::{EffectiveState, EvalEnv, FactStore, Pat, Read};
pub use expr::Slot;
pub use input::{
    bridge_answer_shape, bridge_result_writes, raise_judges, split_occasion, str_of,
    type_placeholder, BridgeAnswer, MockSet,
};
pub use machine::{
    render_fact, value_to_json, value_to_string, BridgeReads, Carry, EvalObserver, EvalSnapshot,
    Machine, Seed, WalkError, LINE_DELIVERY_KEYS, MENU_MARK_KEYS, NOTE_NO_DECISION,
    NOTE_NO_ELIGIBLE, NOTE_SKIPPED, PLUGIN_DELIVERY_KEYS,
};
pub use runtime::{
    resolve_bridge_writes, AdvanceBy, Await, ClockPosition, Continuation, Evaluation, Event,
    HostWriteError, Input, MenuAwait, MenuOptionAwait, Output, Phase, PickInput, Rejected, Runtime,
    SaveInput, Seed as RuntimeSeed, Snapshot, SnapshotJsonInput, State, StateWrite, StreamLine,
    WritesInput,
};
pub use stage::{Stage, StageActor, StageBackground, StageCg, StageMusic, StageVideo};
pub use value::{value_text, UnresolvedAtom, Value};
