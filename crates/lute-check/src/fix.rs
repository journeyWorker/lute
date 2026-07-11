//! `lute fix` migration codemod (dsl §7.1, §7.3). Byte-exact,
//! comment-preserving span rewrites over one `.lute` document:
//!
//!   1. **`:line[speaker]{…}: text` → `@speaker{…}: text`** — the removed 0.0.1
//!      bracket content-line form. The parser REJECTS `:line[` with an
//!      `E-UNCLASSIFIED` diagnostic and recovers by DROPPING the line node, so
//!      there is no AST node to drive the rewrite from. Instead the parser
//!      attaches a `migrate`-kind `Fixit` (a `TextEdit` replacing the
//!      `:line[speaker]` span with `@speaker` — the CURRENT 0.2.2 sigil,
//!      foundation C1) to that diagnostic; phase 1 applies those fix-its.
//!   2. **`<choice>`/`<hub>`-choice `as="…"` → `into="…"`** — the persist-target
//!      attr rename. `as="…"` on a choice PARSES cleanly (a generic attr; its
//!      persist meaning is a CHECK-stage concern), so once phase 1 has removed
//!      the `:line[` parse errors the document parses clean and phase 2 walks the
//!      AST's `<choice>`/`<hub>` choices for an `as` key and rewrites it to
//!      `into`. **`as` on a CONTENT LINE stays** — it is a display-label override
//!      (dsl §7.1), never a persist target, so `Line.attrs` are never touched.
//!   3. **any OTHER content-line leading `:` sigil → `@`** (dsl 0.2.2 §7.1,
//!      Task C3 foundation, now that the 0.3.0 grammar break — C1 — has
//!      landed and `@` is the only legal content-line sigil) — the parser
//!      attaches the SAME kind of single-byte `migrate` `Fixit` as rule 1 to
//!      every `:`-led content line's `E-UNCLASSIFIED` diagnostic (regardless
//!      of what follows: missing second `:`, malformed attrs, …), so PHASE 1
//!      now migrates every sigil in one pass, same as rule 1. The phase-2
//!      AST walk below (over `Line` nodes with a leading `:` byte) is kept as
//!      a defensive no-op — by construction phase 1 already rewrote every
//!      `:`-led line, so phase 2's re-parse never sees one — rather than
//!      deleted, since it costs nothing and guards against a future gap in
//!      the parser's fix-it coverage.
//!
//! Mirrors `tag.rs`'s splice discipline: collect target `(start, end,
//! replacement)` spans, then splice back-to-front (descending `byte_start`) so
//! earlier offsets stay valid. Spans are ORIGINAL-source offsets and
//! comment-blanking is length-preserving (parser SPAN-FIDELITY contract), so a
//! byte offset maps 1:1 onto the original text.

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, Choice, Line, Node};
use lute_syntax::parse;

/// The result of a migration pass: the (possibly rewritten) document text and
/// how many span edits were applied across both phases.
#[derive(Clone, Debug, PartialEq)]
pub struct FixResult {
    pub text: String,
    pub changed: usize,
}

/// Migrate a 0.0.1-shaped document toward 0.2.2-readiness in place (see
/// module docs). Idempotent, deterministic, total: an already-migrated
/// document (or one whose phase-1 output still fails to parse) is returned
/// with `changed: 0` / phase-1-only.
pub fn fix_document(text: &str) -> FixResult {
    // -- phase 1: apply the parser's `migrate` fix-its (back-to-front) — the
    // `:line[speaker]` bracket form (rule 1) AND any other `:`-led content
    // line's sigil (rule 3) both attach one here, so phase 1 migrates every
    // sigil in the document, not just the legacy bracket form --
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

    // -- phase 2: re-parse; if clean, rewrite choice/hub `as` keys to `into`.
    // The leading-`:` sigil walk below (rule 3) is a defensive no-op here —
    // phase 1 already rewrote every `:`-led line via its own `migrate`
    // fix-it (see module docs) — kept in case a future parser change adds a
    // `:`-led shape phase 1 doesn't cover.
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
    // Quest bodies (dsl 0.2.0 §6.7) can nest `<branch>`/`<choice as=>` too —
    // migrate them like scene bodies (mirrors the doc.quests traversal every
    // other 0.2.0 walker uses).
    for quest in &doc2.quests {
        collect_choices(&quest.body, &mut choices);
    }
    let mut lines: Vec<&Line> = Vec::new();
    for shot in &doc2.shots {
        collect_lines(&shot.body, &mut lines);
    }
    for quest in &doc2.quests {
        collect_lines(&quest.body, &mut lines);
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
    let bytes1 = text1.as_bytes();
    for l in &lines {
        // `Line.span.byte_start` is the offset of the leading sigil
        // (`parse_line` sets `span = self.span(cstart, line_end)` where
        // `cstart` is the sigil byte itself, dsl §7.1) — a single-byte
        // replace. Always a no-op in practice (see the phase-2 comment
        // above): phase 1's `migrate` fix-its already rewrote every `:`-led
        // line, so every `Line` reaching here already starts `@`.
        let start = l.span.byte_start;
        if bytes1.get(start) == Some(&b':') {
            edits2.push((start, start + 1, "@".to_string()));
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
            Node::On(o) => collect_choices(&o.body, out),
            Node::Objective(o) => collect_choices(&o.body, out),
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
        }
    }
}

/// Collect every content `Line` in document order, recursing into branch
/// choices' bodies, hub choices' bodies, match-arm bodies, and on/objective
/// bodies (mirrors `collect_choices` above; dsl 0.2.2 §7.1, Task C3
/// foundation — a `Line` never nests inside a `Directive`/`Set`/`Timeline`).
fn collect_lines<'a>(nodes: &'a [Node], out: &mut Vec<&'a Line>) {
    for node in nodes {
        match node {
            Node::Line(l) => out.push(l),
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_lines(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
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
            Node::On(o) => collect_lines(&o.body, out),
            Node::Objective(o) => collect_lines(&o.body, out),
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
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
            out.text.contains("@bianca{emotion=\"x\"}: hi"),
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
            out.text.contains("@narrator: plain"),
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
        assert_eq!(out.changed, 2, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@bianca: hi"),
            "nested content-line sigil not migrated, got:\n{}",
            out.text
        );
    }

    #[test]
    fn migrates_hub_choice_as_to_into() {
        let out = fix_document(&wrap(
            "<hub id=\"h\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:bianca: hi\n</choice>\n</hub>\n",
        ));
        assert_eq!(out.changed, 2, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@bianca: hi"),
            "nested content-line sigil not migrated, got:\n{}",
            out.text
        );
    }

    #[test]
    fn content_line_as_label_override_is_untouched() {
        // `:bianca{as="???"}: hi` is a display-label override (dsl §7.1), NOT a
        // persist target — the `as` attr itself must survive untouched, even
        // though the line's leading sigil still migrates to `@` (dsl 0.2.2
        // §7.1, Task C3).
        let src = wrap(":bianca{as=\"curt\"}: hi\n");
        let out = fix_document(&src);
        assert_eq!(out.changed, 1, "only the sigil migrates: {}", out.changed);
        assert!(
            out.text.contains("@bianca{as=\"curt\"}: hi"),
            "label-override `as` must survive untouched, got:\n{}",
            out.text
        );
    }

    #[test]
    fn already_010_choice_only_sigil_migrates() {
        // A doc already migrated to 0.1.0 (`into=`, no `:line[`) has nothing
        // left for the `as`→`into` rule to fire on; only the new sigil
        // rewrite fires (dsl 0.2.2 §7.1, Task C3).
        let src = wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" into=\"run.flag\">\n:speaker: hi\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 1, "got:\n{}", out.text);
        assert!(out.text.contains("@speaker: hi"), "got:\n{}", out.text);
        assert!(
            out.text.contains("into=\"run.flag\""),
            "already-migrated choice attr must stay untouched, got:\n{}",
            out.text
        );
    }

    #[test]
    fn migrates_both_line_and_choice_as() {
        let src = wrap(
            ":line[bianca]{emotion=\"x\"}: hi\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:fixer: yo\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        // phase1 (`:line[` removal + both lines' `:`→`@`) + phase2 (`as`→`into`).
        assert_eq!(out.changed, 3, "all rules fire; got:\n{}", out.text);
        assert!(out.text.contains("@bianca{emotion=\"x\"}: hi"), "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.flag\">"),
            "got:\n{}",
            out.text
        );
        assert!(out.text.contains("@fixer: yo"), "got:\n{}", out.text);
        // Idempotent: re-running the migrated doc changes nothing.
        let again = fix_document(&out.text);
        assert_eq!(again.changed, 0, "second pass is a no-op");
        assert_eq!(again.text, out.text);
    }

    #[test]
    fn migrates_choice_as_inside_quest_on_and_objective_bodies() {
        // 0.2.0 merge fix: `fix_document` phase 2 must also seed from
        // `doc.quests` (not just `doc.shots`) and `collect_choices` must
        // recurse `Node::On`/`Node::Objective` bodies, so a `<branch>`/
        // `<choice as=>` nested inside EITHER a quest's `<on>` body or an
        // `<objective>` body gets migrated — pre-merge, `fix_document` only
        // walked `doc.shots`, so this quest doc would parse clean but yield
        // `changed: 0` and leave both `as=` keys untouched.
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.x\">\n:narrator: hi\n</choice>\n</branch>\n</on>\n<objective id=\"o\" done=\"run.d\">\n<branch id=\"b2\">\n<choice id=\"c2\" label=\"M\" as=\"run.y\">\n:narrator: yo\n</choice>\n</branch>\n</objective>\n</quest>\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 4, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.x\">"),
            "on-nested choice not migrated, got:\n{}",
            out.text
        );
        assert!(
            out.text.contains("<choice id=\"c2\" label=\"M\" into=\"run.y\">"),
            "objective-nested choice not migrated, got:\n{}",
            out.text
        );
        assert!(!out.text.contains("as=\"run.x\""), "got:\n{}", out.text);
        assert!(!out.text.contains("as=\"run.y\""), "got:\n{}", out.text);
        assert!(
            out.text.contains("@narrator: hi"),
            "on-nested line sigil not migrated, got:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@narrator: yo"),
            "objective-nested line sigil not migrated, got:\n{}",
            out.text
        );
    }

    #[test]
    fn migrates_speaker_colon_to_at() {
        let out = fix_document("## Shot 1.\n:bianca{code=\"0010\"}: hi\n:narrator: x\n");
        assert!(out.text.contains("@bianca{code=\"0010\"}: hi"));
        assert!(out.text.contains("@narrator: x"));
        // idempotent
        assert_eq!(fix_document(&out.text).text, out.text);
    }
}
