//! Graph-independent identity rename diagnostics.

use lute_core_span::Span;

/// One authored-ledger validation failure, retaining its manifest location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<Span>,
}

/// `<code>: <message>`, the form the project model turns into a diagnostic.
impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RenameError {}
