//! §8.1 (T1–T3): the writer-voiced `E-CEL-PARSE` message contract.
//!
//! A CEL parse failure reaches [`fill_document`](lute_cel::fill_document) as a
//! [`CelParseError`] whose `message` carries the embedded backend parser's own
//! vocabulary verbatim — raw ANTLR text on the recoverable-error path
//! (lute-cel/src/lib.rs:298-315: "no viable alternative…", "token recognition
//! error…") or the blanket `"invalid CEL expression"` on the unrecoverable
//! backend-panic path (lib.rs:316-327). T1 forbids EITHER from ever reaching a
//! writer-facing surface (CLI human, CLI JSON `message`, LSP).
//!
//! [`translate_cel_parse`] replaces that text with a **pre-parse lexical scan**
//! of the raw slot text — string-mask aware via [`lute_cel::cel_string_mask`],
//! independent of the backend's own error taxonomy (that independence is what
//! makes the six T2 detections reliably implementable without depending on
//! backend error strings). `backend`'s span is read ONLY as a last-resort
//! position for the T3 fallback — never its `message`. `lute-cel` itself stays
//! byte-untouched by this task: it remains parse-only, with no message policy
//! of its own (D1's "closed profile / one evaluator" discipline extended to
//! "one message policy, and it lives in the checker").
//!
//! Detection order (each rule scans over `cel_string_mask(raw)`-masked bytes so
//! a `&`/`|`/`=`/`and`/`or`/`not` INSIDE a CEL string literal is inert; first
//! rule to match wins):
//! 1. the raw slot is whitespace-only.
//! 2. an unbalanced (never-closed) quote.
//! 3. `=<` / `=>` (a reversed comparison operator).
//! 4. a bare `=` (assignment syntax where CEL wants `==`).
//! 5. a bare `&` / `|` (C-style single-character logic).
//! 6. a whole-identifier `and` / `or` / `not` outside a member-access segment.
//!
//! Anything else falls to the T3 fallback: a neutral "not a valid condition
//! expression" naming the slot text, at the backend's recovered span when it
//! looks like a real position inside this slot, else the whole slot.

use lute_cel::{cel_string_mask, CelParseError};
use lute_core_span::{Fixit, Span, TextEdit};

/// The §8.1 translation of one failed CEL slot: the writer-voiced message, any
/// machine-applicable fixits (T2 — `kind: "refactor"`, D16), and the span the
/// diagnostic should carry. `span: None` means "no rule found a narrower
/// location than the whole slot" — the wiring site (`check.rs`) falls back to
/// the slot's own span. Every byte offset here (in `span` and in each fixit's
/// `TextEdit.span`) is already document-relative (rebased on `slot_span`), but
/// `line`/`column`/`utf16_range` are left zeroed — the house zero-then-
/// normalize convention (`check.rs`'s `normalize_spans` fills them in from one
/// shared `TextIndex`, same as every other ad hoc span producer in this crate).
pub(crate) struct Translation {
    pub message: String,
    pub fixits: Vec<Fixit>,
    pub span: Option<Span>,
}

/// §8.1 T1–T3. `raw` = the FAILED slot's full authored text (never a substring
/// of it — the detections below need the whole fragment: rule 1 checks the
/// entire slot, rule 2 needs to know whether the string that opened the tail
/// ever closed, rule 4's suggested rewrite splices into the whole slot).
/// `slot_span` = that same slot's full document-relative span (every produced
/// span/fixit-edit is `slot_span.byte_start`-rebased from a LOCAL offset into
/// `raw`). `backend` = lute-cel's own [`CelParseError`] for this slot, read
/// ONLY for its `span` on the T3 fallback — never its `message`.
pub(crate) fn translate_cel_parse(raw: &str, slot_span: Span, backend: &CelParseError) -> Translation {
    // Rule 1: whitespace-only. `fill_document` already filters a literally
    // whitespace-only `raw` before ever calling `parse_slot` (a structural gap,
    // not a CEL fragment — D10, dsl §8.1), so this never fires through that
    // path; it exists so `translate_cel_parse` is correct for ANY caller that
    // hands it a failed slot without that upstream filter (e.g. a `defs:` `cel:`
    // body). D10: this only REWORDS the existing backend-panic path ("invalid
    // CEL expression") — no new diagnostic appears where one was silent before.
    if raw.trim().is_empty() {
        return Translation {
            message: "the condition is empty (dsl 0.4 §8.1)".to_string(),
            fixits: Vec::new(),
            span: None,
        };
    }

    let mask = cel_string_mask(raw);

    // Rule 2: an unbalanced quote — the tail of `raw` opens a string literal
    // that never closes.
    if let Some(open) = unbalanced_quote(raw) {
        return Translation {
            message: "unclosed quote in the condition (dsl 0.4 §8.1)".to_string(),
            fixits: Vec::new(),
            span: Some(rebase(slot_span, open, raw.len())),
        };
    }

    // Rule 3: a reversed comparison, `=<` / `=>`.
    if let Some((pos, right)) = scan_reversed_compare(raw, &mask) {
        let wrong = if right == "<=" { "=<" } else { "=>" };
        return Translation {
            message: format!("`{wrong}` is not an operator — did you mean `{right}`? (dsl 0.4 §8.1)"),
            fixits: vec![splice_fixit(
                format!("swap to `{right}`"),
                slot_span,
                pos,
                pos + 2,
                right,
            )],
            span: Some(rebase(slot_span, pos, pos + 2)),
        };
    }

    // Rule 4: a bare `=` (not part of `==`/`!=`/`<=`/`>=`/`=<`/`=>`).
    if let Some(pos) = scan_bare_eq(raw, &mask) {
        let mut corrected = raw.to_string();
        corrected.replace_range(pos..pos + 1, "==");
        return Translation {
            message: format!(
                "`=` assigns; comparison is `==` — did you mean `{corrected}`? (dsl 0.4 §8.1)"
            ),
            fixits: vec![splice_fixit(
                "change `=` to `==`".to_string(),
                slot_span,
                pos,
                pos + 1,
                "==",
            )],
            span: Some(rebase(slot_span, pos, pos + 1)),
        };
    }

    // Rule 5: a bare `&` / `|` (not part of `&&`/`||`).
    if let Some((pos, ch)) = scan_bare_logical(raw, &mask) {
        let doubled = if ch == '&' { "&&" } else { "||" };
        return Translation {
            message: format!(
                "`{ch}` is not an operator here — use `{doubled}` (or `is=\"a|b\"` for a literal \
                 alternation) (dsl 0.4 §8.1)"
            ),
            fixits: vec![splice_fixit(
                format!("change `{ch}` to `{doubled}`"),
                slot_span,
                pos,
                pos + 1,
                doubled,
            )],
            span: Some(rebase(slot_span, pos, pos + 1)),
        };
    }

    // Rule 6: a whole-identifier `and` / `or` / `not`, outside any
    // member-access segment (`scene.and.x`'s `and` is a field name, not the
    // keyword — only a token whose immediately preceding byte is NOT `.`
    // qualifies).
    if let Some((start, end, word)) = scan_word_operator(raw, &mask) {
        let replacement = match word {
            "and" => "&&",
            "or" => "||",
            "not" => "!",
            _ => unreachable!("scan_word_operator only ever returns and/or/not"),
        };
        return Translation {
            message: format!(
                "words are not operators in a condition — use `&&` / `||` / `!` (`{word}` here \
                 means `{replacement}`, dsl 0.4 §8.1)"
            ),
            fixits: vec![splice_fixit(
                format!("change `{word}` to `{replacement}`"),
                slot_span,
                start,
                end,
                replacement,
            )],
            span: Some(rebase(slot_span, start, end)),
        };
    }

    // T3 fallback: neutral, names the slot text, never the backend's message.
    // Use the backend's recovered position only when it plausibly lands inside
    // THIS slot (lute-cel always populates it validly today, but the check is
    // defensive per §8.1 T3's "when one was recovered, else the whole slot").
    let span = if backend.span.byte_start >= slot_span.byte_start
        && backend.span.byte_start <= slot_span.byte_start + raw.len()
    {
        Some(backend.span)
    } else {
        None
    };
    Translation {
        message: format!("not a valid condition expression: `{raw}` (dsl 0.4 §8.1)"),
        fixits: Vec::new(),
        span,
    }
}

/// Rebase a LOCAL `raw`-relative byte range onto `slot_span`'s document
/// coordinates, zeroed `line`/`column`/`utf16_range` (recomputed later by
/// `normalize_spans` through a shared `TextIndex` — the house convention every
/// other ad hoc span producer in this crate follows, e.g. `check.rs`'s
/// `zeroed_span`, `cel_resolve.rs:857-861`).
fn rebase(slot_span: Span, local_start: usize, local_end: usize) -> Span {
    Span {
        byte_start: slot_span.byte_start + local_start,
        byte_end: slot_span.byte_start + local_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// A single-edit "splice this local range to `new_text`" fixit (D16: always
/// `kind: "refactor"`, never `"migrate"` — `lute fix` cannot apply these by
/// construction, fix.rs never reads checker diagnostics). Every T2 rewrite
/// here is an unambiguous, deterministic token substitution, so `confidence`
/// is the same 100 the house `W-CHOICE-INTO-NO-PERSIST` fixits use.
fn splice_fixit(
    title: String,
    slot_span: Span,
    local_start: usize,
    local_end: usize,
    new_text: &str,
) -> Fixit {
    Fixit {
        title,
        kind: "refactor".to_string(),
        edit: vec![TextEdit {
            span: rebase(slot_span, local_start, local_end),
            new_text: new_text.to_string(),
        }],
        confidence: 100,
    }
}

/// Does the TAIL of `raw` open a quote (`'`/`"`) that never closes? Mirrors
/// [`cel_string_mask`]'s own scanning loop exactly (§4.4: escapes, quote
/// matching) so "did this string ever close" agrees with what the mask itself
/// considers string content. Returns the byte offset the unterminated quote
/// opened at.
fn unbalanced_quote(raw: &str) -> Option<usize> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' || c == b'"' {
            let quote = c;
            let start = i;
            i += 1;
            let mut closed = false;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 1;
                    if i < b.len() {
                        i += 1;
                    }
                    continue;
                }
                if b[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                return Some(start);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// The first (leftmost) `=<` or `=>` outside a string literal. Returns the
/// byte offset of the `=` and the CORRECT two-character operator.
fn scan_reversed_compare(raw: &str, mask: &[bool]) -> Option<(usize, &'static str)> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if !mask[i] && b[i] == b'=' && !mask[i + 1] {
            if b[i + 1] == b'<' {
                return Some((i, "<="));
            }
            if b[i + 1] == b'>' {
                return Some((i, ">="));
            }
        }
        i += 1;
    }
    None
}

/// The first bare `=` outside a string literal — one that is not half of a
/// recognized two-character operator (`==`, `!=`, `<=`, `>=`, `=<`, `=>`).
fn scan_bare_eq(raw: &str, mask: &[bool]) -> Option<usize> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if mask[i] {
            i += 1;
            continue;
        }
        let next_is_eq = i + 1 < b.len() && !mask[i + 1] && b[i + 1] == b'=';
        match b[i] {
            b'=' | b'!' | b'<' | b'>' if next_is_eq => i += 2, // == != <= >=
            b'=' if i + 1 < b.len() && !mask[i + 1] && matches!(b[i + 1], b'<' | b'>') => i += 2, // =< =>
            b'=' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// The first bare `&` or `|` outside a string literal — one that is not half
/// of `&&`/`||`.
fn scan_bare_logical(raw: &str, mask: &[bool]) -> Option<(usize, char)> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if mask[i] {
            i += 1;
            continue;
        }
        let doubled = i + 1 < b.len() && !mask[i + 1] && b[i + 1] == b[i];
        match b[i] {
            b'&' if doubled => i += 2,
            b'|' if doubled => i += 2,
            b'&' => return Some((i, '&')),
            b'|' => return Some((i, '|')),
            _ => i += 1,
        }
    }
    None
}

/// The first whole-identifier `and`/`or`/`not` outside a string literal AND
/// outside a member-access segment (immediately preceded by `.`). Identifiers
/// are scanned as maximal ASCII `[A-Za-z_][A-Za-z0-9_]*` runs, so a longer
/// word like `andy` is never mistaken for `and` (no separate word-boundary
/// check needed — the run is already the whole word).
fn scan_word_operator<'a>(raw: &'a str, mask: &[bool]) -> Option<(usize, usize, &'a str)> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if mask[i] || !is_ident_start(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && !mask[i] && is_ident_continue(b[i]) {
            i += 1;
        }
        let word = &raw[start..i];
        if matches!(word, "and" | "or" | "not") {
            let is_member_segment = start > 0 && raw.as_bytes()[start - 1] == b'.';
            if !is_member_segment {
                return Some((start, i, word));
            }
        }
    }
    None
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(byte_start: usize, byte_end: usize) -> CelParseError {
        CelParseError {
            message: "no viable alternative at input '…'".to_string(), // never surfaced
            span: Span {
                byte_start,
                byte_end,
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            },
        }
    }

    fn slot_span(len: usize) -> Span {
        Span {
            byte_start: 100,
            byte_end: 100 + len,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        }
    }

    #[test]
    fn rule1_whitespace_only_never_leaks_backend_text() {
        let t = translate_cel_parse("   ", slot_span(3), &backend(100, 103));
        assert_eq!(t.message, "the condition is empty (dsl 0.4 §8.1)");
        assert!(t.fixits.is_empty());
        assert!(t.span.is_none());
    }

    #[test]
    fn rule2_unbalanced_quote_spans_the_open_quote_to_end() {
        let raw = "'unterminated";
        let t = translate_cel_parse(raw, slot_span(raw.len()), &backend(100, 100 + raw.len()));
        assert!(t.message.contains("unclosed quote"), "{}", t.message);
        assert!(t.fixits.is_empty());
        assert_eq!(t.span, Some(Span { byte_start: 100, byte_end: 100 + raw.len(), line: 0, column: 0, utf16_range: (0, 0) }));
    }

    #[test]
    fn rule2_ignores_a_string_that_does_close() {
        // The mask-scan mirror agrees with `cel_string_mask`: a closed string
        // anywhere in `raw` is not "unbalanced", even with junk after it.
        assert_eq!(unbalanced_quote("'a' + "), None);
    }

    #[test]
    fn rule3_reversed_compare_fixit_splices_correctly() {
        let raw = "x =< 1";
        let t = translate_cel_parse(raw, slot_span(raw.len()), &backend(100, 100 + raw.len()));
        assert!(t.message.contains("`<=`"), "{}", t.message);
        let f = &t.fixits[0];
        let mut spliced = raw.to_string();
        let e = &f.edit[0];
        let local = (e.span.byte_start - 100)..(e.span.byte_end - 100);
        spliced.replace_range(local, &e.new_text);
        assert_eq!(spliced, "x <= 1");
    }

    #[test]
    fn rule4_bare_eq_suggests_the_whole_corrected_slot() {
        let raw = "run.act = 1";
        let t = translate_cel_parse(raw, slot_span(raw.len()), &backend(100, 100 + raw.len()));
        assert!(
            t.message.contains("did you mean `run.act == 1`"),
            "{}",
            t.message
        );
        assert_eq!(t.fixits.len(), 1);
    }

    #[test]
    fn rule4_skips_real_comparison_operators() {
        for ok in ["a == b", "a != b", "a <= b", "a >= b", "a < b", "a > b"] {
            let mask = cel_string_mask(ok);
            assert_eq!(scan_bare_eq(ok, &mask), None, "{ok:?} must not flag a bare =");
        }
    }

    #[test]
    fn rule5_bare_logical_ops() {
        let mask = cel_string_mask("a & b");
        assert_eq!(scan_bare_logical("a & b", &mask), Some((2, '&')));
        let mask = cel_string_mask("a && b");
        assert_eq!(scan_bare_logical("a && b", &mask), None);
    }

    #[test]
    fn rule6_word_operator_skips_member_access_segments() {
        let raw = "scene.and.x";
        let mask = cel_string_mask(raw);
        assert_eq!(scan_word_operator(raw, &mask), None, "field name `and` must not flag");
    }

    #[test]
    fn rule6_word_operator_finds_whole_word_only() {
        let raw = "andy && b";
        let mask = cel_string_mask(raw);
        assert_eq!(scan_word_operator(raw, &mask), None, "`andy` is not the keyword `and`");
    }

    #[test]
    fn masked_bytes_never_trigger_any_rule() {
        // A well-formed condition whose STRING CONTENT contains every T2
        // trigger byte must translate to nothing rule-worthy — string-mask
        // awareness (used only defensively here; the real gate is that a
        // successful parse never reaches `translate_cel_parse` at all).
        let raw = "run.s == 'a = b & c and d'";
        let mask = cel_string_mask(raw);
        assert_eq!(scan_bare_eq(raw, &mask), None);
        assert_eq!(scan_bare_logical(raw, &mask), None);
        assert_eq!(scan_word_operator(raw, &mask), None);
        assert_eq!(scan_reversed_compare(raw, &mask), None);
        assert_eq!(unbalanced_quote(raw), None);
    }

    #[test]
    fn fallback_never_repeats_backend_message() {
        let raw = "(";
        let t = translate_cel_parse(raw, slot_span(raw.len()), &backend(100, 101));
        assert_eq!(t.message, "not a valid condition expression: `(` (dsl 0.4 §8.1)");
        assert!(t.fixits.is_empty());
    }

    #[test]
    fn fallback_ignores_an_out_of_range_backend_span() {
        let raw = "(";
        // A backend span far outside this slot must not leak through as-is.
        let t = translate_cel_parse(raw, slot_span(raw.len()), &backend(9_999, 10_000));
        assert!(t.span.is_none());
    }
}
