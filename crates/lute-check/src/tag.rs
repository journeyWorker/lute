//! `lute tag` localization pass (dsl §12): back-fill a stable `code` into every
//! untagged content line (`@speaker{…}: text`, §7.1) and every untagged call
//! of a directive with a `text` attribute (0.51.0 §5.3). Pure + total; the
//! CLI wraps this with file I/O and names the directives with `text`
//! attributes from the document's capability snapshot.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::Severity;
use lute_manifest::schema::DIRECTIVE_TEXT_CODE_ATTR;
use lute_syntax::ast::{Arm, AttrValue, Directive, Document, Line, Node};
use lute_syntax::parse;

/// The result of tagging: the (possibly rewritten) document text, how many
/// content lines received a new `code` and component instance keys were added
/// (`added` is the sum of both).
#[derive(Clone, Debug, PartialEq)]
pub struct TagOutcome {
    pub text: String,
    pub added: usize,
    /// Of `added`: the `::use` / `<beat use=…>` instance keys.
    pub instances: usize,
}

/// Back-fill a `code` attribute into every content line that lacks one (§12),
/// and into every directive call whose tag is in `text_tags` — the
/// directives with a `text` attribute (0.51.0 §5.3) — that lacks one.
/// Existing codes are never touched; new codes step above THAT SPEAKER's
/// highest existing numeric `code` (a per-speaker counter, dsl §12), or that
/// directive tag's (a keyspace of its own); different speakers have
/// independent sequences (`fixer` 0010…, `marina` 0010…), so their codes
/// coexist and the lineId (§12) keys on speaker + code. Directive calls are
/// numbered in source order, as the compiler back-fills them.
/// Idempotent, deterministic, total (a structurally broken doc is returned
/// unchanged with `added: 0`).
pub fn tag_document(text: &str, text_tags: &BTreeSet<String>) -> TagOutcome {
    let (doc, diags) = parse(text);
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return TagOutcome {
            text: text.to_string(),
            added: 0,
            instances: 0,
        };
    }
    let bytes = text.as_bytes();
    let mut inserts: Vec<(usize, String)> = Vec::new();
    for bodies in identity_scopes(&doc) {
        let mut lines = Vec::new();
        for body in &bodies {
            collect_lines(body, &mut lines);
        }
        tag_scope(lines, bytes, &mut inserts);
        let directives = scope_text_directives(&bodies, text_tags);
        tag_directive_scope(&directives, bytes, &mut inserts);
    }
    let codes = inserts.len();
    tag_components(text, &mut inserts);
    if inserts.is_empty() {
        return TagOutcome {
            text: text.to_string(),
            added: 0,
            instances: 0,
        };
    }
    inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let mut out = text.to_string();
    for (at, inserted) in &inserts {
        out.insert_str(*at, inserted);
    }
    TagOutcome {
        text: out,
        added: inserts.len(),
        instances: inserts.len() - codes,
    }
}

fn tag_components(text: &str, inserts: &mut Vec<(usize, String)>) {
    let owner = "document".to_string();
    let mut used: std::collections::BTreeMap<(String, String), std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    let mut missing = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        for marker in ["::use{", "<beat "] {
            let mut cursor = 0usize;
            while let Some(rel) = line[cursor..].find(marker) {
                let start = cursor + rel;
                let close_rel = find_opener_end(&line[start..], marker == "::use{");
                let Some(close_rel) = close_rel else { break };
                let close = start + close_rel;
                let raw = &line[start..=close];
                let key = if marker == "::use{" {
                    "component"
                } else {
                    "use"
                };
                if let Some(component) = quoted_attr(raw, key) {
                    let scope = (owner.clone(), component);
                    if let Some(instance) = quoted_attr(raw, "instance") {
                        if let Some(n) = instance.strip_prefix("use-").and_then(|s| s.parse().ok())
                        {
                            used.entry(scope).or_default().insert(n);
                        }
                    } else {
                        missing.push((offset + start, offset + close, marker == "::use{", scope));
                    }
                }
                cursor = close + 1;
            }
        }
        offset += line.len();
    }
    for (start, close, directive, scope) in missing {
        let occupied = used.entry(scope).or_default();
        let mut n = 1;
        while occupied.contains(&n) {
            n += 1;
        }
        occupied.insert(n);
        let at = if directive {
            close
        } else if text[start..=close].ends_with("/>") {
            close - 1
        } else {
            close
        };
        inserts.push((at, format!(" instance=\"use-{n:03}\"")));
    }
}
fn component_tagged_text(text: &str) -> (String, usize) {
    let mut inserts = Vec::new();
    tag_components(text, &mut inserts);
    if inserts.is_empty() {
        return (text.to_string(), 0);
    }
    inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let mut out = text.to_string();
    for (at, inserted) in &inserts {
        out.insert_str(*at, inserted);
    }
    (out, inserts.len())
}

fn quoted_attr(raw: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=\"");
    let start = raw.find(&marker)? + marker.len();
    let end = raw[start..].find('"')?;
    Some(raw[start..start + end].to_string())
}
fn find_opener_end(raw: &str, directive: bool) -> Option<usize> {
    let end = if directive { b'}' } else { b'>' };
    let mut quoted = false;
    let mut escaped = false;
    for (i, byte) in raw.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && byte == end {
            return Some(i);
        }
    }
    None
}

/// Back-fill codes for ONE identity scope (dsl 0.2.0 §7) — the whole document
/// for the scene scope, a single `<quest>` for a quest scope — appending
/// `(byte offset, inserted string)` insertions into `inserts` in document
/// order. Existing codes are never touched; new codes step above THIS SCOPE's
/// highest existing numeric `code` PER SPEAKER (dsl §12): `fixer` 0010/0020…,
/// `marina` 0010/0020… coexist within one scope, and — since each scope gets
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

/// One line's code change in a renumber: the 1-based source `line`, its code
/// before (`None` for a line without one) and the code it gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRenumber {
    pub line: u32,
    pub from: Option<String>,
    pub to: String,
}

/// The outcome of a renumber (`lute tag --renumber`).
#[derive(Clone, Debug, PartialEq)]
pub enum RetagOutcome {
    /// Codes are locked, but component instances were added.
    LockedInstances { text: String, added: usize },
    /// Codes are locked and no component instance was missing.
    Locked,
    /// A structural parse defect; no rewrite occurred.
    Broken,
    /// The renumbered text and counts; `changes` lists each changed code in
    /// document order within its identity scope (`renumbered` entries).
    Renumbered {
        text: String,
        renumbered: usize,
        skipped: usize,
        changes: Vec<LineRenumber>,
    },
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
        // the second `:` (`@marina: hi` -> `@marina{code="0010"}: hi`).
        (speaker_end, format!("{{code=\"{code}\"}}"))
    }
}

/// Renumber every content line's `code` (`lute tag --renumber --commit`): each
/// identity scope (the scene, then each `<quest>` — dsl 0.2.0 §7 — then each
/// `<entry>`, dsl 0.19.0 §4, then each `<beat>` bundle, dsl 0.23.0 §4) restarts a
/// per-speaker counter and assigns 0010/0020/… in document order, REWRITING
/// existing codes in place (the one thing [`tag_document`] never does).
///
/// This is a drafting tool: while a scene is being written, insertions and
/// deletions leave the back-filled sequence gappy and out of order, and a
/// clean renumber keeps codes readable. Once a document's codes are
/// published identity — exported for localization or voice — renumbering
/// breaks the `lineId` join (`loc import`, `compile --locale-bundle`), so a
/// frontmatter `codesLocked:` key refuses the operation. The guard fails
/// CLOSED: any `codesLocked:` value other than exactly `false` locks
/// (a typo like `codesLocked: "no"` must never silently unlock).
///
/// Idempotent (a second run is byte-identical) and total, like
/// [`tag_document`].
pub fn retag_document(text: &str, text_tags: &BTreeSet<String>) -> RetagOutcome {
    let (doc, diags) = parse(text);
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return RetagOutcome::Broken;
    }
    if codes_locked(&doc.meta.raw_yaml) {
        let (tagged, added) = component_tagged_text(text);
        return if added == 0 {
            RetagOutcome::Locked
        } else {
            RetagOutcome::LockedInstances {
                text: tagged,
                added,
            }
        };
    }

    let bytes = text.as_bytes();
    // (start, end, replacement): end == start is a pure insertion.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut renumbered = 0usize;
    let mut skipped = 0usize;
    let mut changes: Vec<LineRenumber> = Vec::new();

    for bodies in identity_scopes(&doc) {
        let mut lines: Vec<&Line> = Vec::new();
        for body in &bodies {
            collect_lines(body, &mut lines);
        }
        retag_scope(
            lines,
            bytes,
            &mut edits,
            &mut renumbered,
            &mut skipped,
            &mut changes,
        );
        // 0.51.0 §5.3: the scope's directive codes, per tag, likewise.
        let directives = scope_text_directives(&bodies, text_tags);
        retag_directive_scope(
            &directives,
            bytes,
            &mut edits,
            &mut renumbered,
            &mut skipped,
            &mut changes,
        );
    }

    if edits.is_empty() {
        return RetagOutcome::Renumbered {
            text: text.to_string(),
            renumbered: 0,
            skipped,
            changes,
        };
    }

    // Splice back-to-front (descending offset) so earlier offsets stay valid.
    edits.sort_by_key(|(at, _, _)| std::cmp::Reverse(*at));
    let mut out = text.to_string();
    for (start, end, replacement) in &edits {
        out.replace_range(*start..*end, replacement);
    }

    changes.sort_by_key(|c| c.line);
    RetagOutcome::Renumbered {
        text: out,
        renumbered,
        skipped,
        changes,
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
    changes: &mut Vec<LineRenumber>,
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
                changes.push(LineRenumber {
                    line: line.span.line,
                    from: match &attr.value {
                        AttrValue::Str(s) => Some(s.clone()),
                        _ => None,
                    },
                    to: code,
                });
                *renumbered += 1;
            }
            None => {
                let (at, inserted) = code_insert(line, bytes, &code);
                edits.push((at, at, inserted));
                changes.push(LineRenumber {
                    line: line.span.line,
                    from: None,
                    to: code,
                });
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
    let Ok(yaml_serde::Value::Mapping(map)) = yaml_serde::from_str::<yaml_serde::Value>(raw_yaml)
    else {
        return false;
    };
    match map.get(yaml_serde::Value::String("codesLocked".into())) {
        None => false,
        Some(yaml_serde::Value::Bool(false)) => false,
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
                            collect_lines(body, out);
                        }
                    }
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_lines(b, out);
                }
            }
            Node::Objective(o) => collect_lines(&o.body, out),
            Node::On(o) => collect_lines(&o.body, out),
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// The document's identity scopes (dsl 0.2.0 §7), each as its bodies: the
/// scene (every shot), then each `<quest>`, `<entry>` and bundle `<beat>`.
fn identity_scopes(doc: &Document) -> Vec<Vec<&Vec<Node>>> {
    std::iter::once(doc.sections.iter().map(|s| &s.body).collect())
        .chain(doc.quests.iter().map(|q| vec![&q.body]))
        .chain(doc.entries.iter().map(|e| vec![&e.body]))
        .chain(doc.beats.iter().map(|b| vec![&b.body]))
        .collect()
}

/// The calls of `text_tags` directives in one scope, in source order — the
/// order the compiler back-fills them in (a `<timeline>` plays its clips in
/// another).
fn scope_text_directives<'a>(
    bodies: &[&'a Vec<Node>],
    text_tags: &BTreeSet<String>,
) -> Vec<&'a Directive> {
    let mut out = Vec::new();
    if text_tags.is_empty() {
        return out;
    }
    let has_text = |tag: &str| text_tags.contains(tag);
    for body in bodies {
        crate::match_check::collect_text_directives(body, &has_text, &mut out);
    }
    out.sort_by_key(|d| d.span.byte_start);
    out
}

/// Where and what to splice to give the call `d` a `code` attribute: first
/// in its `{…}` block (after the `::tag`), or a fresh block.
fn directive_code_insert(d: &Directive, bytes: &[u8], code: &str) -> (usize, String) {
    let tag_end = d.span.byte_start + 2 + d.tag.len();
    if bytes.get(tag_end) == Some(&b'{') {
        let at = tag_end + 1;
        let inserted = if bytes.get(at) == Some(&b'}') {
            format!("{DIRECTIVE_TEXT_CODE_ATTR}=\"{code}\"")
        } else {
            format!("{DIRECTIVE_TEXT_CODE_ATTR}=\"{code}\" ")
        };
        (at, inserted)
    } else {
        (
            tag_end,
            format!("{{{DIRECTIVE_TEXT_CODE_ATTR}=\"{code}\"}}"),
        )
    }
}

/// The call's `code` attribute, if it has one.
fn directive_code(d: &Directive) -> Option<&lute_syntax::ast::Attr> {
    d.attrs.iter().find(|a| a.key == DIRECTIVE_TEXT_CODE_ATTR)
}

/// Back-fill one scope's untagged directive calls (`directives`, source
/// order): per tag, above that tag's highest authored numeric code, `+10`,
/// `{:04}` — the compiler's scheme (`lute-compile` `address.rs`).
fn tag_directive_scope(
    directives: &[&Directive],
    bytes: &[u8],
    inserts: &mut Vec<(usize, String)>,
) {
    let mut max_code: BTreeMap<&str, u64> = BTreeMap::new();
    for d in directives {
        let max = max_code.entry(d.tag.as_str()).or_insert(0);
        if let Some(AttrValue::Str(s)) = directive_code(d).map(|a| &a.value) {
            if let Ok(n) = s.trim().parse::<u64>() {
                *max = (*max).max(n);
            }
        }
    }
    for d in directives.iter().filter(|d| directive_code(d).is_none()) {
        let counter = max_code.entry(d.tag.as_str()).or_insert(0);
        let Some(next) = counter.checked_add(10) else {
            continue;
        };
        *counter = next;
        inserts.push(directive_code_insert(d, bytes, &format!("{next:04}")));
    }
}

/// Renumber one scope's directive calls (source order) with a fresh counter
/// per tag, as [`retag_scope`] renumbers lines per speaker.
fn retag_directive_scope(
    directives: &[&Directive],
    bytes: &[u8],
    edits: &mut Vec<(usize, usize, String)>,
    renumbered: &mut usize,
    skipped: &mut usize,
    changes: &mut Vec<LineRenumber>,
) {
    let mut counter: BTreeMap<&str, u64> = BTreeMap::new();
    for d in directives {
        let attr = directive_code(d);
        let from = match attr.map(|a| &a.value) {
            Some(AttrValue::Str(s)) => Some(s.clone()),
            Some(_) => {
                *skipped += 1;
                continue;
            }
            None => None,
        };
        let next = counter.entry(d.tag.as_str()).or_insert(0);
        let Some(nc) = next.checked_add(10) else {
            continue;
        };
        *next = nc;
        let code = format!("{nc:04}");
        if from.as_deref() == Some(code.as_str()) {
            continue;
        }
        match attr {
            Some(attr) => edits.push((
                attr.span.byte_start,
                attr.span.byte_end,
                format!("{DIRECTIVE_TEXT_CODE_ATTR}=\"{code}\""),
            )),
            None => {
                let (at, inserted) = directive_code_insert(d, bytes, &code);
                edits.push((at, at, inserted));
            }
        }
        changes.push(LineRenumber {
            line: d.span.line,
            from,
            to: code,
        });
        *renumbered += 1;
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
        let out = tag_document(NO_ATTRS, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@narrator{code=\"0010\"}: hi there"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn tags_line_with_existing_attrs() {
        let out = tag_document(WITH_ATTRS, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 1);
        // `code` is merged as the FIRST attribute, existing attrs preserved.
        assert!(
            out.text.contains("@fixer{code=\"0010\" mono}: hmm"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn no_attr_block_gets_fresh_code_block() {
        // §7.1 no-attr path: `@marina: hi` -> `@marina{code="0010"}: hi`.
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@marina: hi\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 1);
        assert!(
            out.text.contains("@marina{code=\"0010\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    #[test]
    fn merge_into_existing_attr_block_is_first() {
        // §7.1 merge path: `code` lands as the FIRST attr, right after `{`.
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@marina{emotion=\"x\"}: hi\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 1);
        assert!(
            out.text
                .contains("@marina{code=\"0010\" emotion=\"x\"}: hi"),
            "got:\n{}",
            out.text
        );
    }

    // ── retag_document (`lute tag --renumber`) ──────────────────────────────

    fn renumbered(out: RetagOutcome) -> (String, usize, usize) {
        match out {
            RetagOutcome::Renumbered {
                text,
                renumbered,
                skipped,
                ..
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
        let (text, n, skipped) =
            renumbered(retag_document(src, &std::collections::BTreeSet::new()));
        assert_eq!((n, skipped), (3, 0));
        assert!(
            text.contains("@fixer{code=\"0010\"}: first"),
            "got:\n{text}"
        );
        assert!(
            text.contains("@fixer{code=\"0020\" mono}: second"),
            "got:\n{text}"
        );
        assert!(
            text.contains("@fixer{code=\"0030\"}: third"),
            "got:\n{text}"
        );
    }

    #[test]
    fn retag_reports_each_changed_line_with_its_old_and_new_code() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0050\"}: first\n\
                   @fixer{code=\"0020\"}: second\n\
                   @fixer: third\n";
        let RetagOutcome::Renumbered { changes, .. } =
            retag_document(src, &std::collections::BTreeSet::new())
        else {
            panic!("expected Renumbered");
        };
        assert_eq!(
            changes,
            vec![
                LineRenumber {
                    line: 8,
                    from: Some("0050".into()),
                    to: "0010".into()
                },
                LineRenumber {
                    line: 10,
                    from: None,
                    to: "0030".into()
                },
            ]
        );
    }

    #[test]
    fn tag_splits_codes_from_instance_keys() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer: first\n\
                   ::use{component=\"fire\"}\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
        assert_eq!((out.added, out.instances), (2, 1));
    }

    #[test]
    fn retag_keeps_per_speaker_sequences_independent() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0090\"}: a\n\
                   @marina{code=\"0070\"}: b\n\
                   @fixer: c\n";
        let (text, n, _) = renumbered(retag_document(src, &std::collections::BTreeSet::new()));
        assert_eq!(n, 3);
        assert!(text.contains("@fixer{code=\"0010\"}: a"), "got:\n{text}");
        assert!(text.contains("@marina{code=\"0010\"}: b"), "got:\n{text}");
        assert!(text.contains("@fixer{code=\"0020\"}: c"), "got:\n{text}");
    }

    #[test]
    fn retag_is_idempotent() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=\"0050\"}: first\n\
                   @fixer: second\n";
        let (once, n1, _) = renumbered(retag_document(src, &std::collections::BTreeSet::new()));
        assert_eq!(n1, 2);
        let (twice, n2, _) = renumbered(retag_document(&once, &std::collections::BTreeSet::new()));
        assert_eq!(n2, 0, "second run must be a no-op");
        assert_eq!(once, twice);
    }

    #[test]
    fn retag_matches_fresh_tag_output() {
        // A renumbered doc and a freshly-tagged doc are indistinguishable.
        let untagged = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                        @fixer: a\n@marina: b\n@fixer: c\n";
        let fresh = tag_document(untagged, &std::collections::BTreeSet::new()).text;
        let (forced, _, _) =
            renumbered(retag_document(untagged, &std::collections::BTreeSet::new()));
        assert_eq!(fresh, forced);
    }

    #[test]
    fn retag_refuses_codes_locked() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: true\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: kept\n";
        assert_eq!(retag_document(src, &BTreeSet::new()), RetagOutcome::Locked);
    }

    #[test]
    fn retag_lock_fails_closed_on_malformed_value() {
        // `codesLocked: "no"` is not `false` — a typo must never unlock.
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: \"no\"\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: kept\n";
        assert_eq!(retag_document(src, &BTreeSet::new()), RetagOutcome::Locked);
    }

    #[test]
    fn retag_codes_locked_false_unlocks() {
        let src =
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncodesLocked: false\n---\n\
                   ## Shot 1.\n@fixer{code=\"0050\"}: a\n";
        let (text, n, _) = renumbered(retag_document(src, &std::collections::BTreeSet::new()));
        assert_eq!(n, 1);
        assert!(text.contains("code=\"0010\""), "got:\n{text}");
    }

    #[test]
    fn retag_skips_non_string_code_and_consumes_no_slot() {
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                   @fixer{code=@customId}: special\n\
                   @fixer: plain\n";
        let (text, n, skipped) =
            renumbered(retag_document(src, &std::collections::BTreeSet::new()));
        assert_eq!((n, skipped), (1, 1));
        assert!(
            text.contains("@fixer{code=@customId}: special"),
            "got:\n{text}"
        );
        // The @ref line consumed no slot: the plain line starts the sequence.
        assert!(
            text.contains("@fixer{code=\"0010\"}: plain"),
            "got:\n{text}"
        );
    }

    /// dsl 0.23.0 §4: each lore `<beat>` bundle is its own identity scope —
    /// a fresh per-speaker counter per beat, independent of entries and of
    /// other beats — and a beat's branch-choice lines share its scope.
    const BUNDLE: &str = "---\nid: ship.records\nkind: lore\n---\n\
                          <entry id=\"note\">\n@n: e1\n</entry>\n\
                          <beat id=\"dock\" on=\"talk\">\n@n: b1\n\
                          <branch id=\"ask\">\n<choice id=\"c\" text=\"C\">\n@n: b2\n</choice>\n</branch>\n\
                          </beat>\n\
                          <beat id=\"yard\" on=\"talk\">\n@n{code=\"0040\"}: y1\n@n: y2\n</beat>\n";

    #[test]
    fn tag_bundle_beats_get_their_own_scopes() {
        let out = tag_document(BUNDLE, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 4, "got:\n{}", out.text);
        for want in [
            "@n{code=\"0010\"}: e1",
            "@n{code=\"0010\"}: b1",
            "@n{code=\"0020\"}: b2",
            "@n{code=\"0040\"}: y1",
            "@n{code=\"0050\"}: y2",
        ] {
            assert!(out.text.contains(want), "missing {want} in:\n{}", out.text);
        }
    }

    #[test]
    fn retag_bundle_beats_restart_counters() {
        let (text, n, _) = renumbered(retag_document(BUNDLE, &std::collections::BTreeSet::new()));
        assert_eq!(n, 5, "got:\n{text}");
        for want in [
            "@n{code=\"0010\"}: e1",
            "@n{code=\"0010\"}: b1",
            "@n{code=\"0020\"}: b2",
            "@n{code=\"0010\"}: y1",
            "@n{code=\"0020\"}: y2",
        ] {
            assert!(text.contains(want), "missing {want} in:\n{text}");
        }
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
        let (text, n, _) = renumbered(retag_document(src, &std::collections::BTreeSet::new()));
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
        let out = tag_document(ALREADY, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 0);
        assert_eq!(out.text, ALREADY);
    }

    #[test]
    fn new_codes_step_above_same_speaker_max() {
        // same speaker `a`: one tagged 0050 + one untagged -> untagged gets 0060
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a{code=\"0050\"}: one\n@a: two\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let once = tag_document(NO_ATTRS, &std::collections::BTreeSet::new()).text;
        let twice = tag_document(&once, &std::collections::BTreeSet::new());
        assert_eq!(twice.added, 0);
        assert_eq!(twice.text, once);
    }

    #[test]
    fn tagged_output_still_parses_clean() {
        // the rewritten doc must have no NEW parse errors (the inserted code attr is valid)
        let out = tag_document(NO_ATTRS, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let twice = tag_document(&out.text, &std::collections::BTreeSet::new());
        assert_eq!(twice.added, 0, "must be idempotent");
        assert_eq!(twice.text, out.text);
    }

    #[test]
    fn comment_brace_in_text_is_not_an_attr_block() {
        // A `{` inside a comment in the TEXT (after the second `:`) must NOT be
        // mistaken for an attr block; the code goes in a fresh `{code=…}` after
        // the speaker ident, the text is untouched, and re-tagging is a no-op.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@marina: hi /* { */ there\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
        assert_eq!(out.added, 1);
        assert!(
            out.text
                .contains("@marina{code=\"0010\"}: hi /* { */ there"),
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
        let twice = tag_document(&out.text, &std::collections::BTreeSet::new());
        assert_eq!(
            twice.added, 0,
            "second tag run must be a no-op (idempotent)"
        );
        assert_eq!(twice.text, out.text, "idempotent: byte-identical on re-run");
    }

    #[test]
    fn code_above_u32_max_does_not_collide() {
        // same speaker `a` at u32::MAX + an untagged `a` line -> a's counter steps
        // to 4294967305 (u64 counter, so no saturation/collision at u32::MAX).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a{code=\"4294967295\"}: one\n@a: two\n";
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let out = tag_document(src, &std::collections::BTreeSet::new());
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
        let once = tag_document(src, &std::collections::BTreeSet::new()).text;
        let twice = tag_document(&once, &std::collections::BTreeSet::new());
        assert_eq!(twice.added, 0);
        assert_eq!(twice.text, once);
    }
    #[test]
    fn tags_component_instances_and_is_idempotent() {
        let src = "---\nkind: scene\n---\n## Shot\n::use{component=\"fire\" flare=\"high\"}\n<beat use=\"talk\" id=\"a\">\n@n: hi\n</beat>\n";
        let mut inserts = Vec::new();
        tag_components(src, &mut inserts);
        inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let mut once = src.to_string();
        for (at, inserted) in &inserts {
            once.insert_str(*at, inserted);
        }
        assert_eq!(inserts.len(), 2);
        assert!(once.contains("component=\"fire\""));
        assert!(once.matches("instance=\"use-001\"").count() == 2);
        let mut second = Vec::new();
        tag_components(&once, &mut second);
        assert!(second.is_empty());
    }
    #[test]
    fn component_instances_increment_within_document_owner() {
        let src = "## One\n::use{component=\"fire\"}\n## Two\n::use{component=\"fire\"}\n";
        let mut inserts = Vec::new();
        tag_components(src, &mut inserts);
        inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let mut out = src.to_string();
        for (at, inserted) in &inserts {
            out.insert_str(*at, inserted);
        }
        assert_eq!(out.matches("instance=\"use-001\"").count(), 1);
        assert_eq!(out.matches("instance=\"use-002\"").count(), 1);
    }

    #[test]
    fn force_locked_adds_instances_without_retagging_lines() {
        let src = "---\nkind: scene\ncodesLocked: true\n---\n## One\n::use{component=\"fire\"}\n@a{code=\"0050\"}: kept\n";
        match retag_document(src, &std::collections::BTreeSet::new()) {
            RetagOutcome::LockedInstances { text, added } => {
                assert_eq!(added, 1);
                assert!(text.contains("instance=\"use-001\""));
                assert!(text.contains("code=\"0050\""));
            }
            other => panic!("expected locked instance migration, got {other:?}"),
        }
    }
}
