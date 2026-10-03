//! The one walker (`docs/design/runtime-unification.md`): [`Machine`]
//! executes a compiled artifact; a [`Driver`] supplies what the runtimes
//! decide differently (decisions, refusals, bridge answers, whether an
//! unknown halts) and receives every transcript record.
//!
//! Records are `serde_json::Value` objects, one per executed construct, in
//! the shapes `lute run --json` prints (`conformance/*/expected.json`):
//! `line` (`addr`, `speaker`, `text`, plus [`LINE_DELIVERY_KEYS`] when the
//! artifact carries them), the stage kinds (`addr`, `kind`), `set` (`path`,
//! `value`, plus `effectOf: <tag>` for a directive's declared effect and
//! `bridgeResult: <field>` when that effect wrote an answered bridge result),
//! `assert` (`fact`), `retract` (`pattern`), `skipped` (`effect`
//! and `path` / `fact` / `pattern`), `choice` (`branch`, `chose`, plus
//! `note` / `scripted` when undecided and [`MENU_MARK_KEYS`]), `hub` (`hub`,
//! `prompt`, `chose`, the same marks), `match` (`result`), `barrier`,
//! `end` (`reason`), `plugin` (`tag`, `external`, `unresolvedEffects`,
//! `answered` / `unanswered`, `note`), `accept` (`quest`, `at` / `ignored`),
//! `occasion`, `objective`, `quest` (`state`, `failedBy`), `grant`, `entry`,
//! `beat` and `exclusive` (`text`).
//!
//! [`Driver::observe`] receives the judgments that write no record, for
//! `lute trace`: `{kind: "quest", quest, outcome: active | complete |
//! failed | never | awaiting accept, guard, forced}` (a seeded status is
//! `guard: "seeded"`), `{kind: "objective", quest, objective, outcome: done
//! | pending | failed, guard}`, `{kind: "on", event, quest, addr, outcome:
//! fires | skipped, guard}`, `{kind: "acceptSpent", quest, parent, why}` and
//! `{kind: "jump", addr}` (every `jump` taken: an authored `::next`, or a
//! structural one the source map hangs source-only steps on). Under
//! [`Machine::with_arm_probe`] (the differential harness's IR oracle) also
//! `{kind: "armExpr", addr, arm, expr, held, reads}`.
//!
//! wasm-clean: nothing here touches the filesystem, the process or threads.

pub mod cadence;
mod driver;
mod machine;
pub mod record;
pub mod seam;
pub mod session;
pub(crate) mod store;

pub use driver::{
    guard_premise, BridgeCall, BridgeQueues, BridgeReply, Driver, Forced, GuardRead, Menu,
    MenuKind, MenuOption, OnUnknown, Pick, ScriptedChoices, SiteKind, UnknownSite, Verdict,
};
pub use machine::{
    expr_to_cel, render_fact, value_to_json, value_to_string, BridgeReads, Carry, EvalObserver,
    EvalSnapshot, Machine, Seed, LINE_DELIVERY_KEYS, MENU_MARK_KEYS, NOTE_NO_DECISION,
    NOTE_NO_ELIGIBLE, NOTE_SKIPPED,
};
pub use record::{line_head, render_attrs, said_line};
