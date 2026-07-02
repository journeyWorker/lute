//! `lute-lsp`: the Lute language server (Phase 6).
//!
//! `check()` is the contract, not the LSP protocol — this crate is a thin
//! `tower-lsp-server` wrapper that funnels document opens/changes into the shared
//! [`lute_check::check`] core and republishes its diagnostics over the wire. It
//! owns no validation logic; every diagnostic originates in a Phase-3/Phase-4
//! validator with its own tests.
//!
//! Task 6.1 covers exactly two surfaces:
//! - [`convert::to_lsp_diagnostic`], the pure byte-span -> UTF-16 `Range`
//!   conversion (kept `pub` so Task 6.2's divergence golden can call it), and
//! - [`backend::Backend`], a `LanguageServer` that advertises FULL text sync +
//!   `publishDiagnostics` and nothing else — hover/completion/navigation/folding/
//!   semantic-tokens/symbols land in Tasks 6.3/6.4.

pub mod backend;
pub mod convert;
