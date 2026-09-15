//! Incremental framing for streamed Lute shot-body continuations.
//!
//! This module deliberately stops at syntax. It identifies complete top-level
//! body units and parses those units into syntax nodes; checking, lowering, and
//! artifact assembly remain caller-owned whole-document operations.

use std::ops::Range;

use lute_core_span::Diagnostic;

use crate::ast::Node;
use crate::lex::content_text_start;
use crate::parser::parse_body_fragment;

/// Why the current suffix cannot yet be emitted as a complete body unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NeedMoreInput {
    /// The current physical line has not been terminated by a newline.
    Line,
    /// The current physical line ends inside a quoted attribute value.
    QuotedAttribute,
    /// A `/* ... */` comment has not been closed.
    BlockComment,
    /// One or more nested elements still require matching closing tags.
    NestedBlock {
        /// Open tag names, from the outermost element to the innermost.
        open_tags: Vec<String>,
    },
}

/// One safely delimited, top-level shot-body source unit.
///
/// `range` uses byte offsets in the complete chunk stream. Syntax-node and
/// diagnostic spans are relative to `source`, avoiding a fabricated enclosing
/// `Document`. Valid Lute produces exactly one node; malformed but delimited
/// input may produce zero nodes and diagnostics.
#[derive(Clone, Debug)]
pub struct ContinuationUnit {
    pub range: Range<usize>,
    pub source: String,
    pub nodes: Vec<Node>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The result of appending a chunk.
#[derive(Clone, Debug)]
pub struct ContinuationBatch {
    /// Complete units discovered by this append, in source order.
    pub units: Vec<ContinuationUnit>,
    /// Present when a buffered suffix is not yet safely delimited.
    pub need_more: Option<NeedMoreInput>,
}

/// Unterminated source left by [`IncrementalContinuationParser::finish`].
#[derive(Clone, Debug)]
pub struct IncompleteContinuation {
    /// Byte range in the complete chunk stream.
    pub range: Range<usize>,
    /// The exact un-emitted source suffix.
    pub source: String,
    /// Existing Lute parser diagnostics, with spans relative to `source`.
    pub diagnostics: Vec<Diagnostic>,
}

/// Final output from an incremental continuation stream.
#[derive(Clone, Debug)]
pub struct ContinuationFinalization {
    /// Units made complete by EOF (a newline is not required at EOF).
    pub units: Vec<ContinuationUnit>,
    /// Unterminated syntax, if any. It is never emitted as a canonical unit.
    pub incomplete: Option<IncompleteContinuation>,
}

/// Incrementally frames and parses Lute shot-body continuations.
///
/// Chunks may end anywhere, including inside UTF-8 text, physical lines,
/// quoted attributes, comments, or nested elements. A unit is emitted only
/// after its physical line is delimited and every element opened by that unit
/// is closed. Leading blank/comment trivia stays attached to the next unit.
///
/// This parser does not compile or merge a `Document`. Callers that want an IR
/// artifact must assemble canonical document source and use the ordinary
/// whole-document checker/compiler APIs.
#[derive(Debug, Default)]
pub struct IncrementalContinuationParser {
    buffer: String,
    stream_offset: usize,
    scan_pos: usize,
    in_block_comment: bool,
    open_tags: Vec<String>,
    pending_content: bool,
}

impl IncrementalContinuationParser {
    /// Construct an empty incremental continuation parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append text and return every newly complete top-level body unit.
    pub fn push(&mut self, chunk: &str) -> ContinuationBatch {
        self.buffer.push_str(chunk);
        let boundaries = self.scan_complete_lines();
        let units = self.take_units(&boundaries);
        ContinuationBatch {
            units,
            need_more: self.need_more(),
        }
    }

    /// Finish the stream, treating EOF as the end of its final physical line.
    ///
    /// Complete EOF-delimited leaves are emitted. An open tag or block comment
    /// remains in `incomplete` and is parsed only to surface the existing Lute
    /// diagnostics such as `E-UNCLOSED-TAG` or `E-COMMENT-UNTERMINATED`.
    pub fn finish(mut self) -> ContinuationFinalization {
        let mut units = Vec::new();
        let boundaries = self.scan_complete_lines();
        units.extend(self.take_units(&boundaries));

        if !self.buffer.is_empty() && self.scan_pos < self.buffer.len() {
            let end = self.buffer.len();
            let complete = self.process_line(self.scan_pos, end);
            self.scan_pos = end;
            if complete {
                units.extend(self.take_units(&[end]));
            }
        }

        if !self.pending_content && !self.in_block_comment && self.open_tags.is_empty() {
            return ContinuationFinalization { units, incomplete: None };
        }

        let start = self.stream_offset;
        let source = self.buffer;
        let (_, diagnostics) = parse_body_fragment(&source);
        ContinuationFinalization {
            units,
            incomplete: Some(IncompleteContinuation {
                range: start..start + source.len(),
                source,
                diagnostics,
            }),
        }
    }

    fn scan_complete_lines(&mut self) -> Vec<usize> {
        let mut boundaries = Vec::new();
        while let Some(rel) = self.buffer[self.scan_pos..].find('\n') {
            let line_end = self.scan_pos + rel;
            if self.process_line(self.scan_pos, line_end) {
                boundaries.push(line_end + 1);
            }
            self.scan_pos = line_end + 1;
        }
        boundaries
    }

    /// Process one physical line. Returns true when the current pending unit
    /// becomes safely delimited at this line end.
    fn process_line(&mut self, start: usize, end: usize) -> bool {
        let raw = &self.buffer[start..end];
        let clean = blank_comments(raw, &mut self.in_block_comment);
        let trimmed = clean.trim();

        if !trimmed.is_empty() {
            self.pending_content = true;
            match tag_shape(trimmed) {
                Some(TagShape::Open { name, self_closing, inline_closed }) => {
                    if !self_closing && !inline_closed {
                        self.open_tags.push(name);
                    }
                }
                Some(TagShape::Close(name)) => {
                    if self.open_tags.last().is_some_and(|open| open == &name) {
                        self.open_tags.pop();
                    }
                }
                None => {}
            }
        }

        let complete = self.pending_content && !self.in_block_comment && self.open_tags.is_empty();
        if complete {
            self.pending_content = false;
        }
        complete
    }

    fn take_units(&mut self, boundaries: &[usize]) -> Vec<ContinuationUnit> {
        let mut units = Vec::with_capacity(boundaries.len());
        let mut start = 0;
        for &end in boundaries {
            let source = self.buffer[start..end].to_string();
            let (nodes, diagnostics) = parse_body_fragment(&source);
            units.push(ContinuationUnit {
                range: (self.stream_offset + start)..(self.stream_offset + end),
                source,
                nodes,
                diagnostics,
            });
            start = end;
        }
        if start != 0 {
            self.buffer.drain(..start);
            self.stream_offset += start;
            self.scan_pos -= start;
        }
        units
    }

    fn need_more(&self) -> Option<NeedMoreInput> {
        if self.buffer.is_empty() {
            return None;
        }
        let tail = &self.buffer[self.scan_pos..];
        let lexical = inspect_tail(tail, self.in_block_comment);
        if lexical.in_block_comment {
            return Some(NeedMoreInput::BlockComment);
        }
        if !self.open_tags.is_empty() {
            return Some(NeedMoreInput::NestedBlock { open_tags: self.open_tags.clone() });
        }
        if lexical.in_string {
            return Some(NeedMoreInput::QuotedAttribute);
        }
        if !tail.is_empty() {
            return Some(NeedMoreInput::Line);
        }
        None
    }
}

#[derive(Debug)]
struct TailState {
    in_block_comment: bool,
    in_string: bool,
}

fn inspect_tail(line: &str, mut in_block_comment: bool) -> TailState {
    let (_, in_string) = blank_comments_inner(line, &mut in_block_comment);
    TailState { in_block_comment, in_string }
}

fn blank_comments(line: &str, in_block_comment: &mut bool) -> String {
    blank_comments_inner(line, in_block_comment).0
}

/// A line-local counterpart of the whole-document comment scanner. The output
/// is byte-length preserving, which keeps the parser's spans faithful.
fn blank_comments_inner(line: &str, in_block_comment: &mut bool) -> (String, bool) {
    let bytes = line.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut text_start = if *in_block_comment {
        usize::MAX
    } else {
        content_text_start(line).unwrap_or(usize::MAX)
    };

    while i < bytes.len() {
        if *in_block_comment {
            out[i] = b' ';
            if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                out[i + 1] = b' ';
                i += 2;
                *in_block_comment = false;
                let probe = String::from_utf8_lossy(&out[..i]).into_owned() + &line[i..];
                text_start = content_text_start(&probe).unwrap_or(usize::MAX);
            } else {
                i += 1;
            }
            continue;
        }
        if i >= text_start {
            break;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if bytes[i] == b'\\' {
                escaped = true;
            } else if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/')
            && out[..i].iter().all(|b| *b == b' ' || *b == b'\t')
        {
            out[i..].fill(b' ');
            break;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            *in_block_comment = true;
            continue;
        }
        i += 1;
    }

    // SAFETY: non-comment bytes are unchanged UTF-8; every byte inside comments
    // is replaced with ASCII space, so the result remains valid UTF-8.
    (String::from_utf8(out).expect("comment blanking preserves UTF-8"), in_string)
}

#[derive(Debug)]
enum TagShape {
    Open { name: String, self_closing: bool, inline_closed: bool },
    Close(String),
}

fn tag_shape(line: &str) -> Option<TagShape> {
    let bytes = line.as_bytes();
    if line.starts_with("</") {
        let name = tag_name(&line[2..]);
        return (!name.is_empty()).then_some(TagShape::Close(name));
    }
    if !line.starts_with('<') {
        return None;
    }
    let name = tag_name(&line[1..]);
    if name.is_empty() {
        return None;
    }

    let mut i = 1 + name.len();
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        if in_string {
            if escaped {
                escaped = false;
            } else if bytes[i] == b'\\' {
                escaped = true;
            } else if bytes[i] == b'"' {
                in_string = false;
            }
        } else if bytes[i] == b'"' {
            in_string = true;
        } else if bytes[i] == b'>' {
            let self_closing = i > 0 && bytes[i - 1] == b'/';
            let rest = &line[i + 1..];
            let inline_closed = !self_closing && contains_close(rest, &name);
            return Some(TagShape::Open { name, self_closing, inline_closed });
        }
        i += 1;
    }
    None
}

fn tag_name(rest: &str) -> String {
    rest.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn contains_close(rest: &str, name: &str) -> bool {
    rest.match_indices("</").any(|(at, _)| {
        let candidate = &rest[at + 2..];
        candidate.starts_with(name)
            && candidate[name.len()..]
                .as_bytes()
                .first()
                .is_some_and(|b| !b.is_ascii_alphanumeric() && *b != b'_' && *b != b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Node;
    use crate::parser::{E_COMMENT_UNTERMINATED, E_UNCLOSED_TAG};

    fn one_node(unit: &ContinuationUnit) -> &Node {
        assert!(unit.diagnostics.is_empty(), "{:?}", unit.diagnostics);
        assert_eq!(unit.nodes.len(), 1);
        &unit.nodes[0]
    }

    #[test]
    fn buffers_a_line_split_across_chunks() {
        let mut parser = IncrementalContinuationParser::new();
        let first = parser.push("@narrator: hel");
        assert!(first.units.is_empty());
        assert_eq!(first.need_more, Some(NeedMoreInput::Line));

        let second = parser.push("lo\n");
        assert_eq!(second.units.len(), 1);
        assert!(matches!(one_node(&second.units[0]), Node::Line(line) if line.text == "hello"));
        assert_eq!(second.units[0].source, "@narrator: hello\n");
        assert_eq!(second.units[0].range, 0..17);
    }

    #[test]
    fn buffers_a_directive_until_its_line_is_delimited() {
        let mut parser = IncrementalContinuationParser::new();
        assert_eq!(parser.push("::sh").need_more, Some(NeedMoreInput::Line));
        let batch = parser.push("ow{character=\"mira\"}\n");
        assert!(matches!(one_node(&batch.units[0]), Node::Directive(d) if d.tag == "show"));
    }

    #[test]
    fn never_splits_a_nested_branch() {
        let mut parser = IncrementalContinuationParser::new();
        let first = parser.push(
            "<branch id=\"route\">\n<choice id=\"a\" label=\"A\">\n@n: A\n</choice>\n",
        );
        assert!(first.units.is_empty());
        assert_eq!(
            first.need_more,
            Some(NeedMoreInput::NestedBlock { open_tags: vec!["branch".into()] })
        );

        let second = parser.push(
            "<choice id=\"b\" label=\"B\">\n<on event=\"resume\">\n@n: nested\n</on>\n@n: B\n</choice>\n</branch>\n",
        );
        assert_eq!(second.units.len(), 1);
        assert!(matches!(one_node(&second.units[0]), Node::Branch(branch) if branch.choices.len() == 2));
    }

    #[test]
    fn quoted_attribute_can_cross_chunk_boundaries() {
        let mut parser = IncrementalContinuationParser::new();
        let first = parser.push("::show{character=\"mi");
        assert_eq!(first.need_more, Some(NeedMoreInput::QuotedAttribute));
        let second = parser.push("ra\", pose=\"a } > /* pose\"}\n");
        assert_eq!(second.units.len(), 1);
        assert!(matches!(one_node(&second.units[0]), Node::Directive(_)));
    }

    #[test]
    fn comments_can_cross_chunks_without_exposing_fake_tags() {
        let mut parser = IncrementalContinuationParser::new();
        let first = parser.push("/* ignored <branch>\n</bra");
        assert_eq!(first.need_more, Some(NeedMoreInput::BlockComment));
        let second = parser.push("nch> */\n@n: visible\n");
        assert_eq!(second.units.len(), 1);
        assert!(matches!(one_node(&second.units[0]), Node::Line(_)));
        assert!(second.units[0].source.starts_with("/* ignored"));
    }

    #[test]
    fn incomplete_match_is_emitted_only_after_completion() {
        let mut parser = IncrementalContinuationParser::new();
        let first = parser.push("<match on=\"scene.route\">\n<when is=\"a\">\n@n: A\n</when>\n");
        assert!(first.units.is_empty());
        assert!(matches!(first.need_more, Some(NeedMoreInput::NestedBlock { .. })));

        let second = parser.push("<otherwise>\n@n: B\n</otherwise>\n</match>\n");
        assert_eq!(second.units.len(), 1);
        assert!(matches!(one_node(&second.units[0]), Node::Match(m) if m.arms.len() == 2));
    }

    #[test]
    fn finish_reports_existing_unterminated_diagnostics_without_emitting() {
        let mut block = IncrementalContinuationParser::new();
        block.push("<branch id=\"route\">\n");
        let finished = block.finish();
        assert!(finished.units.is_empty());
        let incomplete = finished.incomplete.expect("open block must remain incomplete");
        assert!(incomplete.diagnostics.iter().any(|d| d.code == E_UNCLOSED_TAG));

        let mut comment = IncrementalContinuationParser::new();
        comment.push("/* unfinished");
        let finished = comment.finish();
        let incomplete = finished.incomplete.expect("open comment must remain incomplete");
        assert!(incomplete.diagnostics.iter().any(|d| d.code == E_COMMENT_UNTERMINATED));
    }

    #[test]
    fn finish_emits_a_complete_final_line_without_newline() {
        let mut parser = IncrementalContinuationParser::new();
        parser.push("@n: eof");
        let finished = parser.finish();
        assert!(finished.incomplete.is_none());
        assert_eq!(finished.units.len(), 1);
        assert!(matches!(one_node(&finished.units[0]), Node::Line(line) if line.text == "eof"));
    }

    #[test]
    fn finish_discards_complete_comment_only_trivia() {
        let mut parser = IncrementalContinuationParser::new();
        parser.push("// note\n/* complete */");
        let finished = parser.finish();
        assert!(finished.units.is_empty());
        assert!(finished.incomplete.is_none());
    }
}
