//! Editor protocol projections of the shared resolver.
pub mod completion;
pub mod folding;
pub mod hover;
pub mod nav;
pub mod semtok;
pub mod symbols;

use lute_core_span::Span;
use lute_manifest::core::{JUMP_DIRECTIVE, LABEL_DIRECTIVE, LABEL_NAME_ATTR};
use lute_resolve::cursor::span_contains;
use lute_semantic::{IdentityMetadata, IdentitySource};
use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Line, Node};

/// Canonical identity attached to a source cursor. LSP ranges remain span
/// based; this metadata is advisory and never uses a command `position` or a
/// byte span as identity.
#[derive(Clone, Debug)]
pub struct IdentityAt {
    pub span: Span,
    pub identity: IdentityMetadata,
    pub component_scope: Option<String>,
}

/// Resolve identity metadata for the authored line/use nearest `off`.
/// Consumers may use the span for navigation and the identity for durable
/// joins; untagged constructs are explicitly marked unstable.
pub fn identity_at(doc: &Document, off: usize) -> Option<IdentityAt> {
    fn attrs(attrs: &[lute_syntax::ast::Attr], key: &str) -> Option<(String, Span)> {
        attrs.iter().find(|attr| attr.key == key).and_then(|attr| match &attr.value {
            AttrValue::Str(value) => Some((value.clone(), attr.value_span)),
            _ => None,
        })
    }
    fn scan(nodes: &[Node], off: usize) -> Option<IdentityAt> {
        for node in nodes {
            match node {
                Node::Line(line) if line.span.byte_start <= off && off <= line.span.byte_end => {
                    let code = attrs(&line.attrs, "code");
                    let (computed, source, stable) = match code {
                        Some((code, _)) => (format!("{}.{}", line.speaker, code), IdentitySource::Authored, true),
                        None => (format!("{}.untagged", line.speaker), IdentitySource::Fallback, false),
                    };
                    return Some(IdentityAt {
                        span: line.span,
                        identity: IdentityMetadata { kind: "line".into(), source, computed, stable },
                        component_scope: None,
                    });
                }
                Node::Directive(d) if d.span.byte_start <= off && off <= d.span.byte_end && d.tag == "use" => {
                    let component = attrs(&d.attrs, "component").map(|(v, _)| v).unwrap_or_default();
                    let instance = attrs(&d.attrs, "instance");
                    let (computed, source, stable) = match instance {
                        Some((key, _)) => (format!("{component}#{key}"), IdentitySource::Authored, true),
                        None => (format!("{component}#untagged"), IdentitySource::Fallback, false),
                    };
                    return Some(IdentityAt {
                        span: d.span,
                        identity: IdentityMetadata { kind: "componentInstance".into(), source, computed: computed.clone(), stable },
                        component_scope: Some(computed),
                    });
                }
                Node::Branch(branch) => {
                    for choice in &branch.choices {
                        if let Some(found) = scan(&choice.body, off) { return Some(found); }
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let body = match arm {
                            lute_syntax::ast::Arm::When { body, .. } | lute_syntax::ast::Arm::Otherwise { body, .. } => body,
                        };
                        if let Some(found) = scan(body, off) { return Some(found); }
                    }
                }
                Node::Hub(hub) => {
                    for body in hub.bodies() {
                        if let Some(found) = scan(body, off) { return Some(found); }
                    }
                }
                Node::Objective(objective) => if let Some(found) = scan(&objective.body, off) { return Some(found); },
                Node::On(on) => if let Some(found) = scan(&on.body, off) { return Some(found); },
                Node::Line(_) | Node::Timeline(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) | Node::Directive(_) => {}
            }
        }
        None
    }
    doc.sections.iter().find_map(|section| scan(&section.body, off))
        .or_else(|| doc.quests.iter().find_map(|quest| scan(&quest.body, off)))
        .or_else(|| doc.entries.iter().find_map(|entry| scan(&entry.body, off)))
        .or_else(|| doc.beats.iter().find_map(|beat| scan(&beat.body, off)))
}

/// Every body of `doc` in document order: section bodies, then quest, lore
/// entry and lore beat bodies — the same roots the cursor resolver walks.
fn root_bodies(doc: &Document) -> impl Iterator<Item = &[Node]> {
    doc.sections
        .iter()
        .map(|s| s.body.as_slice())
        .chain(doc.quests.iter().map(|q| q.body.as_slice()))
        .chain(doc.entries.iter().map(|e| e.body.as_slice()))
        .chain(doc.beats.iter().map(|b| b.body.as_slice()))
}

/// Visit every node of `nodes`, descending through every nested body
/// (choices, hub returns, match arms, `<on>`, `<objective>`). Timeline clips
/// are not nodes; [`for_each_directive`] reaches their directives.
fn walk_nodes<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a Node)) {
    for node in nodes {
        f(node);
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| walk_nodes(&c.body, f)),
            Node::Hub(h) => h.bodies().for_each(|body| walk_nodes(body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    walk_nodes(body, f);
                }
            }
            Node::On(o) => walk_nodes(&o.body, f),
            Node::Objective(ob) => walk_nodes(&ob.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// Every directive of `doc` in document order, `<timeline>` clips included.
pub fn for_each_directive<'a>(doc: &'a Document, f: &mut impl FnMut(&'a Directive)) {
    for body in root_bodies(doc) {
        walk_nodes(body, &mut |node| match node {
            Node::Directive(d) => f(d),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|track| &track.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        f(d);
                    }
                }
            }
            _ => {}
        });
    }
}

/// The content line whose span contains `off`, at any nesting depth.
pub fn line_at(doc: &Document, off: usize) -> Option<&Line> {
    let mut found = None;
    for body in root_bodies(doc) {
        walk_nodes(body, &mut |node| {
            if let Node::Line(l) = node {
                if found.is_none() && span_contains(l.span, off) {
                    found = Some(l);
                }
            }
        });
    }
    found
}

/// The quoted string value of `d`'s `key` attribute with its value span.
fn str_attr<'a>(d: &'a Directive, key: &str) -> Option<(&'a str, Span)> {
    d.attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some((s.as_str(), a.value_span)),
        _ => None,
    })
}

/// Every `::label{name}` declaration (dsl 0.37.0 §3.5): the label name and
/// its `name=` value span, in document order. Labels share one
/// document-wide namespace.
pub fn label_decls(doc: &Document) -> Vec<(&str, Span)> {
    let mut out = Vec::new();
    for_each_directive(doc, &mut |d| {
        if d.tag == LABEL_DIRECTIVE {
            out.extend(str_attr(d, LABEL_NAME_ATTR));
        }
    });
    out
}

/// Every `::jump{to}` target (dsl 0.37.0 §3.5): the target label name and
/// its `to=` value span, in document order.
pub fn jump_targets(doc: &Document) -> Vec<(&str, Span)> {
    let mut out = Vec::new();
    for_each_directive(doc, &mut |d| {
        if d.tag == JUMP_DIRECTIVE {
            out.extend(str_attr(d, "to"));
        }
    });
    out
}

/// The label name under `off` when the cursor rests on a `::jump{to=…}`
/// value or a `::label{name=…}` value; `None` anywhere else.
pub fn label_name_at(doc: &Document, off: usize) -> Option<&str> {
    let mut found = None;
    for_each_directive(doc, &mut |d| {
        let key = match d.tag.as_str() {
            JUMP_DIRECTIVE => "to",
            LABEL_DIRECTIVE => LABEL_NAME_ATTR,
            _ => return,
        };
        if let Some((name, span)) = str_attr(d, key) {
            if found.is_none() && span_contains(span, off) {
                found = Some(name);
            }
        }
    });
    found
}
