//! Pre-parse trivia layer (dsl §4.2, §4.4, §6.1).
//!
//! Two responsibilities, applied *before* the line parser (T2.3):
//! 1. [`peel_frontmatter`] — split a leading YAML `---` … `---` block off the
//!    document body. Frontmatter uses YAML's own comment rules, so it is peeled
//!    (not comment-stripped) here and handed to the checker verbatim.
//! 2. [`strip_comments`] / [`strip_comments_checked`] — remove `/* … */` block
//!    comments from the post-frontmatter body. Per §4.2 comments do not nest and
//!    there are no `//` line comments; per §4.4 a `/*` inside a quoted string is
//!    *not* a comment. The scan is a single pass over the whole body, so a block
//!    comment that spans newlines is dropped correctly (see [`strip_comments_checked`]).

use lute_core_span::Span;

#[derive(Debug, PartialEq)]
pub enum CommentError {
    Unterminated,
}

/// Peel a leading YAML `---` … `---` frontmatter block off `text`.
///
/// Returns `(Some((inner_yaml, span)), body_start)` when a well-formed block is
/// present, where `inner_yaml` is the content between the delimiter lines and
/// `body_start` is the byte offset of the first body byte after the closing
/// `---` line. When there is no opening delimiter — or no matching closing one —
/// returns `(None, 0)`; the checker (dsl §6.1) flags a dangling opener.
pub fn peel_frontmatter(text: &str) -> Result<(Option<(String, Span)>, usize), CommentError> {
    if !text.starts_with("---\n") && text != "---" {
        return Ok((None, 0));
    }
    let after_open = 4; // "---\n"
    // find a line that is exactly "---"
    let bytes = text.as_bytes();
    let mut i = after_open;
    let mut line_start = after_open;
    while i <= bytes.len() {
        let at_eol = i == bytes.len() || bytes[i] == b'\n';
        if at_eol {
            let line = &text[line_start..i];
            if line == "---" {
                let inner = text[after_open..line_start].to_string();
                let body_start = if i < bytes.len() { i + 1 } else { i };
                let span = Span {
                    byte_start: 0,
                    byte_end: body_start,
                    line: 1,
                    column: 1,
                    utf16_range: (0, 0),
                };
                return Ok((Some((inner, span)), body_start));
            }
            line_start = i + 1;
        }
        i += 1;
    }
    Ok((None, 0)) // no closing delimiter: treated as no frontmatter (checker flags)
}

/// Strip `/* … */` block comments from `text`, falling back to the original on
/// an unterminated comment. Prefer [`strip_comments_checked`] where the error
/// matters. See it for the exact scanning rules.
pub fn strip_comments(text: &str) -> String {
    strip_comments_checked(text).unwrap_or_else(|_| text.to_string())
}

/// Strip `/* … */` block comments from `text` in a single whole-body pass.
///
/// Scanning rules (dsl §4.2, §4.4):
/// - A comment runs from `/*` to the next `*/`; comments do not nest.
/// - Comments are dropped verbatim, including any newlines they span, so a
///   multi-line block comment is removed correctly when the whole body is
///   scanned at once.
/// - There are no `//` line comments.
/// - `in_string` (toggled by an unescaped `"`) suppresses comment recognition,
///   so `/*`/`*/` inside a quoted `String`/`CelString` value are preserved.
/// - EOF reached inside a comment yields [`CommentError::Unterminated`].
pub fn strip_comments_checked(text: &str) -> Result<String, CommentError> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some((_, c)) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if c == '/' && matches!(chars.peek(), Some((_, '*'))) {
            chars.next(); // consume '*'
            let mut closed = false;
            while let Some((_, d)) = chars.next() {
                if d == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    chars.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err(CommentError::Unterminated);
            }
            continue; // comment dropped
        }
        out.push(c);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peels_yaml_frontmatter() {
        let doc = "---\ncharacter: bianca\n---\n# Title\n";
        let (fm, body_start) = peel_frontmatter(doc).unwrap();
        assert!(fm.unwrap().0.contains("character: bianca"));
        assert_eq!(&doc[body_start..], "# Title\n");
    }

    #[test]
    fn strips_block_comment_but_not_inside_string() {
        assert_eq!(
            strip_comments(r#"::sfx{sound="a /* b */ c"} /* real */"#).trim_end(),
            r#"::sfx{sound="a /* b */ c"}"#
        );
    }

    #[test]
    fn unterminated_comment_errors() {
        assert!(matches!(
            strip_comments_checked("foo /* bar"),
            Err(CommentError::Unterminated)
        ));
    }

    #[test]
    fn strips_multiline_block_comment() {
        // A `/* … */` block spanning several newlines (as in the reference
        // example's header block) is removed in a single whole-body pass.
        assert_eq!(strip_comments("a /* x\ny\nz */ b"), "a  b");
    }

    #[test]
    fn preserves_block_delims_inside_quoted_string_across_body() {
        // `/*` inside a quoted string is not a comment, even when a *real*
        // comment appears later in the same whole-body scan.
        let input = "::x{s=\"/* not a comment */\"}\n/* real */";
        assert_eq!(strip_comments(input), "::x{s=\"/* not a comment */\"}\n");
    }
}
