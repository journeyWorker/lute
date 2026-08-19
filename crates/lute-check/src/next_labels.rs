//! dsl 0.12.0 whole-document label pass: `::mark{id}` / a content line's
//! `id=` register a DOCUMENT-WIDE forward-jump label; `::next{to}` resolves
//! against that ONE table. Three diagnostics: `E-MARK-DUP` (a label id
//! reused — mark/mark, mark/line-id, or line-id/line-id, ALL share one
//! namespace), `E-NEXT-UNDEFINED` (a `to=` naming no label anywhere in the
//! document), `E-NEXT-BACKWARD` (a `to=` naming a label at or before the
//! `::next` site's own document position — forward-only, dsl 0.12.0: the
//! walk's DAG stays acyclic).
//!
//! Position is a MONOTONIC counter ticked at every label-bearing site
//! (`::mark`, a line with `id=`) and every `::next` site, in the SAME
//! depth-first document order `lute-compile::stage::walk_seq` flattens
//! records in (top-level nodes in order; a `<branch>`/`<hub>` choice body,
//! or a `<match>` arm body, recursed in order) — so "forward" here means
//! exactly what `lute-compile::address`'s addr-lexicographic order will
//! mean once compiled. Only label/next sites are ticked (not every node):
//! skipping uninteresting nodes never changes the RELATIVE order of two
//! ticked sites, so the counter stays a sound (if sparse) position axis.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Attr, AttrValue, Document, Node};

/// `E-MARK-DUP` (dsl 0.12.0): a label id (`::mark{id}` or a line's `id=`)
/// reused anywhere in the document — one namespace, mark and line ids alike.
pub const E_MARK_DUP: &str = "E-MARK-DUP";

/// `E-NEXT-UNDEFINED` (dsl 0.12.0): `::next{to}` names no label anywhere in
/// the document.
pub const E_NEXT_UNDEFINED: &str = "E-NEXT-UNDEFINED";

/// `E-NEXT-BACKWARD` (dsl 0.12.0): `::next{to}` names a label at or before
/// its own document position — jumps are forward-only.
pub const E_NEXT_BACKWARD: &str = "E-NEXT-BACKWARD";

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

fn attr_span(attrs: &[Attr], key: &str) -> Option<Span> {
    attrs.iter().find(|a| a.key == key).map(|a| a.value_span)
}

/// One label's DEFINING position — first occurrence only; a repeat is
/// `E-MARK-DUP` at the repeat's own span and never overwrites the table.
struct LabelSite {
    pos: u64,
}

/// One `::next{to}` site: its own document position, the target id, and the
/// span to anchor `E-NEXT-UNDEFINED`/`E-NEXT-BACKWARD` at.
struct NextSite {
    pos: u64,
    to: String,
    span: Span,
}

struct Collector {
    pos: u64,
    labels: BTreeMap<String, LabelSite>,
    dups: Vec<Diagnostic>,
    nexts: Vec<NextSite>,
}

impl Collector {
    fn tick(&mut self) {
        self.pos += 1;
    }

    fn record_label(&mut self, id: &str, span: Span) {
        if self.labels.contains_key(id) {
            self.dups.push(diag(
                E_MARK_DUP,
                format!("label `{id}` is already declared elsewhere in this document (dsl 0.12.0)"),
                span,
            ));
        } else {
            self.labels.insert(id.to_string(), LabelSite { pos: self.pos });
        }
        self.tick();
    }

    fn record_next(&mut self, d: &lute_syntax::ast::Directive) {
        if let Some(to) = attr_str(&d.attrs, "to") {
            let span = attr_span(&d.attrs, "to").unwrap_or(d.span);
            self.nexts.push(NextSite {
                pos: self.pos,
                to: to.to_string(),
                span,
            });
        }
        self.tick();
    }

    /// Mirrors `lute-compile::stage::walk_seq`'s recursion shape (top-level
    /// nodes, then each `<branch>`/`<hub>` choice body / `<match>` arm body
    /// in order) closely enough that RELATIVE label/next ordering here
    /// agrees with the compiled addr-lexicographic order — `<timeline>`
    /// clips carry no `Node`s (a `::mark`/`::next` inside one is rejected
    /// outright by `check.rs`'s timeline-clip loop, mirroring `::end`), so
    /// they are ticked once as an opaque leaf and never recursed into.
    fn walk(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Directive(d) if d.tag == lute_manifest::core::MARK_DIRECTIVE => {
                    match attr_str(&d.attrs, "id") {
                        Some(id) => self.record_label(id, d.span),
                        None => self.tick(),
                    }
                }
                Node::Directive(d) if d.tag == lute_manifest::core::NEXT_DIRECTIVE => {
                    self.record_next(d);
                }
                Node::Line(l) => match attr_str(&l.attrs, "id") {
                    Some(id) => self.record_label(id, l.span),
                    None => self.tick(),
                },
                Node::Directive(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) | Node::Timeline(_) => {
                    self.tick();
                }
                Node::Branch(b) => {
                    self.tick();
                    for c in &b.choices {
                        self.walk(&c.body);
                    }
                }
                Node::Hub(h) => {
                    self.tick();
                    for c in &h.choices {
                        self.walk(&c.body);
                    }
                }
                Node::Match(m) => {
                    self.tick();
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => self.walk(body),
                        }
                    }
                }
                Node::On(o) => {
                    self.tick();
                    self.walk(&o.body);
                }
                Node::Objective(o) => {
                    self.tick();
                    self.walk(&o.body);
                }
            }
        }
    }
}

/// dsl 0.12.0 whole-document pass: `E-MARK-DUP` / `E-NEXT-UNDEFINED` /
/// `E-NEXT-BACKWARD`. Walks `doc.shots` then `doc.quests`, each recursively
/// — mirrors `reachability::check_reachability_in`'s own walk shape. The
/// label NAMESPACE is document-wide: ids are NOT reset between shots/quests
/// — a mark in shot 1 and a `::next` in shot 4 resolve against the SAME
/// table shot 1 populated (dsl 0.12.0: "a single table" — this is also what
/// lets a guarded `::next` join a LATER shot, `lute-compile::address`'s
/// document-wide named-label resolution pass).
pub fn check_next_labels(doc: &Document) -> Vec<Diagnostic> {
    let mut c = Collector {
        pos: 0,
        labels: BTreeMap::new(),
        dups: Vec::new(),
        nexts: Vec::new(),
    };
    for shot in &doc.shots {
        c.walk(&shot.body);
    }
    for quest in &doc.quests {
        c.walk(&quest.body);
    }
    let Collector { labels, dups, nexts, .. } = c;
    let mut diags = dups;
    for next in nexts {
        match labels.get(&next.to) {
            None => diags.push(diag(
                E_NEXT_UNDEFINED,
                format!("`::next` targets undefined label `{}` (dsl 0.12.0)", next.to),
                next.span,
            )),
            Some(label) if label.pos <= next.pos => diags.push(diag(
                E_NEXT_BACKWARD,
                format!(
                    "`::next` targets label `{}`, which is not forward of this `::next` in document order (dsl 0.12.0)",
                    next.to
                ),
                next.span,
            )),
            Some(_) => {}
        }
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        let full = format!(
            "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{src}\n"
        );
        let (doc, _) = lute_syntax::parse(&full);
        doc
    }

    fn codes(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn clean_forward_mark_and_next_is_clean() {
        let d = doc("@narrator: hi\n::next{to=\"x\"}\n::mark{id=\"x\"}\n@narrator: there\n");
        assert!(check_next_labels(&d).is_empty());
    }

    #[test]
    fn forward_line_id_target_is_clean() {
        let d = doc("::next{to=\"x\"}\n@narrator{id=\"x\"}: there\n");
        assert!(check_next_labels(&d).is_empty());
    }

    #[test]
    fn undefined_target_errors() {
        let d = doc("::next{to=\"nope\"}\n");
        assert_eq!(codes(&check_next_labels(&d)), ["E-NEXT-UNDEFINED"]);
    }

    #[test]
    fn backward_target_errors() {
        let d = doc("::mark{id=\"x\"}\n@narrator: hi\n::next{to=\"x\"}\n");
        assert_eq!(codes(&check_next_labels(&d)), ["E-NEXT-BACKWARD"]);
    }

    #[test]
    fn duplicate_mark_ids_error() {
        let d = doc("::mark{id=\"x\"}\n::mark{id=\"x\"}\n");
        assert_eq!(codes(&check_next_labels(&d)), ["E-MARK-DUP"]);
    }

    #[test]
    fn mark_and_line_id_collision_errors() {
        let d = doc("::mark{id=\"x\"}\n@narrator{id=\"x\"}: hi\n");
        assert_eq!(codes(&check_next_labels(&d)), ["E-MARK-DUP"]);
    }
}
