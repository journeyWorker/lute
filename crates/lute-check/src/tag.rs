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

    // Highest existing numeric `code` (parseable as `u32`); default 0.
    let mut max_code: u32 = 0;
    for line in &lines {
        for attr in &line.attrs {
            if attr.key == "code" {
                if let AttrValue::Str(s) = &attr.value {
                    if let Ok(n) = s.trim().parse::<u32>() {
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
    let mut next_code = max_code;
    let mut inserts: Vec<(usize, String)> = Vec::with_capacity(untagged.len());
    for line in &untagged {
        next_code = next_code.saturating_add(10);
        let code = format!("{next_code:04}");
        let hdr_start = line.span.byte_start;
        // Header = `:line[speaker]{attrs}?: WS`, before the opaque text.
        let header = &text[hdr_start..line.text_span.byte_start];
        if let Some(rel) = header.bytes().position(|b| b == b'{') {
            // Existing attr block: insert after the `{`, with a trailing space
            // only when the block is non-empty (next byte isn't `}`).
            let at = hdr_start + rel + 1;
            let inserted = if bytes.get(at) == Some(&b'}') {
                format!("code=\"{code}\"")
            } else {
                format!("code=\"{code}\" ")
            };
            inserts.push((at, inserted));
        } else if let Some(rel) = header.bytes().position(|b| b == b']') {
            // No attr block: insert a fresh `{code="ID"}` after the speaker `]`.
            let at = hdr_start + rel + 1;
            inserts.push((at, format!("{{code=\"{code}\"}}")));
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
        added: untagged.len(),
    }
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
}
