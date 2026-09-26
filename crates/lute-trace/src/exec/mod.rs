//! The one walker (`docs/design/runtime-unification.md`): [`Machine`]
//! executes a compiled artifact; a [`Driver`] supplies what the runtimes
//! decide differently (decisions, refusals, bridge answers, whether an
//! unknown halts) and receives every transcript record.
//!
//! Records are `serde_json::Value` objects, one per executed construct, in
//! the shapes `lute run --json` prints (`conformance/*/expected.json`):
//! `line` (`addr`, `speaker`, `text`, plus [`LINE_DELIVERY_KEYS`] when the
//! artifact carries them), the stage kinds (`addr`, `kind`), `set` (`path`,
//! `value`), `assert` (`fact`), `retract` (`pattern`), `skipped` (`effect`
//! and `path` / `fact` / `pattern`), `choice` (`branch`, `chose`, plus
//! `note` / `scripted` when undecided and [`MENU_MARK_KEYS`]), `hub` (`hub`,
//! `prompt`, `chose`, the same marks), `match` (`result`), `barrier`,
//! `end` (`reason`), `plugin` (`tag`, `external`, `unresolvedEffects`,
//! `answered` / `unanswered`, `note`), `accept` (`quest`, `at` / `ignored`),
//! `occasion`, `objective`, `quest` (`state`, `failedBy`), `grant`, `entry`,
//! `beat` and `exclusive` (`text`).
//!
//! wasm-clean: nothing here touches the filesystem, the process or threads.

mod driver;
mod machine;

pub use driver::{
    BridgeCall, BridgeQueues, BridgeReply, Driver, Forced, Menu, MenuKind, MenuOption, OnUnknown,
    Pick, ScriptedChoices, SiteKind, UnknownSite, Verdict,
};
pub use machine::{
    render_fact, value_to_json, value_to_string, BridgeReads, Carry, Machine, Seed,
    LINE_DELIVERY_KEYS, MENU_MARK_KEYS, NOTE_NO_DECISION, NOTE_NO_ELIGIBLE, NOTE_SKIPPED,
};
