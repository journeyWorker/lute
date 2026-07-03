//! CEL parse wrapper for the Lute LSP.
//!
//! Wraps `cel-parser` 0.10.1 to parse the CEL fragment inside a `CelSlot`,
//! records the resulting AST in a [`CelArena`] (handing out a [`CelAstHandle`]),
//! and maps any parse error back into a document-relative [`Span`].
//!
//! It also provides [`scan_refs`], a DSL-level scanner for `@ref`/`$` tokens that
//! runs on the ORIGINAL source text (before token substitution) so the reported
//! names and spans reflect what the author actually wrote.
//!
//! ## cel-parser 0.10.1 limitation (T3.2 / T4.3 carry-forward)
//! On a SUCCESSFUL parse, cel-parser drops all source positions: `SourceInfo`
//! (which carries 0-based byte offsets via `offset_for(id)`) is only attached to
//! `ParseError` on the failure path, never to a successfully parsed `IdedExpr`.
//! Therefore we CANNOT recover per-node byte offsets from a successful CEL AST
//! without forking the crate — and we do not. The stored AST is used for
//! STRUCTURE only (walking Select/Ident chains); sub-expression positions come
//! from our own [`scan_refs`] or fall back to the enclosing slot span at check
//! time (T4.3).

use lute_core_span::Span;
use lute_syntax::cel_ast::CelAstHandle;

pub mod fill;
pub use fill::fill_document;

/// Owns every parsed CEL AST and hands out opaque [`CelAstHandle`]s indexing into it.
#[derive(Default)]
pub struct CelArena {
    asts: Vec<cel_parser::ast::IdedExpr>,
}

impl CelArena {
    /// Resolve a handle back to its parsed AST node, if it belongs to this arena.
    pub fn get(&self, h: CelAstHandle) -> Option<&cel_parser::ast::IdedExpr> {
        self.asts.get(h.0 as usize)
    }
}

/// A CEL parse failure with a document-relative span (line/column filled at check time).
#[derive(Clone, Debug)]
pub struct CelParseError {
    pub message: String,
    pub span: Span,
}

/// A DSL-level `@ref` or `$` (subject) token found in raw CEL source.
#[derive(Clone, Debug)]
pub struct RefUse {
    /// For `@name`, the bare name (no `@`). For `$`, the literal `"$"`.
    pub name: String,
    /// True for the `$` subject token, false for an `@ref`.
    pub is_dollar: bool,
    /// Byte span of the whole token within the raw source (`@name` including the `@`).
    pub span: Span,
    /// `Some` when the `@name` is IMMEDIATELY followed by a `(...)` call group
    /// (dsl §8.1 `@name(args)`); `None` for a bare `@ref` or the `$` subject.
    pub call: Option<Call>,
}

/// A parenthesized call group following an `@name` (dsl §8.1 `@name(args)`).
#[derive(Clone, Debug)]
pub struct Call {
    /// Byte span of the whole `(...)` group (parens inclusive), within the raw source.
    pub span: Span,
    /// One byte span per top-level, comma-separated argument (whitespace-trimmed).
    /// Empty for `@name()`.
    pub args: Vec<Span>,
}

/// For each byte of `raw`, whether that byte lies inside a CEL **string literal**
/// (§4.4). The opening and closing quote bytes are themselves marked `true`, so a
/// scanner can treat any position with `mask[i] == true` as string content to be
/// skipped.
///
/// CEL string literals are single- (`'…'`) or double-quoted (`"…"`) with `\`
/// escaping; an escaped quote (`\'`) does not close the literal. Every byte of a
/// multibyte character inside a string is marked, so indexing the returned `Vec`
/// by any byte offset into `raw` is always valid. An unterminated literal marks
/// through end-of-input (a malformed fragment degrades safely: its bytes are
/// treated as string content rather than mis-tokenized as DSL `@`/`$`).
///
/// Shared with the LSP feature layer (`lute_lsp::features::path_tokens`/`path_at`,
/// S3) so DSL-token and state-path scanning agree on string boundaries.
pub fn cel_string_mask(raw: &str) -> Vec<bool> {
    let b = raw.as_bytes();
    let mut mask = vec![false; b.len()];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' || c == b'"' {
            let quote = c;
            mask[i] = true; // opening quote
            i += 1;
            while i < b.len() {
                mask[i] = true;
                if b[i] == b'\\' {
                    // Escape: the next byte is literal string content, not a close.
                    i += 1;
                    if i < b.len() {
                        mask[i] = true;
                        i += 1;
                    }
                    continue;
                }
                if b[i] == quote {
                    i += 1; // closing quote (already marked)
                    break;
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    mask
}

/// Scan raw CEL source for DSL-level `@ref` / `$` tokens.
///
/// Runs on the ORIGINAL `raw` (never the substituted string) so names and spans
/// reflect the real source text. A `@`/`$` inside a CEL string literal (§4.4) is
/// a literal character, not a DSL token, so it is skipped ([`cel_string_mask`]).
pub fn scan_refs(raw: &str) -> Vec<RefUse> {
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mask = cel_string_mask(raw);
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' && !mask[i] {
            let start = i;
            i += 1;
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            let name = raw[s..i].to_string();
            let span = byte_span(start, i);
            // dsl §8.1: an `@name` IMMEDIATELY followed by `(` (no whitespace) is a
            // parameterized call. Scan the group with STRICT bracket matching,
            // honoring string literals.
            let mut call = None;
            if i < b.len() && b[i] == b'(' && !mask[i] {
                let open = i;
                // Stack of expected closers. Seeded with the outer `)` matching the
                // initial `(`; every nested opener pushes its own matching closer. A
                // closer must match the stack top (else the input is malformed); the
                // group closes exactly when the stack empties. Brackets inside CEL
                // string literals are ignored (`mask`).
                let mut stack: Vec<u8> = vec![b')'];
                let mut j = open + 1;
                let mut close = None;
                while j < b.len() {
                    if !mask[j] {
                        match b[j] {
                            b'(' => stack.push(b')'),
                            b'[' => stack.push(b']'),
                            b'{' => stack.push(b'}'),
                            b')' | b']' | b'}' => {
                                if stack.last() == Some(&b[j]) {
                                    stack.pop();
                                    if stack.is_empty() {
                                        close = Some(j);
                                        break;
                                    }
                                } else {
                                    // Mismatched closer: malformed group -> degrade.
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    j += 1;
                }
                if let Some(close) = close {
                    let args = split_args(raw, &mask, open + 1, close);
                    call = Some(Call {
                        span: byte_span(open, close + 1),
                        args,
                    });
                }
                // Whether the group matched or degraded (mismatched closer / EOF
                // before the stack empties), `i` is left at the `(`: on a match we
                // record the call but do NOT advance past the group, so subsequent
                // iterations scan the interior and emit RefUse entries for any nested
                // `@ref`/`$`; on a degrade `call` stays None. Either way the `(` (and
                // any `,`/`)` bytes) fall through the else branch as ordinary text,
                // so the cursor always advances (no infinite loop).
            }
            out.push(RefUse {
                name,
                is_dollar: false,
                span,
                call,
            });
        } else if b[i] == b'$' && !mask[i] {
            out.push(RefUse {
                name: "$".into(),
                is_dollar: true,
                span: byte_span(i, i + 1),
                call: None,
            });
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Build a byte-only [`Span`]; line/column/utf16 are recomputed by the caller's
/// `TextIndex` at check time.
fn byte_span(s: usize, e: usize) -> Span {
    Span {
        byte_start: s,
        byte_end: e,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Split the interior of a call group (`start..end` byte range of `raw`, exclusive
/// of the parens) into per-argument trimmed spans, honoring string literals and
/// nested parens. An all-whitespace interior yields no args.
fn split_args(raw: &str, mask: &[bool], start: usize, end: usize) -> Vec<Span> {
    let b = raw.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut seg_start = start;
    let mut i = start;
    let push_seg = |args: &mut Vec<Span>, s: usize, e: usize| {
        // trim ASCII whitespace at both ends; skip an empty segment
        let mut a = s;
        let mut z = e;
        while a < z && raw.as_bytes()[a].is_ascii_whitespace() {
            a += 1;
        }
        while z > a && raw.as_bytes()[z - 1].is_ascii_whitespace() {
            z -= 1;
        }
        if a < z {
            args.push(byte_span(a, z));
        }
    };
    while i < end {
        let c = b[i];
        if !mask[i] {
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    push_seg(&mut args, seg_start, i);
                    seg_start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    push_seg(&mut args, seg_start, end);
    args
}

/// Parse the CEL fragment `raw` (found at `base_byte` within the document) and
/// record it in `arena`, returning a handle. On failure, the returned span is
/// document-relative: `byte_start` is the error position mapped into the doc and
/// `byte_end` is the end of the slot.
pub fn parse_slot(
    arena: &mut CelArena,
    raw: &str,
    base_byte: usize,
) -> Result<CelAstHandle, CelParseError> {
    // Length-preserving substitution (`@`->' ', `$`->'_') so byte offsets in the
    // prepared string line up 1:1 with `raw`.
    let prepared = substitute_dsl_tokens(raw);
    // cel-parser 0.10.1 (source-verified): `Parser::parse(mut self, &str) ->
    // Result<IdedExpr, ParseErrors>` consumes `self`, so build a fresh parser
    // per call. No free `cel_parser::parse` exists.
    //
    // ROBUSTNESS: cel-parser's antlr4rust backend hits an `unreachable!()` panic
    // (not an `Err`) on many transient-malformed inputs an LSP sees every
    // keystroke (e.g. "", "1 +", "1 == ", "(", "a &&", "'unterminated"). We
    // MUST NOT let that crash the server, so we run the parse under
    // `catch_unwind` and treat a panic as an unrecoverable parse of the whole
    // slot. `silence_antlr_panic()` keeps the log clean for exactly that panic.
    silence_antlr_panic();
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cel_parser::Parser::new().parse(&prepared)
    }));
    match parsed {
        Ok(Ok(expr)) => {
            let h = CelAstHandle(arena.asts.len() as u32);
            arena.asts.push(expr);
            Ok(h)
        }
        Ok(Err(errs)) => {
            // `ParseErrors.errors` is pre-sorted by position; take the primary one.
            let (msg, byte) = match errs.errors.first() {
                Some(e) => (e.msg.clone(), linecol_to_byte(raw, e.pos.0, e.pos.1)),
                None => ("CEL parse error".to_string(), 0),
            };
            let start = base_byte + byte;
            Err(CelParseError {
                message: msg,
                span: Span {
                    byte_start: start,
                    byte_end: base_byte + raw.len(),
                    line: 0,
                    column: 0,
                    utf16_range: (0, 0),
                },
            })
        }
        // Backend panic: no usable position — flag the whole slot.
        Err(_) => Err(CelParseError {
            message: "invalid CEL expression".to_string(),
            span: Span {
                byte_start: base_byte,
                byte_end: base_byte + raw.len(),
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            },
        }),
    }
}

/// Install (once) a panic hook that swallows cel-parser's known-benign
/// antlr4rust `unreachable!()` panic on malformed CEL, while forwarding every
/// other panic to the previous hook. Without this, `catch_unwind` still recovers
/// but the default hook floods stderr on each malformed keystroke.
fn silence_antlr_panic() {
    use std::sync::Once;
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let benign = info
                .location()
                .is_some_and(|l| l.file().contains("antlr4rust"));
            if !benign {
                prev(info);
            }
        }));
    });
}

/// cel-parser reports errors as 1-based `(line, column)` where `column` is
/// code-point based (antlr). Convert to a BYTE offset within `raw` (which equals
/// `prepared` byte-for-byte since substitution is length-preserving). Robust to
/// multi-line CEL and multibyte characters.
fn linecol_to_byte(raw: &str, line: isize, col: isize) -> usize {
    let line = line.max(1) as usize; // 1-based
    let col = col.max(1) as usize; // 1-based, code points
    let mut cur_line = 1usize;
    let mut byte = 0usize;
    let bytes = raw.as_bytes();
    // advance to the start of `line`
    while cur_line < line && byte < bytes.len() {
        if bytes[byte] == b'\n' {
            cur_line += 1;
        }
        byte += 1;
    }
    // advance col-1 code points within the line
    let mut cp = 1usize;
    let mut idx = byte;
    while cp < col && idx < bytes.len() && bytes[idx] != b'\n' {
        // step one UTF-8 code point
        idx += 1;
        while idx < bytes.len() && (bytes[idx] & 0xC0) == 0x80 {
            idx += 1;
        }
        cp += 1;
    }
    idx.min(raw.len())
}

/// Replace DSL-level tokens with length-preserving placeholders so cel-parser
/// accepts the fragment and byte offsets stay aligned with `raw`:
/// `@` -> `' '` (the following name survives as a bare identifier), `$` -> `'_'`.
/// Iterates over chars so multibyte source stays byte-length-preserving.
///
/// A `@`/`$` INSIDE a CEL string literal (§4.4) is literal content — an enum-arm
/// value like `'@gold'` or `'$5'` — and is left untouched, so the parsed
/// `Val::String` keeps its real bytes and `match_check` compares them correctly
/// ([`cel_string_mask`]).
fn substitute_dsl_tokens(raw: &str) -> String {
    let mask = cel_string_mask(raw);
    let mut s = String::with_capacity(raw.len());
    for (i, c) in raw.char_indices() {
        match c {
            '@' if !mask[i] => s.push(' '),
            '$' if !mask[i] => s.push('_'),
            other => s.push(other),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_cel_and_records_ast() {
        let mut arena = CelArena::default();
        let h = parse_slot(&mut arena, "scene.affect.bianca >= 1", 0).unwrap();
        assert!(arena.get(h).is_some());
    }

    #[test]
    fn invalid_cel_error_span_is_document_relative() {
        let mut arena = CelArena::default();
        // base_byte = 100 => the error offset must be >= 100
        let err = parse_slot(&mut arena, "1 +", 100).unwrap_err();
        assert!(err.span.byte_start >= 100);
    }

    #[test]
    fn dollar_and_ref_parse_as_identifiers() {
        // `$` and `@fond` are DSL-level; assert our detector finds them before
        // handing the (substituted) fragment to cel-parser.
        let refs = scan_refs("@fond && $ == 'gold'");
        assert!(refs.iter().any(|r| r.name == "fond"));
        assert!(refs.iter().any(|r| r.is_dollar));
    }

    #[test]
    fn ref_inside_single_quoted_cel_string_is_not_a_ref() {
        // §4.4/§8: a `@` inside a CEL string literal is a literal character, not a
        // DSL @ref. Only the real `@ref` OUTSIDE the string is a RefUse.
        let refs = scan_refs("@real == 'literal @x here'");
        assert!(refs.iter().any(|r| r.name == "real"), "real @ref lost");
        assert!(
            !refs.iter().any(|r| r.name == "x"),
            "@x inside a CEL string must NOT be a RefUse: {refs:?}"
        );
    }

    #[test]
    fn dollar_inside_double_quoted_cel_string_is_not_a_dollar() {
        // A `$` inside a `"..."` CEL string literal is literal, not the subject
        // token; a real `$` outside a string still is.
        let refs = scan_refs("$ == \"a $ b\"");
        assert_eq!(
            refs.iter().filter(|r| r.is_dollar).count(),
            1,
            "only the subject `$` outside the string is a dollar token: {refs:?}"
        );
    }

    #[test]
    fn escaped_quote_inside_string_does_not_end_it() {
        // A `\'` escape keeps the string open, so a following `@x` is still inside
        // the literal and not a ref.
        let refs = scan_refs(r"'it\'s @x' == foo");
        assert!(
            !refs.iter().any(|r| r.name == "x"),
            "@x after an escaped quote is still inside the string: {refs:?}"
        );
    }

    #[test]
    fn substitute_leaves_dsl_tokens_inside_strings_intact() {
        // substitute_dsl_tokens must NOT rewrite `@`/`$` inside a CEL string
        // literal (they are literal content, e.g. an enum-arm value like '@gold').
        // Outside a string, `@` -> ' ' and `$` -> '_' (length-preserving).
        let out = substitute_dsl_tokens("@r && '@lit $lit' == $");
        assert!(
            out.contains("'@lit $lit'"),
            "string content mutated: {out:?}"
        );
        assert_eq!(out.len(), "@r && '@lit $lit' == $".len());
        assert!(!out.starts_with('@'), "outer @r not substituted: {out:?}");
        assert!(out.ends_with('_'), "outer $ not substituted: {out:?}");
    }

    #[test]
    fn error_offset_is_within_slot_bounds() {
        // Tighter than the doc-relative test: a multi-token error offset must land
        // strictly inside the slot: >= base_byte AND < base_byte + raw.len().
        let mut arena = CelArena::default();
        let base = 100usize;
        let raw = "1 2"; // adjacent literals => parse error mid-fragment
        let err = parse_slot(&mut arena, raw, base).unwrap_err();
        assert!(
            err.span.byte_start >= base,
            "offset {} < base {base}",
            err.span.byte_start
        );
        assert!(
            err.span.byte_start < base + raw.len(),
            "offset {} >= slot end {}",
            err.span.byte_start,
            base + raw.len()
        );
    }

    #[test]
    fn malformed_cel_never_panics_and_returns_full_slot_span() {
        // cel-parser's antlr4rust backend panics (not Err) on these transient
        // inputs an LSP sees every keystroke; parse_slot must convert each to a
        // clean, document-relative error instead of crashing.
        let mut arena = CelArena::default();
        let base = 100usize;
        for raw in ["", "1 +", "1 == ", "(", ")", "a &&", "'unterminated"] {
            let err = parse_slot(&mut arena, raw, base).unwrap_err();
            assert_eq!(err.span.byte_start, base, "raw={raw:?}");
            assert_eq!(err.span.byte_end, base + raw.len(), "raw={raw:?}");
        }
    }

    #[test]
    fn scan_refs_captures_call_form() {
        let refs = scan_refs("@atLeast(2)");
        let r = refs.iter().find(|r| r.name == "atLeast").expect("ref");
        let call = r.call.as_ref().expect("call captured");
        assert_eq!(call.args.len(), 1);
    }

    #[test]
    fn scan_refs_bare_ref_has_no_call() {
        let refs = scan_refs("@fond");
        assert!(refs
            .iter()
            .find(|r| r.name == "fond")
            .unwrap()
            .call
            .is_none());
    }

    #[test]
    fn scan_refs_empty_args() {
        let refs = scan_refs("@now()");
        assert_eq!(
            refs.iter()
                .find(|r| r.name == "now")
                .unwrap()
                .call
                .as_ref()
                .unwrap()
                .args
                .len(),
            0
        );
    }

    #[test]
    fn scan_refs_commas_in_nested_paren_and_string_not_split() {
        // top-level args: `max(a, b)` and `'x,y'` -> exactly 2 args
        let refs = scan_refs("@pick(max(a, b), 'x,y')");
        let call = refs
            .iter()
            .find(|r| r.name == "pick")
            .unwrap()
            .call
            .as_ref()
            .unwrap();
        assert_eq!(
            call.args.len(),
            2,
            "commas inside nested parens/strings must not split"
        );
    }

    #[test]
    fn scan_refs_space_before_paren_is_not_a_call() {
        // `@x (y)` is a bare ref `@x` then a separate parenthesized group.
        let refs = scan_refs("@x (y)");
        assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
    }

    #[test]
    fn scan_refs_unterminated_paren_degrades_to_no_call() {
        // never panic; an unterminated `(` yields no call (bare ref).
        let refs = scan_refs("@x(a, b");
        assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
    }

    #[test]
    fn scan_refs_nested_ref_inside_call_is_still_scanned() {
        // Bug 1: a nested `@ref` inside call args must ALSO get its own RefUse,
        // while the outer `@name(...)` still records its call group.
        let refs = scan_refs("@outer(@missing)");
        let outer = refs.iter().find(|r| r.name == "outer").expect("outer");
        let call = outer.call.as_ref().expect("outer call captured");
        assert_eq!(call.args.len(), 1);
        let missing = refs
            .iter()
            .find(|r| r.name == "missing")
            .expect("nested @missing scanned");
        assert!(missing.call.is_none(), "nested @missing has no call");
    }

    #[test]
    fn scan_refs_dollar_inside_call_is_still_scanned() {
        // Bug 1: a `$` inside call args must ALSO get its own RefUse.
        let refs = scan_refs("@outer($)");
        assert!(refs.iter().any(|r| r.is_dollar), "nested $ not scanned");
        let outer = refs.iter().find(|r| r.name == "outer").expect("outer");
        assert!(outer.call.is_some(), "outer call captured");
    }

    #[test]
    fn scan_refs_nested_call_inside_call() {
        // Bug 1: nested calls are all captured.
        let refs = scan_refs("@outer(@inner(x))");
        let outer = refs.iter().find(|r| r.name == "outer").expect("outer");
        assert!(outer.call.is_some(), "outer call captured");
        let inner = refs.iter().find(|r| r.name == "inner").expect("inner");
        assert!(inner.call.is_some(), "inner call captured");
    }

    #[test]
    fn scan_refs_mismatched_closer_degrades_to_no_call() {
        // Bug 2: `]` cannot close a `(` group -> malformed -> no call.
        let refs = scan_refs("@x(]");
        assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
    }

    #[test]
    fn scan_refs_mismatched_closer_inside_degrades() {
        // Bug 2: a `]` with no matching `[` inside the group is malformed.
        let refs = scan_refs("@x(a]b)");
        assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
    }
}
