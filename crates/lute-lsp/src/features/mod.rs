//! Editor protocol projections of the shared resolver.
pub mod completion;
pub mod folding;
pub mod hover;
pub mod nav;
pub mod semtok;
pub mod symbols;


use lute_core_span::Span;
use lute_model::{IdentityMetadata, IdentitySource};
use lute_syntax::ast::{AttrValue, Document, Node};

/// Canonical identity attached to a source cursor. LSP ranges remain span
/// based; this metadata is advisory and never uses `addr` or a byte span as
/// identity.
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
    doc.shots.iter().find_map(|shot| scan(&shot.body, off))
        .or_else(|| doc.quests.iter().find_map(|quest| scan(&quest.body, off)))
        .or_else(|| doc.entries.iter().find_map(|entry| scan(&entry.body, off)))
        .or_else(|| doc.beats.iter().find_map(|beat| scan(&beat.body, off)))
}
