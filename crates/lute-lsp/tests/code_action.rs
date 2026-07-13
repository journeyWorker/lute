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

/// Mirrors `lute-check/src/check.rs::choice_into_no_persist_diag`'s two-fixit
/// shape: a `W-CHOICE-INTO-NO-PERSIST` warning over `into="run.x"` (bytes
/// 10..22 in the fixture text below) with two author-chosen remedies —
/// insert `persist="run" ` before it, or delete it outright.
fn choice_into_diag() -> Diagnostic {
    Diagnostic {
        code: "W-CHOICE-INTO-NO-PERSIST".to_string(),
        severity: Severity::Warning,
        message: "`into=` without `persist=` records nothing".to_string(),
        span: span(10, 22),
        layer: Layer::Logic,
        fixits: vec![
            Fixit {
                title: "add persist=\"run\"".to_string(),
                kind: "refactor".to_string(),
                edit: vec![text_edit(10, 10, "persist=\"run\" ")],
                confidence: 100,
            },
            Fixit {
                title: "remove into=".to_string(),
                kind: "refactor".to_string(),
                edit: vec![text_edit(9, 22, "")],
                confidence: 100,
            },
        ],
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

/// A `W-CHOICE-INTO-NO-PERSIST` diagnostic with 2 fixits -> 2 actions, each
/// splicing the source to its own expected result (dsl 0.4 §7.3, D16).
#[test]
fn two_fixits_become_two_actions_with_expected_splices() {
    let text = "0123456789into=\"run.x\"9999";
    // bytes 10..22 == `into="run.x"`
    assert_eq!(&text[10..22], "into=\"run.x\"");
    let idx = TextIndex::new(text);
    let d = choice_into_diag();
    let uri = test_uri();
    let actions = code_actions_for_fixits(&[d], &uri, whole_doc_range(), &idx);
    assert_eq!(actions.len(), 2, "2 fixits must yield 2 actions");

    assert_eq!(actions[0].title, "add persist=\"run\"");
    assert_eq!(actions[0].kind, Some(lsp_types::CodeActionKind::QUICKFIX));
    let edit0 = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri];
    assert_eq!(edit0.len(), 1);
    assert_eq!(
        splice(text, &edit0[0], &idx),
        "0123456789persist=\"run\" into=\"run.x\"9999"
    );

    assert_eq!(actions[1].title, "remove into=");
    let edit1 = &actions[1].edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri];
    assert_eq!(splice(text, &edit1[0], &idx), "0123456789999");

    // Each action carries only its OWN diagnostic, not a shared list.
    assert_eq!(actions[0].diagnostics.as_ref().unwrap().len(), 1);
    assert_eq!(
        actions[0].diagnostics.as_ref().unwrap()[0].code,
        Some(lsp_types::NumberOrString::String(
            "W-CHOICE-INTO-NO-PERSIST".to_string()
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
    let text = "0123456789into=\"run.x\"9999";
    let idx = TextIndex::new(text);
    let d = choice_into_diag();
    let uri = test_uri();
    // A range confined to line 0 char 0..1, nowhere near byte 10..22
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
