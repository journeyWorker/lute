//! Post-expansion line identity (0.21.1 T1-10, lamplight F37).
//!
//! `check()` enforces `(speaker, code)` uniqueness — the key every `lineId` and
//! `voiceKey` derives from (dsl §12) — over the document as AUTHORED, and over
//! each component body in isolation. Neither sees the stream the compiler
//! actually addresses: a component's lines are stamped with the HOST's
//! identity prefix at each `::use`, so a tagged component line collides with a
//! host line carrying the same `(speaker, code)`, and a tagged component
//! `::use`d twice collides with itself. Both used to compile to records
//! sharing one `lineId` without a word.
//!
//! This pass runs on the normalized (expanded) tree, inside
//! [`crate::normalize::normalize_document`], so every consumer of that tree —
//! `lute check`'s compile gate, `lute compile`, `compile --all`, `play`,
//! `trace` — refuses the collision with the same `E-DUP-LINE-CODE`
//! `check()` uses for the authored case.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Document, Line, Node};

use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

/// The component a line was expanded from: its name and the span of the
/// OUTERMOST `::use` that brought it in — the only position of it in the
/// document being compiled (nested uses carry component-file spans).
type Origin = Option<(String, Span)>;

/// Every `(speaker, code)` pair repeated within one identity scope of the
/// EXPANDED `doc` where at least one of the two lines came from a component.
/// Pairs repeated among the document's own lines are `check()`'s (it gates
/// before normalization ever runs), as are pairs within one component body.
/// Scopes mirror `check_line_codes`: all shots together, each `<quest>`, each
/// `<entry>`.
pub(crate) fn expanded_line_code_diags(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let scopes = std::iter::once(doc.shots.iter().map(|s| &s.body).collect::<Vec<_>>())
        .chain(doc.quests.iter().map(|q| vec![&q.body]))
        .chain(doc.entries.iter().map(|e| vec![&e.body]));
    for bodies in scopes {
        let mut lines = Vec::new();
        for body in bodies {
            collect(body, &mut Vec::new(), &mut lines);
        }
        check_scope(&lines, &mut diags);
    }
    diags
}

fn collect<'a>(nodes: &'a [Node], stack: &mut Vec<(String, Span)>, out: &mut Vec<(&'a Line, Origin)>) {
    for node in nodes {
        match node {
            Node::Directive(d) if d.tag == COMPONENT_BEGIN => {
                let name = d
                    .attrs
                    .iter()
                    .find_map(|a| match (&*a.key, &a.value) {
                        ("component", AttrValue::Str(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                stack.push((name, d.span));
            }
            Node::Directive(d) if d.tag == COMPONENT_END => {
                stack.pop();
            }
            Node::Line(l) => out.push((l, stack.first().cloned())),
            Node::Branch(b) => {
                for c in &b.choices {
                    collect(&c.body, &mut stack.clone(), out);
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    collect(&c.body, &mut stack.clone(), out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect(body, &mut stack.clone(), out)
                        }
                    }
                }
            }
            Node::On(o) => collect(&o.body, &mut stack.clone(), out),
            Node::Objective(o) => collect(&o.body, &mut stack.clone(), out),
            Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn check_scope(lines: &[(&Line, Origin)], diags: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<(&str, String), (&Line, &Origin)> = BTreeMap::new();
    for (line, origin) in lines {
        let Some(code) = authored_code(line) else {
            continue;
        };
        let key = (line.speaker.as_str(), code.clone());
        let Some((first_line, first_origin)) = seen.get(&key).copied() else {
            seen.insert(key, (line, origin));
            continue;
        };
        if first_origin.is_none() && origin.is_none() {
            continue; // the authored document's own repeat — `check()` owns it
        }
        let speaker = &line.speaker;
        diags.push(Diagnostic {
            code: "E-DUP-LINE-CODE".to_string(),
            severity: Severity::Error,
            message: format!(
                "duplicate `code=\"{code}\"` for speaker `{speaker}` after component \
                 expansion: {} and {} compile to one `lineId`/`voiceKey` (dsl §12) — remove \
                 `code=` from the component's line (an untagged line gets a distinct code at \
                 each use) or give one of them a different code",
                describe(first_line, first_origin),
                describe(line, origin),
            ),
            span: origin.as_ref().map_or(line.span, |(_, at)| *at),
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
}

fn describe(line: &Line, origin: &Origin) -> String {
    match origin {
        None => format!(
            "this document's `@{}` line at {}:{}",
            line.speaker, line.span.line, line.span.column
        ),
        Some((component, at)) => format!(
            "component `{component}`'s `@{}` line (expanded by the `::use` at {}:{})",
            line.speaker, at.line, at.column
        ),
    }
}

/// The trimmed authored string `code` — the exact key the addressing pass
/// derives identity from (mirrors `lute_check`'s `check_line_codes`).
fn authored_code(line: &Line) -> Option<String> {
    line.attrs.iter().find(|a| a.key == "code").and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.trim().to_string()),
        _ => None,
    })
}
