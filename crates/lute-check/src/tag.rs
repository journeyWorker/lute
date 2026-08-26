//! `lute tag` localization pass (dsl §12): back-fill a stable `code` into every
//! untagged content line (`@speaker{…}: text`, §7.1). Pure + total; the CLI
//! wraps this with file I/O.

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, AttrValue, Line, Node};
use lute_syntax::parse;

/// The result of tagging: the (possibly rewritten) document text and how many
/// content lines received a new `code`.
#[derive(Clone, Debug, PartialEq)]
pub struct TagOutcome {
    pub text: String,
    pub added: usize,
}

/// Back-fill a `code` attribute into every content line that lacks one (§12).
/// Existing codes are never touched; new codes step above THAT SPEAKER's highest
/// existing numeric `code` (a per-speaker counter, dsl §12); different speakers
/// have independent sequences (`fixer` 0010…, `bianca` 0010…), so their codes
/// coexist and the lineId (§12) keys on speaker + code.
/// Idempotent, deterministic, total (a structurally broken doc is returned
/// unchanged with `added: 0`).
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

    let bytes = text.as_bytes();
    let mut inserts: Vec<(usize, String)> = Vec::new();

    // Scene identity scope (dsl 0.2.0 §7): every shot's `:line`s (into branch
    // choices' + match arms' bodies) share ONE scope — the whole document —
    // unchanged from 0.1.0.
    let mut scene_lines: Vec<&Line> = Vec::new();
    for shot in &doc.shots {
        collect_lines(&shot.body, &mut scene_lines);
    }
    tag_scope(scene_lines, bytes, &mut inserts);

    // Per-quest identity scope (dsl 0.2.0 §7): a quest's lines (reached via
    // its `<on>`/`<objective>` arms) are scoped PER `<quest>` — each `<quest>`
    // is its own identity domain, so the SAME (speaker, code) pair may repeat
    // across two different quests without colliding (mirrors
    // `match_check.rs::check_line_codes`'s per-quest scoping). Each quest
    // therefore gets its OWN fresh per-speaker counter.
    for quest in &doc.quests {
        let mut quest_lines: Vec<&Line> = Vec::new();
        collect_lines(&quest.body, &mut quest_lines);
        tag_scope(quest_lines, bytes, &mut inserts);
    }

    if inserts.is_empty() {
        return TagOutcome {
            text: text.to_string(),
            added: 0,
        };
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

/// Back-fill codes for ONE identity scope (dsl 0.2.0 §7) — the whole document
/// for the scene scope, a single `<quest>` for a quest scope — appending
/// `(byte offset, inserted string)` insertions into `inserts` in document
/// order. Existing codes are never touched; new codes step above THIS SCOPE's
/// highest existing numeric `code` PER SPEAKER (dsl §12): `fixer` 0010/0020…,
/// `bianca` 0010/0020… coexist within one scope, and — since each scope gets
/// its own fresh counter map — the same (speaker, code) pair may recur in a
/// DIFFERENT scope without colliding.
fn tag_scope(lines: Vec<&Line>, bytes: &[u8], inserts: &mut Vec<(usize, String)>) {
    // Per-speaker highest existing numeric `code` within this scope. Default 0.
    let mut max_code: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for line in &lines {
        let cur = max_code.entry(line.speaker.clone()).or_insert(0);
        for attr in &line.attrs {
            if attr.key == "code" {
                if let AttrValue::Str(s) = &attr.value {
                    if let Ok(n) = s.trim().parse::<u64>() {
                        *cur = (*cur).max(n);
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

    for line in &untagged {
        // Next code in THIS speaker's sequence (above its existing max). A
        // speaker whose counter overflows fails closed for THIS line only
        // (`continue`, not `break`), so other speakers' lines still tag.
        let counter = max_code.entry(line.speaker.clone()).or_insert(0);
        let Some(nc) = counter.checked_add(10) else {
            continue;
        };
        *counter = nc;
        let code = format!("{nc:04}");
        let (at, inserted) = code_insert(line, bytes, &code);
        inserts.push((at, inserted));
    }
}

/// Where and what to splice to give `line` a fresh `code` attribute.
///
/// 0.1.0 content line: `:speaker{attrs}?: text` (dsl §7.1). We derive the
/// insertion point from the parsed span rather than re-scanning raw bytes:
/// spans are ORIGINAL-source offsets and comment-blanking is
/// length-preserving (parser SPAN-FIDELITY contract), so a `text` byte
/// offset maps 1:1 onto what the parser saw.
///   - `speaker_end` is the byte just past the speaker ident. An ident
///     holds no comments/whitespace, so it is exactly `speaker.len()`
///     bytes past the leading `:` (`span.byte_start`).
///   - An attr block exists iff that byte is `{`: the parser only accepts
///     a `{` FLUSH against the ident, and `{` is never a comment byte, so
///     this single byte is authoritative (a `{` in a comment or in the
///     text can never sit here).
fn code_insert(line: &Line, bytes: &[u8], code: &str) -> (usize, String) {
    let speaker_end = line.span.byte_start + 1 + line.speaker.len();
    if bytes.get(speaker_end) == Some(&b'{') {
        // Existing attr block: merge `code` as the FIRST attribute, right
        // after `{`. Trailing space only when the block is non-empty (the
        // next byte isn't the closing `}`).
        let at = speaker_end + 1;
        let inserted = if bytes.get(at) == Some(&b'}') {
            format!("code=\"{code}\"")
        } else {
            format!("code=\"{code}\" ")
        };
        (at, inserted)
    } else {
        // No attr block: fresh `{code="ID"}` between the speaker ident and
        // the second `:` (`@bianca: hi` -> `@bianca{code="0010"}: hi`).
        (speaker_end, format!("{{code=\"{code}\"}}"))
    }
}

/// The outcome of a FORCE renumber ([`retag_document`], `lute tag --force`).
#[derive(Clone, Debug, PartialEq)]
pub enum RetagOutcome {
    /// Frontmatter carries `codesLocked` — the document's codes are published
    /// identity (they key `lineId`/`voiceKey`, dsl §12) and renumbering is
    /// refused. Remove the key (or set it `false`) to renumber again.
    Locked,
    /// A structural (Error-severity) parse defect — never rewrite a broken
    /// doc; the input is left untouched.
    Broken,
    /// The renumbered text. `renumbered` counts lines whose code was written
    /// (changed or newly inserted); `skipped` counts lines whose `code` is
    /// not a string literal (an `@ref` is intentional identity, not a
    /// sequence member — left alone).
    Renumbered {
        text: String,
        renumbered: usize,
        skipped: usize,
    },
}

/// FORCE-renumber every content line's `code` (`lute tag --force`): each
/// identity scope (the scene, then each `<quest>` — dsl 0.2.0 §7) restarts a
/// per-speaker counter and assigns 0010/0020/… in document order, REWRITING
/// existing codes in place (the one thing [`tag_document`] never does).
///
/// This is a drafting tool: while a scene is being written, insertions and
/// deletions leave the back-filled sequence gappy and out of order, and a
/// clean renumber keeps codes readable. Once a document's codes are
/// published identity — exported for localization or voice — renumbering
/// breaks the `lineId` join (`loc import`, `compile --locales`), so a
/// frontmatter `codesLocked:` key refuses the operation. The guard fails
/// CLOSED: any `codesLocked:` value other than exactly `false` locks
/// (a typo like `codesLocked: "no"` must never silently unlock).
///
/// Idempotent (a second run is byte-identical) and total, like
/// [`tag_document`].
pub fn retag_document(text: &str) -> RetagOutcome {
    let (doc, diags) = parse(text);
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return RetagOutcome::Broken;
    }
    if codes_locked(&doc.meta.raw_yaml) {
        return RetagOutcome::Locked;
    }

    let bytes = text.as_bytes();
    // (start, end, replacement): end == start is a pure insertion.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut renumbered = 0usize;
    let mut skipped = 0usize;

    let mut scene_lines: Vec<&Line> = Vec::new();
    for shot in &doc.shots {
        collect_lines(&shot.body, &mut scene_lines);
    }
    retag_scope(scene_lines, bytes, &mut edits, &mut renumbered, &mut skipped);
    for quest in &doc.quests {
        let mut quest_lines: Vec<&Line> = Vec::new();
        collect_lines(&quest.body, &mut quest_lines);
        retag_scope(quest_lines, bytes, &mut edits, &mut renumbered, &mut skipped);
    }

    if edits.is_empty() {
        return RetagOutcome::Renumbered {
            text: text.to_string(),
            renumbered: 0,
            skipped,
        };
    }

    // Splice back-to-front (descending offset) so earlier offsets stay valid.
    edits.sort_by_key(|(at, _, _)| std::cmp::Reverse(*at));
    let mut out = text.to_string();
    for (start, end, replacement) in &edits {
        out.replace_range(*start..*end, replacement);
    }

    RetagOutcome::Renumbered {
        text: out,
        renumbered,
        skipped,
    }
}

/// Renumber ONE identity scope in document order with a fresh per-speaker
/// counter (0010 step 10 — the same shape [`tag_scope`] back-fills, so a
/// renumbered doc and a freshly-tagged doc are indistinguishable). A line
/// whose existing code already equals its recomputed value produces NO edit
/// (idempotence); a non-string `code` value consumes no counter slot and is
/// counted in `skipped`.
fn retag_scope(
    lines: Vec<&Line>,
    bytes: &[u8],
    edits: &mut Vec<(usize, usize, String)>,
    renumbered: &mut usize,
    skipped: &mut usize,
) {
    let mut counter: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for line in lines {
        let code_attr = line.attrs.iter().find(|a| a.key == "code");
        if let Some(attr) = code_attr {
            if !matches!(attr.value, AttrValue::Str(_)) {
                *skipped += 1;
                continue;
            }
        }
        let next = counter.entry(line.speaker.clone()).or_insert(0);
        let Some(nc) = next.checked_add(10) else {
            continue;
        };
        *next = nc;
        let code = format!("{nc:04}");
        match code_attr {
            Some(attr) => {
                if matches!(&attr.value, AttrValue::Str(s) if s == &code) {
                    continue; // already the right code — no edit, no count
                }
                // Replace the whole `code="…"` attr in place; `attr.span`
                // covers key through closing quote.
                edits.push((
                    attr.span.byte_start,
                    attr.span.byte_end,
                    format!("code=\"{code}\""),
                ));
                *renumbered += 1;
            }
            None => {
                let (at, inserted) = code_insert(line, bytes, &code);
                edits.push((at, at, inserted));
                *renumbered += 1;
            }
        }
    }
}

/// dsl §12 publish guard: `true` iff frontmatter declares `codesLocked:` with
/// any value other than exactly `false`. Fail-closed on malformed values —
/// a lock typo must never silently unlock a published document. Malformed
/// YAML cannot lock (the checker reports it separately); an absent key is
/// unlocked.
pub fn codes_locked(raw_yaml: &str) -> bool {
    let Ok(serde_yaml::Value::Mapping(map)) =
        serde_yaml::from_str::<serde_yaml::Value>(raw_yaml)
    else {
        return false;
    };
    match map.get(serde_yaml::Value::String("codesLocked".into())) {
        None => false,
        Some(serde_yaml::Value::Bool(false)) => false,
        Some(_) => true,
    }
}

/// Collect every `Node::Line` in document order, descending into branch
/// choices' bodies, match arms' bodies, hub choices' bodies, and on/objective
/// bodies (mirrors `check.rs::Walker::walk` / `match_check.rs::collect_lines`'s
/// recursion — dsl 0.2.0 §7 extends the walk into quest arms).
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
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_lines(&choice.body, out);
                }
            }
            Node::Objective(o) => collect_lines(&o.body, out),
            Node::On(o) => collect_lines(&o.body, out),
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_ATTRS: &str =
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi there\n";
    const WITH_ATTRS: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@fixer{mono}: hmm\n";
    const ALREADY: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@fixer{code=\"0010\"}: kept\n";

    #[test]
    fn tags_line_without_attrs() {
        let out = tag_document(NO_ATTRS);
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: hi there"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn tags_line_with_existing_attrs() {
        let out = tag_document(WITH_ATTRS);
        assert_eq!(out.added, 1);
        // `code` is merged as the FIRST attribute, existing attrs preserved.
        assert!(
            out.text
                .contains("@fixer{code=\"0010\" mono}: hmm"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn no_attr_block_gets_fresh_code_block() {
        // §7.1 no-attr path: `@bianca: hi` -> `@bianca{code="0010"}: hi`.
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@bianca: hi\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@bianca{code=\"0010\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn merge_into_existing_attr_block_is_first() {
        // §7.1 merge path: `code` lands as the FIRST attr, right after `{`.
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@bianca{emotion=\"x\"}: hi\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@bianca{code=\"0010\" emotion=\"x\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    // ── retag_document (`lute tag --force`) ────────────────────────────────

    fn renumbered(out: RetagOutcome) -> (String, usize, usize) {
        match out {
            RetagOutcome::Renumbered {
                text,
                renumbered,
                skipped,
            } => (text, renumbered, skipped),
            other => panic!("expected Renumbered, got {other:?}"),
        }
    }

    #[test]
    fn retag_renumbers_out_of_order_codes_in_document_order() {
        // Drafting left the sequence gappy and out of order: 0050 then 0010.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0050\"}: first\n\
                   @fixer{code=\"0010\" mono}: second\n\
                   @fixer: third\n";
        let (text, n, skipped) = renumbered(retag_document(src));
        assert_eq!((n, skipped), (3, 0));
        assert!(text.contains("@fixer{code=\"0010\"}: first"), "got:\n{text}");
        assert!(
            text.contains("@fixer{code=\"0020\" mono}: second"),
            "got:\n{text}"
        );
        assert!(text.contains("@fixer{code=\"0030\"}: third"), "got:\n{text}");
    }

    #[test]
    fn retag_keeps_per_speaker_sequences_independent() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0090\"}: a\n\
                   @bianca{code=\"0070\"}: b\n\
                   @fixer: c\n";
        let (text, n, _) = renumbered(retag_document(src));
        assert_eq!(n, 3);
        assert!(text.contains("@fixer{code=\"0010\"}: a"), "got:\n{text}");
        assert!(text.contains("@bianca{code=\"0010\"}: b"), "got:\n{text}");
        assert!(text.contains("@fixer{code=\"0020\"}: c"), "got:\n{text}");
    }

    #[test]
    fn retag_is_idempotent() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0050\"}: first\n\
                   @fixer: second\n";
        let (once, n1, _) = renumbered(retag_document(src));
        assert_eq!(n1, 2);
        let (twice, n2, _) = renumbered(retag_document(&once));
        assert_eq!(n2, 0, "second run must be a no-op");
        assert_eq!(once, twice);
    }

    #[test]
    fn retag_matches_fresh_tag_output() {
        // A renumbered doc and a freshly-tagged doc are indistinguishable.
        let untagged = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                        @fixer: a\n@bianca: b\n@fixer: c\n";
        let fresh = tag_document(untagged).text;
        let (forced, _, _) = renumbered(retag_document(untagged));
        assert_eq!(fresh, forced);
    }

    #[test]
    fn retag_refuses_codes_locked() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: true\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: kept\n";
        assert_eq!(retag_document(src), RetagOutcome::Locked);
    }

    #[test]
    fn retag_lock_fails_closed_on_malformed_value() {
        // `codesLocked: "no"` is not `false` — a typo must never unlock.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: \"no\"\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: kept\n";
        assert_eq!(retag_document(src), RetagOutcome::Locked);
    }

    #[test]
    fn retag_codes_locked_false_unlocks() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: false\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: a\n";
        let (text, n, _) = renumbered(retag_document(src));
        assert_eq!(n, 1);
        assert!(text.contains("code=\"0010\""), "got:\n{text}");
    }

    #[test]
    fn retag_skips_non_string_code_and_consumes_no_slot() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=@customId}: special\n\
                   @fixer: plain\n";
        let (text, n, skipped) = renumbered(retag_document(src));
        assert_eq!((n, skipped), (1, 1));
        assert!(text.contains("@fixer{code=@customId}: special"), "got:\n{text}");
        // The @ref line consumed no slot: the plain line starts the sequence.
        assert!(text.contains("@fixer{code=\"0010\"}: plain"), "got:\n{text}");
    }

    #[test]
    fn retag_quest_scopes_restart_counters() {
        // Mirrors `tag_document`'s scoping: each <quest> is an independent
        // identity domain (dsl 0.2.0 §7) — the second quest's counter
        // restarts, so both lines renumber to 0010 without colliding.
        let src = "---\nkind: quest\n---\n\
                   <quest id=\"q1\">\n\
                   <objective id=\"o1\" done=\"a\"/>\n\
                   <on event=\"questActive\">\n\
                   @fixer{code=\"0800\"}: first quest line\n\
                   </on>\n\
                   </quest>\n\
                   <quest id=\"q2\">\n\
                   <objective id=\"o2\" done=\"b\"/>\n\
                   <on event=\"questActive\">\n\
                   @fixer{code=\"0900\"}: second quest line\n\
                   </on>\n\
                   </quest>\n";
        let (text, n, _) = renumbered(retag_document(src));
        assert_eq!(n, 2);
        assert!(
            text.contains("@fixer{code=\"0010\"}: first quest line"),
            "got:\n{text}"
        );
        assert!(
            text.contains("@fixer{code=\"0010\"}: second quest line"),
            "got:\n{text}"
        );
    }
    #[test]
    fn already_tagged_is_untouched_and_idempotent() {
        let out = tag_document(ALREADY);
        assert_eq!(out.added, 0);
        assert_eq!(out.text, ALREADY);
    }

    #[test]
    fn new_codes_step_above_same_speaker_max() {
        // same speaker `a`: one tagged 0050 + one untagged -> untagged gets 0060
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a{code=\"0050\"}: one\n@a: two\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(out.text.contains("@a{code=\"0050\"}: one"));
        assert!(
            out.text.contains("@a{code=\"0060\"}: two"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn per_speaker_counters_are_independent() {
        // interleaved speakers: each starts its OWN sequence at 0010.
        // a: "one"(untagged), "three"(untagged) -> 0010, 0020 ; b: "two"(untagged) -> 0010
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a: one\n@b: two\n@a: three\n";
        let out = tag_document(src);
        assert_eq!(out.added, 3);
        assert!(
            out.text.contains("@a{code=\"0010\"}: one"),
            "got:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@b{code=\"0010\"}: two"),
            "b starts its own sequence:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@a{code=\"0020\"}: three"),
            "a's second line:\n{}",
            out.text
        );
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
    fn comment_inside_attr_block_is_preserved() {
        // A `/* … */` comment inside the attr block is blanked before parsing but
        // kept in the original text; merging `code` right after `{` must preserve
        // it, parse clean, and be idempotent on a second run.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator{ /* keep me */ emotion=\"x\"}: hi\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(out.text.contains("code=\"0010\""), "got:\n{}", out.text);
        assert!(out.text.contains("emotion=\"x\""), "got:\n{}", out.text);
        assert!(
            out.text.contains("/* keep me */"),
            "comment preserved:\n{}",
            out.text
        );
        let (_d, diags) = lute_syntax::parse(&out.text);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == lute_core_span::Severity::Error),
            "{diags:?}"
        );
        let twice = tag_document(&out.text);
        assert_eq!(twice.added, 0, "must be idempotent");
        assert_eq!(twice.text, out.text);
    }

    #[test]
    fn comment_brace_in_text_is_not_an_attr_block() {
        // A `{` inside a comment in the TEXT (after the second `:`) must NOT be
        // mistaken for an attr block; the code goes in a fresh `{code=…}` after
        // the speaker ident, the text is untouched, and re-tagging is a no-op.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@bianca: hi /* { */ there\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@bianca{code=\"0010\"}: hi /* { */ there"),
            "got:\n{}",
            out.text
        );
        let (_d, diags) = lute_syntax::parse(&out.text);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == lute_core_span::Severity::Error),
            "{diags:?}"
        );
        let twice = tag_document(&out.text);
        assert_eq!(twice.added, 0, "second tag run must be a no-op (idempotent)");
        assert_eq!(twice.text, out.text, "idempotent: byte-identical on re-run");
    }

    #[test]
    fn code_above_u32_max_does_not_collide() {
        // same speaker `a` at u32::MAX + an untagged `a` line -> a's counter steps
        // to 4294967305 (u64 counter, so no saturation/collision at u32::MAX).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a{code=\"4294967295\"}: one\n@a: two\n";
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
    fn code_at_u64_max_fails_closed_no_collision() {
        // same speaker `a` at u64::MAX + an untagged `a` line -> a's counter
        // overflows -> fail closed for that line (per speaker).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a{code=\"18446744073709551615\"}: one\n@a: two\n";
        let out = tag_document(src);
        assert_eq!(
            out.added, 0,
            "counter overflow must fail closed, not emit a colliding code"
        );
        assert_eq!(out.text.matches("18446744073709551615").count(), 1);
    }

    // ---- dsl 0.2.0 §7: quest bodies (on/objective arms) ----

    #[test]
    fn quest_on_body_line_is_tagged() {
        // Before the fix, `tag_document` walked `doc.shots` only and a
        // quest doc (no shots) tagged ZERO lines even with untagged content
        // reachable via a `<quest>`'s `<on>` arm.
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n@narrator: hi\n</on>\n</quest>\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1, "got:\n{}", out.text);
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn quest_objective_body_line_is_tagged() {
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\">\n@narrator: hi\n</objective>\n</quest>\n";
        let out = tag_document(src);
        assert_eq!(out.added, 1, "got:\n{}", out.text);
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn per_quest_code_scope_is_independent() {
        // §7: each `<quest>` is its own identity domain — the SAME speaker
        // starts its OWN 0010 sequence in EACH quest, independent of every
        // other quest (and of the scene scope).
        let src = "---\nkind: quest\n---\n\
                   <quest id=\"q1\">\n<on event=\"questComplete\">\n@narrator: one\n</on>\n</quest>\n\
                   <quest id=\"q2\">\n<on event=\"questComplete\">\n@narrator: two\n</on>\n</quest>\n";
        let out = tag_document(src);
        assert_eq!(out.added, 2, "got:\n{}", out.text);
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: one"),
            "q1 starts its own sequence:\n{}",
            out.text
        );
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: two"),
            "q2 starts its own sequence independent of q1:\n{}",
            out.text
        );
    }

    #[test]
    fn quest_tagging_is_idempotent() {
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n@narrator: hi\n</on>\n</quest>\n";
        let once = tag_document(src).text;
        let twice = tag_document(&once);
        assert_eq!(twice.added, 0);
        assert_eq!(twice.text, once);
    }
}
