//! Shared execution runtime for concrete and preview walks.

pub mod cadence;
pub mod clock;
pub mod datalog;
pub mod driver;
pub mod eval;
pub mod expr;
pub mod input;
pub mod index;
pub mod machine;
pub mod runtime;
pub mod seam;
pub mod schema;
pub mod session;
pub mod store;
pub mod value;

pub use eval::{EffectiveState, EvalEnv, FactStore, Pat, Read};
pub use input::{
    bridge_answer_shape, bridge_result_writes, raise_judges, split_occasion, str_of,
    type_placeholder, BridgeAnswer, MockSet,
};
pub use machine::{
    render_fact, value_to_json, value_to_string, BridgeReads, Carry, EvalObserver, EvalSnapshot,
    Machine, Seed, LINE_DELIVERY_KEYS, MENU_MARK_KEYS, NOTE_NO_DECISION, NOTE_NO_ELIGIBLE,
    NOTE_SKIPPED,
};
pub use expr::Slot;
pub use value::{value_text, UnresolvedAtom, Value};
pub use driver::{guard_premise, BridgeCall, BridgeQueues, BridgeReply, Driver, Forced, GuardRead, Menu, MenuKind, MenuOption, OnUnknown, Pick, ScriptedChoices, SiteKind, UnknownSite, Verdict};
pub use runtime::{AdvanceBy, Await, ClockPosition, Continuation, Event, Input, MenuAwait, MenuOptionAwait, Output, Phase, PickInput, Rejected, Runtime, SaveInput, Seed as RuntimeSeed, State, StateWrite, WritesInput};
