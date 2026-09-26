//! The compile-side source map (runtime-unification design §3.5): for every
//! emitted record, the authored construct it came from — span, authored
//! text, rendered guards — so a walker that executes the IR can still report
//! in source terms (`lute trace`'s decisions, coverage sites and skipped
//! writes). Produced by [`crate::compile_mapped`] next to the artifact and
//! never serialized: the artifact's bytes do not depend on it.
//!
//! Every text here is rendered the way `lute-trace`'s AST walk renders it
//! today, from the same normalized + expanded tree compile lowers, so a
//! report built from the map matches the report built from the AST.

use std::collections::BTreeMap;

use lute_core_span::Span;
use lute_syntax::ast::{CelSlot, Choice, IsPattern};

/// The map for one compiled document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceMap {
    /// Every emitted record, by its final `addr`.
    pub by_addr: BTreeMap<String, SourceInfo>,
    /// Source-only steps after the last record of an addressing unit, by
    /// the unit's 1-based number (the `addr` shot segment).
    pub trailing: BTreeMap<i64, Vec<SourceMarker>>,
    /// Every `<quest>`, by id.
    pub quests: BTreeMap<String, QuestSource>,
    /// Every `<entry>`, by its (document-local) id.
    pub entries: BTreeMap<String, Span>,
    /// Every bundle `<beat>`, by its canonical `<document id>.<beat id>`.
    pub beats: BTreeMap<String, Span>,
}

/// Where one record came from.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceInfo {
    /// The authored construct's span. For a `match` / `choice` / `hub`
    /// record the construct's own (the coverage site); for an arm's
    /// closing `jump` the construct's too; for an injected record the
    /// node that caused the injection.
    pub span: Span,
    /// A `match` record: the subject's authored text when `@def` / `$`
    /// expansion rewrote it (`Decision.authoredId`).
    pub authored_id: Option<String>,
    /// A `match` record: one entry per authored arm, `<otherwise>`
    /// included, in source order. A `choice` / `hub` record: one per
    /// option, in record order. Empty for every other record.
    pub arms: Vec<ArmSource>,
    /// The authored directive tag of a record lowered from a `::directive`
    /// (the record kind may differ: a plugin directive's record lowering).
    pub directive: Option<String>,
    /// A `set` record synthesized from `<choice into="…">`.
    pub sugar: bool,
    /// A `set` / `assert` / `retract` record: the authored write
    /// (`run.x += 1`, `knows(a, b)`), as a re-read entry reports it skipped.
    pub write_text: Option<String>,
    /// A record the stage reducer injected (anchor, preload, pos reset,
    /// hide): it has no authored node of its own.
    pub injected: bool,
    /// A `jump` record lowered from an authored `::next`, not the
    /// structural jump that closes an arm.
    pub authored_jump: bool,
    /// Source-only steps (`::mark`, component boundaries, a `::clear` or
    /// `::use` that lowers to no record of its own) walked right before
    /// this record.
    pub before: Vec<SourceMarker>,
}

impl SourceInfo {
    /// A record from the construct at `span`, with nothing else to say.
    pub fn at(span: Span) -> Self {
        SourceInfo {
            span,
            authored_id: None,
            arms: Vec::new(),
            directive: None,
            sugar: false,
            write_text: None,
            injected: false,
            authored_jump: false,
            before: Vec::new(),
        }
    }
}

/// One `<match>` arm or one `<choice>` of a branch / hub.
#[derive(Clone, Debug, PartialEq)]
pub struct ArmSource {
    pub span: Span,
    /// The expanded guard text as trace renders it (`is="a|b" && <test>`
    /// for an arm, the `when` text for a choice); `None` when unguarded or
    /// `<otherwise>`.
    pub guard: Option<String>,
    /// The same, as authored, when expansion rewrote it
    /// (`Decision.authoredGuard`).
    pub authored_guard: Option<String>,
}

impl ArmSource {
    /// A `<when is test>` arm (the guard text `lute trace` shows).
    pub fn when(span: Span, is: Option<&IsPattern>, test: &CelSlot) -> Self {
        ArmSource {
            span,
            guard: render_arm_guard(is, &test.raw),
            authored_guard: test
                .authored
                .as_deref()
                .and_then(|t| render_arm_guard(is, t)),
        }
    }

    /// An `<otherwise>` arm.
    pub fn otherwise(span: Span) -> Self {
        ArmSource {
            span,
            guard: None,
            authored_guard: None,
        }
    }

    /// A branch or hub `<choice>`.
    pub fn choice(c: &Choice) -> Self {
        ArmSource {
            span: c.span,
            guard: trimmed(c.when.as_ref().map(|s| s.raw.as_str())),
            authored_guard: trimmed(c.when.as_ref().and_then(|s| s.authored.as_deref())),
        }
    }
}

/// A source construct that emits no record but is a step of the walk.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceMarker {
    /// The directive tag (`mark`, the component sentinels, `clear`, `use`).
    pub tag: String,
    pub span: Span,
    /// Set on the component expansion sentinels.
    pub component: Option<ComponentBoundary>,
}

/// Which end of a component expansion a sentinel marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentBoundary {
    Begin,
    End,
}

/// One `<quest>`: its span and the guard texts trace reports for it.
#[derive(Clone, Debug, PartialEq)]
pub struct QuestSource {
    pub span: Span,
    /// The expanded `start=` / `fail=` text, trimmed; `None` when absent.
    pub start: Option<String>,
    pub fail: Option<String>,
    pub objectives: BTreeMap<String, ObjectiveSource>,
    /// Every `<on>` handler record's `addr` -> the handler's span.
    pub handlers: BTreeMap<String, Span>,
}

/// One `<objective>`.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveSource {
    pub span: Span,
    /// The expanded `done=` / `by=` / `until=` text, trimmed.
    pub done: Option<String>,
    pub by: Option<String>,
    pub until: Option<String>,
}

/// The guard text `lute trace` shows for an arm: its `is` pattern and `test` guard as
/// one line.
fn render_arm_guard(is: Option<&IsPattern>, test_raw: &str) -> Option<String> {
    let test = test_raw.trim();
    match (is, test.is_empty()) {
        (Some(p), true) => Some(format!("is=\"{}\"", p.raw)),
        (Some(p), false) => Some(format!("is=\"{}\" && {test}", p.raw)),
        (None, true) => None,
        (None, false) => Some(test.to_string()),
    }
}

/// A slot's text, trimmed; `None` when absent or blank.
pub(crate) fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}
