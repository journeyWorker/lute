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
            self.emit_line(E_LEGACY_CONTENT_SIGIL, LEGACY_CONTENT_SIGIL_MESSAGE, i, Layer::Content);
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
        Some(Node::Line(Line { speaker, attrs, when, text, text_span, interps, span }))
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
