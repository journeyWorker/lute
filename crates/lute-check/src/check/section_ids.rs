//! Section identity (dsl 0.37.0 §3.1, D9): a `## Heading {#id}` id is unique
//! among the sections of one document.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Document;

/// `E-SECTION-DUP`: two sections of one document carry the same `{#id}`.
pub const E_SECTION_DUP: &str = "E-SECTION-DUP";

/// `E-SECTION-DUP` at every repeat of a section `{#id}` in `doc`, naming the
/// section that holds it first.
pub(super) fn check_section_ids(doc: &Document) -> Vec<Diagnostic> {
    let mut first: std::collections::BTreeMap<&str, (usize, Span)> = Default::default();
    let mut out = Vec::new();
    for (index, section) in doc.sections.iter().enumerate() {
        let Some((id, span)) = &section.id else {
            continue;
        };
        match first.get(id.as_str()) {
            Some((at, earlier)) => out.push(Diagnostic {
                code: E_SECTION_DUP.to_string(),
                severity: Severity::Error,
                message: format!(
                    "section id `{{#{id}}}` is already used by section {} (line {}) — a section \
                     id is unique within its document (dsl 0.37.0 §3.1)",
                    at + 1,
                    earlier.line
                ),
                evidence: None,
                span: *span,
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }),
            None => {
                first.insert(id, (index, *span));
            }
        }
    }
    out
}
