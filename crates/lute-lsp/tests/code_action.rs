//! `Diagnostic.fixits` -> LSP `CodeAction` mapping (Task 15, D16) — the
//! mapping function unit-tested DIRECTLY against hand-built `Diagnostic`s, no
//! live server (`Backend::code_action` is a thin wrapper around
//! [`code_actions_for_fixits`] plus the per-document diagnostic cache;
//! exercising the pure function here is sufficient to pin its contract).

use lute_core_span::{Diagnostic, Fixit, Layer, Severity, Span, TextEdit, TextIndex};
use lute_lsp::code_action::code_actions_for_fixits;
use std::str::FromStr;
use tower_lsp_server::ls_types as lsp_types;

fn test_uri() -> lsp_types::Uri {
    lsp_types::Uri::from_str("file:///test.lute").unwrap()
}

fn span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// The whole-document range every test requests code actions over — wide
/// enough that "overlaps the request range" never itself excludes a
/// diagnostic under test (that gate gets its OWN dedicated test below).
fn whole_doc_range() -> lsp_types::Range {
    lsp_types::Range {
        start: lsp_types::Position {
            line: 0,
            character: 0,
        },
        end: lsp_types::Position {
            line: 1000,
            character: 0,
        },
    }
}

fn text_edit(byte_start: usize, byte_end: usize, new_text: &str) -> TextEdit {
    TextEdit {
        span: span(byte_start, byte_end),
        new_text: new_text.to_string(),
    }
}

/// Mirrors `lute-check`'s `E-PERSIST-REMOVED` migrate diagnostic (0.6.0
/// §2.2/§2.3): a `<choice>` carrying `persist="run"` (bytes 8..21 in the
/// fixture text below) is an error, with ONE machine-applicable `"migrate"`
/// fixit — delete the attr plus its one trailing separator space (bytes
/// 8..22), leaving the bare `into=` that records on its own.
fn persist_removed_diag() -> Diagnostic {
    Diagnostic {
        code: "E-PERSIST-REMOVED".to_string(),
        severity: Severity::Error,
        message: "`persist=` was removed from the language; delete it (dsl 0.6.0 §2.2)"
            .to_string(),
        span: span(8, 21),
        layer: Layer::Logic,
        fixits: vec![Fixit {
            title: "remove persist=".to_string(),
            kind: "migrate".to_string(),
            edit: vec![text_edit(8, 22, "")],
            confidence: 100,
        }],
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Splice `edit` (byte_start/byte_end/new_text is the ONLY thing the mapping
/// fn reads off a source `TextEdit`) into `text` — proves an action's edit
/// lands on the exact expected bytes, mirroring how a real client applies it.
/// The wire `TextEdit` carries only an LSP `Range`, so this re-derives the
/// byte offsets the SAME way a real client would have to: scanning `idx` for
/// the byte whose position round-trips to that `Range` endpoint.
fn splice(text: &str, e: &lsp_types::TextEdit, idx: &TextIndex) -> String {
    let start = byte_of(text, idx, e.range.start);
    let end = byte_of(text, idx, e.range.end);
    format!("{}{}{}", &text[..start], e.new_text, &text[end..])
}

fn byte_of(text: &str, idx: &TextIndex, pos: lsp_types::Position) -> usize {
    for b in 0..=text.len() {
        if !text.is_char_boundary(b) {
            continue;
        }
        let p = idx.position(b);
        if p.line - 1 == pos.line && p.utf16_col == pos.character {
            return b;
        }
    }
    panic!("no byte offset for {pos:?}");
}

/// An `E-PERSIST-REMOVED` diagnostic with its 1 migrate fixit -> 1 action
/// splicing the source to the persist-deleted result, carrying its own
/// diagnostic (dsl 0.6.0 §2.2/§2.3, D16). Pins the fixit-count → action-count
/// mapping and splice correctness on a real 0.6.0 diagnostic shape.
#[test]
fn migrate_fixit_becomes_one_action_with_expected_splice() {
    let text = "<choice persist=\"run\" into=\"run.x\">";
    // bytes 8..21 == `persist="run"`; 8..22 also eats the trailing space.
    assert_eq!(&text[8..21], "persist=\"run\"");
    let idx = TextIndex::new(text);
    let d = persist_removed_diag();
    let uri = test_uri();
    let actions = code_actions_for_fixits(&[d], &uri, whole_doc_range(), &idx);
    assert_eq!(actions.len(), 1, "1 fixit must yield exactly 1 action");

    assert_eq!(actions[0].title, "remove persist=");
    assert_eq!(actions[0].kind, Some(lsp_types::CodeActionKind::QUICKFIX));
    let edit0 = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri];
    assert_eq!(edit0.len(), 1);
    assert_eq!(splice(text, &edit0[0], &idx), "<choice into=\"run.x\">");

    // The action carries only its OWN diagnostic, not a shared list.
    assert_eq!(actions[0].diagnostics.as_ref().unwrap().len(), 1);
    assert_eq!(
        actions[0].diagnostics.as_ref().unwrap()[0].code,
        Some(lsp_types::NumberOrString::String(
            "E-PERSIST-REMOVED".to_string()
        ))
    );
}

/// An `E-CEL-PARSE` diagnostic with the §8.1 T2 `==` fixit -> exactly 1
/// action splicing `run.act = 1` to `run.act == 1` (the §8.4 message shape's
/// changed-code fixture).
#[test]
fn cel_parse_eq_fixit_becomes_one_action() {
    let text = "when=\"run.act = 1\"";
    // The slot text `run.act = 1` sits at bytes 6..17.
    assert_eq!(&text[6..17], "run.act = 1");
    let idx = TextIndex::new(text);
    let d = Diagnostic {
        code: "E-CEL-PARSE".to_string(),
        severity: Severity::Error,
        message: "`=` assigns; comparison is `==` — did you mean `run.act == 1`?".to_string(),
        span: span(6, 17),
        layer: Layer::Cel,
        fixits: vec![Fixit {
            title: "change `=` to `==`".to_string(),
            kind: "refactor".to_string(),
            edit: vec![text_edit(6, 17, "run.act == 1")],
            confidence: 100,
        }],
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    let uri = test_uri();
    let actions = code_actions_for_fixits(&[d], &uri, whole_doc_range(), &idx);
    assert_eq!(actions.len(), 1, "1 fixit must yield exactly 1 action");
    let edit = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri];
    assert_eq!(splice(text, &edit[0], &idx), "when=\"run.act == 1\"");
}

/// A fixit-less diagnostic contributes no action.
#[test]
fn fixit_less_diagnostic_yields_no_action() {
    let idx = TextIndex::new("some text here");
    let d = Diagnostic {
        code: "E-UNDECLARED".to_string(),
        severity: Severity::Error,
        message: "unknown path".to_string(),
        span: span(0, 4),
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    let uri = test_uri();
    let actions = code_actions_for_fixits(&[d], &uri, whole_doc_range(), &idx);
    assert!(actions.is_empty());
}

/// A diagnostic whose span sits entirely outside the requested range
/// contributes no action even though it has fixits — the request range gate
/// is not a no-op.
#[test]
fn out_of_range_diagnostic_with_fixits_yields_no_action() {
    let text = "<choice persist=\"run\" into=\"run.x\">";
    let idx = TextIndex::new(text);
    let d = persist_removed_diag();
    let uri = test_uri();
    // A range confined to line 0 char 0..1, nowhere near byte 8..21
    // (single-line text, so this is a genuinely disjoint sub-range).
    let narrow = lsp_types::Range {
        start: lsp_types::Position {
            line: 0,
            character: 0,
        },
        end: lsp_types::Position {
            line: 0,
            character: 1,
        },
    };
    let actions = code_actions_for_fixits(&[d], &uri, narrow, &idx);
    assert!(actions.is_empty());
}
