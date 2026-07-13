//! Pure `lute_core_span::Diagnostic` -> LSP `Diagnostic` conversion (Task 6.1).
//!
//! This is the *only* place byte spans become LSP `Range`s, and it is kept pure
//! (no async, no server state) so the divergence golden (Task 6.2) can call it
//! directly and assert the LSP surface reports byte-for-byte identical positions
//! to the headless CLI path.
//!
//! ## The UTF-16 invariant
//! LSP `Position.character` is a UTF-16 code-unit offset within its line, NOT a
//! byte column. We never hand-roll that count: each byte offset is mapped through
//! [`TextIndex::position`], which already walks the line slice
//! `chars().map(len_utf16).sum()` — multibyte-safe by construction. `check()`
//! re-derives every span from its bytes through one shared [`TextIndex`] before
//! returning (the determinism carry-forward in `lute_check::check`'s docs); by
//! mapping the *same* `byte_start`/`byte_end` through a `TextIndex` built from the
//! *same* text, the LSP and headless surfaces agree to the code unit.
//!
//! ## Field mapping
//! - `span.byte_start` / `span.byte_end` -> `Range { start, end }` via
//!   `Position { line: p.line - 1, character: p.utf16_col }` (LSP is 0-based on
//!   both axes; `TextIndex` reports 1-based line / 0-based UTF-16 col).
//! - [`Severity`] -> `DiagnosticSeverity` (Error/Warning/Info/Hint -> the four LSP
//!   severities).
//! - `code` -> `NumberOrString::String` (our codes are stable strings, e.g.
//!   `E-UNDECLARED`); `source` is fixed to `"lute"`; `message` is carried verbatim.
//!
//! `layer` and `provenance` are Phase-6.3+ concerns and are intentionally not
//! surfaced here yet. `fixits` are Task 15's concern too, but consumed
//! elsewhere ([`crate::code_action`] maps them to `CodeAction`s directly off
//! the cached original `Diagnostic`s — this module never surfaces them on the
//! published `Diagnostic` itself, which has no such field on the wire).
//! `covered` (dsl 0.4.0 §8.2 C1/C5, Task 14) IS surfaced here (Task 15): each
//! entry becomes one `DiagnosticRelatedInformation` "also here" pointer in the
//! SAME document, through the identical `Span -> Range` conversion every other
//! position in this module uses.

use lute_core_span::{Diagnostic, Severity, Span, TextIndex};
// v0.23 of `tower-lsp-server` re-exports the LSP type crate as `ls_types`
// (backed by `ls-types` 0.0.6), not `lsp_types`. Alias it so the signature reads
// as the brief's contract (`-> lsp_types::Diagnostic`) while binding the real path.
use tower_lsp_server::ls_types as lsp_types;

/// Convert a single core [`Diagnostic`] to its LSP wire form, resolving the byte
/// span to a UTF-16 [`Range`](lsp_types::Range) through `idx`. `uri` is the
/// document the diagnostic belongs to — needed only to stamp `covered`'s
/// related-information `Location`s (dsl 0.4.0 §8.2, Task 15); every OTHER field
/// is `uri`-independent.
///
/// `idx` MUST index the exact document text the diagnostic's byte offsets refer
/// to (a fresh `TextIndex::new(&text)` over the open document), so positions match
/// the headless path that `check()` normalized against.
pub fn to_lsp_diagnostic(d: &Diagnostic, idx: &TextIndex, uri: &lsp_types::Uri) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: to_lsp_range(&d.span, idx),
        severity: Some(to_lsp_severity(d.severity)),
        code: Some(lsp_types::NumberOrString::String(d.code.clone())),
        source: Some("lute".into()),
        message: d.message.clone(),
        related_information: covered_related_information(d, idx, uri),
        ..Default::default()
    }
}

/// Map `d.covered` (dsl 0.4.0 §8.2 C1/C5: `lute-check`'s `collapse_same_root`,
/// Task 14) to LSP `DiagnosticRelatedInformation` entries — one "also here"
/// pointer per folded repeat occurrence, in the SAME document `uri` the primary
/// diagnostic lives in. `None` (never `Some(vec![])`) when `covered` is empty —
/// the overwhelming common case — so the wire form omits the field entirely
/// (`related_information`'s `skip_serializing_if`).
fn covered_related_information(
    d: &Diagnostic,
    idx: &TextIndex,
    uri: &lsp_types::Uri,
) -> Option<Vec<lsp_types::DiagnosticRelatedInformation>> {
    if d.covered.is_empty() {
        return None;
    }
    Some(
        d.covered
            .iter()
            .map(|span| lsp_types::DiagnosticRelatedInformation {
                location: lsp_types::Location {
                    uri: uri.clone(),
                    range: to_lsp_range(span, idx),
                },
                message: "also here".to_string(),
            })
            .collect(),
    )
}

/// Map a byte [`Span`] to an LSP [`Range`](lsp_types::Range): each endpoint's byte
/// offset goes through [`TextIndex::position`] and is de-1-indexed for the line and
/// used as-is for the (already 0-based UTF-16) character. `pub(crate)` so
/// [`crate::code_action`] (Task 15) reuses the SAME conversion for fixit-edit
/// spans instead of hand-rolling a second one.
pub(crate) fn to_lsp_range(span: &Span, idx: &TextIndex) -> lsp_types::Range {
    lsp_types::Range {
        start: to_lsp_position(span.byte_start, idx),
        end: to_lsp_position(span.byte_end, idx),
    }
}

/// Map one byte offset to an LSP [`Position`](lsp_types::Position). `TextIndex`
/// reports a 1-based `line` and a 0-based `utf16_col`; LSP wants 0-based on both.
fn to_lsp_position(byte: usize, idx: &TextIndex) -> lsp_types::Position {
    let p = idx.position(byte);
    lsp_types::Position {
        line: p.line - 1,
        character: p.utf16_col,
    }
}

/// Core [`Severity`] -> LSP `DiagnosticSeverity` (total; the four map 1:1).
fn to_lsp_severity(sev: Severity) -> lsp_types::DiagnosticSeverity {
    match sev {
        Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
        Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
        Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        Severity::Hint => lsp_types::DiagnosticSeverity::HINT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::{Layer, Span};
    use std::str::FromStr;

    fn test_uri() -> lsp_types::Uri {
        lsp_types::Uri::from_str("file:///test.lute").unwrap()
    }

    /// Text whose byte 5 lands on line 3 at UTF-16 column 1: `a`\n`b`\n`hello`.
    /// Bytes: a(0) \n(1) b(2) \n(3) h(4) e(5) l(6) l(7) o(8). `position(5)` ->
    /// line 3, line_start 4, slice "h" -> 1 UTF-16 unit -> `character == 1`.
    fn line_index() -> TextIndex<'static> {
        TextIndex::new("a\nb\nhello")
    }

    fn diag(code: &str, sev: Severity, span: Span) -> Diagnostic {
        Diagnostic {
            code: code.into(),
            severity: sev,
            message: "x".into(),
            span,
            layer: Layer::Cel,
            fixits: vec![],
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        }
    }

    /// The brief's failing test: a byte span becomes a UTF-16 range and the string
    /// code round-trips as `NumberOrString::String`.
    #[test]
    fn diagnostic_uses_utf16_range() {
        let d = Diagnostic {
            code: "E-UNDECLARED".into(),
            severity: Severity::Error,
            message: "x".into(),
            span: Span {
                byte_start: 5,
                byte_end: 8,
                line: 3,
                column: 2,
                utf16_range: (5, 8),
            },
            layer: Layer::Cel,
            fixits: vec![],
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        };
        let l = to_lsp_diagnostic(&d, &line_index(), &test_uri());
        assert_eq!(l.range.start.line, 2, "line 3 -> 0-based 2");
        assert_eq!(l.range.start.character, 1, "utf16 col within line 3");
        assert_eq!(
            l.range.end.character, 4,
            "byte 8 -> slice \"hell\" -> 4 utf16 units"
        );
        assert_eq!(
            l.code.unwrap(),
            lsp_types::NumberOrString::String("E-UNDECLARED".into())
        );
        assert_eq!(l.source.as_deref(), Some("lute"));
        assert_eq!(l.message, "x");
    }

    /// A span whose start and end straddle a newline maps to a two-line LSP range.
    /// Text `abc`\n`def`\n`ghi`: byte 1 -> line 0 col 1, byte 5 -> line 1 col 1.
    #[test]
    fn ascii_multiline_span_spans_two_lines() {
        let idx = TextIndex::new("abc\ndef\nghi");
        let d = diag(
            "E-X",
            Severity::Error,
            Span {
                byte_start: 1,
                byte_end: 5,
                line: 1,
                column: 2,
                utf16_range: (1, 5),
            },
        );
        let l = to_lsp_diagnostic(&d, &idx, &test_uri());
        assert_eq!((l.range.start.line, l.range.start.character), (0, 1));
        assert_eq!((l.range.end.line, l.range.end.character), (1, 1));
    }

    /// The point of the invariant: a multibyte char *before* the span makes the
    /// UTF-16 column diverge from the byte column. `😀` (U+1F600) is 4 UTF-8 bytes
    /// but 2 UTF-16 units, so the `x` at byte 4 sits at UTF-16 character 2 — not 4.
    #[test]
    fn multibyte_line_utf16_col_differs_from_byte_col() {
        let text = "\u{1F600}x"; // "😀x"
        let idx = TextIndex::new(text);
        assert_eq!(text.find('x'), Some(4), "byte column of x is 4");
        let d = diag(
            "E-X",
            Severity::Warning,
            Span {
                byte_start: 4,
                byte_end: 5,
                line: 1,
                column: 5,
                utf16_range: (2, 3),
            },
        );
        let l = to_lsp_diagnostic(&d, &idx, &test_uri());
        assert_eq!(l.range.start.line, 0);
        assert_eq!(
            l.range.start.character, 2,
            "utf16 col is 2, NOT the byte column 4"
        );
        assert_ne!(l.range.start.character, 4, "proves UTF-16 col != byte col");
        assert_eq!(l.range.end.character, 3);
    }

    /// All four severities map 1:1 to the LSP severities.
    #[test]
    fn severity_maps_all_four() {
        let idx = line_index();
        let span = Span {
            byte_start: 0,
            byte_end: 1,
            line: 1,
            column: 1,
            utf16_range: (0, 1),
        };
        let cases = [
            (Severity::Error, lsp_types::DiagnosticSeverity::ERROR),
            (Severity::Warning, lsp_types::DiagnosticSeverity::WARNING),
            (Severity::Info, lsp_types::DiagnosticSeverity::INFORMATION),
            (Severity::Hint, lsp_types::DiagnosticSeverity::HINT),
        ];
        for (core, lsp) in cases {
            let l = to_lsp_diagnostic(&diag("E-X", core, span), &idx, &test_uri());
            assert_eq!(l.severity, Some(lsp), "{core:?} must map to {lsp:?}");
        }
    }

    /// Task 15 (dsl 0.4.0 §8.2 C1/C5): a non-empty `covered` becomes one
    /// `DiagnosticRelatedInformation` per entry, each pointing at that span's
    /// own `Range` (through the SAME `Span -> Range` conversion) in the SAME
    /// `uri` the primary diagnostic was published for. An empty `covered`
    /// (the common case) must leave `related_information` absent (`None`), not
    /// `Some(vec![])` — the wire form should omit the field, not emit an empty
    /// array.
    #[test]
    fn covered_spans_become_related_information() {
        let idx = TextIndex::new("aaaa\nbbbb\ncccc\n");
        let mut d = diag(
            "E-UNDECLARED",
            Severity::Error,
            Span {
                byte_start: 0,
                byte_end: 4,
                line: 1,
                column: 1,
                utf16_range: (0, 4),
            },
        );
        d.covered = vec![
            Span {
                byte_start: 5,
                byte_end: 9,
                line: 2,
                column: 1,
                utf16_range: (5, 9),
            },
            Span {
                byte_start: 10,
                byte_end: 14,
                line: 3,
                column: 1,
                utf16_range: (10, 14),
            },
        ];
        let uri = test_uri();
        let l = to_lsp_diagnostic(&d, &idx, &uri);
        let related = l.related_information.expect("covered must populate related_information");
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].location.uri, uri);
        assert_eq!((related[0].location.range.start.line, related[0].location.range.start.character), (1, 0));
        assert_eq!((related[1].location.range.start.line, related[1].location.range.start.character), (2, 0));
        assert_eq!(related[0].message, "also here");
        assert_eq!(related[1].message, "also here");

        // Empty `covered` -> `None`, not `Some(vec![])`.
        let no_covered = diag("E-X", Severity::Warning, d.span);
        let l2 = to_lsp_diagnostic(&no_covered, &idx, &uri);
        assert!(l2.related_information.is_none());
    }
}
