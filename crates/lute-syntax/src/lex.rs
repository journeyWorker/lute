//! Pre-parse trivia layer (dsl §4.2, §4.4, §6.1).
//!
//! Two responsibilities, applied *before* the line parser (T2.3):
//! 1. [`peel_frontmatter`] — split a leading YAML `---` … `---` block off the
//!    document body. Frontmatter uses YAML's own comment rules, so it is peeled
//!    (not comment-stripped) here and handed to the checker verbatim.
//! 2. [`strip_comments`] / [`strip_comments_checked`] — blank `/* … */` block
//!    comments *and* line-leading `//` comments in the post-frontmatter body.
//!    Per §4.2 block comments do not nest; a `//` is a comment only at the start
//!    of a body line (after leading trivia) and runs to EOL; a comment inside a
//!    quoted `String`/`CelString`, or anywhere inside a content line's opaque
//!    `Text` (past the second `:`), is *not* a comment (§4.4). The scan is a
//!    single whole-body pass; each comment is replaced byte-for-byte with spaces
//!    (newlines kept) so a multi-line block comment is removed without shifting
//!    any following span (see [`strip_comments_checked`]).

use crate::parser::is_ident_byte;
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

/// Byte offset (line-relative) just past the second `:` of a 0.1.0 content line
/// (`":" Ident Attrs? ":"`), i.e. where opaque `Text` begins (dsl §4.2, §4.4).
/// `None` if the trimmed line is not a content line (a `::`-directive, a missing
/// second `:`, or no leading `:`ident). Quote-aware inside the optional `{…}`
/// attr list so a `"}"` in an attr value does not close the block early.
///
/// Both comment scans use this as the opacity boundary: a `"` at/after it is a
/// literal `Text` character (not a `String`/`CelString` delimiter), and — per
/// §4.2 exclusion 2 — no `/*` or `//` is recognized at/after it, since `Text` is
/// truly opaque to EOL.
pub(crate) fn content_text_start(line: &str) -> Option<usize> {
    let ws = line.len() - line.trim_start().len();
    let b = line.as_bytes();
    let mut j = ws;
    if b.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    if j >= b.len() || b[j] == b':' || !b[j].is_ascii_alphabetic() {
        return None; // `::` directive or not an ident — not a content line
    }
    while j < b.len() && is_ident_byte(b[j]) {
        j += 1;
    }
    if b.get(j) == Some(&b'{') {
        let mut in_str = false;
        j += 1;
        while j < b.len() {
            match b[j] {
                b'"' if !in_str => in_str = true,
                b'"' if in_str && b[j - 1] != b'\\' => in_str = false,
                b'}' if !in_str => break,
                _ => {}
            }
            j += 1;
        }
        j += 1; // past '}' (or EOL — caller degrades safely)
    }
    (b.get(j) == Some(&b':')).then_some(j + 1)
}

/// Absolute opaque-`Text` boundary of the line beginning at byte `line_start`
/// in `text` (dsl §4.2 exclusion 2), or [`usize::MAX`] when the line is not a
/// content line (so nothing on it is opaque and a `"` toggles the String state
/// normally). See [`content_text_start`].
pub(crate) fn text_start_for_line(text: &str, line_start: usize) -> usize {
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |n| line_start + n);
    content_text_start(&text[line_start..line_end]).map_or(usize::MAX, |rel| line_start + rel)
}

/// Opaque-`Text` boundary of the line at `line_start`, computed from the
/// *blanked* scan view rather than raw `text`: the prefix already emitted to
/// `out` (every comment so far replaced by spaces) up to the scan position
/// `scanned`, joined with the not-yet-scanned raw tail of that line.
///
/// Blanking a `/* … */` (or a line-leading `//`) that precedes the content
/// construct on the same line (dsl §4.2 permits inline trivia before line
/// classification) can reveal a content line the *raw* line hid — its un-blanked
/// comment bytes would defeat the `:`ident prefix match in [`content_text_start`].
/// Recomputing on the blanked view fixes the boundary so the opaque `Text` after
/// the second `:` is not re-scanned for comments. Returns [`usize::MAX`] when the
/// line is not a content line. Requires `line_start <= scanned == out.len()`.
pub(crate) fn line_text_start_blanked(
    out: &str,
    text: &str,
    line_start: usize,
    scanned: usize,
) -> usize {
    let line_end = text[scanned..]
        .find('\n')
        .map_or(text.len(), |n| scanned + n);
    let mut line = String::with_capacity(line_end - line_start);
    line.push_str(&out[line_start..scanned]); // blanked prefix on this line
    line.push_str(&text[scanned..line_end]); // raw tail on this line
    content_text_start(&line).map_or(usize::MAX, |rel| line_start + rel)
}

/// Strip `/* … */` block comments and line-leading `//` comments from `text` in
/// a single whole-body pass.
///
/// Scanning rules (dsl §4.2, §4.4):
/// - A block comment runs from `/*` to the next `*/`; block comments do not nest.
/// - A `//` is a comment only when it is line-leading — the first non-trivia on
///   its line (after leading whitespace and any blanked comment) — and runs to
///   EOL. A mid-line `//` is ordinary content. Line comments never span lines.
/// - A comment (its delimiters and body) is blanked in place: every byte is
///   replaced by a space except `\n`, which is kept at its original offset. This
///   preserves byte length and every following span, so a multi-line block
///   comment is removed without shifting later positions.
/// - `in_string` (toggled by an unescaped `"`) suppresses comment recognition,
///   so `/*`/`*/`/`//` inside a quoted `String`/`CelString` value are preserved.
/// - Past a content line's second `:` the `Text` is truly opaque (§4.2 exclusion
///   2, via [`content_text_start`]): no comment is recognized there and a `"` is
///   a literal character, not a String delimiter. A String never spans a raw
///   newline (§4.4), so the string state resets at each line start.
/// - EOF reached inside a block comment yields [`CommentError::Unterminated`].
pub fn strip_comments_checked(text: &str) -> Result<String, CommentError> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut in_string = false;
    let mut escaped = false;
    // Opaque-`Text` boundary of the CURRENT line (absolute byte offset); `MAX`
    // when the line is not a content line. At/after it the `Text` is opaque
    // (§4.2 exclusion 2): no comment is recognized and a `"` is a literal
    // character, not a String delimiter. Recomputed at every line start (and
    // after a blanked comment that spanned newlines).
    let mut text_start = text_start_for_line(text, 0);
    // Byte offset where the current line begins; kept in step with `text_start`
    // so a comment blanked mid-line can recompute the boundary from the blanked
    // view of *this* line (see `line_text_start_blanked`).
    let mut line_start = 0usize;
    while let Some((a, c)) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            } else if c == '\n' {
                // A String must not contain a raw newline (§4.4): recover the scan
                // rather than let a stray `"` swallow the rest of the document.
                in_string = false;
                line_start = a + 1;
                text_start = text_start_for_line(text, line_start);
            }
            continue;
        }
        if c == '\n' {
            out.push(c);
            line_start = a + 1;
            text_start = text_start_for_line(text, line_start);
            continue;
        }
        // Past the second `:` of a content line the `Text` is truly opaque
        // (§4.2 exclusion 2): no comment/String is recognized to EOL.
        if a >= text_start {
            out.push(c);
            continue;
        }
        // Line-leading `//` (only whitespace / blanked trivia precedes it on
        // this line) → blank to EOL (§4.2). A mid-line `//` is content.
        if c == '/'
            && matches!(chars.peek(), Some((_, '/')))
            && out[line_start..a].bytes().all(|x| x == b' ' || x == b'\t')
        {
            let line_end = text[a..].find('\n').map_or(text.len(), |n| a + n);
            for _ in a..line_end {
                out.push(' '); // no `\n` in [a, line_end): every byte a space
            }
            while matches!(chars.peek(), Some(&(pos, _)) if pos < line_end) {
                chars.next();
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
            let mut end = None;
            while let Some((_, d)) = chars.next() {
                if d == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    let (slash, _) = chars.next().expect("peeked '/'");
                    end = Some(slash + 1); // '/' is one byte
                    break;
                }
            }
            let Some(b) = end else {
                return Err(CommentError::Unterminated);
            };
            // Blank the whole comment byte-range (`/*` … `*/`) in place: a space
            // for every byte except `\n`, which is kept at its original offset.
            // Preserves `out.len() == text.len()` and every newline position.
            let comment = &text.as_bytes()[a..b];
            for &byte in comment {
                out.push(if byte == b'\n' { '\n' } else { ' ' });
            }
            // Blanking a comment can *reveal* a content line on this line: leading
            // same-line trivia before the `:`ident (dsl §4.2), or a comment before
            // the classifying `:`. A multi-line comment also advances the scan onto
            // a later line. Either way the boundary is recomputed from the *blanked*
            // view (`out`, all comments so far → spaces) so the raw comment bytes no
            // longer defeat the content-line match and the opaque `Text` after the
            // second `:` is not re-scanned for comments.
            if let Some(nl) = comment.iter().rposition(|&x| x == b'\n') {
                line_start = a + nl + 1;
            }
            text_start = line_text_start_blanked(&out, text, line_start, b);
            continue;
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
    fn blanks_multiline_block_comment_preserving_layout() {
        // A `/* … */` block spanning several newlines is *blanked*, not deleted:
        // every comment byte becomes a space except `\n`, which is kept at its
        // original offset. Byte length and newline positions are preserved so no
        // following span shifts.
        let input = "a /* x\ny\nz */ b";
        let out = strip_comments_checked(input).unwrap();
        // a + 5 spaces, \n, 1 space, \n, 5 spaces + b (verified by construction).
        assert_eq!(out, "a     \n \n     b");
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn len_preserved_with_multibyte_comment() {
        // A multi-line-agnostic comment containing multi-byte UTF-8 (`é` = 2
        // bytes) still blanks byte-for-byte: `out.len() == input.len()`.
        let input = "a /* ééé */ b";
        let out = strip_comments_checked(input).unwrap();
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn newlines_preserved() {
        // Every `\n` stays at its exact original byte offset, including newlines
        // that fall inside a blanked comment.
        let input = "line1 /* c1\nc2 */\nline2\n";
        let out = strip_comments_checked(input).unwrap();
        let in_nl: Vec<usize> = input.match_indices('\n').map(|(i, _)| i).collect();
        let out_nl: Vec<usize> = out.match_indices('\n').map(|(i, _)| i).collect();
        assert_eq!(in_nl, out_nl);
    }

    #[test]
    fn offset_preserved() {
        // Blanking keeps every non-comment byte at its original offset, so a
        // token after a comment is found at the same byte index it had in input.
        let input = "pre /* c */post";
        let out = strip_comments_checked(input).unwrap();
        assert_eq!(out.find("pre"), Some(0));
        assert_eq!(out.find("post"), input.find("post"));
    }

    #[test]
    fn preserves_block_delims_inside_quoted_string_across_body() {
        // `/*` inside a quoted string is not a comment, even when a *real*
        // comment appears later in the same whole-body scan.
        let input = "::x{s=\"/* not a comment */\"}\n/* real */";
        assert_eq!(
            strip_comments(input).trim_end(),
            "::x{s=\"/* not a comment */\"}"
        );
    }

    #[test]
    fn comment_not_recognized_inside_content_text() {
        // §4.2 exclusion 2: past a content line's second `:` the `Text` is truly
        // opaque, so a `/*` or a mid-`Text` `//` is literal, not a comment.
        // Nothing is blanked and the line survives verbatim.
        let input = ":bianca: I love /* c */ and // b";
        let out = strip_comments_checked(input).unwrap();
        assert_eq!(out, input, "content Text must be verbatim: {out:?}");
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn content_text_quote_does_not_leak_across_newline() {
        // A `"` inside opaque `Text` is literal (never opens a String), so it
        // cannot leak string state past the newline and suppress a real comment
        // on the next line (§4.2, §4.4: a String never spans a raw newline).
        let input = ":bianca: he said \"hi\n/* real */";
        let out = strip_comments_checked(input).unwrap();
        assert!(out.contains("he said \"hi"), "opaque Text altered: {out:?}");
        assert!(!out.contains("real"), "next-line comment survived: {out:?}");
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn leading_comment_before_content_line_keeps_text_opaque() {
        // §4.2: a block comment may precede the content construct on the same
        // line. Blanking it must recompute the opaque-`Text` boundary from the
        // *blanked* view (else the raw leading bytes hide the `:`ident) so the
        // `Text` after the second `:` stays opaque: its inner `/* c */` is
        // literal and NOT stripped, while the leading trivia IS blanked.
        let input = "/* pre */ :bianca: keep /* c */ here";
        let out = strip_comments_checked(input).unwrap();
        assert!(!out.contains("pre"), "leading comment survived: {out:?}");
        assert!(out.contains(":bianca:"), "construct lost: {out:?}");
        assert!(
            out.contains("keep /* c */ here"),
            "opaque Text altered: {out:?}"
        );
        // Length-preserving invariant holds (spans map 1:1 to source).
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn line_comment_inside_quoted_value_is_not_a_comment() {
        // §4.2 exclusion 1: a `//` inside a quoted `String`/`CelString` value is
        // content, protected by the quote guard — even a `//` (and a `}`) inside
        // a content line's attr value, which `content_text_start` quote-scans so
        // the opaque-`Text` boundary lands past the *real* closing `}` `:`.
        let directive = "::sfx{url=\"a // b\"}";
        assert_eq!(
            strip_comments_checked(directive).unwrap(),
            directive,
            "`//` inside a directive attr value must survive"
        );
        let content = ":bianca{u=\"} // x\"}: hi";
        assert_eq!(
            strip_comments_checked(content).unwrap(),
            content,
            "comment chars inside a content-line attr value must survive"
        );
    }

    #[test]
    fn line_comment_after_leading_block_comment_is_still_trivia() {
        // A `//` whose only same-line predecessors are blanked trivia (a leading
        // block comment) is still line-leading (§4.2, §4.3: recognition happens
        // *after* comments are stripped) → the whole line is blanked to EOL.
        let input = "/* x */ // note\nkeep";
        let out = strip_comments_checked(input).unwrap();
        assert!(!out.contains("note"), "line comment survived: {out:?}");
        assert!(!out.contains("x */"), "block comment survived: {out:?}");
        assert!(out.contains("keep"), "following line lost: {out:?}");
        assert_eq!(out.len(), input.len());
    }
}
