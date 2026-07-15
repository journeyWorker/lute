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
//!   4. **`<choice>`/`<hub>`-choice `persist="run"` → deleted** (dsl 0.6.0
//!      §2.3) when the choice ALSO carries `into=`. Under 0.6.0 `into=` alone
//!      records the run fact, so deleting a `persist="run"` from an
//!      `into=`-carrying choice is meaning-preserving in BOTH directions (the
//!      pair recorded before; the bare `into=` records now) — clearing the D16
//!      bar for an automatic, unprompted `"migrate"` codemod, unlike 0.4.0's
//!      retired `"refactor"` remedies. A `persist=` with any other value, or
//!      without `into=`, was already an error and stays MANUAL (the checker's
//!      `E-PERSIST-REMOVED` still offers the deletion; `lute fix` applies it
//!      only in this provable shape). Runs in phase 2's AST walk beside the
//!      `as`→`into` rule; comment-preserving and idempotent (the deletion
//!      removes the only `persist=` key, so a re-run never re-fires).
//!   5. **shot-heading `## Shot N.`/`## Scene N.` prefix strip** (dsl 0.6.0
//!      §3.4) — the pre-0.6.0 `## Shot|Scene <int>.` heading grammar is gone
//!      (§3.1: a heading is free text now), so a legacy `## Shot 3. The Alley`
//!      would otherwise leak the grammar prefix into the free-text title. When
//!      a title FOLLOWS the `Shot|Scene <int>.` prefix, strip the prefix (and
//!      its separating whitespace) → `## The Alley`. A BARE `## Shot 3.` /
//!      `## Scene 3.` (no trailing title) is LEFT UNTOUCHED — stripping would
//!      leave an empty heading, and the bare form is itself a valid free
//!      title. Byte-exact, comment-preserving, idempotent (a stripped title no
//!      longer matches the prefix shape).
//!
//! Mirrors `tag.rs`'s splice discipline: collect target `(start, end,
//! replacement)` spans, then splice back-to-front (descending `byte_start`) so
//! earlier offsets stay valid. Spans are ORIGINAL-source offsets and
//! comment-blanking is length-preserving (parser SPAN-FIDELITY contract), so a
//! byte offset maps 1:1 onto the original text.

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, AttrValue, Choice, Line, Node};
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

    // -- `<choice>`/`<hub>`-choice `persist="run"` → deleted (dsl 0.6.0 §2.3),
    // when the choice ALSO carries `into=`. Under 0.6.0 `into=` alone records,
    // so deleting a `persist="run"` from an `into=`-carrying choice is
    // meaning-preserving in BOTH directions — it clears the D16 bar for an
    // automatic `"migrate"` codemod (unlike 0.4.0's retired `"refactor"`
    // remedies). A `persist=` with any other value, or without `into=`, was
    // already an error and stays MANUAL (E-PERSIST-REMOVED offers the deletion;
    // `lute fix` applies it only in this provable shape). Reuses
    // `widen_removed_attr` so the LSP fixit and `lute fix` agree byte-for-byte.
    // Idempotent: the deletion removes the only `persist=` key, so a re-run
    // never re-fires.
    for c in &choices {
        if !c.attrs.iter().any(|a| a.key == "into") {
            continue;
        }
        if let Some(p) = c.attrs.iter().find(|a| a.key == "persist") {
            if matches!(&p.value, AttrValue::Str(s) if s == "run") {
                let (start, end) =
                    widen_removed_attr(bytes1, p.span.byte_start, p.span.byte_end);
                edits2.push((start, end, String::new()));
            }
        }
    }

    // -- shot-heading `Shot N.`/`Scene N.` prefix strip (dsl 0.6.0 §3.4): the
    // pre-0.6.0 `## Shot|Scene <int>.` grammar is gone (§3.1 — a heading is
    // free text now), so a legacy `## Shot 3. The Alley` leaks the grammar
    // prefix into the free-text title. Strip `Shot|Scene <int>. ` when a title
    // FOLLOWS it, leaving `## The Alley`; a BARE `## Shot 3.`/`## Scene 3.` (no
    // trailing title) is LEFT UNTOUCHED — stripping would empty the heading,
    // and the bare form is a valid free title. `Shot.span.byte_start` is the
    // heading line's leading `#`. Byte-exact, comment-preserving, idempotent
    // (a stripped title no longer matches the prefix shape).
    for shot in &doc2.shots {
        if let Some((start, end)) = shot_prefix_delete(bytes1, shot.span.byte_start) {
            edits2.push((start, end, String::new()));
        }
    }

    // -- `delivery="…"` → bare flag (dsl 0.2.2 §7.1, Task D3 foundation):
    // runs in this SAME edits2/text1 pass as the sigil + `as`→`into` rules
    // above. `delivery="thought"`/`"voiceover"` rewrite to the bare
    // `{mono}`/`{vo}` flags; `delivery="spoken"` is dropped outright (spoken
    // is the 0.2.2 default, no flag needed) — if it was the line's only
    // attr the now-empty `{…}` is dropped too (a brace-less
    // `@speaker: text` line parses fine per `parse_line` above), otherwise
    // just the attr plus one adjacent whitespace separator is removed so
    // the remaining attrs stay single-space-clean. Idempotent: a bare
    // `{mono}`/`{vo}` flag has no `delivery` key, so it never matches. A
    // `delivery=` attr span sits strictly inside `{…}`, well past the
    // leading sigil byte the rule above targets, so the two rules never
    // overlap.
    for l in &lines {
        for a in &l.attrs {
            if a.key != "delivery" {
                continue;
            }
            let AttrValue::Str(v) = &a.value else {
                continue;
            };
            let start = a.span.byte_start;
            let end = a.span.byte_end;
            match v.as_str() {
                "thought" => edits2.push((start, end, "mono".to_string())),
                "voiceover" => edits2.push((start, end, "vo".to_string())),
                "spoken" => {
                    if l.attrs.len() == 1 {
                        match find_enclosing_braces(bytes1, l.span.byte_start, l.text_span.byte_start, start, end) {
                            Some((bs, be)) => edits2.push((bs, be, String::new())),
                            None => edits2.push((start, end, String::new())),
                        }
                    } else {
                        let (ws, we) = widen_removed_attr(bytes1, start, end);
                        edits2.push((ws, we, String::new()));
                    }
                }
                _ => {}
            }
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
            Node::Assert(_) | Node::Retract(_) => {}
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
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// Given a `delivery=` [`Attr`](lute_syntax::ast::Attr) span `[attr_start,
/// attr_end)` that is a content line's ONLY attr, locate the enclosing
/// `{`/`}` byte pair so the codemod can drop the whole now-empty `{…}`
/// (Task D3) rather than leave stray braces behind. Only whitespace may sit
/// between the brace and the attr (that's how `scan_attrs` reaches it as the
/// sole entry), so a non-whitespace, non-brace byte aborts the scan.
/// `[scan_start, scan_end)` bounds the search to the line's own
/// speaker/attrs prefix (`Line.span.byte_start` .. `Line.text_span.byte_start`)
/// so it can never wander into the line's text or a neighboring line.
fn find_enclosing_braces(
    bytes: &[u8],
    scan_start: usize,
    scan_end: usize,
    attr_start: usize,
    attr_end: usize,
) -> Option<(usize, usize)> {
    let mut open = None;
    let mut i = attr_start;
    while i > scan_start {
        i -= 1;
        match bytes[i] {
            b'{' => {
                open = Some(i);
                break;
            }
            b' ' | b'\t' => continue,
            _ => break,
        }
    }
    let open = open?;
    let mut close = None;
    let mut j = attr_end;
    while j < scan_end {
        match bytes[j] {
            b'}' => {
                close = Some(j);
                break;
            }
            b' ' | b'\t' => j += 1,
            _ => break,
        }
    }
    let close = close?;
    Some((open, close + 1))
}

/// Widen a to-be-deleted attr's `[start, end)` span to also swallow ONE
/// adjacent whitespace separator, so dropping a first/middle attr among
/// siblings doesn't leave a stray double space and dropping the LAST attr
/// doesn't leave a stray leading space before `}` (dsl §4.5 attrs are
/// whitespace-separated). Prefers the TRAILING separator; falls back to the
/// LEADING one only when there's no trailing whitespace to take (i.e. the
/// attr was last, immediately followed by `}`/line end).
fn widen_removed_attr(bytes: &[u8], start: usize, end: usize) -> (usize, usize) {
    let mut new_end = end;
    while new_end < bytes.len() && matches!(bytes[new_end], b' ' | b'\t') {
        new_end += 1;
    }
    if new_end > end {
        return (start, new_end);
    }
    let mut new_start = start;
    while new_start > 0 && matches!(bytes[new_start - 1], b' ' | b'\t') {
        new_start -= 1;
    }
    (new_start, end)
}

/// dsl 0.6.0 §3.4: the byte range of a legacy `Shot|Scene <int>. ` grammar
/// prefix to DELETE from a free-text shot heading, when a title FOLLOWS it —
/// `## Shot 3. The Alley` → `## The Alley`. `line_start` is the heading line's
/// first byte (the leading `#`, i.e. `Shot.span.byte_start`). Returns `None`
/// for a BARE `## Shot 3.`/`## Scene 3.` (no trailing title: stripping would
/// empty the heading, and the bare form is a valid free title now) and for any
/// heading that isn't the legacy `Shot|Scene <int>.` shape. The deleted range
/// runs from the keyword's first byte through the whitespace separating the
/// period from the title, so the surviving title keeps its exact bytes.
fn shot_prefix_delete(bytes: &[u8], line_start: usize) -> Option<(usize, usize)> {
    if bytes.get(line_start..line_start + 3) != Some(b"## ".as_slice()) {
        return None;
    }
    let byte = |k: usize| bytes.get(k).copied();
    // Skip any extra whitespace between `## ` and the keyword.
    let mut i = line_start + 3;
    while matches!(byte(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let kw_start = i;
    let kw_len = if bytes.get(kw_start..kw_start + 4) == Some(b"Shot".as_slice()) {
        4
    } else if bytes.get(kw_start..kw_start + 5) == Some(b"Scene".as_slice()) {
        5
    } else {
        return None;
    };
    i = kw_start + kw_len;
    // One or more spaces, then a 1+ digit integer, then a literal `.`.
    let ws0 = i;
    while matches!(byte(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if i == ws0 {
        return None;
    }
    let dig0 = i;
    while matches!(byte(i), Some(b'0'..=b'9')) {
        i += 1;
    }
    if i == dig0 || byte(i) != Some(b'.') {
        return None;
    }
    i += 1; // consume `.`
    // Separating whitespace, then a non-empty title up to end of line.
    let ws1 = i;
    while matches!(byte(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if i == ws1 {
        return None; // bare `## Shot 3.` (no space+title after the period)
    }
    let rest_start = i;
    let mut j = rest_start;
    while !matches!(byte(j), None | Some(b'\n' | b'\r')) {
        j += 1;
    }
    let mut end = j;
    while end > rest_start && matches!(byte(end - 1), Some(b' ' | b'\t')) {
        end -= 1;
    }
    if end == rest_start {
        return None; // only trailing whitespace after the period
    }
    Some((kw_start, rest_start))
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
    fn lute_fix_never_touches_bare_into() {
        // D16 / 0.6.0 §2.3: `lute fix`'s persist-removal rule fires ONLY on a
        // `persist="run"` + `into=` pair. A bare `into=` (no `persist=`) records
        // on its own under 0.6.0 — nothing to migrate, and no `as` attr to
        // rewrite either, so `lute fix` leaves it byte-identical (changed == 0).
        // The content line already uses the current `@` sigil.
        let src = wrap(
            "<branch id=\"b\">\n<choice id=\"help\" label=\"Help\" into=\"run.metHelpfully\">\n@bianca: hi\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 0, "got:\n{}", out.text);
        assert_eq!(
            out.text, src,
            "lute fix must leave a bare `into=` byte-identical (D16)"
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

    #[test]
    fn migrates_delivery_attr_to_flag() {
        let out = fix_document("## Shot 1.\n@x{delivery=\"thought\"}: a\n@y{delivery=\"voiceover\"}: b\n");
        assert!(out.text.contains("{mono}") && out.text.contains("{vo}"), "got:\n{}", out.text);
        assert!(!out.text.contains("delivery="), "got:\n{}", out.text);
    }

    #[test]
    fn migrates_delivery_spoken_is_removed() {
        // `delivery="spoken"` is the 0.2.2 default — removed outright, no
        // flag substituted, and (being the line's only attr) the now-empty
        // `{…}` drops too so the line reads as a bare `@speaker: text`
        // (confirmed the parser accepts a brace-less content line).
        let out = fix_document("## Shot 1.\n@z{delivery=\"spoken\"}: c\n");
        assert!(!out.text.contains("delivery="), "got:\n{}", out.text);
        assert!(
            !out.text.contains('{') && !out.text.contains('}'),
            "empty braces must be dropped too, got:\n{}",
            out.text
        );
        assert!(out.text.contains("@z: c"), "got:\n{}", out.text);
        // still parses clean.
        let (_doc, diags) = parse(&out.text);
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "migrated output must still parse: {diags:?}"
        );
    }

    #[test]
    fn migrates_delivery_spoken_among_other_attrs_keeps_braces() {
        // `spoken` removal among sibling attrs must not orphan them or leave
        // a stray double space — only the `delivery=` attr (plus exactly one
        // adjacent separator) is dropped, whichever side it sits on.
        let out = fix_document(
            "## Shot 1.\n@z{emotion=\"x\" delivery=\"spoken\"}: c\n@w{delivery=\"spoken\" emotion=\"x\"}: d\n",
        );
        assert!(!out.text.contains("delivery="), "got:\n{}", out.text);
        assert!(out.text.contains("@z{emotion=\"x\"}: c"), "got:\n{}", out.text);
        assert!(out.text.contains("@w{emotion=\"x\"}: d"), "got:\n{}", out.text);
    }

    #[test]
    fn delivery_flag_migration_is_idempotent() {
        // Already-bare `{mono}`/`{vo}` flags (no `delivery=` key) must emit
        // no edits — the rule only ever fires on a `delivery=` attr key.
        let src = "## Shot 1.\n@x{mono}: a\n@y{vo}: b\n@z: c\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 0, "already-bare flags must not be touched: {}", out.text);
        assert_eq!(out.text, src);
    }

    #[test]
    fn migrates_choice_persist_run_with_into_deletes_persist() {
        // dsl 0.6.0 §2.3: a `persist="run"` on an `into=`-carrying choice is
        // meaning-preserving to delete (the pair recorded before; `into=`
        // records now) — `lute fix` applies the `"migrate"` deletion.
        let out = fix_document(&wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" persist=\"run\" into=\"run.x\">\n@bianca: hi\n</choice>\n</branch>\n",
        ));
        assert_eq!(out.changed, 1, "only the persist attr is deleted; got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.x\">"),
            "persist= must splice away leaving one clean space, got:\n{}",
            out.text
        );
        assert!(!out.text.contains("persist="), "got:\n{}", out.text);
        // still parses clean, and re-running is a no-op (idempotent).
        let (_doc, diags) = parse(&out.text);
        assert!(!diags.iter().any(|d| d.severity == Severity::Error), "{diags:?}");
        let again = fix_document(&out.text);
        assert_eq!(again.changed, 0, "second pass is a no-op");
        assert_eq!(again.text, out.text);
    }

    #[test]
    fn migrates_hub_choice_persist_run_with_into_deletes_persist() {
        let out = fix_document(&wrap(
            "<hub id=\"h\">\n<choice id=\"c\" label=\"L\" persist=\"run\" into=\"run.x\" exit>\n@bianca: hi\n</choice>\n</hub>\n",
        ));
        assert_eq!(out.changed, 1, "got:\n{}", out.text);
        assert!(
            out.text.contains("<choice id=\"c\" label=\"L\" into=\"run.x\" exit>"),
            "got:\n{}",
            out.text
        );
        assert!(!out.text.contains("persist="), "got:\n{}", out.text);
    }

    #[test]
    fn lute_fix_leaves_persist_without_into_manual() {
        // dsl 0.6.0 §2.3: a `persist=` with NO `into=` was already an error and
        // stays MANUAL — `lute fix` can't prove the deletion is meaning-
        // preserving, so the doc is byte-identical (the checker's
        // E-PERSIST-REMOVED still offers the fixit for a human to apply).
        let src = wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" persist=\"run\">\n@bianca: hi\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 0, "got:\n{}", out.text);
        assert_eq!(out.text, src, "persist without into must stay manual");
    }

    #[test]
    fn lute_fix_leaves_persist_non_run_value_manual() {
        // Only the provable `persist="run"` shape is auto-migrated; a `persist=`
        // with any other value stays MANUAL even alongside `into=`.
        let src = wrap(
            "<branch id=\"b\">\n<choice id=\"c\" label=\"L\" persist=\"scene\" into=\"run.x\">\n@bianca: hi\n</choice>\n</branch>\n",
        );
        let out = fix_document(&src);
        assert_eq!(out.changed, 0, "got:\n{}", out.text);
        assert_eq!(out.text, src, "non-run persist value must stay manual");
    }

    #[test]
    fn strips_shot_heading_prefix_when_title_follows() {
        // dsl 0.6.0 §3.4: `## Shot 3. The Alley` → `## The Alley` — the legacy
        // grammar prefix is stripped, the free-text title survives byte-exact.
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 3. The Alley\n@bianca: hi\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 1, "only the heading prefix is stripped; got:\n{}", out.text);
        assert!(out.text.contains("## The Alley\n"), "got:\n{}", out.text);
        assert!(!out.text.contains("Shot 3."), "grammar prefix must be gone; got:\n{}", out.text);
        // idempotent: the stripped title no longer matches the prefix shape.
        let again = fix_document(&out.text);
        assert_eq!(again.changed, 0, "second pass is a no-op; got:\n{}", again.text);
        assert_eq!(again.text, out.text);
    }

    #[test]
    fn strips_scene_heading_prefix_when_title_follows() {
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Scene 2. Rooftop at dusk\n@bianca: hi\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 1, "got:\n{}", out.text);
        assert!(out.text.contains("## Rooftop at dusk\n"), "got:\n{}", out.text);
        assert!(!out.text.contains("Scene 2."), "got:\n{}", out.text);
    }

    #[test]
    fn leaves_bare_shot_heading_untouched() {
        // A BARE `## Shot 3.` (no trailing title) is a valid free title now —
        // stripping would leave an empty heading, so `lute fix` leaves it be.
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 3.\n@bianca: hi\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 0, "bare `## Shot 3.` must be untouched; got:\n{}", out.text);
        assert_eq!(out.text, src);
    }

    #[test]
    fn leaves_bare_scene_heading_untouched() {
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Scene 2.\n@bianca: hi\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 0, "bare `## Scene 2.` must be untouched; got:\n{}", out.text);
        assert_eq!(out.text, src);
    }

    #[test]
    fn leaves_free_shot_heading_untouched() {
        // A heading that isn't the legacy `Shot|Scene <int>.` shape is a plain
        // free title — never touched.
        let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## The Alley\n@bianca: hi\n";
        let out = fix_document(src);
        assert_eq!(out.changed, 0, "a non-grammar free heading is untouched; got:\n{}", out.text);
        assert_eq!(out.text, src);
    }
}
