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
//!    normative §4.3 precedence (`## ` → `# ` → `::set{` → `::` → `@`ident →
//!    `<`tag → error). Block opens (`<branch>`/`<match>`/`<timeline>`) recurse
//!    via a per-block loop that matches the JSX self-naming close by tag name.

use crate::ast::*;
use crate::datalog::{parse_fact, DatalogError, FactPattern};
use crate::lex::{
    line_text_start_blanked, peel_frontmatter, strip_comments_checked, text_start_for_line,
    unclosed_frontmatter_end, CommentError,
};
use lute_core_span::{Diagnostic, Fixit, Layer, Severity, Span, TextEdit, TextIndex};

mod attrs;
mod blocks;
mod foreign;
mod framing;
mod line;

/// Diagnostic code: a body line matched no §4.3 rule (rule 7).
pub const E_UNCLASSIFIED: &str = "E-UNCLASSIFIED";
/// Diagnostic code: a block open (`<tag>`) had no matching `</tag>` close, so
/// the `<tag>…</tag>` bracket rule (§5) for a nesting construct (§7.3, §7.4)
/// was left open at EOF.
pub const E_UNCLOSED_TAG: &str = "E-UNCLOSED-TAG";
/// Diagnostic code: non-staging content inside a `<timeline>`/`<track>` (§7.4).
pub const E_TIMELINE_CONTENT: &str = "E-TIMELINE-CONTENT";
/// Diagnostic code: a `<branch>` body held a non-`<choice>` child, or a
/// `<match>` body a non-`<when>`/`<otherwise>` child (§7.3).
pub const E_LOGIC_CONTENT: &str = "E-LOGIC-CONTENT";
/// Diagnostic code: a `/* … */` block comment ran to EOF (§4.2).
pub const E_COMMENT_UNTERMINATED: &str = "E-COMMENT-UNTERMINATED";
/// Diagnostic code: a backslash escape in a quoted `String` value was not one
/// of the four defined escapes `\"` `\\` `\n` `\t` (§4.4). `\'` is exempted
/// because a `CelString` value (indistinguishable at the parser layer) may
/// embed a CEL single-quoted string whose own `\'` escape is well-formed.
pub const E_STRING_ESCAPE: &str = "E-STRING-ESCAPE";
/// Diagnostic code: an attribute value was single-quoted (`key='…'`). The
/// attribute grammar (§4.4/§4.5) quotes values with `"` only — a `'` there is
/// not a delimiter, so without this error the quotes would silently become
/// part of the value (a `label='"Hi."'` showed `'"Hi."'`). A `"` inside a
/// value is written `\"`.
pub const E_ATTR_QUOTE: &str = "E-ATTR-QUOTE";
/// Diagnostic code: a `::set{ path … }` whose path is not followed by an
/// assignment operator (`=`, `+=`, `-=`, `*=`) — `run.clues - 1`,
/// `run.clues 2`, `run.clues == 2` (dsl 0.27.0 §2, T1-1).
pub const E_SET_SHAPE: &str = "E-SET-SHAPE";
/// Diagnostic code: a name that is not an identifier written after a `.` in a
/// state path (`run.lab-b2`) — written as a quoted index instead,
/// `run["lab-b2"]`.
pub const E_PATH_IDENT: &str = "E-PATH-IDENT";
/// Diagnostic code: a `{{` interpolation had no closing `}}` before end of line
/// (§7.6).
pub const E_INTERP_UNTERMINATED: &str = "E-INTERP-UNTERMINATED";
/// Diagnostic code: a document `# ` title appeared after the first shot, or a
/// second `# ` title appeared. At most one title MAY precede the first shot
/// (§6.2 / I1).
pub const E_TITLE_PLACEMENT: &str = "E-TITLE-PLACEMENT";
/// Diagnostic code: an `::assert`/`::retract` payload does not parse per the
/// Appendix C fact-pattern grammar (dsl 0.3.0 §5, Appendix C / D3).
pub const E_DATALOG_PARSE: &str = "E-DATALOG-PARSE";
/// Diagnostic code: an `::assert`/`::retract` payload contains a compound/
/// function term (dsl 0.3.0 §7.1).
pub const E_DATALOG_FUNCTION: &str = "E-DATALOG-FUNCTION";
/// Diagnostic code (dsl 0.5.0 §2.1): a content-shaped line (`@speaker…`,
/// `::directive`, `<tag>`) appears before the first `## ` shot heading —
/// content belongs inside a shot body (`0.6.0 §3.3`). Split off the
/// [`E_UNCLASSIFIED`] catch-all so the message names the real problem.
pub const E_CONTENT_OUTSIDE_SHOT: &str = "E-CONTENT-OUTSIDE-SHOT";
/// Diagnostic code (dsl 0.5.0 §2.1): a content line uses `[…]` where
/// attribute braces `{…}` are expected (e.g. `@mira[emotion="x"]: …`,
/// `0.1 §7.1`). Split off the missing-second-colon path so the message names
/// the bracket-vs-brace mistake instead of a generic "needs a second `:`".
pub const E_CONTENT_LINE_BRACKET: &str = "E-CONTENT-LINE-BRACKET";
/// Diagnostic code (dsl 0.5.0 §2.1, §2.3): a `<tag …>` opener's `>`/`/>` was
/// not reached on the tag's own physical line (its attributes ran past the
/// newline). Lute's one-physical-line model (§2.3) is retained, not relaxed —
/// this NAMES the violation instead of a misleading [`E_UNCLOSED_TAG`] /
/// [`E_UNCLASSIFIED`] for the same root cause.
pub const E_TAG_NOT_ONE_LINE: &str = "E-TAG-NOT-ONE-LINE";
/// Diagnostic code (dsl 0.5.0 §2.1/§2.2): a body line uses the pre-0.2.2
/// leading-`:` content-line speaker sigil (`:speaker: text`, replaced by
/// `@speaker{…}: text` in 0.2.2, dsl §7.1). This is a precisely-recognized
/// deprecated shape with its own migration `Fixit` (`lute fix` applies it
/// automatically) — split off the [`E_UNCLASSIFIED`] catch-all so the code
/// itself names the (fixable) deprecation instead of the residual
/// "unrecognized line" bucket.
pub const E_LEGACY_CONTENT_SIGIL: &str = "E-LEGACY-CONTENT-SIGIL";
/// Diagnostic code (dsl §2.3): an element's body — and, in the worst case, its
/// matching `</tag>` close — was written on the opener's own physical line
/// (`<tag …>body</tag>`). That single-line form is deliberately **not**
/// supported: an element with children uses the block form, children on their
/// **own** lines. Distinct from [`E_TAG_NOT_ONE_LINE`], whose subject is the
/// OPENER (its `>`/attributes running past the newline) — here the opener is
/// impeccable and the BODY is misplaced, so reusing that code would tell the
/// author to fix something already correct. Naming it also denies the three
/// misdirecting diagnostics this shape used to cause — a missing-close claim
/// against a close that is right there, an "unexpected block" against a
/// well-formed sibling, and (worst) `E-NONEXHAUSTIVE` against a `<match>`
/// whose arms merely failed to parse.
pub const E_TAG_INLINE_BODY: &str = "E-TAG-INLINE-BODY";

/// Parse a `.lute` document into its AST and parse diagnostics.
///
/// Never panics: malformed structure degrades to diagnostics + best-effort AST.
pub fn parse(text: &str) -> (Document, Vec<Diagnostic>) {
    let idx = TextIndex::new(text);
    let mut diags = Vec::new();
    let frame = parse_frontmatter(text, &idx, &mut diags);
    let body = parse_body(text, frame.body_start, &idx, &mut diags);
    let mut p = initialize_parser(
        idx,
        body,
        frame.body_start,
        &frame.raw_yaml,
        diags,
    );
    let (title, shots, quests, entries, beats) = p.parse_document_inner();
    let doc = Document {
        meta: Meta {
            raw_yaml: frame.raw_yaml,
            span: frame.meta_span,
        },
        title,
        shots,
        quests,
        entries,
        beats,
        span: Span::from_bytes(&p.idx, 0, text.len()),
    };
    (doc, p.diags)
}

struct ParseFrame {
    raw_yaml: String,
    meta_span: Span,
    body_start: usize,
}

fn parse_frontmatter(
    text: &str,
    idx: &TextIndex,
    diags: &mut Vec<Diagnostic>,
) -> ParseFrame {
    let (mut fm, mut body_start) = peel_frontmatter(text).unwrap_or((None, 0));
    if let Some(open) = unclosed_frontmatter_end(text) {
        let at = Span::from_bytes(idx, 0, 3);
        let (end, message, fix) = match &open.near_fence {
            Some((range, line, written)) => (
                range.start,
                format!(
                    "the frontmatter opened on line 1 is never closed — `{}` on line {line} is \
                     not a closing fence, which is exactly `---`",
                    written.trim()
                ),
                (
                    Span::from_bytes(idx, range.start, range.end),
                    "---".to_string(),
                ),
            ),
            None => {
                let lead = if text[..open.end].ends_with('\n') {
                    ""
                } else {
                    "\n"
                };
                (
                    open.end,
                    format!(
                        "the frontmatter opened on line 1 is never closed — add a `---` line \
                         after line {}",
                        open.last_line
                    ),
                    (
                        Span::from_bytes(idx, open.end, open.end),
                        format!("{lead}---\n"),
                    ),
                )
            }
        };
        diags.push(Diagnostic {
            code: "E-META-PARSE".into(),
            severity: Severity::Error,
            message,
            evidence: None,
            span: at,
            layer: Layer::Content,
            fixits: vec![Fixit {
                title: "Close the frontmatter with `---`".to_string(),
                kind: "quickfix".to_string(),
                edit: vec![TextEdit {
                    span: fix.0,
                    new_text: fix.1,
                }],
                confidence: 90,
            }],
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
        let body_at = open.near_fence.as_ref().map_or(end, |(range, _, _)| {
            text[range.end..]
                .find('\n')
                .map_or(text.len(), |n| range.end + n + 1)
        });
        let span = Span {
            byte_start: 0,
            byte_end: body_at,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        };
        fm = Some((text[4..end].to_string(), span));
        body_start = body_at;
    }
    let (raw_yaml, meta_span) = match fm {
        Some((yaml, span)) => (yaml, span),
        None => (String::new(), framing::zero_span()),
    };
    ParseFrame {
        raw_yaml,
        meta_span,
        body_start,
    }
}

fn parse_body(
    text: &str,
    body_start: usize,
    idx: &TextIndex,
    diags: &mut Vec<Diagnostic>,
) -> String {
    let body_slice = &text[body_start..];
    match strip_comments_checked(body_slice) {
        Ok(stripped) => stripped,
        Err(CommentError::Unterminated) => {
            let pos = find_unterminated_comment(body_slice);
            diags.push(Diagnostic {
                code: E_COMMENT_UNTERMINATED.into(),
                severity: Severity::Error,
                message: "unterminated `/* … */` block comment".into(),
                evidence: None,
                span: Span::from_bytes(idx, body_start + pos, text.len()),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            body_slice.to_string()
        }
    }
}

fn initialize_parser<'a>(
    idx: TextIndex<'a>,
    body: String,
    body_start: usize,
    raw_yaml: &str,
    diags: Vec<Diagnostic>,
) -> Parser<'a> {
    let lines = split_lines(&body);
    Parser {
        idx,
        body,
        body_start,
        lines,
        cursor: 0,
        diags,
        doc_kind: framing::frontmatter_scalar(raw_yaml, "kind"),
        template_component: framing::template_component(raw_yaml),
        top_block: None,
        hoisted: Vec::new(),
        open_blocks: Vec::new(),
    }
}
/// Parse a standalone sequence of shot-body constructs.
///
/// This is crate-internal because the stable public fragment API is
/// [`crate::incremental::IncrementalContinuationParser`]. Unlike [`parse`],
/// this helper does not synthesize a title, shot, or document around a
/// fragment: doing so would turn non-canonical continuation input into a
/// canonical artifact and would shift every source span.
pub(crate) fn parse_body_fragment(text: &str) -> (Vec<Node>, Vec<Diagnostic>) {
    let idx = TextIndex::new(text);
    let mut diags = Vec::new();
    let body = match strip_comments_checked(text) {
        Ok(stripped) => stripped,
        Err(CommentError::Unterminated) => {
            let pos = find_unterminated_comment(text);
            diags.push(Diagnostic {
                code: E_COMMENT_UNTERMINATED.into(),
                severity: Severity::Error,
                message: "unterminated `/* … */` block comment".into(),
                evidence: None,
                span: Span::from_bytes(&idx, pos, text.len()),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            text.to_string()
        }
    };
    let lines = split_lines(&body);
    let mut parser = Parser {
        idx,
        body,
        body_start: 0,
        lines,
        cursor: 0,
        diags,
        doc_kind: None,
        template_component: None,
        top_block: None,
        hoisted: Vec::new(),
        open_blocks: Vec::new(),
    };
    let nodes = parser.parse_shot_body();
    (nodes, parser.diags)
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
    /// The frontmatter's literal `kind:` value, when written — only for
    /// kind-aware recovery messages (`lore`/`quest` bodies have no shots).
    doc_kind: Option<String>,
    /// A component that declares a `beat:` header template: its name. Its
    /// body is a beat body, written without a shot — content before any
    /// `## ` heading is a shot of its own, headed by that name.
    template_component: Option<String>,
    /// The top-level block (`<entry>` / `<beat>` / `<quest>`) whose body is
    /// being parsed — what a top-level opener nested inside it names
    /// (lamplight F23).
    top_block: Option<TopBlock>,
    /// Top-level blocks opened inside another one by mistake, parsed as its
    /// siblings; `parse_document_inner` files them after the outer block.
    hoisted: Vec<Hoisted>,
    /// Every block element whose body is being parsed, outermost first: its
    /// tag name and the 1-based line of its opener. A `</tag>` either names
    /// one of them — it ends every block opened inside that one — or none,
    /// and then it is a stray close reported against the block that IS open.
    open_blocks: Vec<(String, u32)>,
}

/// An open top-level block (see [`Parser::top_block`]).
pub(crate) struct TopBlock {
    tag: &'static str,
    id: String,
    /// 1-based line of its opener.
    line: u32,
    /// A nested opener was already reported against it (one error per
    /// unclosed block, however many siblings follow).
    reported: bool,
}

/// A top-level block parsed while another one was still open.
pub(crate) enum Hoisted {
    Entry(Entry),
    Beat(BundleBeat),
    Quest(Quest),
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
            evidence: None,
            span: self.span_o(start, end),
            layer,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }

    /// Emit a diagnostic spanning the content of line `i`.
    fn emit_line(&mut self, code: &str, msg: &str, i: usize, layer: Layer) {
        let a = self.orig(self.line_content_start(i));
        let b = self.orig(self.line_content_end(i));
        self.emit_o(code, msg.to_string(), a, b, layer);
    }

    /// Emit the residual [`E_UNCLASSIFIED`] "unrecognized line" catch-all
    /// (dsl 0.5.0 §2.1), with the most specific pointer the line allows: an
    /// Ink/Yarn shape names what Lute writes instead ([`foreign::foreign_line`],
    /// round-6 T3-60); a line that reads as the wrapped tail of the content
    /// line / `<tag` above gets the §2.3 one-physical-line note; other text
    /// is a line missing its `@speaker:` head ([`foreign::speakerless`]).
    fn emit_unclassified(&mut self, i: usize, layer: Layer) {
        let line = self.trimmed(i);
        let msg = if let Some(hint) = foreign::foreign_line(&line) {
            format!("unrecognized line: {hint}")
        } else if i > 0 && foreign::continues(&self.trimmed(i - 1), &line) {
            "unrecognized line (note: a content line / tag cannot span multiple physical lines)"
                .to_string()
        } else if let Some(hint) = foreign::speakerless(&line) {
            format!("unrecognized line: {hint}")
        } else {
            "unrecognized line".to_string()
        };
        self.emit_line(E_UNCLASSIFIED, &msg, i, layer);
    }

    // -- top-level document ----------------------------------------------------

    fn skip_blanks(&mut self) {
        while self.cursor < self.lines.len() && self.trimmed(self.cursor).is_empty() {
            self.cursor += 1;
        }
    }

    #[allow(clippy::type_complexity)]
    fn parse_document_inner(
        &mut self,
    ) -> (
        Option<(String, Span)>,
        Vec<Shot>,
        Vec<Quest>,
        Vec<Entry>,
        Vec<BundleBeat>,
    ) {
        let mut title = None;
        let mut shots = Vec::new();
        let mut quests = Vec::new();
        let mut entries = Vec::new();
        let mut beats = Vec::new();
        // One report per region of content outside every block: the first
        // content-shaped line is reported, and the rest of the region (its
        // other content lines and the closes that pair with them) is folded
        // into that one diagnostic. `(diag index, first line, last line)`.
        let mut outside: Option<(usize, usize, usize)> = None;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            let opens_block = trimmed.starts_with("## ")
                || trimmed.starts_with("# ")
                || (trimmed.starts_with('<')
                    && matches!(
                        open_tag_name(&trimmed).as_deref(),
                        Some("quest" | "entry" | "beat")
                    ));
            if opens_block {
                self.close_outside_region(outside.take());
            } else if outside.is_some()
                && (trimmed.starts_with("</") || is_content_shaped_line(&trimmed))
            {
                if let Some((_, _, last)) = outside.as_mut() {
                    *last = self.cursor;
                }
                self.cursor += 1;
                continue;
            }
            if trimmed.starts_with("## ") {
                shots.push(self.parse_shot());
            } else if trimmed.starts_with('<')
                && open_tag_name(&trimmed).as_deref() == Some("quest")
            {
                quests.push(self.parse_quest());
            } else if trimmed.starts_with('<')
                && open_tag_name(&trimmed).as_deref() == Some("entry")
            {
                entries.push(self.parse_entry());
            } else if trimmed.starts_with('<') && open_tag_name(&trimmed).as_deref() == Some("beat")
            {
                beats.push(self.parse_bundle_beat());
            } else if trimmed.starts_with("# ") && shots.is_empty() && title.is_none() {
                title = Some(self.parse_title());
            } else if trimmed.starts_with("# ") {
                // §6.2/I1: a `# ` title is well-placed only once, before the
                // first shot. Reaching here means the slot is taken (a second
                // title) or a shot already opened (a late title).
                self.emit_line(
                    E_TITLE_PLACEMENT,
                    "document title must appear at most once, before the first shot (dsl §6.2)",
                    self.cursor,
                    Layer::Content,
                );
                self.cursor += 1;
            } else if trimmed.starts_with("</") {
                // RC1 (dsl 0.5.0 §2.1): a top-level stray `</tag>` close is
                // never content — mirror `parse_shot_body`'s in-shot handling
                // (an unmatched close is always `E-UNCLOSED-TAG`, not
                // `E-CONTENT-OUTSIDE-SHOT`).
                self.report_stray_close();
                self.cursor += 1;
            } else if let Some(name) = self
                .template_component
                .clone()
                .filter(|_| is_content_shaped_line(&trimmed))
            {
                // A beat template's body is a beat body: it needs no shot.
                shots.push(self.parse_headless_shot(name));
            } else if is_content_shaped_line(&trimmed) {
                // dsl 0.5.0 §2.1: a content-shaped line reached here only
                // because no shot/scene is currently open (this loop never
                // sees a line consumed by `parse_shot_body`) — it belongs
                // inside a shot/scene body, not at document top level. A
                // lore or quest document has no shots (lamplight F23): the
                // advice names its own block instead of a `## ` heading.
                let msg = match self.doc_kind.as_deref() {
                    Some("lore") => {
                        "content in a lore document lives inside an `<entry>` or `<beat>` block"
                    }
                    Some("quest") => "content in a quest document lives inside a `<quest>` block",
                    _ => "content lives inside a shot; add a `## <title>` heading above it",
                };
                outside = Some((self.diags.len(), self.cursor, self.cursor));
                self.emit_line(E_CONTENT_OUTSIDE_SHOT, msg, self.cursor, Layer::Content);
                self.cursor += 1;
            } else {
                self.emit_unclassified(self.cursor, Layer::Content);
                self.cursor += 1;
            }
            // lamplight F23: blocks opened inside the one just parsed are
            // its siblings.
            for h in std::mem::take(&mut self.hoisted) {
                match h {
                    Hoisted::Entry(e) => entries.push(e),
                    Hoisted::Beat(b) => beats.push(b),
                    Hoisted::Quest(q) => quests.push(q),
                }
            }
        }
        self.close_outside_region(outside);
        (title, shots, quests, entries, beats)
    }

    /// Widen a region's one `E-CONTENT-OUTSIDE-SHOT` over every line folded
    /// into it and name that extent.
    fn close_outside_region(&mut self, region: Option<(usize, usize, usize)>) {
        let Some((at, first, last)) = region else {
            return;
        };
        if last == first {
            return;
        }
        let end = self.orig(self.line_content_end(last));
        let last_line = self.span_o(end, end).line;
        let (start, first_line) = (self.diags[at].span.byte_start, self.diags[at].span.line);
        let span = self.span_o(start, end);
        let d = &mut self.diags[at];
        d.message = format!("{} (this covers lines {first_line}–{last_line})", d.message);
        d.span = span;
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

    /// `ShotBlock ::= ShotHeading Node*`. Consumes the heading line then every
    /// body node up to the next `## ` heading or EOF.
    ///
    /// dsl 0.6.0 §3.1: `ShotHeading ::= "## " Text` (Text non-empty after
    /// trimming). The heading is an opaque title — the `Shot|Scene <int>.`/
    /// bookend grammar, `HeadingKind` classification, authored numbers, and
    /// `E-SHOT-HEADING` are all removed (shots are numbered by document order,
    /// §3.2). The non-empty guarantee is structural, not enforced here: the
    /// `## ` detector runs on the trimmed line (which carries no trailing
    /// whitespace), so a bare `## ` never matches and no empty-title shot forms.
    fn parse_shot(&mut self) -> Shot {
        let i = self.cursor;
        let cstart = self.line_content_start(i);
        let head_end = self.line_content_end(i);
        let full = self.trimmed(i);
        let heading = full.strip_prefix("## ").unwrap_or(&full).trim().to_string();
        let start_o = self.orig(cstart);
        let head_end_o = self.orig(head_end);
        self.cursor += 1;
        let body = self.parse_shot_body();
        let end_o = body.last().map(node_end).unwrap_or(head_end_o);
        Shot {
            heading,
            body,
            span: self.span_o(start_o, end_o),
        }
    }

    /// A beat template's body written without a `## ` heading: the shot
    /// runs from the content line at `cursor` to the next heading, headed
    /// by the component's name (`heading`).
    fn parse_headless_shot(&mut self, heading: String) -> Shot {
        let start_o = self.orig(self.line_content_start(self.cursor));
        let body = self.parse_shot_body();
        let end_o = body.last().map(node_end).unwrap_or(start_o);
        Shot {
            heading,
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
                self.report_stray_close();
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
        if trimmed.starts_with("::assert{") {
            return Some(self.parse_fact_directive(false));
        }
        if trimmed.starts_with("::retract{") {
            return Some(self.parse_fact_directive(true));
        }
        if trimmed.starts_with("::set{") {
            return self.parse_set();
        }
        if trimmed.starts_with("::") {
            return Some(self.parse_directive());
        }
        // dsl §4.3 rule 5: content line. The sigil is `@` (0.2.2, foundation
        // C1); a lone leading `:` here is the sigil removed in 0.2.2 (through
        // 0.2.1), OR the older `:line[speaker]` bracket form (0.0.1) — both
        // route into `parse_line`, which accepts `@ident` as a real `Line`
        // and rejects any `:`-led shape with a `migrate` fix-it (dsl §7.1) so
        // `lute fix` (Task C3) can bulk-migrate a whole pre-0.2.2 document.
        // `::` rules already matched above, so a lone `:` here is never `::`.
        // dsl 0.26.0 §3.2: `@@who:` speaks as a component's `speaker` param.
        if is_line_head(&trimmed) {
            return self.parse_line();
        }
        if trimmed.starts_with('<') {
            match open_tag_name(&trimmed).as_deref() {
                Some("branch") => return Some(Node::Branch(self.parse_branch())),
                Some("match") => return Some(Node::Match(self.parse_match())),
                Some("timeline") => return Some(Node::Timeline(self.parse_timeline())),
                Some("hub") => return Some(Node::Hub(self.parse_hub())),
                Some("on") => return Some(Node::On(self.parse_on())),
                Some("objective") => return Some(Node::Objective(self.parse_objective())),
                Some(tag @ ("entry" | "beat" | "quest")) if self.top_block.is_some() => {
                    self.parse_nested_top_block(tag);
                    return None;
                }
                Some(tag @ ("choice" | "when" | "otherwise" | "track" | "reward" | "return")) => {
                    self.parse_misplaced_child(tag);
                    return None;
                }
                other => {
                    let msg = match other {
                        Some(tag @ ("entry" | "beat" | "quest")) => format!(
                            "a `<{tag}>` block belongs at the top level of the document, not \
                             inside a shot"
                        ),
                        // round-6 T3-60: a Yarn `<<command>>`, an Ink thread
                        // `<- knot` or glue `<>` names its Lute form.
                        Some(tag) => foreign::foreign_line(&trimmed).map_or_else(
                            || format!("unexpected `<{tag}>` block here"),
                            |hint| format!("unrecognized line: {hint}"),
                        ),
                        None => foreign::foreign_line(&trimmed).map_or_else(
                            || "unexpected block here".to_string(),
                            |hint| format!("unrecognized line: {hint}"),
                        ),
                    };
                    self.emit_line(E_UNCLASSIFIED, &msg, self.cursor, Layer::Logic);
                    self.cursor += 1;
                    return None;
                }
            }
        }
        if trimmed.starts_with("# ") {
            // §6.2/I1: a `# ` H1 title inside a shot body is a misplaced title,
            // not a generic unclassified line. (`## ` shot headings never reach
            // here — parse_shot_body breaks on them.) Round-6 T3-60: Ink writes
            // a tag line this way, so say what `#` is not.
            self.emit_line(
                E_TITLE_PLACEMENT,
                "document title must appear at most once, before the first shot; inside a shot \
                 `#` starts neither a comment nor a tag (a comment is `// …` on its own line)",
                self.cursor,
                Layer::Content,
            );
            self.cursor += 1;
            return None;
        }
        self.emit_unclassified(self.cursor, Layer::Content);
        self.cursor += 1;
        None
    }

    // -- leaf nodes ------------------------------------------------------------

    /// `Directive ::= "::" Ident Attrs?` (§7.2). Layer = Staging.
    ///
    /// dsl 0.12.0 §…: `::next{to when?}`'s `when` is extracted into a typed
    /// CEL slot the SAME way `Line.when`/`Choice.when` are (`take_cel`).
    /// dsl 0.26.0 §4: every directive's `when=` is its guard, so it is
    /// extracted for every tag; which directives may carry one is the
    /// checker's call (a `<track>` clip never keeps one, `parse_track`).
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
        let (mut attrs, end) = if j < e && b[j] == b'{' {
            let (attrs, after) = self.scan_attrs(j + 1, b'}');
            (attrs, after)
        } else {
            (Vec::new(), j)
        };
        let when = attrs::take_cel(&mut attrs, "when", CelKind::Condition);
        let span = self.span(cstart, end);
        self.cursor += 1;
        Node::Directive(Directive {
            tag,
            attrs,
            when,
            span,
        })
    }

    /// `Set ::= "::set{" Path WS AssignOp WS CelExpr (WS "when=" Quoted)? "}"`
    /// (§7.3.4; dsl 0.24.0 §1 adds the trailing guard). Layer = Logic.
    fn parse_set(&mut self) -> Option<Node> {
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
        // The path as named: dotted segments and quoted indexes
        // (`run.visits["lab-b2"]`) are one path, canonical with `.`.
        let mut canon = String::new();
        // The first name written after a `.` that is not an identifier
        // (`run.lab-b2`): the path is still the one meant, but a condition
        // cannot spell it so, and neither does a `::set`.
        let mut glued: Option<String> = None;
        loop {
            let run_start = j;
            // A `-` joins the path only inside a segment — `run.clues-found`,
            // `run.lab-b2`, `run.zero-coke-001 = 1`. Before a lone number,
            // `=`, a space or the end it is the author's operator:
            // `run.clues-1` is `run.clues -= 1` meant (FS-F3), `run.clues-=1`
            // is `run.clues -= 1`.
            while j < n
                && (ib[j] == b'.'
                    || (is_ident_byte(ib[j]) && (ib[j] != b'-' || set_hyphen_joins(inner, j))))
            {
                j += 1;
            }
            let run = &inner[run_start..j];
            if glued.is_none() {
                glued = run
                    .split('.')
                    .enumerate()
                    .find(|&(k, seg)| {
                        (k > 0 || run_start > path_start)
                            && !seg.is_empty()
                            && !lute_manifest::ident::is_ident(seg)
                    })
                    .map(|(_, seg)| seg.to_string());
            }
            canon.push_str(run);
            match crate::path::read_index(&inner[j..]) {
                Some((name, len)) if j > path_start => {
                    canon.push('.');
                    canon.push_str(&name);
                    j += len;
                }
                _ => break,
            }
        }
        let named_end = j;
        let glued_msg = glued.map(|name| {
            let segs: Vec<&str> = canon.split('.').collect();
            crate::path::glued_message(&inner[path_start..named_end], &name, &segs)
        });
        // dsl 0.24.0 §3/§4: `run.approval[@who]` — a `per:` family member
        // chosen by a component param, bound to `run.approval.<arg>` at each
        // `::use`. Only the `[@ident]` form joins the path.
        if ib.get(j) == Some(&b'[') && ib.get(j + 1) == Some(&b'@') {
            let mut k = j + 2;
            while k < n && is_ident_byte(ib[k]) {
                k += 1;
            }
            if k > j + 2 && ib.get(k) == Some(&b']') {
                canon.push_str(&inner[j..k + 1]);
                j = k + 1;
            }
        }
        // dsl 0.28.0 §3: `run.count[occasion.target]` — the member a kind or
        // `for=` beat runs for, bound when the write executes. The index
        // joins the path exactly as spelled; the checker judges it per member.
        const TARGET_INDEX: &str = "[occasion.target]";
        // Any other bracket index (`run.count[cod]`) is one `E-SET-SHAPE`
        // naming the quoted member, recovered as that path so the leftover
        // `[…]` does not cascade into operator/expression errors (with no
        // node at all when the index names no member).
        let mut literal_index: Option<(String, Option<(String, String)>)> = None;
        if inner[j..].starts_with(TARGET_INDEX) {
            canon.push_str(TARGET_INDEX);
            j += TARGET_INDEX.len();
        } else if j > path_start && ib.get(j) == Some(&b'[') && ib.get(j + 1) != Some(&b'@') {
            if let Some(close) = inner[j..].find(']') {
                let key = inner[j + 1..j + close].trim();
                let member = (!key.is_empty()
                    && key
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
                .then(|| {
                    (
                        format!("{canon}.{key}"),
                        format!("{}[\"{key}\"]", &inner[path_start..j]),
                    )
                });
                literal_index = Some((inner[path_start..j + close + 1].to_string(), member));
                j += close + 1;
            }
        }
        // `run.aff.@who` — a param as a dotted segment. Only `run.aff[@who]`
        // indexes a family; recover as that path so the error does not
        // cascade into `E-UNDECLARED run.aff.` / `E-CEL-PARSE`.
        let mut param_segment: Option<String> = None;
        if j > path_start && ib[j - 1] == b'.' && ib.get(j) == Some(&b'@') {
            let mut k = j + 1;
            while k < n && is_ident_byte(ib[k]) {
                k += 1;
            }
            if k > j + 1 {
                param_segment = Some(format!("{}[{}]", canon.trim_end_matches('.'), &inner[j..k]));
                j = k;
            }
        }
        let path_end = j;
        let path = param_segment
            .clone()
            .or_else(|| {
                literal_index
                    .as_ref()
                    .and_then(|(_, member)| member.as_ref().map(|(canon, _)| canon.clone()))
            })
            .unwrap_or(canon);
        let path_span = self.span(inner_start + path_start, inner_start + path_end);
        while j < n && (ib[j] == b' ' || ib[j] == b'\t') {
            j += 1;
        }
        let op_start = j;
        let rest = &inner[j..];
        // dsl 0.27.0 §2 (T1-1): only `=`, `+=`, `-=` and `*=` assign. Anything
        // else is `E-SET-SHAPE` — never a silent `=` that eats the author's
        // operator (`run.clues - 1` became `run.clues = 1`). The recovered node
        // takes the operator the author most likely meant, so the error does
        // not cascade into type errors on the leftover expression.
        // (op, bytes consumed, Some(guess) when the shape is wrong)
        let (op, skip, shape_err): (&str, usize, Option<Option<&str>>) = if rest.starts_with("+=") {
            ("+=", 2, None)
        } else if rest.starts_with("-=") {
            ("-=", 2, None)
        } else if rest.starts_with("*=") {
            ("*=", 2, None)
        } else if rest.starts_with("==") {
            ("=", 2, Some(Some("=")))
        } else if rest.starts_with("=+") {
            // `=+ 1` (the operator's halves swapped); `=-1` is `= -1`.
            ("+=", 2, Some(Some("+=")))
        } else if rest.starts_with('=') {
            ("=", 1, None)
        } else if rest.starts_with("++") {
            ("+=", 2, Some(Some("+=")))
        } else if rest.starts_with("--") {
            ("-=", 2, Some(Some("-=")))
        } else if rest.starts_with('+') {
            ("+=", 1, Some(Some("+=")))
        } else if rest.starts_with('-') {
            ("-=", 1, Some(Some("-=")))
        } else if rest.starts_with('*') {
            ("*=", 1, Some(Some("*=")))
        } else if rest.starts_with(':') {
            // `run.clues: 4` — the YAML habit.
            ("=", 1, Some(Some("=")))
        } else {
            ("=", 0, Some(None))
        };
        // `run.clues++` / `run.clues--` step by one.
        let step = rest.starts_with("++") || rest.starts_with("--");
        j += skip;
        while j < n && (ib[j] == b' ' || ib[j] == b'\t') {
            j += 1;
        }
        let expr_start = j;
        let tail = &inner[expr_start..];
        let (expr_len, when) = match split_set_when(tail) {
            Some((expr_len, q_open, q_close)) => {
                let (a, b) = (
                    inner_start + expr_start + q_open,
                    inner_start + expr_start + q_close,
                );
                let slot = CelSlot::raw(
                    CelKind::Condition,
                    self.body[a..b].to_string(),
                    self.span(a, b),
                );
                (expr_len, Some(slot))
            }
            None => (tail.trim_end().len(), None),
        };
        let expr_end = expr_start + expr_len;
        let expr = CelSlot::raw(
            CelKind::SetExpr,
            inner[expr_start..expr_end].to_string(),
            self.span(inner_start + expr_start, inner_start + expr_end),
        );
        // `::set{ add 1 to run.cluesFound }`: the text does not open with a
        // state path and an operator (or a value), so any operator guess
        // (`add = 1 to …`) would be invented. Name the shape once and emit no
        // node, so the leftover words do not cascade into `E-UNDECLARED` /
        // `E-CEL-PARSE`.
        let first_word_is_bare = shape_err == Some(None)
            && expr.raw.split_whitespace().next().is_some_and(|w| {
                w.bytes().all(|c| c.is_ascii_alphabetic() || c == b'_')
                    && !matches!(w, "true" | "false" | "null")
            });
        if shape_err.is_some() && (!path.contains('.') || first_word_is_bare) {
            let (a, b) = (
                self.orig(inner_start + path_start),
                self.orig(inner_start + expr_end.max(op_start)),
            );
            self.emit_o(
                E_SET_SHAPE,
                "`::set` takes `<path> <op> <value>`, e.g. `run.cluesFound += 1`".to_string(),
                a,
                b,
                Layer::Logic,
            );
            self.cursor += 1;
            return None;
        }
        // The path as the author spells it, for the hints below.
        let shown = match &literal_index {
            Some((_, Some((_, quoted)))) => quoted.clone(),
            _ => inner[path_start..path_end].to_string(),
        };
        if let Some(guess) = shape_err {
            let value = match expr.raw.trim() {
                "" if step => "1",
                value => value,
            };
            let hint = match (guess, value.is_empty()) {
                (Some(g), true) => format!(" with no value — write `{shown} {g} <value>`"),
                (None, true) => format!(" — write `{shown} = <value>`"),
                (Some(g), false) => format!(" — did you mean `{shown} {g} {value}`?"),
                (None, false) => format!(" — did you mean `{shown} = {value}`?"),
            };
            let end = expr_end.max(op_start);
            let written = self.body[inner_start + op_start..inner_start + end].trim();
            let found = if written.is_empty() {
                "nothing".to_string()
            } else {
                format!("`{written}`")
            };
            let msg = format!(
                "`::set` needs an assignment operator after `{shown}` — `=` (replace), `+=` \
                 (add), `-=` (subtract) or `*=` (multiply) — but found {found}{hint}"
            );
            let (a, b) = (
                self.orig(inner_start + op_start),
                self.orig(inner_start + end),
            );
            self.emit_o(E_SET_SHAPE, msg, a, b, Layer::Logic);
        }
        if param_segment.is_some() {
            let written = self.body[inner_start + path_start..inner_start + path_end].to_string();
            let msg = format!(
                "`{written}`: a path segment cannot be a param — index the family: `{path}`"
            );
            let (a, b) = (
                self.orig(inner_start + path_start),
                self.orig(inner_start + path_end),
            );
            self.emit_o(E_SET_SHAPE, msg, a, b, Layer::Logic);
        }
        if let Some((written, member)) = &literal_index {
            let fix = member
                .as_ref()
                .map(|(_, quoted)| format!("quote the member's name, `{quoted}`; "))
                .unwrap_or_default();
            let msg = format!(
                "`{written}`: {fix}an unquoted index in a `::set` path is only \
                 `[occasion.target]` (in a beat or entry that targets a kind or runs for each \
                 member of one) or a component's `[@param]`"
            );
            let (a, b) = (
                self.orig(inner_start + path_start),
                self.orig(inner_start + path_end),
            );
            self.emit_o(E_SET_SHAPE, msg, a, b, Layer::Logic);
            if member.is_none() {
                self.cursor += 1;
                return None;
            }
        }
        if let Some(msg) = glued_msg {
            let (a, b) = (
                self.orig(inner_start + path_start),
                self.orig(inner_start + named_end),
            );
            self.emit_o(E_PATH_IDENT, msg, a, b, Layer::Logic);
        }
        let span = self.span(cstart, node_end);
        self.cursor += 1;
        Some(Node::Set(Set {
            path,
            path_span,
            op: op.to_string(),
            expr,
            span,
            when,
        }))
    }

    /// `Assert ::= "::assert{" FactPattern "}"` / `Retract ::= "::retract{"
    /// FactPattern "}"` (dsl 0.3.0 §5, Appendix C). Layer = Logic. The payload
    /// is parsed by the ONE shared Datalog grammar (`crate::datalog::parse_fact`,
    /// 0.3.0 T1) — NOT the `key="value"` attr form `::set`/`::directive` use.
    /// A parse failure still produces a node carrying the D13 empty-relation
    /// sentinel (`pattern.relation == ""`) so every downstream consumer skips
    /// an already-diagnosed pattern in exactly one place. Wildcard legality
    /// (`_` in `::assert`) is NOT checked here — that's the checker's job
    /// (0.3.0 T10); the parser accepts `_` in both directives. dsl 0.26.0 §4:
    /// a trailing `when="…"` after the pattern is the write's guard (split
    /// exactly like `::set`'s, `split_set_when`).
    fn parse_fact_directive(&mut self, retract: bool) -> Node {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + leading_ws(&self.body[s..e]);
        let prefix_len = if retract {
            "::retract".len()
        } else {
            "::assert".len()
        };
        let open = cstart + prefix_len; // at '{'
        let close = self.find_matching_brace(open);
        self.cursor += 1;

        let Some(close) = close else {
            self.emit_line(
                E_DATALOG_PARSE,
                "malformed fact pattern: missing closing `}` (dsl 0.3.0 §5, Appendix C)",
                i,
                Layer::Logic,
            );
            let span = self.span(cstart, e);
            return build_fact_node(
                retract,
                sentinel_fact_pattern(),
                self.orig(open + 1),
                String::new(),
                span,
                None,
            );
        };

        let inner_start = open + 1;
        let (inner_end, when) = match split_set_when(&self.body[inner_start..close]) {
            Some((expr_len, q_open, q_close)) => {
                let (a, b) = (inner_start + q_open, inner_start + q_close);
                let slot = CelSlot::raw(
                    CelKind::Condition,
                    self.body[a..b].to_string(),
                    self.span(a, b),
                );
                (inner_start + expr_len, Some(slot))
            }
            None => (close, None),
        };
        let inner = &self.body[inner_start..inner_end];
        let trim_lead = leading_ws(inner);
        let raw = inner.trim().to_string();
        let base = inner_start + trim_lead; // body-relative start of `raw`
        let pattern_base = self.orig(base);
        let span = self.span(cstart, close + 1);

        let pattern = match parse_fact(&raw) {
            Ok(p) => p,
            Err(DatalogError::FunctionTerm { at, name }) => {
                self.emit_o(
                    E_DATALOG_FUNCTION,
                    format!(
                        "`{name}(…)` — function/compound terms are not Datalog; terms are declared constants or (in rules) variables (dsl 0.3.0 §7.1)"
                    ),
                    self.orig(base + at),
                    self.orig(base + at + name.len().max(1)),
                    Layer::Logic,
                );
                sentinel_fact_pattern()
            }
            Err(DatalogError::Malformed { at, msg }) => {
                self.emit_o(
                    E_DATALOG_PARSE,
                    format!("malformed fact pattern: {msg} (dsl 0.3.0 §5, Appendix C)"),
                    self.orig(base + at),
                    self.orig(base + at),
                    Layer::Logic,
                );
                sentinel_fact_pattern()
            }
        };

        build_fact_node(retract, pattern, pattern_base, raw, span, when)
    }


}
// -- free helpers -------------------------------------------------------------


/// Whether the `-` at `at` in a `::set` body joins its path segment
/// (`run.lab-b2`, `run.zero-coke-001 = 1`) rather than opening the author's
/// operator (`run.clues-1` for `-= 1`, `run.clues-=1`). Before a letter or
/// `_` it joins; before a number it joins only when the path goes on, or an
/// assignment operator follows the number.
fn set_hyphen_joins(inner: &str, at: usize) -> bool {
    let b = inner.as_bytes();
    match b.get(at + 1) {
        Some(c) if c.is_ascii_alphabetic() || *c == b'_' => true,
        Some(c) if c.is_ascii_digit() => {
            let end = at
                + 1
                + b[at + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
            match b.get(end) {
                Some(c) if is_ident_byte(*c) || matches!(c, b'.' | b'[') => true,
                _ => {
                    let rest = inner[end..].trim_start();
                    rest.starts_with('=') && !rest.starts_with("==")
                        || ["+=", "-=", "*="].iter().any(|op| rest.starts_with(op))
                }
            }
        }
        _ => false,
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

/// dsl 0.24.0 §1: split a `::set` tail (`<expr> when="<cel>"`) at its
/// trailing guard. Returns `(expr_len, q_open, q_close)` — the trimmed
/// expression's length and the byte range of the guard text between its
/// double quotes, all relative to `tail` — or `None` when the tail does not
/// END in a well-formed `when="…"`. The scan skips quoted CEL strings (so a
/// `"when=…"` string literal in the expression never splits) and requires a
/// non-empty expression before whitespace-separated `when`. A malformed or
/// non-trailing `when=` stays part of the expression, where the CEL parse
/// reports it.
fn split_set_when(tail: &str) -> Option<(usize, usize, usize)> {
    let b = tail.as_bytes();
    let mut quote: Option<u8> = None;
    let mut k = 0;
    while k < b.len() {
        let c = b[k];
        if let Some(q) = quote {
            if c == b'\\' {
                k += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            k += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = Some(c);
            k += 1;
            continue;
        }
        if k > 0 && (b[k - 1] == b' ' || b[k - 1] == b'\t') && tail[k..].starts_with("when") {
            let mut m = k + "when".len();
            while m < b.len() && (b[m] == b' ' || b[m] == b'\t') {
                m += 1;
            }
            if m < b.len() && b[m] == b'=' {
                m += 1;
                while m < b.len() && (b[m] == b' ' || b[m] == b'\t') {
                    m += 1;
                }
                if m < b.len() && b[m] == b'"' {
                    let q_open = m + 1;
                    if let Some(rel) = tail[q_open..].find('"') {
                        let q_close = q_open + rel;
                        let expr_len = tail[..k].trim_end().len();
                        if tail[q_close + 1..].trim().is_empty() && expr_len > 0 {
                            return Some((expr_len, q_open, q_close));
                        }
                    }
                }
            }
        }
        k += 1;
    }
    None
}

/// The D13 malformed-parse sentinel: an empty-relation [`FactPattern`] every
/// downstream consumer (checker, compiler) recognizes as "already diagnosed,
/// skip".
fn sentinel_fact_pattern() -> FactPattern {
    FactPattern {
        relation: String::new(),
        relation_span: (0, 0),
        args: Vec::new(),
        span: (0, 0),
    }
}

fn build_fact_node(
    retract: bool,
    pattern: FactPattern,
    pattern_base: usize,
    raw: String,
    span: Span,
    when: Option<CelSlot>,
) -> Node {
    if retract {
        Node::Retract(Retract {
            pattern,
            pattern_base,
            raw,
            span,
            when,
        })
    } else {
        Node::Assert(Assert {
            pattern,
            pattern_base,
            raw,
            span,
            when,
        })
    }
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

/// True when `trimmed` has the SHAPE of a content-shaped body construct —
/// `@speaker…`, a legacy `:speaker…`/`:line[…]` sigil, an `::directive`, or a
/// `<tag …>` open — regardless of whether it parses cleanly (dsl 0.5.0 §2.1
/// `E-CONTENT-OUTSIDE-SHOT`). Mirrors the content-line shape test in
/// `next_node` plus the `::`/`<` shapes; a truly unrecognized line (matching
/// none of these) stays the residual `E-UNCLASSIFIED` catch-all. A `</tag>`
/// CLOSE is explicitly excluded (RC1): it is never "content", so a stray
/// top-level close must reach the `E-UNCLOSED-TAG` check in
/// `parse_document_inner` instead of being misdiagnosed as
/// `E-CONTENT-OUTSIDE-SHOT`.
fn is_content_shaped_line(trimmed: &str) -> bool {
    if trimmed.starts_with("</") {
        return false;
    }
    if trimmed.starts_with("::") || trimmed.starts_with('<') {
        return true;
    }
    is_line_head(trimmed)
}

/// The content-line head shape: `@speaker`, the legacy `:speaker` sigil, or
/// (dsl 0.26.0 §3.2) a speaker param `@@who` — a sigil and an ident start.
fn is_line_head(trimmed: &str) -> bool {
    let b = trimmed.as_bytes();
    let ident_at = |k: usize| b.get(k).is_some_and(|c| c.is_ascii_alphabetic());
    match b.first() {
        Some(b'@') => ident_at(1) || (b.get(1) == Some(&b'@') && ident_at(2)),
        Some(b':') => ident_at(1),
        _ => false,
    }
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
        Node::Objective(o) => o.span.byte_end,
        Node::On(o) => o.span.byte_end,
        Node::Assert(a) => a.span.byte_end,
        Node::Retract(r) => r.span.byte_end,
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
mod tests;
