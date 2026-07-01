//! Shared checker context threaded through every `check_*` entrypoint.
//!
//! `Ctx` is deliberately minimal at Task 4.2: directive validation only needs it
//! to exist and be passed through. Later tasks EXTEND it in place — T4.3 (CEL
//! type/scope resolution) reads `in_match`/`match_subject` to type the `$`
//! subject inside a `match`; T4.4 (def-assignment §8.1), T4.5 (app-write
//! read-only §9.5), T4.6, and T4.7 add their own fields here. Keep it small and
//! `Default`-able so those tasks can grow it without touching call sites.

/// Analysis mode. `Author` is the interactive LSP default (lenient about
/// catalog staleness); `Ci` is the batch/build mode that later tasks may treat
/// more strictly. T4.2 does not branch on it, but downstream tasks will.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Author,
    Ci,
}

/// Checker context threaded through the directive/CEL/state validators.
///
/// Fields are the minimal set T4.2 needs plus the `match`-scope hooks T4.3 will
/// consume. Later tasks append fields; do not remove any without updating them.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    /// True while validating nodes nested inside a `match` block.
    pub in_match: bool,
    /// The raw CEL subject expression of the enclosing `match`, if any (the `$`
    /// binding T4.3 resolves).
    pub match_subject: Option<String>,
    /// Author (interactive LSP) vs. Ci (batch) analysis mode.
    pub mode: Mode,
}
