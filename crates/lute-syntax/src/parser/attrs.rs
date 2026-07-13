//! Attribute scanner + brace matcher (dsl §4.4 quoting, §4.5 attributes).
//!
//! `{ … }` / `< … >` attribute lists are tokenized on ASCII whitespace, but a
//! `"`-quoted value is opaque: structural chars (`}`, `>`, space, `=`) inside a
//! quoted string are literal (`\` escapes the next char, §4.4). Three value
//! shapes: `key="str"` → [`AttrValue::Str`]; `key=@ref` → [`AttrValue::Ref`]
//! (a [`CelSlot`] of [`CelKind::AttrValue`]); bare `key` → [`AttrValue::BoolTrue`].

use super::{is_ident_byte, Parser, E_STRING_ESCAPE};
use crate::ast::{Attr, AttrValue, CelKind, CelSlot};
use lute_core_span::{Layer, Span};

impl Parser<'_> {
    /// Scan an attribute list from body offset `start` up to the unquoted
    /// terminator byte `term` (`}` for directives, `>` for tags).
    ///
    /// Each [`Attr`] carries `value_span` — the span of its *value* (inner string
    /// for `Str`, the `@ref` for `Ref`, the key for `BoolTrue`) — so `take_cel`
    /// builds `CelSlot`s whose span bounds exactly `raw`. `after` is the body
    /// offset just past the terminator (or line end).
    ///
    /// Takes `&mut self` to emit [`E_STRING_ESCAPE`] (§4.4) for undefined
    /// backslash escapes in quoted values; scanning always continues so one bad
    /// escape degrades safely rather than derailing the attribute list.
    pub(super) fn scan_attrs(&mut self, start: usize, term: u8) -> (Vec<Attr>, usize) {
        let b = self.body.as_bytes();
        let n = b.len();
        let mut attrs = Vec::new();
        // Body-offset `(start, end)` spans of undefined escapes, emitted after
        // the `b` borrow of `self.body` ends (emit needs `&mut self`).
        let mut escapes: Vec<(usize, usize)> = Vec::new();
        let mut j = start;
        loop {
            while j < n && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            if j >= n || b[j] == term || b[j] == b'\n' {
                break;
            }
            let key_start = j;
            while j < n && is_ident_byte(b[j]) {
                j += 1;
            }
            if j == key_start {
                // Not an ident: unparseable token — skip to avoid a spin loop.
                j += 1;
                continue;
            }
            let key = self.body[key_start..j].to_string();
            if j < n && b[j] == b'=' {
                j += 1; // past '='
                if j < n && b[j] == b'"' {
                    j += 1; // past opening quote
                    let inner_start = j;
                    let mut esc = false;
                    while j < n {
                        let c = b[j];
                        if esc {
                            esc = false;
                        } else if c == b'\\' {
                            esc = true;
                            // §4.4: only `\" \\ \n \t` are defined String
                            // escapes. `\'` is exempted — a CelString value
                            // (indistinguishable here) may embed a CEL
                            // single-quoted string using `\'`. Any other escape
                            // is undefined; span the two bytes `\x`. A trailing
                            // `\` with no next byte (`j + 1 == n`) is an
                            // incomplete escape, not flagged.
                            if j + 1 < n {
                                let e = b[j + 1];
                                if !matches!(e, b'"' | b'\\' | b'n' | b't' | b'\'') {
                                    escapes.push((j, j + 2));
                                }
                            }
                        } else if c == b'"' {
                            break;
                        } else if c == b'\n' {
                            // RC2 (dsl §2.3): a physical newline INSIDE a quoted
                            // value is never part of the value — the one-line
                            // model forbids a tag/attribute list from wrapping.
                            // Stop here (unterminated on this line) rather than
                            // scanning into the next physical line looking for
                            // the closing quote.
                            break;
                        }
                        j += 1;
                    }
                    let inner_end = j;
                    if j < n && b[j] == b'"' {
                        j += 1; // past closing quote (only when actually found)
                    }
                    let value = self.body[inner_start..inner_end].to_string();
                    let vspan = self.span(inner_start, inner_end);
                    attrs.push(Attr {
                        key,
                        value: AttrValue::Str(value),
                        value_span: vspan,
                        span: self.span(key_start, j),
                    });
                } else if j < n && b[j] == b'@' {
                    let ref_start = j;
                    j += 1;
                    while j < n && is_ident_byte(b[j]) {
                        j += 1;
                    }
                    if j < n && b[j] == b'(' {
                        // Consume balanced `(...)` args, honoring quotes.
                        j += 1;
                        let mut depth = 1;
                        let mut q: Option<u8> = None;
                        let mut e2 = false;
                        while j < n && depth > 0 {
                            let c = b[j];
                            if c == b'\n' {
                                // RC2 (dsl §2.3): stop at the physical line
                                // end — an `@ref(...)` arg list (quoted or
                                // not) never wraps to a later line.
                                break;
                            }
                            match q {
                                Some(qc) => {
                                    if e2 {
                                        e2 = false;
                                    } else if c == b'\\' {
                                        e2 = true;
                                    } else if c == qc {
                                        q = None;
                                    }
                                }
                                None => match c {
                                    b'"' | b'\'' => q = Some(c),
                                    b'(' => depth += 1,
                                    b')' => depth -= 1,
                                    _ => {}
                                },
                            }
                            j += 1;
                        }
                    }
                    let raw = self.body[ref_start..j].to_string();
                    let vspan = self.span(ref_start, j);
                    let slot = CelSlot::raw(CelKind::AttrValue, raw, vspan);
                    attrs.push(Attr {
                        key,
                        value: AttrValue::Ref(slot),
                        value_span: vspan,
                        span: self.span(key_start, j),
                    });
                } else {
                    // `key=` with a bare/unquoted token: read to whitespace/term.
                    let vstart = j;
                    while j < n && b[j] != b' ' && b[j] != b'\t' && b[j] != term && b[j] != b'\n' {
                        j += 1;
                    }
                    let value = self.body[vstart..j].to_string();
                    let vspan = self.span(vstart, j);
                    attrs.push(Attr {
                        key,
                        value: AttrValue::Str(value),
                        value_span: vspan,
                        span: self.span(key_start, j),
                    });
                }
            } else {
                // Bare ident ⇒ boolean true (§4.5).
                let key_end = key_start + key.len();
                let kspan = self.span(key_start, key_end);
                attrs.push(Attr {
                    key,
                    value: AttrValue::BoolTrue,
                    value_span: kspan,
                    span: kspan,
                });
            }
        }
        let after = if j < n && b[j] == term { j + 1 } else { j };
        // `b`'s borrow of `self.body` has ended; safe to emit via `&mut self`.
        for (s, e) in escapes {
            self.emit_o(
                E_STRING_ESCAPE,
                "only \\\" \\\\ \\n \\t are defined escapes (dsl §4.4)".to_string(),
                self.orig(s),
                self.orig(e),
                Layer::Content,
            );
        }
        (attrs, after)
    }

    /// Body offset of the `}` matching the `{` at `open`, honoring `"`/`'` quotes
    /// (a CEL RHS may contain nested `{}` map literals). `None` if unbalanced.
    pub(super) fn find_matching_brace(&self, open: usize) -> Option<usize> {
        let b = self.body.as_bytes();
        let n = b.len();
        let mut depth = 0usize;
        let mut q: Option<u8> = None;
        let mut esc = false;
        let mut i = open;
        while i < n {
            let c = b[i];
            match q {
                Some(qc) => {
                    if esc {
                        esc = false;
                    } else if c == b'\\' {
                        esc = true;
                    } else if c == qc {
                        q = None;
                    }
                }
                None => match c {
                    b'"' | b'\'' => q = Some(c),
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    b'\n' => return None,
                    _ => {}
                },
            }
            i += 1;
        }
        None
    }
}

/// Take (remove) the string value of attribute `key`, if present.
pub(super) fn take_str(attrs: &mut Vec<Attr>, key: &str) -> Option<String> {
    let pos = attrs.iter().position(|a| a.key == key)?;
    if let AttrValue::Str(s) = &attrs[pos].value {
        let s = s.clone();
        attrs.remove(pos);
        Some(s)
    } else {
        None
    }
}

/// Remove the first attr named `key` and report whether it was present as a
/// bare boolean-true flag (dsl 0.2.0 §6.4 `optional`). A bare `key` with no
/// `=` parses to `AttrValue::BoolTrue`; a `key="…"` value is still consumed but
/// reported `false` (it is not a bare flag).
pub(super) fn take_bool(attrs: &mut Vec<Attr>, key: &str) -> bool {
    if let Some(pos) = attrs.iter().position(|a| a.key == key) {
        return matches!(attrs.remove(pos).value, AttrValue::BoolTrue);
    }
    false
}

/// Take (remove) the string value of attribute `key` together with its
/// `value_span`, if present. Used for literal (non-CEL) attributes like
/// `<when is="…">` (dsl §7.3.1) that must keep their source span without being
/// parsed as CEL.
pub(super) fn take_str_spanned(attrs: &mut Vec<Attr>, key: &str) -> Option<(String, Span)> {
    let pos = attrs.iter().position(|a| a.key == key)?;
    if let AttrValue::Str(s) = &attrs[pos].value {
        let s = s.clone();
        let span = attrs[pos].value_span;
        attrs.remove(pos);
        Some((s, span))
    } else {
        None
    }
}

/// Take (remove) attribute `key` as a typed [`CelSlot`]. A quoted value becomes
/// a raw CEL expression (§4.4 `CelString`); an `@ref` reuses its inner slot.
pub(super) fn take_cel(attrs: &mut Vec<Attr>, key: &str, kind: CelKind) -> Option<CelSlot> {
    let pos = attrs.iter().position(|a| a.key == key)?;
    let attr = attrs.remove(pos);
    let vspan = attr.value_span;
    match attr.value {
        AttrValue::Str(raw) => Some(CelSlot::raw(kind, raw, vspan)),
        AttrValue::Ref(mut slot) => {
            slot.kind = kind;
            Some(slot)
        }
        AttrValue::BoolTrue => Some(CelSlot::raw(kind, String::new(), vspan)),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;

    #[test]
    fn unknown_string_escape_is_diagnosed() {
        let (_, diags) = parse("## Shot 1.\n::sfx{sound=\"a\\qb\"}\n");
        assert!(diags.iter().any(|d| d.code == "E-STRING-ESCAPE"), "{diags:?}");
    }

    #[test]
    fn defined_escapes_pass() {
        let (_, diags) = parse("## Shot 1.\n::sfx{sound=\"a\\\"b\\\\c\\nd\\te\"}\n");
        assert!(
            diags.iter().all(|d| d.code != "E-STRING-ESCAPE"),
            "{diags:?}"
        );
    }

    // Boundary (dsl §4.4): `scan_attrs` cannot know whether a value is a plain
    // `String` or a `CelString` (typing is a checker-layer decision via the
    // manifest). A `CelString` legitimately embeds CEL single-quoted strings
    // whose own escape `\'` (`test="$ == 'a\'b'"`) is NOT a `String` escape, so
    // flagging it would be a false positive. The escape set is validated at the
    // lexical layer with `\'` exempted, leaving genuinely-undefined escapes
    // (`\q`, `\d`, …) flagged for both String and CelString values.
    #[test]
    fn cel_single_quote_escape_is_not_flagged() {
        let (_, diags) = parse("## Shot 1.\n::sfx{note=\"a\\'b\"}\n");
        assert!(
            diags.iter().all(|d| d.code != "E-STRING-ESCAPE"),
            "{diags:?}"
        );
    }

    // A trailing backslash just before EOF must not panic or scan past the
    // (missing) terminator; it is an incomplete escape, not `E-STRING-ESCAPE`.
    #[test]
    fn trailing_backslash_at_eof_is_safe() {
        let (_, diags) = parse("## Shot 1.\n::sfx{sound=\"ab\\");
        assert!(
            diags.iter().all(|d| d.code != "E-STRING-ESCAPE"),
            "{diags:?}"
        );
    }
}
