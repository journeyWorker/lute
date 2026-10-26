//! Duplicate `(speaker, code)` line addressing (`E-DUP-LINE-CODE`, dsl §12).

use super::{diag, Arm, AttrValue, BTreeSet, Diagnostic, Document, Line, Node, Severity};

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
    check_line_codes_with_policy(doc, false, None)
}

/// Check line identity under the governing project's stable-identity policy.
/// With `snapshot`, the codes of directives with a `text` attribute are
/// checked too, per directive tag (0.51.0 §5.3), in the same scopes.
pub(crate) fn check_line_codes_with_policy(
    doc: &Document,
    require_stable: bool,
    snapshot: Option<&lute_manifest::snapshot::CapabilitySnapshot>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let scope = |bodies: &mut dyn Iterator<Item = &Vec<Node>>, diags: &mut Vec<Diagnostic>| {
        let mut lines: Vec<&Line> = Vec::new();
        let mut directives: Vec<&lute_syntax::ast::Directive> = Vec::new();
        for body in bodies {
            collect_lines(body, &mut lines);
            if let Some(snapshot) = snapshot {
                let has_text = |tag: &str| {
                    snapshot
                        .directive(tag)
                        .is_some_and(lute_manifest::schema::DirectiveDecl::has_text_attr)
                };
                collect_text_directives(body, &has_text, &mut directives);
            }
        }
        check_dup_line_codes(&lines, require_stable, diags);
        check_dup_directive_codes(&directives, require_stable, diags);
    };
    scope(&mut doc.sections.iter().map(|s| &s.body), &mut diags);
    for quest in &doc.quests {
        scope(&mut std::iter::once(&quest.body), &mut diags);
    }
    for entry in &doc.entries {
        scope(&mut std::iter::once(&entry.body), &mut diags);
    }
    for beat in &doc.beats {
        scope(&mut std::iter::once(&beat.body), &mut diags);
    }
    diags
}

/// 0.51.0 §5.3: every repeated `(tag, code)` pair among `directives` (one
/// identity scope, document order) is `E-DUP-LINE-CODE` at the later call;
/// with `require_stable`, a call without `code` is `W-LINE-CODE-UNTAGGED`.
fn check_dup_directive_codes(
    directives: &[&lute_syntax::ast::Directive],
    require_stable: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let mut seen: std::collections::BTreeMap<(&str, String), u32> =
        std::collections::BTreeMap::new();
    for d in directives {
        let Some(attr) = d
            .attrs
            .iter()
            .find(|a| a.key == lute_manifest::schema::DIRECTIVE_TEXT_CODE_ATTR)
        else {
            if require_stable {
                diags.push(diag(
                    "W-LINE-CODE-UNTAGGED",
                    Severity::Warning,
                    format!(
                        "`::{}` has `text` attributes but no `code`; run `lute tag` to allocate \
                         one",
                        d.tag
                    ),
                    d.span,
                ));
            }
            continue;
        };
        let AttrValue::Str(code) = &attr.value else {
            continue;
        };
        let code = code.trim().to_string();
        if let Some(first) = seen.get(&(d.tag.as_str(), code.clone())) {
            diags.push(diag(
                "E-DUP-LINE-CODE",
                Severity::Error,
                format!(
                    "`::{}` calls at lines {first} and {} share `code=\"{code}\"`; a (directive, \
                     code) pair is the localization identity of its `text` attributes",
                    d.tag, d.span.line
                ),
                d.span,
            ));
        } else {
            seen.insert((d.tag.as_str(), code), d.span.line);
        }
    }
}

/// Every directive `has_text` admits (by tag: one with a `text` attribute)
/// under `nodes`, in document order, nested bodies and timeline clips
/// included (0.51.0 §5.3).
pub(crate) fn collect_text_directives<'a>(
    nodes: &'a [Node],
    has_text: &dyn Fn(&str) -> bool,
    out: &mut Vec<&'a lute_syntax::ast::Directive>,
) {
    let has_text_d = |d: &lute_syntax::ast::Directive| has_text(&d.tag);
    for node in nodes {
        match node {
            Node::Directive(d) if has_text_d(d) => out.push(d),
            Node::Timeline(t) => {
                for track in &t.tracks {
                    for clip in &track.clips {
                        if let lute_syntax::ast::ClipNode::Directive(d) = &clip.node {
                            if has_text_d(d) {
                                out.push(d);
                            }
                        }
                    }
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_text_directives(&choice.body, has_text, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    collect_text_directives(body, has_text, out);
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_text_directives(b, has_text, out);
                }
            }
            Node::Objective(o) => collect_text_directives(&o.body, has_text, out),
            Node::On(o) => collect_text_directives(&o.body, has_text, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// Flag every repeated `(speaker, code)` pair WITHIN `lines` — the caller
/// decides the identity scope (whole document for a scene, per-`<quest>` for
/// a quest, dsl 0.2.0 §7) by choosing which lines to pass in one call.
fn check_dup_line_codes<'a>(lines: &[&'a Line], require_stable: bool, diags: &mut Vec<Diagnostic>) {
    let mut seen: BTreeSet<(&'a str, String)> = BTreeSet::new();
    for line in lines {
        let Some(code) = authored_code(line) else {
            if require_stable {
                diags.push(diag(
                    "W-LINE-CODE-UNTAGGED",
                    Severity::Warning,
                    "content line has no per-speaker `code`; run `lute tag` to allocate one"
                        .to_string(),
                    line.span,
                ));
            }
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
