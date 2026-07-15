//! `Diagnostic.fixits` -> LSP `CodeAction` mapping (Task 15, D16).
//!
//! lute-lsp's first code-action surface. `check()`'s `Diagnostic` already
//! carries machine-applicable `fixits` — the `E-PERSIST-REMOVED` migrate
//! remedy (0.6.0 §2.2/§2.3) and the §8.1 `E-CEL-PARSE` T2 rewrites (Task 13) —
//! but
//! `lute fix` can never apply them by construction (`fix_document` never reads
//! checker diagnostics, D16): the ONLY surface an author reaches them through
//! is a `textDocument/codeAction` quick fix. This module is the pure mapping
//! function; [`crate::backend::Backend`] wires it to the handler and the
//! per-document diagnostic cache that retains the original `Vec<Diagnostic>`
//! (fixits included — [`crate::convert::to_lsp_diagnostic`]'s published wire
//! form has no `fixits` field) across requests.
//!
//! ## Shape: one `CodeAction` per fixit, not per diagnostic
//! A diagnostic with N fixits (a §8.1 `E-CEL-PARSE` T2 rewrite carries one,
//! but the mapping is per-fixit not per-diagnostic) yields N separate
//! `CodeAction`s — one per remedy — each
//! naming its own [`Fixit::title`] and applying only that fixit's edits. An
//! author facing two mutually exclusive fixes must be offered two choices, not
//! one action that tries to apply both.
//!
//! ## Overlap, not containment
//! A diagnostic is offered when its range OVERLAPS the request `range` (the
//! editor's selection/cursor range) — not full containment — matching how
//! `textDocument/codeAction` selection ranges behave for most editors (a
//! cursor collapsed to a point is a zero-width range that still overlaps any
//! diagnostic it sits inside).

use std::collections::HashMap;

use lute_core_span::{Diagnostic, TextIndex};
use tower_lsp_server::ls_types as lsp_types;

use crate::convert::{to_lsp_diagnostic, to_lsp_range};

/// Build one [`CodeAction`](lsp_types::CodeAction) per [`Fixit`](lute_core_span::Fixit)
/// on every entry of `diagnostics` whose span overlaps `range`, in `uri`'s
/// document. Diagnostics with no fixits (the overwhelming majority) contribute
/// no actions. Each action's `edit` splices that ONE fixit's `TextEdit`s into
/// `uri` through the SAME `Span -> Range` conversion [`crate::convert`] uses
/// for the published diagnostic surface, so a fixit-applied edit and a
/// diagnostic's own reported range always agree to the code unit.
pub fn code_actions_for_fixits(
    diagnostics: &[Diagnostic],
    uri: &lsp_types::Uri,
    range: lsp_types::Range,
    idx: &TextIndex,
) -> Vec<lsp_types::CodeAction> {
    let mut actions = Vec::new();
    for d in diagnostics {
        if d.fixits.is_empty() {
            continue;
        }
        if !ranges_overlap(to_lsp_range(&d.span, idx), range) {
            continue;
        }
        let lsp_diag = to_lsp_diagnostic(d, idx, uri);
        for fixit in &d.fixits {
            let edits: Vec<lsp_types::TextEdit> = fixit
                .edit
                .iter()
                .map(|e| lsp_types::TextEdit {
                    range: to_lsp_range(&e.span, idx),
                    new_text: e.new_text.clone(),
                })
                .collect();
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);
            actions.push(lsp_types::CodeAction {
                title: fixit.title.clone(),
                kind: Some(lsp_types::CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![lsp_diag.clone()]),
                edit: Some(lsp_types::WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
    }
    actions
}

/// Whether two half-open LSP ranges overlap. `Position` derives `Ord` over
/// `(line, character)` lexicographically — exactly LSP's own position
/// ordering — so this is the same comparison every LSP client uses to decide
/// range containment/overlap.
fn ranges_overlap(a: lsp_types::Range, b: lsp_types::Range) -> bool {
    a.start <= b.end && b.start <= a.end
}

