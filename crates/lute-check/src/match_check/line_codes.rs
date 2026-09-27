//! Duplicate `(speaker, code)` line addressing (`E-DUP-LINE-CODE`, dsl §12).

use super::*;

/// dsl §12 (scene) / dsl 0.2.0 §7 (quest): every content `:line`'s `lineId`
/// (`{prefix}.{speaker}_{code}`) and `voiceKey` (`{prefix}.{speaker}-{code}`) derive
/// from its `(speaker, trimmed code)` pair (see `lute-compile`'s addressing
/// pass). Two `:line`s for the SAME speaker carrying the SAME trimmed `code`
/// therefore compile to IDENTICAL `lineId`/`voiceKey` values — corrupting the
/// translation + voice-bank join keys. Flag the SECOND (and each later)
/// occurrence of a repeated `(speaker, code)` pair with `E-DUP-LINE-CODE`, at
/// that line's span.
///
/// Codes are compared as TRIMMED STRINGS — exactly the key the addressing pass
/// uses (`code.trim()`), so ` 0050 ` and `0050` collide but `0050` and `50` do
/// not. Only authored string codes participate; an untagged line derives its
/// code later (uniquely per speaker) and a non-literal code (`@ref`) has no
/// static value to compare, so neither can statically collide.
///
/// **Identity scope** (dsl 0.2.0 §7): a scene's lines share ONE scope — the
/// whole document (all shots), unchanged from 0.1.0. A quest's lines (reached
/// via a `<quest>`'s `<on>`/`<objective>` arms) are scoped PER `<quest>` —
/// each `<quest>` is its own identity domain, so the SAME (speaker, code)
/// pair may repeat across two different quests without colliding, but not
/// twice within one. Each `<entry>` (dsl 0.19.0 §4) and each lore `<beat>`
/// bundle (dsl 0.23.0 §4) is likewise its own
/// identity scope. Document order, deterministic (the caller's final
/// `(byte_start, code)` sort settles ties).
pub fn check_line_codes(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let mut scene_lines: Vec<&Line> = Vec::new();
    for shot in &doc.shots {
        collect_lines(&shot.body, &mut scene_lines);
    }
    check_dup_line_codes(&scene_lines, &mut diags);

    for quest in &doc.quests {
        let mut quest_lines: Vec<&Line> = Vec::new();
        collect_lines(&quest.body, &mut quest_lines);
        check_dup_line_codes(&quest_lines, &mut diags);
    }

    for entry in &doc.entries {
        let mut entry_lines: Vec<&Line> = Vec::new();
        collect_lines(&entry.body, &mut entry_lines);
        check_dup_line_codes(&entry_lines, &mut diags);
    }

    for beat in &doc.beats {
        let mut beat_lines: Vec<&Line> = Vec::new();
        collect_lines(&beat.body, &mut beat_lines);
        check_dup_line_codes(&beat_lines, &mut diags);
    }

    diags
}

/// Flag every repeated `(speaker, code)` pair WITHIN `lines` — the caller
/// decides the identity scope (whole document for a scene, per-`<quest>` for
/// a quest, dsl 0.2.0 §7) by choosing which lines to pass in one call.
fn check_dup_line_codes<'a>(lines: &[&'a Line], diags: &mut Vec<Diagnostic>) {
    let mut seen: BTreeSet<(&'a str, String)> = BTreeSet::new();
    for line in lines {
        let Some(code) = authored_code(line) else {
            continue;
        };
        if !seen.insert((line.speaker.as_str(), code.clone())) {
            diags.push(diag(
                "E-DUP-LINE-CODE",
                Severity::Error,
                format!(
                    "duplicate `:line` `code=\"{code}\"` for speaker `{}`; a (speaker, code) pair \
                     must be unique — its `lineId`/`voiceKey` join keys derive from it (dsl §12)",
                    line.speaker
                ),
                line.span,
            ));
        }
    }
}

/// The line's authored `code`, trimmed to the exact string the addressing pass
/// keys `lineId`/`voiceKey` on. `None` when the line has no `code`, or its
/// `code` is not a string literal (an `@ref`/bare value cannot statically
/// collide).
fn authored_code(line: &Line) -> Option<String> {
    line.attrs
        .iter()
        .find(|a| a.key == "code")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.trim().to_string()),
            _ => None,
        })
}

/// Collect every `Node::Line` in document order, descending into branch choices'
/// and match arms' bodies (mirrors `check.rs::Walker::walk` / `tag.rs`).
pub(crate) fn collect_lines<'a>(nodes: &'a [Node], out: &mut Vec<&'a Line>) {
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
