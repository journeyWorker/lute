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
//! `layer`, `fixits`, and `provenance` are Phase-6.3+ concerns (code actions /
//! related information) and are intentionally not surfaced here yet.

use lute_core_span::{Diagnostic, Severity, Span, TextIndex};
// v0.23 of `tower-lsp-server` re-exports the LSP type crate as `ls_types`
// (backed by `ls-types` 0.0.6), not `lsp_types`. Alias it so the signature reads
// as the brief's contract (`-> lsp_types::Diagnostic`) while binding the real path.
use tower_lsp_server::ls_types as lsp_types;

/// Convert a single core [`Diagnostic`] to its LSP wire form, resolving the byte
/// span to a UTF-16 [`Range`](lsp_types::Range) through `idx`.
///
/// `idx` MUST index the exact document text the diagnostic's byte offsets refer
/// to (a fresh `TextIndex::new(&text)` over the open document), so positions match
/// the headless path that `check()` normalized against.
pub fn to_lsp_diagnostic(d: &Diagnostic, idx: &TextIndex) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: to_lsp_range(&d.span, idx),
        severity: Some(to_lsp_severity(d.severity)),
        code: Some(lsp_types::NumberOrString::String(d.code.clone())),
        source: Some("lute".into()),
        message: d.message.clone(),
        ..Default::default()
    }
}

/// Map a byte [`Span`] to an LSP [`Range`](lsp_types::Range): each endpoint's byte
/// offset goes through [`TextIndex::position`] and is de-1-indexed for the line and
/// used as-is for the (already 0-based UTF-16) character.
fn to_lsp_range(span: &Span, idx: &TextIndex) -> lsp_types::Range {
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
        };
        let l = to_lsp_diagnostic(&d, &line_index());
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
        let l = to_lsp_diagnostic(&d, &idx);
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
        let l = to_lsp_diagnostic(&d, &idx);
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
            let l = to_lsp_diagnostic(&diag("E-X", core, span), &idx);
            assert_eq!(l.severity, Some(lsp), "{core:?} must map to {lsp:?}");
        }
    }
}
