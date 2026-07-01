//! Attribute scanner + brace matcher (dsl §4.4 quoting, §4.5 attributes).
//!
//! `{ … }` / `< … >` attribute lists are tokenized on ASCII whitespace, but a
//! `"`-quoted value is opaque: structural chars (`}`, `>`, space, `=`) inside a
//! quoted string are literal (`\` escapes the next char, §4.4). Three value
//! shapes: `key="str"` → [`AttrValue::Str`]; `key=@ref` → [`AttrValue::Ref`]
//! (a [`CelSlot`] of [`CelKind::AttrValue`]); bare `key` → [`AttrValue::BoolTrue`].

use super::{is_ident_byte, Parser};
use crate::ast::{Attr, AttrValue, CelKind, CelSlot};

impl Parser<'_> {
    /// Scan an attribute list from body offset `start` up to the unquoted
    /// terminator byte `term` (`}` for directives, `>` for tags).
    ///
    /// Each [`Attr`] carries `value_span` — the span of its *value* (inner string
    /// for `Str`, the `@ref` for `Ref`, the key for `BoolTrue`) — so `take_cel`
    /// builds `CelSlot`s whose span bounds exactly `raw`. `after` is the body
    /// offset just past the terminator (or line end).
    pub(super) fn scan_attrs(&self, start: usize, term: u8) -> (Vec<Attr>, usize) {
        let b = self.body.as_bytes();
        let n = b.len();
        let mut attrs = Vec::new();
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
                        } else if c == b'"' {
                            break;
                        }
                        j += 1;
                    }
                    let inner_end = j;
                    if j < n {
                        j += 1; // past closing quote
                    }
                    let value = self.body[inner_start..inner_end].to_string();
                    let vspan = self.span(inner_start, inner_end);
                    attrs.push(Attr { key, value: AttrValue::Str(value), value_span: vspan, span: self.span(key_start, j) });
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
                    attrs.push(Attr { key, value: AttrValue::Ref(slot), value_span: vspan, span: self.span(key_start, j) });
                } else {
                    // `key=` with a bare/unquoted token: read to whitespace/term.
                    let vstart = j;
                    while j < n && b[j] != b' ' && b[j] != b'\t' && b[j] != term && b[j] != b'\n' {
                        j += 1;
                    }
                    let value = self.body[vstart..j].to_string();
                    let vspan = self.span(vstart, j);
                    attrs.push(Attr { key, value: AttrValue::Str(value), value_span: vspan, span: self.span(key_start, j) });
                }
            } else {
                // Bare ident ⇒ boolean true (§4.5).
                let key_end = key_start + key.len();
                let kspan = self.span(key_start, key_end);
                attrs.push(Attr { key, value: AttrValue::BoolTrue, value_span: kspan, span: kspan });
            }
        }
        let after = if j < n && b[j] == term { j + 1 } else { j };
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
