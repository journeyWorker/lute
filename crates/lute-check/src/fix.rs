//! `lute fix` 0.0.1 → 0.1.0 migration codemod (dsl §7.1, §7.3). Two byte-exact,
//! comment-preserving span rewrites over one `.lute` document:
//!
//!   1. **`:line[speaker]{…}: text` → `:speaker{…}: text`** — the removed 0.0.1
//!      bracket content-line form. The 0.1.0 parser REJECTS `:line[` with an
//!      `E-UNCLASSIFIED` diagnostic and recovers by DROPPING the line node, so
//!      there is no AST node to drive the rewrite from. Instead the parser
//!      attaches a `migrate`-kind `Fixit` (a `TextEdit` replacing the
//!      `:line[speaker]` span with `:speaker`) to that diagnostic; phase 1
//!      applies those fix-its.
//!   2. **`<choice>`/`<hub>`-choice `as="…"` → `into="…"`** — the persist-target
//!      attr rename. `as="…"` on a choice PARSES cleanly (a generic attr; its
//!      persist meaning is a CHECK-stage concern), so once phase 1 has removed
//!      the `:line[` parse errors the document parses clean and phase 2 walks the
//!      AST's `<choice>`/`<hub>` choices for an `as` key and rewrites it to
//!      `into`. **`as` on a CONTENT LINE stays** — it is a display-label override
//!      (dsl §7.1), never a persist target, so `Line.attrs` are never touched.
//!
//! Mirrors `tag.rs`'s splice discipline: collect target `(start, end,
//! replacement)` spans, then splice back-to-front (descending `byte_start`) so
//! earlier offsets stay valid. Spans are ORIGINAL-source offsets and
//! comment-blanking is length-preserving (parser SPAN-FIDELITY contract), so a
//! byte offset maps 1:1 onto the original text.

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, Choice, Node};
use lute_syntax::parse;

/// The result of a migration pass: the (possibly rewritten) document text and
/// how many span edits were applied across both phases.
#[derive(Clone, Debug, PartialEq)]
pub struct FixResult {
    pub text: String,
    pub changed: usize,
}

/// Migrate a 0.0.1-shaped document to 0.1.0 in place (see module docs).
/// Idempotent, deterministic, total: an already-0.1.0 document (or one whose
/// phase-1 output still fails to parse) is returned with `changed: 0` /
/// phase-1-only.
pub fn fix_document(text: &str) -> FixResult {
    // -- phase 1: apply the parser's `:line[` migrate fix-its (back-to-front) --
    let (_doc, diags) = parse(text);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for d in &diags {
        for fx in &d.fixits {
            if fx.kind == "migrate" {
                for te in &fx.edit {
                    edits.push((te.span.byte_start, te.span.byte_end, te.new_text.clone()));
                }
            }
        }
    }
    let phase1 = edits.len();
    let text1 = splice(text, edits);

    // -- phase 2: re-parse; if clean, rewrite choice/hub `as` keys to `into` ---
    let (doc2, diags2) = parse(&text1);
    // A remaining parse error means phase 1 didn't fully migrate (or the doc had
    // an unrelated structural error): skip phase 2, return the phase-1 text.
    if diags2.iter().any(|d| d.severity == Severity::Error) {
        return FixResult {
            text: text1,
            changed: phase1,
        };
    }

    let mut choices: Vec<&Choice> = Vec::new();
    for shot in &doc2.shots {
        collect_choices(&shot.body, &mut choices);
    }
    let mut edits2: Vec<(usize, usize, String)> = Vec::new();
    for c in &choices {
        if let Some(a) = c.attrs.iter().find(|a| a.key == "as") {
            // Rewrite only the KEY span (`as`), preserving the value. `Attr.span`
            // starts at the key's first byte (`scan_attrs` builds it as
            // `span(key_start, ..)`), so the key occupies `[byte_start,
            // byte_start + key.len())`.
            let start = a.span.byte_start;
            edits2.push((start, start + a.key.len(), "into".to_string()));
        }
    }
    let phase2 = edits2.len();
    let text2 = splice(&text1, edits2);

    FixResult {
        text: text2,
        changed: phase1 + phase2,
    }
}

/// Apply `(start, end, replacement)` span edits to `text`, splicing back-to-front
/// (descending `start`) so earlier offsets remain valid. Empty `edits` returns
/// `text` verbatim (byte-identical, no allocation churn beyond the owned copy).
fn splice(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    if edits.is_empty() {
        return text.to_string();
    }
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = text.to_string();
    for (start, end, replacement) in &edits {
        out.replace_range(*start..*end, replacement);
    }
    out
}

/// Collect every `<choice>` (branch choices + hub choices) in document order,
/// recursing into choice bodies and match-arm bodies (mirrors
/// `tag.rs::collect_lines`, but collects CHOICES). Never descends into
/// `Line`/`Directive`/`Set`/`Timeline` — a choice never nests there.
fn collect_choices<'a>(nodes: &'a [Node], out: &mut Vec<&'a Choice>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    out.push(choice);
                    collect_choices(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    out.push(choice);
                    collect_choices(&choice.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_choices(body, out)
                        }
                    }
                }
            }
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FM: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

    fn wrap(body: &str) -> String {
        format!("{FM}{body}")
    }

    #[test]
    fn migrates_line_bracket_with_attrs() {
        let out = fix_document(&wrap(":line[bianca]{emotion=\"x\"}: hi\n"));
        assert!(out.changed >= 1, "changed: {}", out.changed);
        assert!(
            out.text.contains(":bianca{emotion=\"x\"}: hi"),
            "got:\n{}",
            out.text
        );
        assert!(!out.text.contains(":line["), "`:line[` must be gone:\n{}", out.text);
    }

    #[test]
    fn migrates_line_bracket_no_attrs() {
        let out = fix_document(&wrap(":line[narrator]: plain\n"));
        assert!(out.changed >= 1, "changed: {}", out.changed);
        assert!(
            out.text.contains(":narrator: plain"),
            "got:\n{}",
            out.text
        );
        assert!(!out.text.contains(":line["), "got:\n{}", out.text);
    }

    #[test]
    fn migrates_branch_choice_as_to_into() {
        let out = fix_document(&wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:bianca: hi\n</choice>\n</branch>\n",
        ));
        assert_eq!(out.changed, 1, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn migrates_hub_choice_as_to_into() {
        let out = fix_document(&wrap(
            "<hub id=\"h\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:bianca: hi\n</choice>\n</hub>\n",
        ));
        assert_eq!(out.changed, 1, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn content_line_as_label_override_is_untouched() {
        // `:bianca{as="???"}: hi` is a display-label override (dsl §7.1), NOT a
        // persist target — it must survive byte-identical.
        let src = wrap(":bianca{as=\"curt\"}: hi\n");
        let out = fix_document(&src);
        assert_eq!(out.changed, 0, "content-line `as` must not migrate");
        assert_eq!(out.text, src, "byte-identical");
    }

    #[test]
    fn already_010_doc_is_byte_identical() {
        let src = wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" into=\"run.flag\">\n:speaker: hi\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 0);
        assert_eq!(out.text, src, "byte-identical");
    }

    #[test]
    fn migrates_both_line_and_choice_as() {
        let src = wrap(
            ":line[bianca]{emotion=\"x\"}: hi\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:fixer: yo\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 2, "both phases fire; got:\n{}", out.text);
        assert!(out.text.contains(":bianca{emotion=\"x\"}: hi"), "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
        // Idempotent: re-running the migrated doc changes nothing.
        let again = fix_document(&out.text);
        assert_eq!(again.changed, 0, "second pass is a no-op");
        assert_eq!(again.text, out.text);
    }
}
