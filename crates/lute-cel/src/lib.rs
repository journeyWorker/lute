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
}

/// Scan raw CEL source for DSL-level `@ref` / `$` tokens.
///
/// Runs on the ORIGINAL `raw` (never the substituted string) so names and spans
/// reflect the real source text.
pub fn scan_refs(raw: &str) -> Vec<RefUse> {
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let start = i;
            i += 1;
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            out.push(RefUse {
                name: raw[s..i].to_string(),
                is_dollar: false,
                span: byte_span(start, i),
            });
        } else if b[i] == b'$' {
            out.push(RefUse {
                name: "$".into(),
                is_dollar: true,
                span: byte_span(i, i + 1),
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
fn substitute_dsl_tokens(raw: &str) -> String {
    let mut s = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '@' => s.push(' '),
            '$' => s.push('_'),
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
    fn error_offset_is_within_slot_bounds() {
        // Tighter than the doc-relative test: a multi-token error offset must land
        // strictly inside the slot: >= base_byte AND < base_byte + raw.len().
        let mut arena = CelArena::default();
        let base = 100usize;
        let raw = "1 2"; // adjacent literals => parse error mid-fragment
        let err = parse_slot(&mut arena, raw, base).unwrap_err();
        assert!(err.span.byte_start >= base, "offset {} < base {base}", err.span.byte_start);
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
}
