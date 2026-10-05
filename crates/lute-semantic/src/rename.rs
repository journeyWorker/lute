//! Graph-independent identity rename diagnostics.

use lute_core_span::Span;

/// One authored-ledger validation failure, retaining its manifest location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<Span>,
}
