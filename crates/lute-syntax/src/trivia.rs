use lute_core_span::{Span, TextIndex};

/// The broad lexical class of one lossless source token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Structure,
    Attribute,
    Cel,
    Prose,
    Yaml,
    Comment,
    Whitespace,
}

/// A span into the borrowed source. Spans are always byte boundaries in `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceToken {
    pub kind: TokenKind,
    pub span: Span,
}

/// A lossless lexical view of source. Concatenating the spans in `tokens` in
/// order reproduces `source` byte-for-byte.
#[derive(Clone, Debug)]
pub struct SourceStream<'a> {
    pub source: &'a str,
    pub tokens: Vec<SourceToken>,
}

fn span(idx: &TextIndex<'_>, start: usize, end: usize) -> Span {
    Span::from_bytes(idx, start, end)
}

fn push(tokens: &mut Vec<SourceToken>, idx: &TextIndex<'_>, kind: TokenKind, start: usize, end: usize) {
    if start < end {
        tokens.push(SourceToken { kind, span: span(idx, start, end) });
    }
}

fn line_kind(line: &str) -> TokenKind {
    let trimmed = line.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() {
        TokenKind::Whitespace
    } else if trimmed.starts_with("//") || trimmed.starts_with("/*") {
        TokenKind::Comment
    } else if trimmed.starts_with("@") || trimmed.starts_with("::") || trimmed.starts_with('<')
        || trimmed.starts_with('#') {
        TokenKind::Structure
    } else {
        TokenKind::Prose
    }
}

/// Build a lossless line/trivia stream without changing or reparsing `source`.
///
/// The parser remains the authority for semantic validity. This scanner is
/// intentionally total: malformed delimiters and arbitrary editor buffers are
/// represented as ordinary source spans rather than causing a panic.
pub fn source_stream(source: &str) -> SourceStream<'_> {
    let idx = TextIndex::new(source);
    let mut tokens = Vec::new();
    let mut yaml_end = 0usize;

    // A leading frontmatter envelope is opaque YAML, including its delimiters.
    if source.starts_with("---\n") || source.starts_with("---\r\n") || source == "---" {
        let open_end = if source.starts_with("---\r\n") { 5 } else { 4 };
        let mut at = open_end;
        while at <= source.len() {
            let end = source[at..].find('\n').map_or(source.len(), |n| at + n);
            let body = &source[at..end];
            if body.trim_end_matches('\r') == "---" {
                yaml_end = if end < source.len() { end + 1 } else { end };
                break;
            }
            if end == source.len() { break; }
            at = end + 1;
        }
    }
    if yaml_end > 0 {
        push(&mut tokens, &idx, TokenKind::Yaml, 0, yaml_end);
    }

    let mut line_start = yaml_end;
    let mut in_block_comment = false;
    while line_start < source.len() {
        let line_end = source[line_start..].find('\n').map_or(source.len(), |n| line_start + n);
        let content_end = line_end;
        let line = &source[line_start..content_end];
        let kind = line_kind(line);
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        push(&mut tokens, &idx, TokenKind::Whitespace, line_start, line_start + indent);
        if in_block_comment || kind == TokenKind::Comment {
            push(&mut tokens, &idx, TokenKind::Comment, line_start + indent, content_end);
            if in_block_comment || line.trim_start_matches([' ', '\t']).starts_with("/*") {
                in_block_comment = !line.contains("*/");
            }
        } else if kind == TokenKind::Whitespace {
            push(&mut tokens, &idx, TokenKind::Whitespace, line_start + indent, content_end);
        } else {
            let body_start = line_start + indent;
            // Preserve a leading structural prefix separately from opaque
            // attribute/CEL/prose payloads where it is easy to identify one.
            let body = &source[body_start..content_end];
            if kind == TokenKind::Structure {
                if let Some(open) = body.find('{') {
                    push(&mut tokens, &idx, TokenKind::Structure, body_start, body_start + open + 1);
                    if let Some(close_rel) = body[open + 1..].rfind('}') {
                        let close = body_start + open + 1 + close_rel;
                        push(&mut tokens, &idx, TokenKind::Attribute, body_start + open + 1, close);
                        push(&mut tokens, &idx, TokenKind::Structure, close, content_end);
                    } else {
                        push(&mut tokens, &idx, TokenKind::Attribute, body_start + open + 1, content_end);
                    }
                } else if let Some(colon) = body.find(':') {
                    push(&mut tokens, &idx, TokenKind::Structure, body_start, body_start + colon + 1);
                    push(&mut tokens, &idx, TokenKind::Prose, body_start + colon + 1, content_end);
                } else {
                    push(&mut tokens, &idx, kind, body_start, content_end);
                }
            } else {
                push(&mut tokens, &idx, kind, body_start, content_end);
            }
        }
        if line_end < source.len() {
            push(&mut tokens, &idx, TokenKind::Whitespace, line_end, line_end + 1);
            line_start = line_end + 1;
        } else {
            line_start = source.len();
        }
    }
    if source.is_empty() {
        return SourceStream { source, tokens };
    }
    SourceStream { source, tokens }
}
