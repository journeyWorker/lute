//! Source locations shared by semantic consumers.

use lute_core_span::Span;
use serde::{Serialize, Serializer};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub span: Span,
}

impl Serialize for SourceLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Location<'a> {
            file: &'a PathBuf,
            span: Span,
        }
        Location {
            file: &self.file,
            span: self.span,
        }
        .serialize(serializer)
    }
}
