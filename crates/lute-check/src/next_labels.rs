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

use std::collections::{BTreeMap, BTreeSet};

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
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
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
                format!("mark `{id}` is already declared elsewhere in this document"),
                span,
            ));
        } else {
            self.labels
                .insert(id.to_string(), LabelSite { pos: self.pos });
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
                Node::Directive(_)
                | Node::Set(_)
                | Node::Assert(_)
                | Node::Retract(_)
                | Node::Timeline(_) => {
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
                    for b in h.bodies() {
                        self.walk(b);
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
/// `E-NEXT-BACKWARD`. Walks `doc.shots`, `doc.quests`, then the lore
/// addressing units — `doc.entries` (dsl 0.19.0) and `doc.beats` (dsl 0.23.0
/// §4) interleaved in source order, as the artifact lays them out — each
/// recursively
/// — mirrors `reachability::check_reachability_in`'s own walk shape. The
/// label NAMESPACE is document-wide: ids are NOT reset between shots/quests
/// — a mark in shot 1 and a `::next` in shot 4 resolve against the SAME
/// table shot 1 populated (dsl 0.12.0: "a single table" — this is also what
/// lets a guarded `::next` join a LATER shot, `lute-compile::address`'s
/// document-wide named-label resolution pass).
pub fn check_next_labels(doc: &Document) -> Vec<Diagnostic> {
    let Collector {
        labels,
        dups,
        nexts,
        ..
    } = collect(doc);
    let mut diags = dups;
    for next in nexts {
        match labels.get(&next.to) {
            None => {
                // Round-6 T3-60: a target that names a heading, a choice or
                // a menu says what it is; otherwise the nearest mark.
                let hint = match not_a_mark(doc, &next.to) {
                    Some(why) => format!(" — {why}"),
                    None => lute_manifest::suggest::did_you_mean(
                        &next.to,
                        labels.keys().map(String::as_str),
                    ),
                };
                diags.push(diag(
                    E_NEXT_UNDEFINED,
                    format!(
                        "`::next` targets `{}`, which no `::mark` or line `id=` in this document declares{hint}",
                        next.to
                    ),
                    next.span,
                ));
            }
            Some(label) if label.pos <= next.pos => diags.push(diag(
                E_NEXT_BACKWARD,
                format!(
                    "`::next` targets mark `{}`, which is not forward of this `::next` in document \
                     order — `::next` only jumps forward; to offer choices again, use a `<hub>` \
                     (it asks until an `exit` choice is taken)",
                    next.to
                ),
                next.span,
            )),
            Some(_) => {}
        }
    }
    diags
}

/// Why `to` is no jump target when it names something else in `doc` — a
/// `## ` heading, a `<branch>`/`<hub>` id, a choice id — or Ink's `END`;
/// `None` when it names nothing (a typo, answered by did-you-mean). A
/// heading matches ignoring case and spacing (`## Lamp Room` for
/// `lampRoom`).
fn not_a_mark(doc: &Document, to: &str) -> Option<String> {
    const TARGET: &str = "a `::next` target is a `::mark{id=\"…\"}` (or a line's `id=`) later in \
                          this document";
    match to {
        "END" => {
            return Some(
                "Ink's `END` ends the whole story, which in Lute is the schema's `terminal:` \
                 condition: a scene makes it hold with an ordinary `::set{…}` (`::end` ends only \
                 this scene, like Ink's `DONE`)"
                    .to_string(),
            );
        }
        "DONE" => return Some("a scene ends with `::end`".to_string()),
        _ => {}
    }
    let fold = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    if let Some(shot) = doc.shots.iter().find(|s| fold(&s.heading) == fold(to)) {
        return Some(format!(
            "`{to}` is the `## {}` heading, not a mark; {TARGET}, so put `::mark{{id=\"{to}\"}}` \
             under that heading",
            shot.heading
        ));
    }
    fn find(nodes: &[Node], to: &str) -> Option<String> {
        nodes.iter().find_map(|node| {
            let (tag, id, choices) = match node {
                Node::Branch(b) => ("branch", Some(b.id.as_str()), b.choices.as_slice()),
                Node::Hub(h) => ("hub", attr_str(&h.attrs, "id"), h.choices.as_slice()),
                Node::Match(m) => {
                    return m.arms.iter().find_map(|arm| match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => find(body, to),
                    })
                }
                Node::On(o) => return find(&o.body, to),
                Node::Objective(o) => return find(&o.body, to),
                _ => return None,
            };
            let menu = id.map_or(format!("`<{tag}>`"), |id| format!("`<{tag} id=\"{id}\">`"));
            if id == Some(to) {
                return Some(format!("`{to}` is the id of {menu}, not a mark"));
            }
            choices
                .iter()
                .find_map(|c| {
                    if c.id == to {
                        Some(format!("`{to}` is a choice id in {menu}, not a mark"))
                    } else {
                        find(&c.body, to)
                    }
                })
                .or_else(|| match node {
                    Node::Hub(h) => h.on_return.as_ref().and_then(|r| find(&r.body, to)),
                    _ => None,
                })
        })
    }
    let bodies = doc
        .shots
        .iter()
        .map(|s| s.body.as_slice())
        .chain(doc.quests.iter().map(|q| q.body.as_slice()))
        .chain(doc.entries.iter().map(|e| e.body.as_slice()))
        .chain(doc.beats.iter().map(|b| b.body.as_slice()));
    for body in bodies {
        if let Some(what) = find(body, to) {
            return Some(format!("{what}; {TARGET}"));
        }
    }
    None
}

/// dsl 0.27.0 (round-5 T3-7): every label id some `::next{to}` in `doc`
/// names — the `::mark`s / `id=` lines a walk can enter by a jump, so
/// content from one of them on is reachable even after an `::end`.
pub(crate) fn next_targets(doc: &Document) -> BTreeSet<String> {
    collect(doc).nexts.into_iter().map(|n| n.to).collect()
}

/// `node` is — or holds, at any depth — a label (`::mark{id}` or a line's
/// `id=`) named in `targets`: a walk can enter it by a jump.
pub(crate) fn holds_label(node: &Node, targets: &BTreeSet<String>) -> bool {
    let named = |attrs: &[Attr]| attr_str(attrs, "id").is_some_and(|id| targets.contains(id));
    let any = |nodes: &[Node]| nodes.iter().any(|n| holds_label(n, targets));
    match node {
        Node::Directive(d) => d.tag == lute_manifest::core::MARK_DIRECTIVE && named(&d.attrs),
        Node::Line(l) => named(&l.attrs),
        Node::Branch(b) => b.choices.iter().any(|c| any(&c.body)),
        Node::Hub(h) => h.bodies().any(|b| any(b)),
        Node::Match(m) => m.arms.iter().any(|arm| match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => any(body),
        }),
        Node::On(o) => any(&o.body),
        Node::Objective(o) => any(&o.body),
        Node::Set(_) | Node::Assert(_) | Node::Retract(_) | Node::Timeline(_) => false,
    }
}

/// The document's labels and `::next` sites, in [`Collector::walk`] order.
fn collect(doc: &Document) -> Collector {
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
    let mut units: Vec<(usize, &[Node])> = doc
        .entries
        .iter()
        .map(|e| (e.span.byte_start, e.body.as_slice()))
        .chain(
            doc.beats
                .iter()
                .map(|b| (b.span.byte_start, b.body.as_slice())),
        )
        .collect();
    units.sort_by_key(|(start, _)| *start);
    for (_, body) in units {
        c.walk(body);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        let full = format!(
            "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{src}\n"
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

    // A misspelled target names the document's closest mark; a target
    // nothing resembles gets no guess.
    #[test]
    fn undefined_target_suggests_the_nearest_mark() {
        let d = doc("::next{to=\"endng\"}\n::mark{id=\"ending\"}\n@narrator: bye\n");
        let diags = check_next_labels(&d);
        assert_eq!(codes(&diags), ["E-NEXT-UNDEFINED"]);
        assert!(
            diags[0].message.contains("did you mean `ending`?"),
            "{}",
            diags[0].message
        );
        let d = doc("::next{to=\"nowhere\"}\n::mark{id=\"ending\"}\n@narrator: bye\n");
        assert!(!check_next_labels(&d)[0].message.contains("did you mean"));
    }

    // Round-6 T3-60: a backward jump says what loops instead.
    #[test]
    fn backward_target_errors() {
        let d = doc("::mark{id=\"x\"}\n@narrator: hi\n::next{to=\"x\"}\n");
        let diags = check_next_labels(&d);
        assert_eq!(codes(&diags), ["E-NEXT-BACKWARD"]);
        assert!(diags[0].message.contains("`<hub>`"), "{}", diags[0].message);
    }

    // Round-6 T3-60: a target naming a heading, a choice, or Ink's `END`
    // says what that id is instead of only "undefined".
    #[test]
    fn undefined_target_names_what_the_id_is() {
        let message = |src: &str| check_next_labels(&doc(src))[0].message.clone();
        let m = message("::next{to=\"gallery\"}\n\n## Gallery\n\n@narrator: fog\n");
        assert!(m.contains("`## Gallery` heading"), "{m}");
        let m = message(
            "<branch id=\"door\">\n  <choice id=\"inside\" label=\"In\">\n    @narrator: in\n  \
             </choice>\n  <choice id=\"stay\" label=\"Stay\">\n    ::next{to=\"inside\"}\n  \
             </choice>\n</branch>\n",
        );
        assert!(m.contains("choice id in `<branch id=\"door\">`"), "{m}");
        // Ink's END ends the story (`terminal:`); DONE ends the scene.
        assert!(message("::next{to=\"END\"}\n").contains("`terminal:`"));
        assert!(message("::next{to=\"DONE\"}\n").contains("a scene ends with `::end`"));
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
