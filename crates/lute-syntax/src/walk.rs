//! Shared CEL-slot traversal — the single source of truth for "every [`CelSlot`]
//! in a [`Document`], in deterministic pre-order".
//!
//! [`for_each_cel_slot`] / [`for_each_cel_slot_mut`] visit every slot exactly
//! once, in the order downstream passes rely on: `lute-cel::fill` assigns each
//! visited slot a monotonic [`lute_core_span::StableId`] (1, 2, 3, … in this
//! order), and the LSP feature layer collects the same set. Because both consume
//! this one walk, adding a slot-bearing AST node is a single-site change here
//! instead of a hunt through every recursive matcher.
//!
//! ## Canonical pre-order (per shot body, per node in source order)
//! - [`Node::Line`] / [`Node::Directive`] → each `AttrValue::Ref` slot in `attrs`
//!   order.
//! - [`Node::Set`] → `expr`.
//! - [`Node::Branch`] → `attrs` refs; then per `choice`: `choice.when` (if any),
//!   `choice.attrs` refs, then recurse `choice.body`.
//! - [`Node::Match`] → `subject`; then per arm: `When{test, body}` → `test` then
//!   recurse `body`; `Otherwise{body}` → recurse `body`.
//! - [`Node::Timeline`] → `duration` (if any); then per track, per clip:
//!   `ClipNode::Directive` → attr refs; `ClipNode::Set` → `expr`.
//!
//! Only `AttrValue::Ref(slot)` attrs are slots; bare/other attr values are not.
//! This order MUST stay byte-identical to what `lute-cel::fill` historically
//! walked — the StableId sequence (and thus determinism, goldens, examples) rides
//! on it.

use crate::ast::{
    Arm, Attr, AttrValue, Branch, CelSlot, ClipNode, Document, Hub, Match, Node, Timeline,
};

/// Visit every [`CelSlot`] in `doc` in the canonical pre-order, borrowing each.
///
/// The slot references share `doc`'s lifetime, so a caller may collect them into a
/// `Vec<&CelSlot>`.
pub fn for_each_cel_slot<'a>(doc: &'a Document, f: &mut impl FnMut(&'a CelSlot)) {
    for shot in &doc.shots {
        body(&shot.body, f);
    }
}

fn attrs<'a>(attrs: &'a [Attr], f: &mut impl FnMut(&'a CelSlot)) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &attr.value {
            f(slot);
        }
    }
}

fn body<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a CelSlot)) {
    for n in nodes {
        node(n, f);
    }
}

fn node<'a>(n: &'a Node, f: &mut impl FnMut(&'a CelSlot)) {
    match n {
        Node::Line(l) => attrs(&l.attrs, f),
        Node::Directive(d) => attrs(&d.attrs, f),
        Node::Set(s) => f(&s.expr),
        Node::Branch(b) => branch(b, f),
        Node::Match(m) => match_node(m, f),
        Node::Timeline(t) => timeline(t, f),
        Node::Hub(h) => hub(h, f),
    }
}

fn branch<'a>(b: &'a Branch, f: &mut impl FnMut(&'a CelSlot)) {
    attrs(&b.attrs, f);
    for choice in &b.choices {
        if let Some(when) = &choice.when {
            f(when);
        }
        attrs(&choice.attrs, f);
        body(&choice.body, f);
    }
}

fn hub<'a>(h: &'a Hub, f: &mut impl FnMut(&'a CelSlot)) {
    attrs(&h.attrs, f);
    for choice in &h.choices {
        if let Some(when) = &choice.when {
            f(when);
        }
        attrs(&choice.attrs, f);
        body(&choice.body, f);
    }
}

fn match_node<'a>(m: &'a Match, f: &mut impl FnMut(&'a CelSlot)) {
    f(&m.subject);
    for arm in &m.arms {
        match arm {
            Arm::When { test, body: b, .. } => {
                f(test);
                body(b, f);
            }
            Arm::Otherwise { body: b, .. } => body(b, f),
        }
    }
}

fn timeline<'a>(t: &'a Timeline, f: &mut impl FnMut(&'a CelSlot)) {
    if let Some(dur) = &t.duration {
        f(dur);
    }
    for track in &t.tracks {
        for clip in &track.clips {
            match &clip.node {
                ClipNode::Directive(d) => attrs(&d.attrs, f),
                ClipNode::Set(s) => f(&s.expr),
            }
        }
    }
}

/// Visit every [`CelSlot`] in `doc` mutably, in the same canonical pre-order.
///
/// The closure sees one slot at a time (never an escaping borrow), so it can
/// rewrite each in place — this is how `lute-cel::fill` stamps `StableId`s and
/// records parse results.
pub fn for_each_cel_slot_mut(doc: &mut Document, f: &mut impl FnMut(&mut CelSlot)) {
    for shot in &mut doc.shots {
        body_mut(&mut shot.body, f);
    }
}

fn attrs_mut(attrs: &mut [Attr], f: &mut impl FnMut(&mut CelSlot)) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &mut attr.value {
            f(slot);
        }
    }
}

fn body_mut(nodes: &mut [Node], f: &mut impl FnMut(&mut CelSlot)) {
    for n in nodes {
        node_mut(n, f);
    }
}

fn node_mut(n: &mut Node, f: &mut impl FnMut(&mut CelSlot)) {
    match n {
        Node::Line(l) => attrs_mut(&mut l.attrs, f),
        Node::Directive(d) => attrs_mut(&mut d.attrs, f),
        Node::Set(s) => f(&mut s.expr),
        Node::Branch(b) => branch_mut(b, f),
        Node::Match(m) => match_node_mut(m, f),
        Node::Timeline(t) => timeline_mut(t, f),
        Node::Hub(h) => hub_mut(h, f),
    }
}

fn branch_mut(b: &mut Branch, f: &mut impl FnMut(&mut CelSlot)) {
    attrs_mut(&mut b.attrs, f);
    for choice in &mut b.choices {
        if let Some(when) = &mut choice.when {
            f(when);
        }
        attrs_mut(&mut choice.attrs, f);
        body_mut(&mut choice.body, f);
    }
}

fn hub_mut(h: &mut Hub, f: &mut impl FnMut(&mut CelSlot)) {
    attrs_mut(&mut h.attrs, f);
    for choice in &mut h.choices {
        if let Some(when) = &mut choice.when {
            f(when);
        }
        attrs_mut(&mut choice.attrs, f);
        body_mut(&mut choice.body, f);
    }
}

fn match_node_mut(m: &mut Match, f: &mut impl FnMut(&mut CelSlot)) {
    f(&mut m.subject);
    for arm in &mut m.arms {
        match arm {
            Arm::When { test, body: b, .. } => {
                f(test);
                body_mut(b, f);
            }
            Arm::Otherwise { body: b, .. } => body_mut(b, f),
        }
    }
}

fn timeline_mut(t: &mut Timeline, f: &mut impl FnMut(&mut CelSlot)) {
    if let Some(dur) = &mut t.duration {
        f(dur);
    }
    for track in &mut t.tracks {
        for clip in &mut track.clips {
            match &mut clip.node {
                ClipNode::Directive(d) => attrs_mut(&mut d.attrs, f),
                ClipNode::Set(s) => f(&mut s.expr),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{for_each_cel_slot, for_each_cel_slot_mut};
    use crate::ast::{
        Arm, Attr, AttrValue, Branch, CelKind, CelSlot, Choice, Clip, ClipNode, Directive,
        Document, Line, Match, Meta, Node, Set, Shot, Timeline, Track, TrackKey,
    };
    use lute_core_span::{Span, StableId};

    fn span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    /// A `@ref` slot carrying `raw` as its marker.
    fn slot(raw: &str) -> CelSlot {
        CelSlot::raw(CelKind::AttrValue, raw.to_string(), span())
    }

    /// An attribute whose value is a `@ref` slot marked `raw`.
    fn ref_attr(key: &str, raw: &str) -> Attr {
        Attr {
            key: key.to_string(),
            value: AttrValue::Ref(slot(raw)),
            value_span: span(),
            span: span(),
        }
    }

    /// A non-slot attribute (bare string) — must be skipped by the walk.
    fn str_attr(key: &str, val: &str) -> Attr {
        Attr {
            key: key.to_string(),
            value: AttrValue::Str(val.to_string()),
            value_span: span(),
            span: span(),
        }
    }

    fn set_node(path: &str, raw: &str) -> Node {
        Node::Set(Set {
            path: path.to_string(),
            path_span: span(),
            op: "=".to_string(),
            expr: slot(raw),
            span: span(),
        })
    }

    /// Build a single-shot document exercising every slot-bearing location, with
    /// each slot's `raw` set to "s0".."s17" in the exact expected pre-order.
    fn rich_doc() -> Document {
        let body = vec![
            // Line: a non-slot attr, then two @ref attrs -> s0, s1.
            Node::Line(Line {
                speaker: "narrator".to_string(),
                attrs: vec![
                    str_attr("style", "bold"),
                    ref_attr("mood", "s0"),
                    ref_attr("focus", "s1"),
                ],
                text: "hi".to_string(),
                text_span: span(),
                interps: Vec::new(),
                span: span(),
            }),
            // Directive: one @ref attr -> s2, plus a non-slot attr.
            Node::Directive(Directive {
                tag: "camera".to_string(),
                attrs: vec![ref_attr("cue", "s2"), str_attr("x", "y")],
                span: span(),
            }),
            // Top-level Set -> s3.
            set_node("scene.a", "s3"),
            // Branch: attrs ref s4; choice A (when s5, attr s6, body Set s7);
            //         choice B (no when, attr s8, body Line ref s9).
            Node::Branch(Branch {
                id: "b".to_string(),
                attrs: vec![ref_attr("flag", "s4")],
                choices: vec![
                    Choice {
                        id: "cA".to_string(),
                        label: "A".to_string(),
                        when: Some(slot("s5")),
                        attrs: vec![ref_attr("pick", "s6")],
                        body: vec![set_node("scene.b", "s7")],
                        span: span(),
                    },
                    Choice {
                        id: "cB".to_string(),
                        label: "B".to_string(),
                        when: None,
                        attrs: vec![ref_attr("pick", "s8")],
                        body: vec![Node::Line(Line {
                            speaker: "narrator".to_string(),
                            attrs: vec![ref_attr("mood", "s9")],
                            text: String::new(),
                            text_span: span(),
                            interps: Vec::new(),
                            span: span(),
                        })],
                        span: span(),
                    },
                ],
                span: span(),
            }),
            // Match: subject s10; When (test s11, body Set s12); Otherwise (body Directive ref s13).
            Node::Match(Match {
                subject: slot("s10"),
                arms: vec![
                    Arm::When {
                        is: None,
                        test: slot("s11"),
                        body: vec![set_node("scene.c", "s12")],
                        span: span(),
                    },
                    Arm::Otherwise {
                        body: vec![Node::Directive(Directive {
                            tag: "fx".to_string(),
                            attrs: vec![ref_attr("k", "s13")],
                            span: span(),
                        })],
                        span: span(),
                    },
                ],
                span: span(),
            }),
            // Timeline: duration s14; clip Directive attrs s15,s16; clip Set s17.
            Node::Timeline(Timeline {
                duration: Some(slot("s14")),
                tracks: vec![Track {
                    key: TrackKey::Channel("fg".to_string()),
                    clips: vec![
                        Clip {
                            node: ClipNode::Directive(Directive {
                                tag: "cut".to_string(),
                                attrs: vec![ref_attr("a", "s15"), ref_attr("b", "s16")],
                                span: span(),
                            }),
                            at: None,
                            span: span(),
                        },
                        Clip {
                            node: ClipNode::Set(Set {
                                path: "scene.e".to_string(),
                                path_span: span(),
                                op: "=".to_string(),
                                expr: slot("s17"),
                                span: span(),
                            }),
                            at: None,
                            span: span(),
                        },
                    ],
                    span: span(),
                }],
                span: span(),
            }),
        ];
        Document {
            meta: Meta {
                raw_yaml: String::new(),
                span: span(),
            },
            title: None,
            shots: vec![Shot {
                heading: "Shot 1.".to_string(),
                number: Some(1),
                body,
                span: span(),
            }],
            span: span(),
        }
    }

    /// The pre-order the walk MUST yield: s0..s17.
    fn expected() -> Vec<String> {
        (0..18).map(|i| format!("s{i}")).collect()
    }

    #[test]
    fn for_each_cel_slot_visits_canonical_preorder() {
        let doc = rich_doc();
        let mut seen = Vec::new();
        for_each_cel_slot(&doc, &mut |s| seen.push(s.raw.clone()));
        assert_eq!(seen, expected());
    }

    #[test]
    fn for_each_cel_slot_mut_visits_same_order_and_can_mutate() {
        let mut doc = rich_doc();
        let mut order = Vec::new();
        let mut n = 0u64;
        for_each_cel_slot_mut(&mut doc, &mut |s| {
            n += 1;
            s.id = StableId(n);
            order.push(s.raw.clone());
        });
        assert_eq!(order, expected(), "mut walk visits the same pre-order");
        assert_eq!(n, 18, "every slot is visited exactly once");

        // Re-walk immutably: ids were assigned 1..=18 in the pre-order.
        let mut ids = Vec::new();
        for_each_cel_slot(&doc, &mut |s| ids.push(s.id.0));
        assert_eq!(ids, (1..=18).collect::<Vec<u64>>());
    }
}
