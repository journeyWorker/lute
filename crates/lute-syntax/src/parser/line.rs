use super::*;

impl Parser<'_> {
    /// `Line ::= "@" Speaker Attrs? ":" WS Text` (dsl §7.1, 0.2.2 — the sigil
    /// was `:` through 0.2.1, foundation C1). Text is opaque to EOL except
    /// `{{…}}` (§4.4, §7.6). Layer = Content. dsl 0.26.0 §3.2: a speaker
    /// `@param` (`@@who:`) is kept with its `@` (`speaker == "@who"`); the
    /// checker holds it to a component's `speaker` params and each `::use`
    /// binds it to the member its argument names.
    pub(super) fn parse_line(&mut self) -> Option<Node> {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + leading_ws(&self.body[s..e]);
        let line_end = s + self.body[s..e].trim_end().len();
        let b = self.body.as_bytes();
        let mut j = cstart + 1; // past the sigil (`@`, or legacy `:`)
        let sp_start = j;
        if b[cstart] == b'@' && b.get(j) == Some(&b'@') {
            j += 1;
        }
        while j < e && is_ident_byte(b[j]) {
            j += 1;
        }
        let speaker = self.body[sp_start..j].to_string();
        if speaker == "line" && j < e && b[j] == b'[' {
            let inner_start = j + 1;
            let mut k = inner_start;
            while k < e && is_ident_byte(b[k]) {
                k += 1;
            }
            let inner = self.body[inner_start..k].to_string();
            let has_close = k < e && b[k] == b']';
            let edit_span = self.span(cstart, if has_close { k + 1 } else { k });
            self.emit_line(E_UNCLASSIFIED, "`:line[speaker]` was removed — write `@speaker{…}: text` (dsl §7.1, 0.2.2)", i, Layer::Content);
            if has_close {
                if let Some(d) = self.diags.last_mut() {
                    d.fixits.push(Fixit {
                        title: "Migrate `:line[speaker]` to `@speaker`".to_string(),
                        kind: "migrate".to_string(),
                        edit: vec![TextEdit { span: edit_span, new_text: format!("@{inner}") }],
                        confidence: 100,
                    });
                }
            }
            self.cursor += 1;
            return None;
        }
        if b[cstart] == b':' {
            let sigil_span = self.span(cstart, cstart + 1);
            self.emit_line(E_LEGACY_CONTENT_SIGIL, "content line sigil `:` was replaced by `@` in 0.2.2 — write `@speaker{…}: text` (dsl §7.1); `lute fix` applies this migration automatically", i, Layer::Content);
            if let Some(d) = self.diags.last_mut() {
                d.fixits.push(Fixit {
                    title: "Migrate content-line sigil `:` to `@`".to_string(),
                    kind: "migrate".to_string(),
                    edit: vec![TextEdit { span: sigil_span, new_text: "@".to_string() }],
                    confidence: 100,
                });
            }
            self.cursor += 1;
            return None;
        }
        if j < e && b[j] == b'[' {
            self.emit_line(E_CONTENT_LINE_BRACKET, "content-line attributes use `{…}`, not `[…]` (dsl 0.1 §7.1)", i, Layer::Content);
            self.cursor += 1;
            return None;
        }
        let mut attrs = Vec::new();
        if j < e && b[j] == b'{' {
            let (a, after) = self.scan_attrs(j + 1, b'}');
            attrs = a;
            j = after;
        }
        let when = attrs::take_cel(&mut attrs, "when", CelKind::Condition);
        let b = self.body.as_bytes();
        if !(j < e && b[j] == b':') {
            self.emit_line(E_UNCLASSIFIED, "content line needs a second `:` before its text (dsl §7.1)", i, Layer::Content);
            self.cursor += 1;
            return None;
        }
        j += 1;
        while j < e && (b[j] == b' ' || b[j] == b'\t') { j += 1; }
        let text_start = j;
        let text_raw = self.body[text_start..line_end.max(text_start)].trim_end();
        let text_end = text_start + text_raw.len();
        let text_span = self.span(text_start, text_end);
        let span = self.span(cstart, line_end);
        self.cursor += 1;
        let text = text_raw.to_string();
        let interps = self.scan_interps(&text, text_start);
        let inline = self.parse_inline(&text, text_start);
        Some(Node::Line(Line { speaker, attrs, when, text, text_span, interps, inline, span }))
    }
    fn parse_inline(&mut self, text: &str, base: usize) -> Vec<InlineNode> {
        let (nodes, _) = self.parse_inline_nodes(text, base, 0, None, false);
        nodes
    }

    pub(super) fn parse_inline_nodes(
        &mut self,
        text: &str,
        base: usize,
        mut j: usize,
        terminator: Option<u8>,
        in_modifier: bool,
    ) -> (Vec<InlineNode>, usize) {
        let b = text.as_bytes();
        let mut nodes = Vec::new();
        let mut plain_start = j;
        let flush = |this: &Self, nodes: &mut Vec<InlineNode>, start: &mut usize, end: usize| {
            if *start < end {
                nodes.push(InlineNode::Text {
                    text: decode_inline_text(&text[*start..end]),
                    span: this.span(base + *start, base + end),
                });
            }
        };
        while j < b.len() {
            if terminator == Some(b[j]) {
                flush(self, &mut nodes, &mut plain_start, j);
                return (nodes, j + 1);
            }
            if b[j] == b'\\' {
                let mut run = j;
                while run < b.len() && b[run] == b'\\' {
                    run += 1;
                }
                if in_modifier
                    && (run - j) % 2 == 1
                    && run < b.len()
                    && !matches!(b[run], b':' | b'[' | b']' | b'{' | b'}' | b'\\')
                {
                    self.emit_o(
                        E_TEXT_ESCAPE,
                        "unknown escape in inline modifier".into(),
                        self.orig(base + run - 1),
                        self.orig(base + run + 1),
                        Layer::Content,
                    );
                }
                // An odd backslash run escapes the byte after it: `\{{` is
                // a literal interpolation, `\:` `\[` `\]` `\{` `\}` literal
                // punctuation that never opens or closes a modifier.
                let escaped = (run - j) % 2 == 1 && run < b.len();
                if escaped && b[run] == b'{' && run + 1 < b.len() && b[run + 1] == b'{' {
                    j = run + 2;
                } else if escaped && matches!(b[run], b':' | b'[' | b']' | b'{' | b'}') {
                    j = run + 1;
                } else {
                    j = run;
                }
                continue;
            }
            if b[j] == b'{' && j + 1 < b.len() && b[j + 1] == b'{' {
                if let Some(rel) = text[j + 2..].find("}}") {
                    flush(self, &mut nodes, &mut plain_start, j);
                    let end = j + 2 + rel + 2;
                    let span = self.span(base + j, base + end);
                    nodes.push(InlineNode::Interpolation(interp_from_inner(
                        &text[j + 2..j + 2 + rel],
                        span,
                    )));
                    j = end;
                    plain_start = j;
                    continue;
                }
                self.emit_o(
                    E_INTERP_UNTERMINATED,
                    "`{{` has no closing `}}` before end of line (dsl §7.6)".into(),
                    self.orig(base + j),
                    self.orig(base + text.len()),
                    Layer::Content,
                );
                break;
            }
            if b[j] == b':' {
                let name_start = j + 1;
                let mut k = name_start;
                while k < b.len() && is_inline_name_byte(b[k]) {
                    k += 1;
                }
                if k > name_start
                    && b[name_start].is_ascii_lowercase()
                    && k < b.len()
                    && matches!(b[k], b'[' | b'{')
                {
                    flush(self, &mut nodes, &mut plain_start, j);
                    let name = text[name_start..k].to_string();
                    let (children, after_content, content_span) = if b[k] == b'[' {
                        let content_start = k + 1;
                        let (children, after) =
                            self.parse_inline_nodes(text, base, content_start, Some(b']'), true);
                        let content_span = if after > content_start && after <= b.len() && b[after - 1] == b']' {
                            Some(self.span(base + content_start, base + after - 1))
                        } else {
                            None
                        };
                        (children, after, content_span)
                    } else {
                        (Vec::new(), k, None)
                    };
                    let mut after = after_content;
                    let attrs = if after < b.len() && b[after] == b'{' {
                        let (attrs, end) = self.parse_inline_attrs(text, base, after);
                        after = end;
                        attrs
                    } else {
                        Vec::new()
                    };
                    let span = self.span(base + j, base + after.min(b.len()));
                    nodes.push(InlineNode::Modifier(InlineModifier {
                        name,
                        attrs,
                        children,
                        span,
                        content_span,
                    }));
                    j = after;
                    plain_start = j;
                    continue;
                }
            }
            j += 1;
        }
        if terminator.is_some() {
            self.emit_o(
                E_TEXT_MODIFIER,
                "inline modifier span is missing a closing `]`".into(),
                self.orig(base + j.min(text.len())),
                self.orig(base + text.len()),
                Layer::Content,
            );
        }
        flush(self, &mut nodes, &mut plain_start, j);
        (nodes, j)
    }

    fn parse_inline_attrs(
        &mut self,
        text: &str,
        base: usize,
        open: usize,
    ) -> (Vec<InlineAttr>, usize) {
        let b = text.as_bytes();
        let mut attrs = Vec::new();
        let mut j = open + 1;
        while j < b.len() {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            if b[j] == b'}' {
                return (attrs, j + 1);
            }
            let key_start = j;
            while j < b.len() && is_inline_name_byte(b[j]) {
                j += 1;
            }
            if j == key_start {
                j += 1;
                continue;
            }
            let key = text[key_start..j].to_string();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let value_start = j;
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let raw_start = j;
                if j < b.len() && b[j] == b'"' {
                    j += 1;
                    while j < b.len() {
                        if b[j] == b'\\' && j + 1 < b.len() {
                            if !matches!(b[j + 1], b':' | b'[' | b']' | b'{' | b'}' | b'\\' | b'"') {
                                self.emit_o(
                                    E_TEXT_ESCAPE,
                                    "unknown escape in inline modifier".into(),
                                    self.orig(base + j),
                                    self.orig(base + j + 2),
                                    Layer::Content,
                                );
                            }
                            j += 2;
                        } else if b[j] == b'"' {
                            j += 1;
                            break;
                        } else {
                            j += 1;
                        }
                    }
                } else {
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'}' {
                        j += 1;
                    }
                }
                let value = decode_inline_text(&text[raw_start..j].trim_matches('"'));
                let value_span = self.span(base + raw_start, base + j);
                attrs.push(InlineAttr {
                    key,
                    value,
                    span: self.span(base + key_start, base + j),
                    value_span,
                });
            } else {
                attrs.push(InlineAttr {
                    key,
                    value: String::new(),
                    span: self.span(base + key_start, base + j),
                    value_span: self.span(base + value_start, base + value_start),
                });
            }
        }
        self.emit_o(
            E_TEXT_MODIFIER,
            "inline modifier attributes are missing a closing `}`".into(),
            self.orig(base + open),
            self.orig(base + text.len()),
            Layer::Content,
        );
        (attrs, text.len())
    }

    /// Scan `{{…}}` interpolations in a content line's `Text` (dsl §7.6).
    fn scan_interps(&mut self, text: &str, text_start_body: usize) -> Vec<Interp> {
        let b = text.as_bytes();
        let mut out = Vec::new();
        let mut j = 0;
        while j + 1 < b.len() {
            if b[j] == b'\\' && text[j + 1..].starts_with("{{") { j += 3; continue; }
            if b[j] == b'{' && b[j + 1] == b'{' {
                match text[j + 2..].find("}}") {
                    None => {
                        let (s, e) = (text_start_body + j, text_start_body + text.len());
                        self.emit_o(E_INTERP_UNTERMINATED, "`{{` has no closing `}}` before end of line (dsl §7.6)".into(), self.orig(s), self.orig(e), Layer::Content);
                        break;
                    }
                    Some(rel) => {
                        let (s, e) = (text_start_body + j, text_start_body + j + 2 + rel + 2);
                        let span = self.span(s, e);
                        out.push(crate::ast::interp_from_inner(&text[j + 2..j + 2 + rel], span));
                        j = j + 2 + rel + 2;
                        continue;
                    }
                }
            }
            j += 1;
        }
        out
    }
}
fn is_inline_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn decode_inline_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut j = 0;
    while j < bytes.len() {
        if bytes[j] == b'\\' && j + 1 < bytes.len()
            && matches!(bytes[j + 1], b':' | b'[' | b']' | b'{' | b'}' | b'\\')
        {
            out.push(bytes[j + 1] as char);
            j += 2;
        } else {
            let ch = raw[j..].chars().next().unwrap_or_default();
            out.push(ch);
            j += ch.len_utf8();
        }
    }
    out
}
