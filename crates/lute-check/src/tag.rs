//! `lute tag` localization pass (dsl §12): back-fill a stable `code` into every
//! untagged content `:line`. Pure + total; the CLI wraps this with file I/O.

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, AttrValue, Line, Node};
use lute_syntax::parse;

/// The result of tagging: the (possibly rewritten) document text and how many
/// `:line`s received a new `code`.
#[derive(Clone, Debug, PartialEq)]
pub struct TagOutcome {
    pub text: String,
    pub added: usize,
}

/// Back-fill a `code` attribute into every `:line` that lacks one (dsl §12).
/// Existing codes are never touched; new codes step above the document's highest
/// existing numeric code. Idempotent, deterministic, total (a structurally
/// broken doc is returned unchanged with `added: 0`).
pub fn tag_document(text: &str) -> TagOutcome {
    let (doc, diags) = parse(text);

    // Never rewrite a broken doc: any error-severity diagnostic means the node
    // stream may be corrupt, so return the input verbatim.
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return TagOutcome {
            text: text.to_string(),
            added: 0,
        };
    }

    // Every `:line` in document order (into branch choices' + match arms' bodies).
    let mut lines: Vec<&Line> = Vec::new();
    for shot in &doc.shots {
        collect_lines(&shot.body, &mut lines);
    }

    // Highest existing numeric `code` (parseable as `u64`); default 0. `u64`
    // (not `u32`) so a code at `u32::MAX` can't saturate and collide.
    let mut max_code: u64 = 0;
    for line in &lines {
        for attr in &line.attrs {
            if attr.key == "code" {
                if let AttrValue::Str(s) = &attr.value {
                    if let Ok(n) = s.trim().parse::<u64>() {
                        max_code = max_code.max(n);
                    }
                }
            }
        }
    }

    // Untagged lines, document order. A line is tagged iff it has a `code` attr.
    let untagged: Vec<&Line> = lines
        .into_iter()
        .filter(|l| !l.attrs.iter().any(|a| a.key == "code"))
        .collect();
    if untagged.is_empty() {
        return TagOutcome {
            text: text.to_string(),
            added: 0,
        };
    }

    // Build the insertions (byte offset + inserted string) in document order,
    // stepping the code above `max_code` by 10 for each untagged line.
    let bytes = text.as_bytes();
    let mut next_code: u64 = max_code;
    let mut inserts: Vec<(usize, String)> = Vec::with_capacity(untagged.len());
    for line in &untagged {
        // Fail closed at the top of the numeric range rather than emit a
        // duplicate (colliding) code.
        let Some(nc) = next_code.checked_add(10) else {
            break;
        };
        next_code = nc;
        let code = format!("{next_code:04}");
        let hdr_start = line.span.byte_start;
        let header = &text[hdr_start..line.text_span.byte_start];
        // The attr block (if any) is a `{` FLUSH against the speaker-closing `]`
        // (grammar `:line[speaker]{attrs}?`, parser.rs:400-408). A `{` anywhere
        // else in the header (e.g. inside a blanked block comment) is NOT an attr
        // block, so we key off the `]` and only accept a `{` immediately after it.
        if let Some(rel_br) = speaker_close(header.as_bytes()) {
            let bracket_end = hdr_start + rel_br + 1; // byte index just past `]`
            if bytes.get(bracket_end) == Some(&b'{') {
                // Existing attr block: insert after the `{`, trailing space only
                // when the block is non-empty (next byte isn't `}`).
                let at = bracket_end + 1;
                let inserted = if bytes.get(at) == Some(&b'}') {
                    format!("code=\"{code}\"")
                } else {
                    format!("code=\"{code}\" ")
                };
                inserts.push((at, inserted));
            } else {
                // No attr block: fresh `{code="ID"}` right after the speaker `]`.
                inserts.push((bracket_end, format!("{{code=\"{code}\"}}")));
            }
        }
    }

    // Splice back-to-front (descending offset) so earlier offsets stay valid.
    inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let mut out = text.to_string();
    for (at, inserted) in &inserts {
        out.insert_str(*at, inserted);
    }

    TagOutcome {
        text: out,
        added: inserts.len(),
    }
}

/// Byte index of the speaker-closing `]` in a `:line` header, skipping
/// `/* … */` block comments (the lexer blanks them before parsing, so a `]`
/// inside a comment is NOT the speaker close). `None` if none outside a comment.
fn speaker_close(header: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < header.len() {
        if header[i] == b'/' && header.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < header.len() && !(header[i] == b'*' && header.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(header.len()); // past the closing */ (clamped)
            continue;
        }
        if header[i] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Collect every `Node::Line` in document order, descending into branch choices'
/// bodies and match arms' bodies (mirrors `check.rs::Walker::walk`'s recursion).
fn collect_lines<'a>(nodes: &'a [Node], out: &mut Vec<&'a Line>) {
    for node in nodes {
        match node {
            Node::Line(l) => out.push(l),
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_lines(&choice.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_lines(body, out)
                        }
                    }
                }
            }
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_ATTRS: &str =
        "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[narrator]: hi there\n";
    const WITH_ATTRS: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[fixer]{delivery=\"thought\"}: hmm\n";
    const ALREADY: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[fixer]{code=\"0010\"}: kept\n";

    #[test]
    fn tags_line_without_attrs() {
        let out = tag_document(NO_ATTRS);
        assert_eq!(out.added, 1);
        assert!(
            out.text
                .contains(":line[narrator]{code=\"0010\"}: hi there"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn tags_line_with_existing_attrs() {
        let out = tag_document(WITH_ATTRS);
        assert_eq!(out.added, 1);
        assert!(out.text.contains("code=\"0010\""), "got:\n{}", out.text);
        assert!(
            out.text.contains("delivery=\"thought\""),
            "existing attr preserved:\n{}",
            out.text
        );
    }

    #[test]
    fn already_tagged_is_untouched_and_idempotent() {
        let out = tag_document(ALREADY);
        assert_eq!(out.added, 0);
        assert_eq!(out.text, ALREADY);
    }

    #[test]
    fn new_codes_step_above_max_existing() {
        // one tagged 0050 + one untagged -> untagged gets 0060 (above max), tagged kept
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[a]{code=\"0050\"}: one\n:line[b]: two\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(out.text.contains(":line[a]{code=\"0050\"}: one"));
        assert!(out.text.contains("code=\"0060\""), "got:\n{}", out.text);
    }

    #[test]
    fn tagging_output_is_idempotent() {
        let once = tag_document(NO_ATTRS).text;
        let twice = tag_document(&once);
        assert_eq!(twice.added, 0);
        assert_eq!(twice.text, once);
    }

    #[test]
    fn tagged_output_still_parses_clean() {
        // the rewritten doc must have no NEW parse errors (the inserted code attr is valid)
        let out = tag_document(NO_ATTRS);
        let (_doc, diags) = lute_syntax::parse(&out.text);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == lute_core_span::Severity::Error),
            "{diags:?}"
        );
    }

    #[test]
    fn comment_brace_in_header_is_not_an_attr_block() {
        // A blanked block comment containing `{` before the `:` must NOT be treated
        // as an attr block; the code goes in a real `{code=...}` after `]`, and a
        // second run is a no-op.
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[narrator] /* { */ : hi\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains(":line[narrator]{code=\"0010\"}"),
            "got:\n{}",
            out.text
        );
        // parses clean AND is now idempotent
        let (_d, diags) = lute_syntax::parse(&out.text);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == lute_core_span::Severity::Error),
            "{diags:?}"
        );
        let twice = tag_document(&out.text);
        assert_eq!(twice.added, 0, "must be idempotent after the comment case");
        assert_eq!(twice.text, out.text);
    }

    #[test]
    fn code_above_u32_max_does_not_collide() {
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[a]{code=\"4294967295\"}: one\n:line[b]: two\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        // new code is strictly above the existing max (no duplicate 4294967295)
        assert!(
            out.text.contains("code=\"4294967305\""),
            "got:\n{}",
            out.text
        );
        assert!(
            out.text.matches("4294967295").count() == 1,
            "existing max untouched + not duplicated:\n{}",
            out.text
        );
    }

    #[test]
    fn comment_bracket_in_speaker_is_not_the_close() {
        // A `]` inside a block comment within the speaker bracket must NOT be taken
        // as the speaker close; re-tagging MUST be idempotent (the bug makes the
        // code land in the comment, get stripped on re-parse, and re-tag forever).
        let src =
            "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[hero/* ] */]: hi\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        let twice = tag_document(&out.text);
        assert_eq!(
            twice.added, 0,
            "second tag run must be a no-op (idempotent)"
        );
        assert_eq!(twice.text, out.text, "idempotent: byte-identical on re-run");
    }

    #[test]
    fn code_at_u64_max_fails_closed_no_collision() {
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[a]{code=\"18446744073709551615\"}: one\n:line[b]: two\n";
        let out = tag_document(src);
        assert_eq!(
            out.added, 0,
            "counter overflow must fail closed, not emit a colliding code"
        );
        assert_eq!(
            out.text.matches("18446744073709551615").count(),
            1,
            "no duplicate of the max code"
        );
    }
}
