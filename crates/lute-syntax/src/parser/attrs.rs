//! Attribute scanner + brace matcher (dsl §4.4 quoting, §4.5 attributes).
//!
//! `{ … }` / `< … >` attribute lists are tokenized on ASCII whitespace, but a
//! `"`-quoted value is opaque: structural chars (`}`, `>`, space, `=`) inside a
//! quoted string are literal (`\` escapes the next char, §4.4). Three value
//! shapes: `key="str"` → [`AttrValue::Str`]; `key=@ref` → [`AttrValue::Ref`]
//! (a [`CelSlot`] of [`CelKind::AttrValue`]); bare `key` → [`AttrValue::BoolTrue`].
//!
//! The scan itself is [`scan_attr_list`], over byte offsets only: the parser
//! builds its [`Attr`]s from it ([`Parser::scan_attrs`]), and
//! [`crate::authored_attrs`] re-reads a node's head as written with the same
//! function, so there is one attribute grammar.

use super::{is_ident_byte, Parser, E_ATTR_QUOTE, E_STRING_ESCAPE};
use crate::ast::{Attr, AttrValue, CelKind, CelSlot};
use lute_core_span::{Layer, Span};

/// One attribute as [`scan_attr_list`] reads it. Every offset indexes the
/// scanned text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScannedAttr {
    /// The key's `(start, end)`.
    pub(crate) key: (usize, usize),
    pub(crate) value: ScannedValue,
    /// Offset just past the attribute (past a closing quote when there is one).
    pub(crate) end: usize,
}

/// The value shape of a [`ScannedAttr`]; each range is the value text as
/// written, without its quotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScannedValue {
    /// `key="…"`: its stored text is [`decode_value`] of the range.
    Quoted(usize, usize),
    /// `key='…'`, `key=“…”` (each an `E-ATTR-QUOTE`) or an unquoted
    /// `key=token`: stored verbatim.
    Verbatim(usize, usize),
    /// `key=@ref` or `key=@ref(args)`, from the `@`.
    Ref(usize, usize),
    /// A bare `key` (dsl §4.5: `true`).
    Flag,
}

/// What one attribute list holds: its attributes, the offset just past the
/// terminator (or where the scan stopped), and the lexical faults to report.
#[derive(Debug, Default)]
pub(crate) struct AttrScan {
    pub(crate) attrs: Vec<ScannedAttr>,
    pub(crate) after: usize,
    /// `(start, end)` of each undefined backslash escape (`E-STRING-ESCAPE`).
    pub(crate) escapes: Vec<(usize, usize)>,
    /// `(start, end)` of each single-quoted value (`E-ATTR-QUOTE`).
    pub(crate) single_quoted: Vec<(usize, usize)>,
    /// `(start, end, quote)` of each curly quote used as a delimiter
    /// (`E-ATTR-QUOTE`).
    pub(crate) curly: Vec<(usize, usize, char)>,
}

/// Scan the attribute list of `text` from offset `start` up to the unquoted
/// terminator byte `term` (`}` for directives and lines, `>` for tags) or the
/// line end. A scan never reads past a `\n` (dsl §2.3).
pub(crate) fn scan_attr_list(text: &str, start: usize, term: u8) -> AttrScan {
    let b = text.as_bytes();
    let n = b.len();
    let mut scan = AttrScan::default();
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
        let key = (key_start, j);
        if !(j < n && b[j] == b'=') {
            // Bare ident ⇒ boolean true (§4.5).
            scan.attrs.push(ScannedAttr {
                key,
                value: ScannedValue::Flag,
                end: j,
            });
            continue;
        }
        j += 1; // past '='
        let value = if j < n && b[j] == b'"' {
            j += 1; // past opening quote
            let inner_start = j;
            let mut esc = false;
            // The first curly double quote that could close this value
            // (`label="Open the oven”>`): used only when no straight `"`
            // closes it on this line.
            let mut curly_close: Option<(usize, char)> = None;
            while j < n {
                let c = b[j];
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                    // §4.4: only `\" \\ \n \t` are defined String escapes.
                    // `\'` is exempted — a CelString value (indistinguishable
                    // here) may embed a CEL single-quoted string using `\'`.
                    // Any other escape is undefined; span the two bytes `\x`.
                    // A trailing `\` with no next byte (`j + 1 == n`) is an
                    // incomplete escape, not flagged.
                    if j + 1 < n {
                        let e = b[j + 1];
                        if !matches!(e, b'"' | b'\\' | b'n' | b't' | b'\'') {
                            scan.escapes.push((j, j + 2));
                        }
                    }
                } else if c == b'"' {
                    break;
                } else if c == b'\n' {
                    // RC2 (dsl §2.3): a physical newline INSIDE a quoted value
                    // is never part of the value — the one-line model forbids
                    // a tag/attribute list from wrapping. Stop here
                    // (unterminated on this line) rather than scanning into
                    // the next physical line looking for the closing quote.
                    break;
                }
                if curly_close.is_none() {
                    if let Some(q @ ('\u{201C}' | '\u{201D}')) = curly_quote_at(b, j) {
                        if closes_value(b, j + 3, term) {
                            curly_close = Some((j, q));
                        }
                    }
                }
                j += 1;
            }
            let mut inner_end = j;
            if j < n && b[j] == b'"' {
                j += 1; // past closing quote (only when actually found)
            } else if let Some((at, q)) = curly_close {
                // Unterminated on this line, but a curly quote sits where the
                // closing `"` belongs: end the value there.
                scan.curly.push((at, at + 3, q));
                inner_end = at;
                j = at + 3;
            }
            ScannedValue::Quoted(inner_start, inner_end)
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
                        // RC2 (dsl §2.3): stop at the physical line end — an
                        // `@ref(...)` arg list (quoted or not) never wraps to
                        // a later line.
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
            ScannedValue::Ref(ref_start, j)
        } else if j < n && b[j] == b'\'' {
            // `key='…'`: not attribute quoting (§4.4 quotes with `"` only).
            // Consume the whole single-quoted run so the rest of the list
            // still parses, keep its inner text as the value, and report
            // `E-ATTR-QUOTE` — never let the quotes leak into the value
            // silently.
            let quote_start = j;
            j += 1;
            let inner_start = j;
            let mut esc = false;
            while j < n && b[j] != b'\n' {
                if esc {
                    esc = false;
                } else if b[j] == b'\\' {
                    esc = true;
                } else if b[j] == b'\'' {
                    break;
                }
                j += 1;
            }
            let inner_end = j;
            if j < n && b[j] == b'\'' {
                j += 1;
            }
            scan.single_quoted.push((quote_start, j));
            ScannedValue::Verbatim(inner_start, inner_end)
        } else if let Some(q) = curly_quote_at(b, j) {
            // `key=“…”`: a word processor's curly quote is not attribute
            // quoting (§4.4). Read the curly-quoted span as the value — up to
            // a matching quote (curly or straight) that ends the token — so
            // the words inside do not become one bogus attribute each, and
            // report `E-ATTR-QUOTE` once.
            let quote_start = j;
            let double = matches!(q, '\u{201C}' | '\u{201D}');
            let inner_start = j + 3;
            let mut k = inner_start;
            let mut close: Option<(usize, usize)> = None;
            while k < n && b[k] != b'\n' {
                let len = match curly_quote_at(b, k) {
                    Some(c) if matches!(c, '\u{201C}' | '\u{201D}') == double => 3,
                    None if b[k] == if double { b'"' } else { b'\'' } => 1,
                    _ => 0,
                };
                if len > 0 && closes_value(b, k + len, term) {
                    close = Some((k, k + len));
                    break;
                }
                k += 1;
            }
            let inner_end = match close {
                Some((end, after)) => {
                    j = after;
                    end
                }
                None => {
                    // No closing quote on this line: read it like a bare
                    // token so the rest of the list still parses.
                    j = inner_start;
                    while j < n && b[j] != b' ' && b[j] != b'\t' && b[j] != term && b[j] != b'\n' {
                        j += 1;
                    }
                    j
                }
            };
            scan.curly.push((quote_start, j, q));
            ScannedValue::Verbatim(inner_start, inner_end)
        } else {
            // `key=` with a bare/unquoted token: read to whitespace/term.
            let vstart = j;
            while j < n && b[j] != b' ' && b[j] != b'\t' && b[j] != term && b[j] != b'\n' {
                j += 1;
            }
            ScannedValue::Verbatim(vstart, j)
        };
        scan.attrs.push(ScannedAttr { key, value, end: j });
    }
    scan.after = if j < n && b[j] == term { j + 1 } else { j };
    scan
}

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
    pub(crate) fn scan_attrs(&mut self, start: usize, term: u8) -> (Vec<Attr>, usize) {
        let scan = scan_attr_list(&self.body, start, term);
        let mut attrs = Vec::with_capacity(scan.attrs.len());
        for a in &scan.attrs {
            let key = self.body[a.key.0..a.key.1].to_string();
            let (value, value_span) = match a.value {
                ScannedValue::Quoted(s, e) => (
                    AttrValue::Str(decode_value(&self.body[s..e])),
                    self.span(s, e),
                ),
                ScannedValue::Verbatim(s, e) => {
                    (AttrValue::Str(self.body[s..e].to_string()), self.span(s, e))
                }
                ScannedValue::Ref(s, e) => {
                    let vspan = self.span(s, e);
                    let slot = CelSlot::raw(CelKind::AttrValue, self.body[s..e].to_string(), vspan);
                    (AttrValue::Ref(slot), vspan)
                }
                ScannedValue::Flag => (AttrValue::BoolTrue, self.span(a.key.0, a.key.1)),
            };
            attrs.push(Attr {
                key,
                value,
                value_span,
                span: self.span(a.key.0, a.end),
            });
        }
        for (s, e) in scan.escapes {
            self.emit_o(
                E_STRING_ESCAPE,
                "only \\\" \\\\ \\n \\t are defined escapes (dsl §4.4)".to_string(),
                self.orig(s),
                self.orig(e),
                Layer::Content,
            );
        }
        for (s, e) in scan.single_quoted {
            self.emit_o(
                E_ATTR_QUOTE,
                "attribute values are quoted with `\"`, not `'`: write `key=\"…\"`, and \
                 write a `\"` inside the value as `\\\"` (dsl §4.4)"
                    .to_string(),
                self.orig(s),
                self.orig(e),
                Layer::Content,
            );
        }
        for (s, e, q) in scan.curly {
            self.emit_o(
                E_ATTR_QUOTE,
                format!(
                    "attribute values use straight quotes `\"` — found `{q}` (a word \
                     processor's curly quote); retype it as `\"` (dsl §4.4)"
                ),
                self.orig(s),
                self.orig(e),
                Layer::Content,
            );
        }
        (attrs, scan.after)
    }

    /// Body offset of the `}` matching the `{` at `open`, honoring `"`/`'` quotes
    /// (a CEL RHS may contain nested `{}` map literals). `None` if unbalanced.
    pub(crate) fn find_matching_brace(&self, open: usize) -> Option<usize> {
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

/// One attribute of a node's head as the author wrote it (dsl 0.50.0 §8.1):
/// lint reads these instead of the AST's typed fields, which drop the text
/// of a lifted attribute (`<reward amount="+005">` keeps only `5`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredAttr<'s> {
    pub key: &'s str,
    /// The value text as written, without its quotes: a quoted value's
    /// escapes and character references are not decoded, an `@ref` keeps
    /// its `@` and arguments, and a bare flag reads `"true"` (dsl §4.5).
    pub raw_value: &'s str,
    /// Byte range of the key in the source.
    pub key_span: std::ops::Range<usize>,
    /// Byte range of the value text in the source (the key's, for a flag).
    pub value_span: std::ops::Range<usize>,
}

/// Every attribute written in the head of the node whose span is `head`,
/// in source order, read with the parser's own scanner ([`scan_attr_list`]).
///
/// The head starts at `head.byte_start`: a `<tag …>` opener, a
/// `::tag{…}` directive or a `@speaker{…}:` content line. A head of another
/// shape (a `::tag` without braces, a line without `{…}`) has no
/// attributes, and so does a span that does not start inside `source`.
/// Block comments on the head line are blanked first, as the parser's
/// whole-body pass does: the head starts outside any comment and string,
/// and a comment still open at the line end runs on past it.
/// Each attribute is returned once per occurrence, so a repeated key is
/// repeated here.
pub fn authored_attrs(source: &str, head: Span) -> Vec<AuthoredAttr<'_>> {
    let start = head.byte_start;
    let Some(rest) = source.get(start..) else {
        return Vec::new();
    };
    let line = rest.find('\n').map_or(rest, |n| &rest[..n]);
    let Some((list, term)) = attr_list_start(line) else {
        return Vec::new();
    };
    let blanked;
    let view = if line.contains("/*") {
        // Close a comment left open at the line end, so it is blanked to
        // the end of the line as the whole-body pass blanks it, then drop
        // the closer again (blanking keeps byte length). The closer is
        // spaced off so it cannot join the line's last byte into a new
        // delimiter (a terminal `/` would make `/*/`).
        let mut closed = String::with_capacity(line.len() + 3);
        closed.push_str(line);
        closed.push_str(" */");
        match crate::lex::strip_comments_checked(&closed) {
            Ok(mut text) => {
                text.truncate(line.len());
                blanked = text;
                blanked.as_str()
            }
            Err(_) => line,
        }
    } else {
        line
    };
    let scan = scan_attr_list(view, list, term);
    scan.attrs
        .into_iter()
        .filter_map(|a| {
            let (value, value_span) = match a.value {
                ScannedValue::Quoted(s, e)
                | ScannedValue::Verbatim(s, e)
                | ScannedValue::Ref(s, e) => (line.get(s..e)?, s..e),
                ScannedValue::Flag => ("true", a.key.0..a.key.1),
            };
            Some(AuthoredAttr {
                key: line.get(a.key.0..a.key.1)?,
                raw_value: value,
                key_span: start + a.key.0..start + a.key.1,
                value_span: start + value_span.start..start + value_span.end,
            })
        })
        .collect()
}

/// Where the attribute list of the head `line` starts, and its terminator:
/// past `<name` (`>`), past `::name{` (`}`) or past `@speaker{` / `@@param{`
/// (`}`) — the openers [`Parser::scan_attrs`] is called on.
fn attr_list_start(line: &str) -> Option<(usize, u8)> {
    let b = line.as_bytes();
    let ident_end = |mut j: usize| {
        while j < b.len() && is_ident_byte(b[j]) {
            j += 1;
        }
        j
    };
    let braced = |j: usize| (b.get(j) == Some(&b'{')).then_some((j + 1, b'}'));
    match b.first()? {
        b'<' => Some((ident_end(1), b'>')),
        b':' if b.get(1) == Some(&b':') => braced(ident_end(2)),
        b'@' => braced(ident_end(if b.get(1) == Some(&b'@') { 2 } else { 1 })),
        _ => None,
    }
}

/// The curly (word-processor) quote — `‘` `’` `“` `”` — whose UTF-8 encoding
/// starts at byte `j`, if any.
fn curly_quote_at(b: &[u8], j: usize) -> Option<char> {
    match b.get(j..j + 3)? {
        [0xE2, 0x80, 0x98] => Some('\u{2018}'),
        [0xE2, 0x80, 0x99] => Some('\u{2019}'),
        [0xE2, 0x80, 0x9C] => Some('\u{201C}'),
        [0xE2, 0x80, 0x9D] => Some('\u{201D}'),
        _ => None,
    }
}

/// Whether the byte at `k` ends an attribute value: whitespace, the list
/// terminator `term`, a self-closing `/`, a line end, or the end of input.
fn closes_value(b: &[u8], k: usize, term: u8) -> bool {
    b.get(k)
        .is_none_or(|&c| matches!(c, b' ' | b'\t' | b'\n' | b'/') || c == term)
}

/// The stored text of a `"`-quoted attribute value (§4.4): its `\"` escapes
/// become `"`, and (dsl 0.24.0, round-3 T1-11) the XML character references
/// an author reaches for first — `&quot;` `&apos;` `&amp;` `&lt;` `&gt;`,
/// decimal `&#NN;` and hex `&#xHH;` — are decoded, so `label="&quot;Hi&quot;"`
/// ships `"Hi"` instead of the literal entity text. Anything else starting
/// with `&` (a bare `&`, `&&`, `&nbsp;`, an out-of-range code point) stays
/// literal. Only the delimiter escape is resolved, because the scanner
/// cannot tell a `String` from a `CelString` value: `\"` means `"` in both,
/// while `\\`, `\n`, `\t` and `\'` inside a `CelString` belong to the CEL
/// string literal they sit in and must reach the CEL parser untouched. Walks
/// escape PAIRS exactly like the scanner, so `\\"` never reads as `\"`. One
/// left-to-right pass, so `&amp;quot;` decodes to `&quot;`, never `"`.
fn decode_value(raw: &str) -> String {
    if !raw.contains("\\\"") && !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        match c {
            '\\' => {
                let mut pair = rest[1..].chars();
                match pair.next() {
                    Some('"') => out.push('"'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
                rest = pair.as_str();
            }
            '&' => match char_reference(rest) {
                Some((ch, len)) => {
                    out.push(ch);
                    rest = &rest[len..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            },
            _ => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// The character a reference at the start of `s` (which begins with `&`)
/// names, and its byte length through the `;`. `None` when it is not one of
/// the decoded forms [`decode_value`] lists.
fn char_reference(s: &str) -> Option<(char, usize)> {
    let end = s[1..].find(';')? + 1;
    let name = &s[1..end];
    let ch = match name {
        "quot" => '"',
        "apos" => '\'',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) if !hex.is_empty() && hex.len() <= 6 => {
                    u32::from_str_radix(hex, 16).ok()?
                }
                None if !num.is_empty()
                    && num.len() <= 7
                    && num.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    num.parse().ok()?
                }
                _ => return None,
            };
            char::from_u32(code)?
        }
    };
    Some((ch, end + 1))
}

/// Take (remove) the string value of attribute `key`, if present.
pub(crate) fn take_str(attrs: &mut Vec<Attr>, key: &str) -> Option<String> {
    let pos = attrs.iter().position(|a| a.key == key)?;
    if let AttrValue::Str(s) = &attrs[pos].value {
        let s = s.clone();
        attrs.remove(pos);
        Some(s)
    } else {
        None
    }
}

/// Take (remove) flag attribute `key` with its span, read through the one
/// flag reader [`AttrValue::flag`] (dsl 0.28.0 §1): bare, `="true"` or
/// `="false"`. Any other value is NOT taken — it stays in `attrs` so the
/// checker reports it (`E-FLAG-VALUE`) instead of it reading as `false`.
pub(crate) fn take_flag(attrs: &mut Vec<Attr>, key: &str) -> Option<(bool, Span)> {
    let pos = attrs.iter().position(|a| a.key == key)?;
    let flag = attrs[pos].value.flag()?;
    Some((flag, attrs.remove(pos).span))
}

/// Take (remove) the string value of attribute `key` together with its
/// `value_span`, if present. Used for literal (non-CEL) attributes like
/// `<when is="…">` (dsl §7.3.1) that must keep their source span without being
/// parsed as CEL.
pub(crate) fn take_str_spanned(attrs: &mut Vec<Attr>, key: &str) -> Option<(String, Span)> {
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
pub(crate) fn take_cel(attrs: &mut Vec<Attr>, key: &str, kind: CelKind) -> Option<CelSlot> {
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
        assert!(
            diags.iter().any(|d| d.code == "E-STRING-ESCAPE"),
            "{diags:?}"
        );
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

    fn first_choice_label(src: &str) -> String {
        let (doc, _) = parse(src);
        doc.sections[0]
            .body
            .iter()
            .find_map(|n| match n {
                crate::ast::Node::Branch(b) => Some(b.choices[0].text.clone()),
                _ => None,
            })
            .expect("a branch")
    }

    // T1-16: `\"` in a quoted value is a defined escape (§4.4) and must be
    // STORED as `"` — it used to reach the label with its backslash.
    #[test]
    fn escaped_double_quote_is_unescaped_in_the_value() {
        let label = first_choice_label(
            "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" text=\"\\\"Hi.\\\" she said\">\n\
             @x: a\n</choice>\n</branch>\n",
        );
        assert_eq!(label, "\"Hi.\" she said");
    }

    // Only the delimiter escape is resolved: a CelString's own escapes (`\'`,
    // `\\`) reach the CEL parser untouched, and `\\"` is an escaped backslash
    // followed by the closing quote, never `\"`.
    #[test]
    fn other_escapes_pass_through_to_the_value() {
        let (doc, _) = parse("## Shot 1.\n::sfx{note=\"a\\'b\\\\\" tail=\"x\"}\n");
        let crate::ast::Node::Directive(d) = &doc.sections[0].body[0] else {
            panic!("directive expected");
        };
        let note = d.attrs.iter().find(|a| a.key == "note").expect("note");
        assert!(
            matches!(&note.value, crate::ast::AttrValue::Str(s) if s == "a\\'b\\\\"),
            "{:?}",
            note.value
        );
        assert!(d.attrs.iter().any(|a| a.key == "tail"), "{:?}", d.attrs);
    }

    // dsl 0.24.0 (round-3 T1-11): XML character references in a quoted value
    // are decoded — they used to ship literally (`&quot;Quoted&quot;`).
    #[test]
    fn character_references_are_decoded_in_the_value() {
        let label = first_choice_label(
            "## Shot 1.\n<branch id=\"b\">\n\
             <choice id=\"c\" text=\"&quot;Quoted&quot; &amp; &lt;b&gt; it&apos;s &#65;&#x42;\">\n\
             @x: a\n</choice>\n</branch>\n",
        );
        assert_eq!(label, "\"Quoted\" & <b> it's AB");
    }

    // Only those forms: a bare `&`, `&&`, an unknown name, a bad number or a
    // missing `;` stay literal, and decoding is one pass (`&amp;quot;`).
    #[test]
    fn other_ampersands_stay_literal() {
        let label = first_choice_label(
            "## Shot 1.\n<branch id=\"b\">\n\
             <choice id=\"c\" text=\"a & b && c &nbsp; &#xZZ; &#; &amp;quot; &quot\">\n\
             @x: a\n</choice>\n</branch>\n",
        );
        assert_eq!(label, "a & b && c &nbsp; &#xZZ; &#; &quot; &quot");
    }

    // T1-16: a single-quoted value is not attribute quoting; it used to become
    // part of the value silently (`'"Hi."'`). Now `E-ATTR-QUOTE`, and the run
    // is consumed whole so the next attribute still parses.
    #[test]
    fn single_quoted_value_is_attr_quote_error() {
        let src = "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" text='Hi there' once>\n\
                   @x: a\n</choice>\n</branch>\n";
        let (doc, diags) = parse(src);
        assert_eq!(
            diags.iter().filter(|d| d.code == "E-ATTR-QUOTE").count(),
            1,
            "{diags:?}"
        );
        let crate::ast::Node::Branch(b) = &doc.sections[0].body[0] else {
            panic!("branch expected");
        };
        assert_eq!(b.choices[0].text, "Hi there");
        assert!(
            b.choices[0].attrs.iter().any(|a| a.key == "once"),
            "{:?}",
            b.choices[0].attrs
        );
    }

    // Round-5 T3-1: word-processor curly quotes around a value used to split
    // it into one bogus attribute per word (`E-UNKNOWN-ATTR` × n, with a
    // did-you-mean). Now one `E-ATTR-QUOTE` at the quote naming it, and the
    // curly-quoted span is the value so the rest of the list still parses.
    #[test]
    fn curly_quoted_value_is_one_attr_quote_error() {
        let src = "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" text=“Open the oven” once>\n\
                   @x: a\n</choice>\n</branch>\n";
        let (doc, diags) = parse(src);
        let quote: Vec<_> = diags.iter().filter(|d| d.code == "E-ATTR-QUOTE").collect();
        assert_eq!(quote.len(), 1, "{diags:?}");
        assert!(
            quote[0].message.contains("found `“`"),
            "{}",
            quote[0].message
        );
        assert_eq!(
            (quote[0].span.line, quote[0].span.column),
            (3, 21),
            "anchored at the curly quote"
        );
        let crate::ast::Node::Branch(b) = &doc.sections[0].body[0] else {
            panic!("branch expected");
        };
        assert_eq!(b.choices[0].text, "Open the oven");
        let keys: Vec<_> = b.choices[0].attrs.iter().map(|a| a.key.as_str()).collect();
        assert!(keys.contains(&"once") && !keys.contains(&"the"), "{keys:?}");
    }

    // A curly quote closing a straight-quoted value (`label="Open”>`) ends the
    // value there instead of swallowing the rest of the line; a curly opener
    // closed by a straight `"` (`sound=“x"`) and single curly quotes in a
    // directive's braces are the same error.
    #[test]
    fn mixed_and_single_curly_quotes_are_attr_quote_errors() {
        let label = first_choice_label(
            "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" text=\"Open it”>\n\
             @x: a\n</choice>\n</branch>\n",
        );
        assert_eq!(label, "Open it");
        for src in [
            "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" text=\"Open it”>\n@x: a\n</choice>\n</branch>\n",
            "## Shot 1.\n::sfx{sound=“a b\" note=\"n\"}\n",
            "## Shot 1.\n::sfx{sound=‘a b’ note=\"n\"}\n",
        ] {
            let (_, diags) = parse(src);
            let codes: Vec<_> = diags.iter().map(|d| d.code.as_str()).collect();
            assert_eq!(codes, ["E-ATTR-QUOTE"], "{src}: {diags:?}");
        }
        let (doc, _) = parse("## Shot 1.\n::sfx{sound=‘a b’ note=\"n\"}\n");
        let crate::ast::Node::Directive(d) = &doc.sections[0].body[0] else {
            panic!("directive expected");
        };
        let sound = d.attrs.iter().find(|a| a.key == "sound").expect("sound");
        assert!(
            matches!(&sound.value, crate::ast::AttrValue::Str(s) if s == "a b"),
            "{:?}",
            sound.value
        );
        assert!(d.attrs.iter().any(|a| a.key == "note"), "{:?}", d.attrs);
    }

    /// `(key, raw value)` of every attribute `authored_attrs` reads at
    /// `head`, after checking each span indexes `src` at that text.
    fn authored(src: &str, head: lute_core_span::Span) -> Vec<(String, String)> {
        crate::authored_attrs(src, head)
            .into_iter()
            .map(|a| {
                assert_eq!(&src[a.key_span.clone()], a.key);
                if a.raw_value != "true" || src[a.value_span.clone()] != *a.key {
                    assert_eq!(&src[a.value_span.clone()], a.raw_value);
                }
                (a.key.to_string(), a.raw_value.to_string())
            })
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn quest_rewards(src: &str) -> Vec<Vec<(String, String)>> {
        let (doc, _) = parse(src);
        doc.quests[0]
            .rewards
            .iter()
            .map(|r| authored(src, r.span))
            .collect()
    }

    const QUEST: &str = "---\nkind: quest\nid: q\n---\n<quest id=\"q\">\n\
                         <objective id=\"o\" done=\"true\"/>\n\
                         <reward kind=\"COINS\" amount=\"+005\"/>\n\
                         <reward kind=\"COINS\" amount=\"5\"/>\n\
                         <reward kind=\"BADGE\"/>\n</quest>\n";

    // dsl 0.50.0 §8.1: a lifted attribute keeps its authored text, and an
    // unauthored one is absent although the AST carries a parsed value.
    #[test]
    fn a_lifted_attribute_keeps_its_authored_text() {
        let (doc, _) = parse(QUEST);
        assert_eq!(
            doc.quests[0].rewards[0].amount,
            Some(crate::ast::RewardAmount::Scalar(5))
        );
        assert_eq!(
            quest_rewards(QUEST),
            [
                pairs(&[("kind", "COINS"), ("amount", "+005")]),
                pairs(&[("kind", "COINS"), ("amount", "5")]),
                pairs(&[("kind", "BADGE")]),
            ]
        );
    }

    #[test]
    fn element_directive_and_line_heads_are_read_as_written() {
        let src = "## Shot 1.\n<branch id=\"route\" menuType=\"timed\">\n\
                   <choice id=\"c\" /* note */ text=\"Go\" optionKey=@key once>\n\
                   @mira{emotion=\"fond\" vo when=\"run.x\"}: Hi\n\
                   ::sfx{sound=\"a\\\"b &amp; c\" volume=@loud(2, \"x y\") loop}\n\
                   ::bg\n</choice>\n</branch>\n";
        let (doc, _) = parse(src);
        let crate::ast::Node::Branch(b) = &doc.sections[0].body[0] else {
            panic!("branch expected");
        };
        assert!(
            b.attrs.iter().all(|a| a.key != "id"),
            "id is lifted out of the AST attrs"
        );
        assert_eq!(
            authored(src, b.span),
            pairs(&[("id", "route"), ("menuType", "timed")])
        );
        let c = &b.choices[0];
        assert_eq!(
            authored(src, c.span),
            pairs(&[
                ("id", "c"),
                ("text", "Go"),
                ("optionKey", "@key"),
                ("once", "true")
            ])
        );
        let body: Vec<_> = c
            .body
            .iter()
            .map(|n| match n {
                crate::ast::Node::Line(l) => authored(src, l.span),
                crate::ast::Node::Directive(d) => authored(src, d.span),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            body,
            [
                pairs(&[("emotion", "fond"), ("vo", "true"), ("when", "run.x")]),
                pairs(&[
                    ("sound", "a\\\"b &amp; c"),
                    ("volume", "@loud(2, \"x y\")"),
                    ("loop", "true"),
                ]),
                Vec::new(),
            ]
        );
    }

    #[test]
    fn crlf_and_multibyte_sources_read_the_same_values() {
        let lf = "---\nkind: quest\nid: q\n---\n<quest id=\"q\" title=\"별빛 퀘스트\">\n\
                  <reward kind=\"별\" amount=\"+005\"/>\n</quest>\n";
        let crlf = lf.replace('\n', "\r\n");
        for src in [lf, crlf.as_str()] {
            let (doc, _) = parse(src);
            assert_eq!(
                authored(src, doc.quests[0].span),
                pairs(&[("id", "q"), ("title", "별빛 퀘스트")]),
                "{src:?}"
            );
            assert_eq!(
                authored(src, doc.quests[0].rewards[0].span),
                pairs(&[("kind", "별"), ("amount", "+005")]),
                "{src:?}"
            );
        }
    }

    // S7 review: a comment opened at the end of the head line and closed on
    // the next one still blanks the head's earlier closed comments, as the
    // whole-body pass does; and (S7 re-review) a terminal `/` does not join
    // the closer added for that into a new comment opener.
    #[test]
    fn a_comment_left_open_at_the_line_end_keeps_earlier_comments_blanked() {
        for src in [
            "## A\n::sfx{/* loop */ sound=\"bell\"} /*\ncontinued */\n",
            "## A\n::sfx{/* } */ sound=\"bell\"} /*\ncontinued */\n",
            "## A\n::sfx{/* loop */ sound=\"bell\"} /\n",
            "## A\n::sfx{/* } */ sound=\"bell\"} /\n",
            "## A\r\n::sfx{/* loop */ sound=\"bell\"} /\r\n",
        ] {
            let (doc, _) = parse(src);
            let crate::ast::Node::Directive(d) = &doc.sections[0].body[0] else {
                panic!("directive expected: {src:?}");
            };
            let parsed: Vec<_> = d.attrs.iter().map(|a| a.key.as_str()).collect();
            assert_eq!(parsed, ["sound"], "{src:?}");
            assert_eq!(
                authored(src, d.span),
                pairs(&[("sound", "bell")]),
                "{src:?}"
            );
        }
        let src = "## A\n<branch id=\"b\">\n<choice /* > */ id=\"c\" text=\"Go\"> /*\n\
                   continued */\n@x: a\n</choice>\n</branch>\n";
        let (doc, _) = parse(src);
        let crate::ast::Node::Branch(b) = &doc.sections[0].body[0] else {
            panic!("branch expected");
        };
        assert_eq!(b.choices[0].text, "Go");
        assert_eq!(
            authored(src, b.choices[0].span),
            pairs(&[("id", "c"), ("text", "Go")])
        );
    }

    #[test]
    fn a_span_that_is_no_head_has_no_attributes() {
        let src = "## Shot 1.\n@x: a\n";
        let (doc, _) = parse(src);
        let crate::ast::Node::Line(l) = &doc.sections[0].body[0] else {
            panic!("line expected");
        };
        assert!(authored(src, l.span).is_empty());
        assert!(authored(src, doc.sections[0].span).is_empty());
        let past_end = lute_core_span::Span {
            byte_start: src.len() + 4,
            byte_end: src.len() + 4,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        };
        assert!(crate::authored_attrs(src, past_end).is_empty());
        // Inside a multi-byte character.
        let mid = lute_core_span::Span {
            byte_start: 1,
            ..past_end
        };
        assert!(crate::authored_attrs("별", mid).is_empty());
    }
}
