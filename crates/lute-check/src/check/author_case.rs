//! Authoring case (dsl 0.37.0 §2.1, D14): attribute, tag-attribute and
//! frontmatter keys are lowerCamelCase. A key containing `_` is
//! `E-AUTHOR-CASE`, naming its lowerCamelCase spelling.

use lute_core_span::{Diagnostic, Layer, Severity};
use lute_syntax::ast::{Arm, Attr, Document, Node};

/// `E-AUTHOR-CASE`: an author key that is not lowerCamelCase (it contains
/// `_`).
pub const E_AUTHOR_CASE: &str = "E-AUTHOR-CASE";

/// The lowerCamelCase spelling of a `snake_case` key: `voice_key` →
/// `voiceKey`. Empty segments (`a__b`, `_a`) are dropped.
pub fn lower_camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for (i, seg) in key.split('_').filter(|s| !s.is_empty()).enumerate() {
        let mut chars = seg.chars();
        if let Some(first) = chars.next() {
            if i == 0 {
                out.extend(first.to_lowercase());
            } else {
                out.extend(first.to_uppercase());
            }
            out.push_str(chars.as_str());
        }
    }
    out
}

fn diag(what: &str, key: &str, span: lute_core_span::Span, layer: Layer) -> Diagnostic {
    Diagnostic {
        code: E_AUTHOR_CASE.to_string(),
        severity: Severity::Error,
        message: format!(
            "{what} `{key}` is not lowerCamelCase — write `{}`; authoring keys never contain `_` \
             (dsl 0.37.0 §2.1)",
            lower_camel(key)
        ),
        evidence: None,
        span,
        layer,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn attrs(list: &[Attr], layer: Layer, out: &mut Vec<Diagnostic>) {
    for a in list.iter().filter(|a| a.key.contains('_')) {
        out.push(diag("attribute", &a.key, a.span, layer));
    }
}

fn nodes(list: &[Node], out: &mut Vec<Diagnostic>) {
    for node in list {
        match node {
            Node::Line(l) => attrs(&l.attrs, Layer::Content, out),
            Node::Directive(d) => attrs(&d.attrs, Layer::Staging, out),
            Node::Branch(b) => {
                attrs(&b.attrs, Layer::Logic, out);
                for c in &b.choices {
                    attrs(&c.attrs, Layer::Logic, out);
                    nodes(&c.body, out);
                }
            }
            Node::Hub(h) => {
                attrs(&h.attrs, Layer::Logic, out);
                for c in &h.choices {
                    attrs(&c.attrs, Layer::Logic, out);
                    nodes(&c.body, out);
                }
                if let Some(r) = &h.on_return {
                    attrs(&r.attrs, Layer::Logic, out);
                    nodes(&r.body, out);
                }
            }
            Node::Match(m) => {
                attrs(&m.attrs, Layer::Logic, out);
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    nodes(body, out);
                }
            }
            Node::Objective(o) => {
                attrs(&o.attrs, Layer::Logic, out);
                for r in &o.rewards {
                    attrs(&r.attrs, Layer::Logic, out);
                }
                nodes(&o.body, out);
            }
            Node::On(o) => {
                attrs(&o.attrs, Layer::Logic, out);
                nodes(&o.body, out);
            }
            Node::Timeline(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// `E-AUTHOR-CASE` for every authored key of `doc` containing `_`: the
/// residual attributes of every line, directive and tag, and the top-level
/// frontmatter keys.
pub(super) fn check_author_case(doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if let Ok(serde_yaml::Value::Mapping(map)) = serde_yaml::from_str::<serde_yaml::Value>(
        crate::chapters::authored_yaml(&doc.meta.raw_yaml),
    ) {
        for key in map.keys().filter_map(serde_yaml::Value::as_str) {
            if key.contains('_') {
                out.push(diag(
                    "frontmatter key",
                    key,
                    crate::meta::meta_key_span(&doc.meta, key),
                    Layer::Content,
                ));
            }
        }
    }
    for s in &doc.sections {
        nodes(&s.body, &mut out);
    }
    for q in &doc.quests {
        attrs(&q.attrs, Layer::Logic, &mut out);
        for r in &q.rewards {
            attrs(&r.attrs, Layer::Logic, &mut out);
        }
        nodes(&q.body, &mut out);
    }
    for e in &doc.entries {
        attrs(&e.attrs, Layer::Logic, &mut out);
        nodes(&e.body, &mut out);
    }
    for b in &doc.beats {
        attrs(&b.attrs, Layer::Logic, &mut out);
        nodes(&b.body, &mut out);
    }
    out
}
