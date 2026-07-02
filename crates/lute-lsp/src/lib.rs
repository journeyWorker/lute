//! `lute-lsp`: the Lute language server (Phase 6).
//!
//! `check()` is the contract, not the LSP protocol — this crate is a thin
//! `tower-lsp-server` wrapper that funnels document opens/changes into the shared
//! [`lute_check::check`] core and republishes its diagnostics over the wire. It
//! owns no validation logic; every diagnostic originates in a Phase-3/Phase-4
//! validator with its own tests.
//!
//! Task 6.1 covers the diagnostic surface:
//! - [`convert::to_lsp_diagnostic`], the pure byte-span -> UTF-16 `Range`
//!   conversion (kept `pub` so Task 6.2's divergence golden can call it), and
//! - [`backend::Backend`], a `LanguageServer` that advertises FULL text sync +
//!   `publishDiagnostics`.
//!
//! Task 6.3 adds the editor feature surface in [`features`]: pure hover /
//! completion / definition / references functions keyed on a cursor byte offset,
//! wired into the backend as thin handlers. Folding / semantic-tokens / symbols
//! land in Task 6.4.

pub mod backend;
pub mod convert;
pub mod features;
