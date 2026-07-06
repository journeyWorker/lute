//! Line-classification parser + recursive block assembly (dsl §4.3, §4.5, §7).
//!
//! [`parse`] turns a `.lute` document into a [`Document`] AST plus a list of
//! parse [`Diagnostic`]s. The pipeline (see the SPAN-FIDELITY contract):
//! 1. [`peel_frontmatter`] splits the leading YAML `---` envelope (§6.1).
//! 2. [`strip_comments_checked`] blanks `/* … */` block comments over the whole
//!    post-frontmatter body **once** — it is length/newline preserving, so a
//!    byte offset in the stripped body plus `body_start` is the correct offset
//!    into the ORIGINAL text (no remap table). All [`Span`]s are original-source
//!    offsets and line/column come from a [`TextIndex`] over the original text.
//! 3. The body is split into lines and each non-blank line is classified by the
//!    normative §4.3 precedence (`## ` → `# ` → `::set{` → `::` → `:`ident →
//!    `<`tag → error). Block opens (`<branch>`/`<match>`/`<timeline>`) recurse
//!    via a per-block loop that matches the JSX self-naming close by tag name.

use crate::ast::*;
use crate::lex::{
    line_text_start_blanked, peel_frontmatter, strip_comments_checked, text_start_for_line,
    CommentError,
};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};

mod attrs;
mod blocks;

/// Diagnostic code: a body line matched no §4.3 rule (rule 7).
pub const E_UNCLASSIFIED: &str = "E-UNCLASSIFIED";
/// Diagnostic code: a block open (`<tag>`) had no matching `</tag>` close.
pub const E_UNCLOSED_TAG: &str = "E-UNCLOSED-TAG";
/// Diagnostic code: non-staging content inside a `<timeline>`/`<track>` (§7.4).
pub const E_TIMELINE_CONTENT: &str = "E-TIMELINE-CONTENT";
/// Diagnostic code: a `<branch>` body held a non-`<choice>` child, or a
/// `<match>` body a non-`<when>`/`<otherwise>` child (§7.3).
pub const E_LOGIC_CONTENT: &str = "E-LOGIC-CONTENT";
/// Diagnostic code: a `/* … */` block comment ran to EOF (§4.2).
pub const E_COMMENT_UNTERMINATED: &str = "E-COMMENT-UNTERMINATED";
/// Diagnostic code: a `## ` heading was neither `Shot|Scene <int>.` nor a
/// bookend keyword (`Prologue|Epilogue|프롤로그|에필로그`) (§6.3).
pub const E_SHOT_HEADING: &str = "E-SHOT-HEADING";

/// Parse a `.lute` document into its AST and parse diagnostics.
///
/// Never panics: malformed structure degrades to diagnostics + best-effort AST.
pub fn parse(text: &str) -> (Document, Vec<Diagnostic>) {
    let idx = TextIndex::new(text);
    let mut diags = Vec::new();

    let (fm, body_start) = peel_frontmatter(text).unwrap_or((None, 0));
    let (raw_yaml, meta_span) = match fm {
        Some((yaml, span)) => (yaml, span),
        None => (String::new(), zero_span()),
    };

    let body_slice = &text[body_start..];
    let body = match strip_comments_checked(body_slice) {
        Ok(stripped) => stripped,
        Err(CommentError::Unterminated) => {
            let pos = find_unterminated_comment(body_slice);
            diags.push(Diagnostic {
                code: E_COMMENT_UNTERMINATED.into(),
                severity: Severity::Error,
                message: "unterminated `/* … */` block comment".into(),
                span: Span::from_bytes(&idx, body_start + pos, text.len()),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
            });
            body_slice.to_string()
        }
    };

    let lines = split_lines(&body);
    let mut p = Parser {
        idx,
        body,
        body_start,
        lines,
        cursor: 0,
        diags,
    };
    let (title, shots) = p.parse_document_inner();

    let doc = Document {
        meta: Meta {
            raw_yaml,
            span: meta_span,
        },
        title,
        shots,
        span: Span::from_bytes(&p.idx, 0, text.len()),
    };
    (doc, p.diags)
}

/// Parser state. Byte offsets used internally are **body-relative** (into
/// `body`); [`Parser::orig`] converts them to original-text offsets for spans.
pub(crate) struct Parser<'a> {
    idx: TextIndex<'a>,
    /// Comment-stripped post-frontmatter body (length-preserving vs. original).
    body: String,
    /// Original-text offset of `body`'s first byte (`body[i]` ↔ `text[body_start+i]`).
    body_start: usize,
    /// `(start, end)` body offsets of each line (end excludes the `\n`).
    lines: Vec<(usize, usize)>,
    cursor: usize,
    diags: Vec<Diagnostic>,
}

impl Parser<'_> {
    // -- span / offset helpers -------------------------------------------------

    /// Body-relative offset → original-text offset.
    fn orig(&self, body_pos: usize) -> usize {
        self.body_start + body_pos
    }

    /// Span from ORIGINAL-text offsets.
    fn span_o(&self, start: usize, end: usize) -> Span {
        Span::from_bytes(&self.idx, start, end)
    }

    /// Span from BODY-relative offsets.
    fn span(&self, start_body: usize, end_body: usize) -> Span {
        self.span_o(self.orig(start_body), self.orig(end_body))
    }

    /// Body offset of the first non-whitespace byte of line `i`.
    fn line_content_start(&self, i: usize) -> usize {
        let (s, e) = self.lines[i];
        s + leading_ws(&self.body[s..e])
    }

    /// Body offset just past the last non-whitespace byte of line `i`.
    fn line_content_end(&self, i: usize) -> usize {
        let (s, e) = self.lines[i];
        s + self.body[s..e].trim_end().len()
    }

    /// Trimmed content of line `i` (owned, to avoid holding a `self.body` borrow).
    fn trimmed(&self, i: usize) -> String {
        let (s, e) = self.lines[i];
        self.body[s..e].trim().to_string()
    }

    // -- diagnostics -----------------------------------------------------------

    fn emit_o(&mut self, code: &str, msg: String, start: usize, end: usize, layer: Layer) {
        self.diags.push(Diagnostic {
            code: code.into(),
            severity: Severity::Error,
            message: msg,
            span: self.span_o(start, end),
            layer,
            fixits: Vec::new(),
            provenance: None,
        });
    }

    /// Emit a diagnostic spanning the content of line `i`.
    fn emit_line(&mut self, code: &str, msg: &str, i: usize, layer: Layer) {
        let a = self.orig(self.line_content_start(i));
        let b = self.orig(self.line_content_end(i));
        self.emit_o(code, msg.to_string(), a, b, layer);
    }

    // -- top-level document ----------------------------------------------------

    fn skip_blanks(&mut self) {
        while self.cursor < self.lines.len() && self.trimmed(self.cursor).is_empty() {
            self.cursor += 1;
        }
    }

    fn parse_document_inner(&mut self) -> (Option<(String, Span)>, Vec<Shot>) {
        let mut title = None;
        let mut shots = Vec::new();
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("## ") {
                shots.push(self.parse_shot());
            } else if trimmed.starts_with("# ") && shots.is_empty() && title.is_none() {
                title = Some(self.parse_title());
            } else {
                self.emit_line(
                    E_UNCLASSIFIED,
                    "unrecognized line",
                    self.cursor,
                    Layer::Content,
                );
                self.cursor += 1;
            }
        }
        (title, shots)
    }

    /// `Title ::= "# " Text` (§6.2). Text is opaque to EOL.
    fn parse_title(&mut self) -> (String, Span) {
        let i = self.cursor;
        let cstart = self.line_content_start(i);
        let cend = self.line_content_end(i);
        let t = self.trimmed(i);
        let text = t.strip_prefix("# ").unwrap_or(&t).to_string();
        self.cursor += 1;
        (text, self.span(cstart, cend))
    }

    /// `ShotBlock ::= ShotHeading Node*` (§6.3). Consumes the heading line then
    /// every body node up to the next `## ` heading or EOF.
    fn parse_shot(&mut self) -> Shot {
        let i = self.cursor;
        let cstart = self.line_content_start(i);
        let head_end = self.line_content_end(i);
        let full = self.trimmed(i);
        let heading = full.strip_prefix("## ").unwrap_or(&full).to_string();
        let number = match classify_heading(&heading) {
            HeadingKind::Numbered(n) => Some(n),
            HeadingKind::Bookend => None,
            HeadingKind::Invalid => {
                self.emit_line(
                    E_SHOT_HEADING,
                    "shot heading must be `Shot N.`/`Scene N.` or Prologue/Epilogue (dsl §6.3)",
                    i,
                    Layer::Content,
                );
                None
            }
        };
        let start_o = self.orig(cstart);
        let head_end_o = self.orig(head_end);
        self.cursor += 1;
        let body = self.parse_shot_body();
        let end_o = body.last().map(node_end).unwrap_or(head_end_o);
        Shot {
            heading,
            number,
            body,
            span: self.span_o(start_o, end_o),
        }
    }

    fn parse_shot_body(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("## ") {
                break; // next shot: leave for the document loop.
            }
            if trimmed.starts_with("</") {
                self.emit_line(
                    E_UNCLOSED_TAG,
                    "closing tag without a matching open",
                    self.cursor,
                    Layer::Logic,
                );
                self.cursor += 1;
                continue;
            }
            if let Some(node) = self.next_node() {
                nodes.push(node);
            }
        }
        nodes
    }

    /// Parse ONE node starting at `cursor` per the §4.3 precedence, or emit an
    /// `E-UNCLASSIFIED` / `E-UNEXPECTED` diagnostic and skip the line.
    /// Precondition: `cursor` is on a non-blank, non-heading, non-close line.
    fn next_node(&mut self) -> Option<Node> {
        let trimmed = self.trimmed(self.cursor);
        if trimmed.starts_with("::set{") {
            return Some(self.parse_set());
        }
        if trimmed.starts_with("::") {
            return Some(self.parse_directive());
        }
        // dsl §4.3 rule 5: `:` ident — content line. (`::` rules already matched above.)
        if trimmed.starts_with(':')
            && trimmed.as_bytes().get(1).is_some_and(|b| b.is_ascii_alphabetic())
        {
            return self.parse_line();
        }
        if trimmed.starts_with('<') {
            match open_tag_name(&trimmed).as_deref() {
                Some("branch") => return Some(Node::Branch(self.parse_branch())),
                Some("match") => return Some(Node::Match(self.parse_match())),
                Some("timeline") => return Some(Node::Timeline(self.parse_timeline())),
                _ => {
                    self.emit_line(
                        E_UNCLASSIFIED,
                        "unexpected block here",
                        self.cursor,
                        Layer::Logic,
                    );
                    self.cursor += 1;
                    return None;
                }
            }
        }
        self.emit_line(
            E_UNCLASSIFIED,
            "unrecognized line",
            self.cursor,
            Layer::Content,
        );
        self.cursor += 1;
        None
    }

    // -- leaf nodes ------------------------------------------------------------

    /// `Directive ::= "::" Ident Attrs?` (§7.2). Layer = Staging.
    fn parse_directive(&mut self) -> Node {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + leading_ws(&self.body[s..e]);
        let b = self.body.as_bytes();
        let mut j = cstart + 2; // past "::"
        let id_start = j;
        while j < e && is_ident_byte(b[j]) {
            j += 1;
        }
        let tag = self.body[id_start..j].to_string();
        let (attrs, end) = if j < e && b[j] == b'{' {
            let (attrs, after) = self.scan_attrs(j + 1, b'}');
            (attrs, after)
        } else {
            (Vec::new(), j)
        };
        let span = self.span(cstart, end);
        self.cursor += 1;
        Node::Directive(Directive { tag, attrs, span })
    }

    /// `Set ::= "::set{" Path WS AssignOp WS CelExpr "}"` (§7.3.4). Layer = Logic.
    fn parse_set(&mut self) -> Node {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + leading_ws(&self.body[s..e]);
        let open = cstart + "::set".len(); // at '{'
        let close = self.find_matching_brace(open);
        let inner_start = open + 1;
        let inner_end = close.unwrap_or(e);
        let node_end = close.map(|c| c + 1).unwrap_or(e);

        let inner = &self.body[inner_start..inner_end];
        let ib = inner.as_bytes();
        let n = ib.len();
        let mut j = 0;
        while j < n && (ib[j] == b' ' || ib[j] == b'\t') {
            j += 1;
        }
        let path_start = j;
        while j < n && (is_ident_byte(ib[j]) || ib[j] == b'.') {
            j += 1;
        }
        let path_end = j;
        let path = inner[path_start..path_end].to_string();
        let path_span = self.span(inner_start + path_start, inner_start + path_end);
        while j < n && (ib[j] == b' ' || ib[j] == b'\t') {
            j += 1;
        }
        let rest = &inner[j..];
        let op = if rest.starts_with("+=") {
            "+="
        } else if rest.starts_with("-=") {
            "-="
        } else if rest.starts_with("*=") {
            "*="
        } else {
            "=" // "=" or a malformed operator: default; the checker validates.
        };
        j += op.len();
        while j < n && (ib[j] == b' ' || ib[j] == b'\t') {
            j += 1;
        }
        let expr_start = j;
        let expr_raw = inner[expr_start..].trim_end();
        let expr_end = expr_start + expr_raw.len();
        let expr = CelSlot::raw(
            CelKind::SetExpr,
            expr_raw.to_string(),
            self.span(inner_start + expr_start, inner_start + expr_end),
        );
        let span = self.span(cstart, node_end);
        self.cursor += 1;
        Node::Set(Set {
            path,
            path_span,
            op: op.to_string(),
            expr,
            span,
        })
    }

    /// `Line ::= ":" Speaker Attrs? ":" WS Text` (dsl §7.1). Text is opaque to
    /// EOL except `{{…}}` (§4.4, §7.6). Layer = Content.
    fn parse_line(&mut self) -> Option<Node> {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + leading_ws(&self.body[s..e]);
        let line_end = s + self.body[s..e].trim_end().len();
        let b = self.body.as_bytes();
        let mut j = cstart + 1; // past ':'
        let sp_start = j;
        while j < e && is_ident_byte(b[j]) {
            j += 1;
        }
        let speaker = self.body[sp_start..j].to_string();
        // Migration fix-it (dsl §7.1): the removed 0.0.1 bracket form.
        if speaker == "line" && j < e && b[j] == b'[' {
            self.emit_line(
                E_UNCLASSIFIED,
                "`:line[speaker]` was removed in 0.1.0 — write `:speaker{…}: text`",
                i,
                Layer::Content,
            );
            self.cursor += 1;
            return None;
        }
        let mut attrs = Vec::new();
        if j < e && b[j] == b'{' {
            let (a, after) = self.scan_attrs(j + 1, b'}');
            attrs = a;
            j = after;
        }
        if !(j < e && b[j] == b':') {
            self.emit_line(
                E_UNCLASSIFIED,
                "content line needs a second `:` before its text (dsl §7.1)",
                i,
                Layer::Content,
            );
            self.cursor += 1;
            return None;
        }
        j += 1; // past second ':'
        while j < e && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        let text_start = j;
        let text_raw = self.body[text_start..line_end.max(text_start)].trim_end();
        let text_end = text_start + text_raw.len();
        let text_span = self.span(text_start, text_end);
        let span = self.span(cstart, line_end);
        self.cursor += 1;
        Some(Node::Line(Line {
            speaker,
            attrs,
            text: text_raw.to_string(),
            text_span,
            interps: Vec::new(), // filled in Task 7
            span,
        }))
    }
}

// -- free helpers -------------------------------------------------------------

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

/// A byte permitted inside an `Ident` / attr key (`[A-Za-z0-9_-]`); the leading
/// alpha requirement is enforced by classification, not this predicate.
pub(crate) fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn leading_ws(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn split_lines(body: &str) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut start = 0;
    for (i, &byte) in body.as_bytes().iter().enumerate() {
        if byte == b'\n' {
            v.push((start, i));
            start = i + 1;
        }
    }
    v.push((start, body.len()));
    v
}

enum HeadingKind {
    Numbered(i64),
    Bookend, // Prologue / Epilogue / 프롤로그 / 에필로그
    Invalid,
}

/// Enforce `ShotHeading` (dsl §6.3, [`E_SHOT_HEADING`]): `Shot|Scene <int>.` or
/// a bookend keyword (`Prologue|Epilogue|프롤로그|에필로그`), each + optional
/// trailing Text. `strip_prefix` on `&str` is byte-safe for the multi-byte
/// Korean keywords.
fn classify_heading(heading: &str) -> HeadingKind {
    for kw in ["Shot", "Scene"] {
        if let Some(rest) = heading.strip_prefix(kw) {
            // §6.3 requires whitespace between the keyword and the Integer, so
            // `Shot1.` is invalid: reject a keyword not followed by WS.
            if !rest.starts_with(char::is_whitespace) {
                return HeadingKind::Invalid;
            }
            let rest = rest.trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let after = &rest[digits.len()..];
            if !digits.is_empty() && after.starts_with('.') {
                // A shot number that overflows i64 degrades to E-SHOT-HEADING
                // rather than panicking (parse() preserves the crate's no-panic
                // guarantee); best-effort AST keeps `number: None`.
                return match digits.parse::<i64>() {
                    Ok(n) => HeadingKind::Numbered(n),
                    Err(_) => HeadingKind::Invalid,
                };
            }
            return HeadingKind::Invalid;
        }
    }
    for kw in ["Prologue", "Epilogue", "프롤로그", "에필로그"] {
        if let Some(rest) = heading.strip_prefix(kw) {
            if rest.is_empty() || rest.starts_with(' ') {
                return HeadingKind::Bookend;
            }
        }
    }
    HeadingKind::Invalid
}

/// Tag name of an open tag line (`<branch …>` → `Some("branch")`).
pub(crate) fn open_tag_name(trimmed: &str) -> Option<String> {
    if trimmed.starts_with("</") {
        return None;
    }
    let rest = trimmed.strip_prefix('<')?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Tag name of a close tag line (`</when>` → `Some("when")`).
pub(crate) fn close_tag_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("</")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn node_end(n: &Node) -> usize {
    match n {
        Node::Line(l) => l.span.byte_end,
        Node::Directive(d) => d.span.byte_end,
        Node::Set(s) => s.span.byte_end,
        Node::Branch(b) => b.span.byte_end,
        Node::Match(m) => m.span.byte_end,
        Node::Timeline(t) => t.span.byte_end,
        Node::Hub(h) => h.span.byte_end,
    }
}

/// Body-relative offset of the `/*` that started the unterminated comment.
/// Mirrors [`strip_comments_checked`]'s scan step for step (skips strings, `//`
/// line comments, and terminated block comments) so the reported position is the
/// exact `/*` that ran to EOF. Like that scan it honours §4.2 exclusion 2: past
/// a content line's second `:` the `Text` is opaque, so a `/*` (or a `"`) there
/// is literal and does not start a comment or a String. The opaque boundary is
/// recomputed from the *blanked* view after every terminated comment — not only
/// at newlines — so leading same-line trivia before the `:`ident (which the raw
/// line hides) cannot leave it stale (see [`line_text_start_blanked`],
/// [`text_start_for_line`]).
fn find_unterminated_comment(body: &str) -> usize {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.char_indices().peekable();
    let mut in_str = false;
    let mut esc = false;
    let mut text_start = text_start_for_line(body, 0);
    let mut line_start = 0usize;
    while let Some((a, c)) = chars.next() {
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            } else if c == '\n' {
                in_str = false;
                line_start = a + 1;
                text_start = text_start_for_line(body, line_start);
            }
            continue;
        }
        if c == '\n' {
            out.push(c);
            line_start = a + 1;
            text_start = text_start_for_line(body, line_start);
            continue;
        }
        // Opaque `Text` past a content line's second `:` (§4.2 exclusion 2): no
        // comment/String is recognized to EOL.
        if a >= text_start {
            out.push(c);
            continue;
        }
        // Line-leading `//` (only whitespace / blanked trivia precedes it) →
        // trivia to EOL (§4.2); mirror the strip scan so a `/*` inside a `//`
        // comment is not mistaken for an unterminated block comment.
        if c == '/'
            && matches!(chars.peek(), Some((_, '/')))
            && out[line_start..a].bytes().all(|x| x == b' ' || x == b'\t')
        {
            let line_end = body[a..].find('\n').map_or(body.len(), |n| a + n);
            for _ in a..line_end {
                out.push(' ');
            }
            while matches!(chars.peek(), Some(&(pos, _)) if pos < line_end) {
                chars.next();
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            continue;
        }
        if c == '/' && matches!(chars.peek(), Some((_, '*'))) {
            chars.next(); // consume '*'
            let mut end = None;
            while let Some((_, d)) = chars.next() {
                if d == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    let (slash, _) = chars.next().expect("peeked '/'");
                    end = Some(slash + 1); // '/' is one byte
                    break;
                }
            }
            let Some(b) = end else {
                return a; // the `/*` at `a` ran to EOF
            };
            // Blank the whole comment range in place (space per byte, `\n` kept)
            // so the blanked view keeps `out.len() == b` and can re-derive the
            // content-line boundary revealed once leading trivia is removed.
            let comment = &body.as_bytes()[a..b];
            for &byte in comment {
                out.push(if byte == b'\n' { '\n' } else { ' ' });
            }
            if let Some(nl) = comment.iter().rposition(|&x| x == b'\n') {
                line_start = a + nl + 1;
            }
            text_start = line_text_start_blanked(&out, body, line_start, b);
            continue;
        }
        out.push(c);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Node;

    #[test]
    fn classifies_set_before_generic_directive() {
        let (doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\n::set{scene.a = 1}\n");
        assert!(diags.is_empty(), "{diags:?}");
        let body = &doc.shots[0].body;
        assert!(
            matches!(body[0], Node::Set(_)),
            "::set must classify as Set, not Directive"
        );
    }

    #[test]
    fn line_text_is_opaque_to_eol() {
        let (doc, _) = parse("---\ncharacter: x\n---\n## Shot 1.\n:narrator: (a) <b> : c\n");
        if let Node::Line(l) = &doc.shots[0].body[0] {
            assert_eq!(l.text, "(a) <b> : c");
            assert_eq!(l.speaker, "narrator");
        } else {
            panic!("expected Line");
        }
    }

    #[test]
    fn unrecognized_line_is_error() {
        let (_doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\ngarbage prose\n");
        assert!(diags.iter().any(|d| d.code == "E-UNCLASSIFIED"));
    }

    #[test]
    fn bad_shot_heading_is_diagnosed() {
        for bad in [
            "## Chapter 1.",
            "## Shot .",
            "## Shot 3",
            "## Prolog",
            "## Shot1.",
            "## Shot 99999999999999999999.",
        ] {
            let (_, diags) = parse(&format!("{bad}\n:narrator: hi.\n"));
            assert!(diags.iter().any(|d| d.code == "E-SHOT-HEADING"), "{bad}");
        }
    }

    #[test]
    fn all_four_heading_keywords_parse() {
        for good in [
            "## Shot 1.",
            "## Scene 2. Title",
            "## Prologue",
            "## Epilogue tail",
            "## 프롤로그",
            "## 에필로그",
        ] {
            let (_, diags) = parse(&format!("{good}\n:narrator: hi.\n"));
            assert!(diags.is_empty(), "{good}: {diags:?}");
        }
    }

    #[test]
    fn otherwise_with_attrs_is_parse_error() {
        let src = "## Shot 1.\n<match on=\"app.rating\">\n<when test=\"$ == 'teen'\">\n:narrator: a.\n</when>\n<otherwise foo=\"bar\">\n:narrator: b.\n</otherwise>\n</match>\n";
        let (_, diags) = parse(src);
        assert!(diags
            .iter()
            .any(|d| d.code == "E-LOGIC-CONTENT" && d.message.contains("otherwise")));
    }

    #[test]
    fn attr_quote_protects_structural_chars() {
        let (doc, _) =
            parse("---\ncharacter: x\n---\n## Shot 1.\n::sfx{sound=\"a } b\" name=\"n\"}\n");
        if let Node::Directive(d) = &doc.shots[0].body[0] {
            assert_eq!(d.attrs.len(), 2);
            assert_eq!(d.attrs[0].key, "sound");
        } else {
            panic!();
        }
    }

    // -- span-fidelity: positions map through a multi-line comment ------------

    #[test]
    fn span_maps_to_original_source_through_comment() {
        // A 3-line block comment precedes the shot; the error must report the
        // ORIGINAL line, not a comment-shifted one.
        let src = "---\ncharacter: x\n---\n/*\n c\n*/\n## Shot 1.\ngarbage\n";
        let (_doc, diags) = parse(src);
        let d = diags
            .iter()
            .find(|d| d.code == "E-UNCLASSIFIED")
            .expect("unclassified diag");
        // `garbage` is line 8 of the original file (1-based).
        assert_eq!(d.span.line, 8, "diag should point at the original line 8");
        assert_eq!(&src[d.span.byte_start..d.span.byte_end], "garbage");
    }

    #[test]
    fn unterminated_comment_is_diagnosed() {
        let (_doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\n/* never ends\n");
        assert!(diags.iter().any(|d| d.code == "E-COMMENT-UNTERMINATED"));
    }

    #[test]
    fn no_unterminated_from_block_comment_inside_content_text() {
        // §4.2 exclusion 2 + blanked-view recompute: after the leading `/* p */`
        // is blanked, the boundary is recomputed to recognize the content line,
        // so its `Text` is opaque and the body `/* boom` is literal — NOT an
        // unterminated block comment. `find_unterminated_comment` reports none.
        let body = "/* p */ :bianca: a /* boom";
        assert_eq!(find_unterminated_comment(body), 0);
    }

    #[test]
    fn no_unterminated_diag_from_block_comment_inside_content_text() {
        // End-to-end: a `/*` inside a content line's opaque `Text` is literal
        // (§4.2 exclusion 2), so no E-COMMENT-UNTERMINATED is raised and the
        // `Text` keeps the `/*` verbatim. The leading `/* p */` is still blanked.
        let src = "---\ncharacter: x\n---\n## Shot 1.\n/* p */ :bianca: a /* boom\n";
        let (doc, diags) = parse(src);
        assert!(
            !diags.iter().any(|d| d.code == E_COMMENT_UNTERMINATED),
            "opaque Text must not raise E-COMMENT-UNTERMINATED: {diags:?}"
        );
        let Node::Line(l) = &doc.shots[0].body[0] else {
            panic!("expected Line")
        };
        assert!(l.text.contains("a /* boom"), "Text lost the literal `/*`: {l:?}");
    }

    #[test]
    fn escaped_backslash_attr_value_text_stays_opaque() {
        // Regression (content_text_start escape state): an attr value ending in
        // an escaped backslash must not spill the string state past `}` `:`, or a
        // `/*` in the opaque `Text` would be wrongly stripped / flagged
        // (§4.2 exclusion 2). The `Text` keeps its `/* … */` verbatim.
        let (doc, diags) = parse("## Shot 1.\n:bianca{u=\"\\\\\"}: keep /* literal */\n");
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Line(l) = &doc.shots[0].body[0] else {
            panic!("expected Line")
        };
        assert_eq!(l.text, "keep /* literal */");
    }

    #[test]
    fn attr_derived_celslot_span_bounds_raw() {
        // Regression (T2.3 review Critical): attr-derived CEL slots must have
        // span == the inner value bytes, so src[slot.span] == slot.raw (matching
        // Set slots). Otherwise Phase-3 CEL sub-diagnostics drift by key.len()+2.
        let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n<when test=\"$ == 'gold'\">\n:narrator: hi\n</when>\n<otherwise>\n:narrator: bye\n</otherwise>\n</match>\n";
        let (doc, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let slot_ok = |s: &CelSlot| {
            let got = &src[s.span.byte_start..s.span.byte_end];
            assert_eq!(got, s.raw, "src[span] must equal raw for kind {:?}", s.kind);
        };
        if let Node::Match(m) = &doc.shots[0].body[0] {
            assert_eq!(m.subject.raw, "scene.choices.number");
            slot_ok(&m.subject);
            if let Arm::When { test, .. } = &m.arms[0] {
                assert_eq!(test.raw, "$ == 'gold'");
                slot_ok(test);
            } else {
                panic!("expected When arm");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn stray_line_under_branch_is_diagnosed() {
        // §7.3: a <branch> body admits only <choice> children. A direct content line is
        // invalid structure and MUST be reported (not silently dropped), mirroring
        // the <track>/E-TIMELINE-CONTENT rule.
        let src = "---\ncharacter: x\n---\n## Shot 1.\n<branch id=\"b\">\n:narrator: stray\n<choice id=\"c\" label=\"L\">\n:narrator: ok\n</choice>\n</branch>\n";
        let (_doc, diags) = parse(src);
        assert!(
            diags.iter().any(|d| d.code == E_LOGIC_CONTENT),
            "stray content line under <branch> must be diagnosed, got {diags:?}"
        );
    }

    #[test]
    fn stray_directive_under_match_is_diagnosed() {
        // §7.3: a <match> body admits only <when>/<otherwise>. A direct ::set is
        // invalid structure and MUST be reported, not silently skipped.
        let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.x\">\n::set{scene.x = 1}\n<otherwise>\n:narrator: ok\n</otherwise>\n</match>\n";
        let (_doc, diags) = parse(src);
        assert!(
            diags.iter().any(|d| d.code == E_LOGIC_CONTENT),
            "stray ::set under <match> must be diagnosed, got {diags:?}"
        );
    }

    #[test]
    fn content_line_short_form() {
        let (doc, diags) = parse("## Shot 1.\n:bianca{code=\"0010\"}: Hello!\n:narrator: Quiet.\n");
        assert!(diags.is_empty(), "{diags:?}");
        let body = &doc.shots[0].body;
        let Node::Line(l) = &body[0] else { panic!() };
        assert_eq!(l.speaker, "bianca");
        assert_eq!(l.text, "Hello!");
        let Node::Line(n) = &body[1] else { panic!() };
        assert_eq!(n.speaker, "narrator");
    }

    #[test]
    fn legacy_line_bracket_form_is_rejected_with_fixit() {
        let (_, diags) = parse("## Shot 1.\n:line[bianca]{code=\"0010\"}: Hello!\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-UNCLASSIFIED");
        assert!(diags[0].message.contains("0.1.0"), "fix-it hint: {}", diags[0].message);
    }

    #[test]
    fn content_line_missing_second_colon_is_error() {
        let (_, diags) = parse("## Shot 1.\n:bianca no colon here\n");
        assert_eq!(diags[0].code, "E-UNCLASSIFIED");
    }

    // -- Task 3: `//` line comments + truly-opaque Text (dsl §4.2) -----------

    #[test]
    fn line_comment_leading_is_trivia() {
        let (doc, diags) = parse("## Shot 1.\n// a note\n:bianca: Hi.\n");
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(doc.shots[0].body.len(), 1);
    }

    #[test]
    fn line_comment_mid_line_is_not_a_comment() {
        // dsl §4.2: `//` only at line start; inside Text it is literal.
        let (doc, _) = parse("## Shot 1.\n:bianca: see https://example.com // really\n");
        let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
        assert!(l.text.contains("https://example.com // really"));
    }

    #[test]
    fn block_comment_not_recognized_inside_text() {
        // dsl §4.2 exclusion 2: Text is truly opaque after the second colon.
        let (doc, diags) = parse("## Shot 1.\n:bianca: I love /* this */ you.\n");
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
        assert_eq!(l.text, "I love /* this */ you.");
    }

    #[test]
    fn unterminated_block_comment_inside_text_is_fine() {
        let (_, diags) = parse("## Shot 1.\n:bianca: half /* open\n:narrator: next line intact\n");
        assert!(diags.is_empty(), "{diags:?}");
    }
}
