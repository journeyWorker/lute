//! Walk a parsed [`Document`] and fill every [`CelSlot`]'s `ast`/`id`.
//!
//! [`fill_document`] is the bridge from the DSL parse (`lute-syntax`) to the CEL
//! parse ([`parse_slot`]): it visits **every** `CelSlot` in a deterministic
//! pre-order, parses its `raw` fragment, and records the result *in place* —
//! `slot.ast = Some(handle)` on success, or the [`CelParseError`] pushed onto the
//! returned `Vec` on failure (leaving `slot.ast = None`). A single malformed CEL
//! fragment NEVER aborts the walk ("CelSlot isolation"): the checker (T4.x) wants
//! every valid slot filled and every bad slot reported, regardless of neighbors.
//!
//! Every visited slot also gets a monotonic [`StableId`] (1, 2, 3, … in pre-order)
//! so downstream passes can address slots stably and uniquely. `StableId(0)` is
//! the unparsed default (see [`CelSlot::raw`]); an id `> 0` therefore means "this
//! slot was visited by `fill_document`".
//!
//! ## Slot enumeration (exhaustive — a missed location is silently unchecked CEL)
//! - [`Set::expr`] — both a top-level [`Node::Set`] and a [`ClipNode::Set`] inside
//!   a timeline track clip.
//! - [`Match::subject`] and each [`Arm::When`]'s `test`.
//! - [`Choice::when`] (optional).
//! - [`Timeline::duration`] (optional).
//! - [`AttrValue::Ref`] inside the `attrs` of every attr-bearing node:
//!   [`Line`], [`Directive`] (incl. clip directives), [`Branch`], [`Choice`].
//! - Recursion into every nested body: [`Shot::body`], [`Choice::body`],
//!   [`Arm::When`]/[`Arm::Otherwise`] bodies, and [`Track::clips`].
//!
//! An empty (or whitespace-only) `raw` is a *structural* gap (e.g. `<match>` with
//! no `on=`), not a CEL fragment: it still receives a `StableId` but is not handed
//! to `parse_slot`, so it produces no spurious "invalid CEL" diagnostic — the
//! structural checker owns that error.

use crate::{parse_slot, CelArena, CelParseError};
use lute_core_span::StableId;
use lute_syntax::ast::{
    Arm, AttrValue, Branch, ClipNode, Document, Match, Node, Timeline,
};

/// Parse every [`CelSlot`] in `doc`, filling `slot.ast`/`slot.id`.
///
/// Returns one [`CelParseError`] per slot that failed to parse. Never aborts on a
/// parse failure and never panics.
pub fn fill_document(arena: &mut CelArena, doc: &mut Document) -> Vec<CelParseError> {
    let mut w = Walk { arena, next_id: 1, errors: Vec::new() };
    for shot in &mut doc.shots {
        w.body(&mut shot.body);
    }
    w.errors
}

/// Pre-order slot visitor carrying the arena, the id counter, and collected errors.
struct Walk<'a> {
    arena: &'a mut CelArena,
    next_id: u64,
    errors: Vec<CelParseError>,
}

impl Walk<'_> {
    /// Fill one slot: assign its `StableId`, then (for a non-empty fragment) parse.
    fn slot(&mut self, slot: &mut lute_syntax::ast::CelSlot) {
        slot.id = StableId(self.next_id);
        self.next_id += 1;
        if slot.raw.trim().is_empty() {
            return; // structural gap, not a CEL fragment — leave ast = None.
        }
        match parse_slot(self.arena, &slot.raw, slot.span.byte_start) {
            Ok(handle) => slot.ast = Some(handle),
            Err(e) => self.errors.push(e),
        }
    }

    /// Fill every `AttrValue::Ref` slot in an attribute list, in order.
    fn attrs(&mut self, attrs: &mut [lute_syntax::ast::Attr]) {
        for attr in attrs {
            if let AttrValue::Ref(slot) = &mut attr.value {
                self.slot(slot);
            }
        }
    }

    /// Walk a sequence of body nodes in source order.
    fn body(&mut self, nodes: &mut [Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    /// Dispatch one node to its slot-bearing children.
    fn node(&mut self, node: &mut Node) {
        match node {
            Node::Line(l) => self.attrs(&mut l.attrs),
            Node::Directive(d) => self.attrs(&mut d.attrs),
            Node::Set(s) => self.slot(&mut s.expr),
            Node::Branch(b) => self.branch(b),
            Node::Match(m) => self.match_node(m),
            Node::Timeline(t) => self.timeline(t),
        }
    }

    fn branch(&mut self, b: &mut Branch) {
        self.attrs(&mut b.attrs);
        for choice in &mut b.choices {
            if let Some(when) = &mut choice.when {
                self.slot(when);
            }
            self.attrs(&mut choice.attrs);
            self.body(&mut choice.body);
        }
    }

    fn match_node(&mut self, m: &mut Match) {
        self.slot(&mut m.subject);
        for arm in &mut m.arms {
            match arm {
                Arm::When { test, body, .. } => {
                    self.slot(test);
                    self.body(body);
                }
                Arm::Otherwise { body, .. } => self.body(body),
            }
        }
    }

    fn timeline(&mut self, t: &mut Timeline) {
        if let Some(dur) = &mut t.duration {
            self.slot(dur);
        }
        for track in &mut t.tracks {
            for clip in &mut track.clips {
                match &mut clip.node {
                    ClipNode::Directive(d) => self.attrs(&mut d.attrs),
                    ClipNode::Set(s) => self.slot(&mut s.expr),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::ast::{Arm, ClipNode, Node};
    use lute_syntax::parse;
    use std::collections::HashSet;

    /// Independent, exhaustive collector of every `&CelSlot` in a document,
    /// written separately from the `fill_document` walk so a shared blind spot in
    /// one is caught by the other's hand-counted expectation.
    fn collect_slots(doc: &lute_syntax::ast::Document) -> Vec<&lute_syntax::ast::CelSlot> {
        fn attrs<'a>(out: &mut Vec<&'a lute_syntax::ast::CelSlot>, attrs: &'a [lute_syntax::ast::Attr]) {
            for a in attrs {
                if let AttrValue::Ref(s) = &a.value {
                    out.push(s);
                }
            }
        }
        fn body<'a>(out: &mut Vec<&'a lute_syntax::ast::CelSlot>, nodes: &'a [Node]) {
            for n in nodes {
                match n {
                    Node::Line(l) => attrs(out, &l.attrs),
                    Node::Directive(d) => attrs(out, &d.attrs),
                    Node::Set(s) => out.push(&s.expr),
                    Node::Branch(b) => {
                        attrs(out, &b.attrs);
                        for c in &b.choices {
                            if let Some(w) = &c.when {
                                out.push(w);
                            }
                            attrs(out, &c.attrs);
                            body(out, &c.body);
                        }
                    }
                    Node::Match(m) => {
                        out.push(&m.subject);
                        for arm in &m.arms {
                            match arm {
                                Arm::When { test, body: b, .. } => {
                                    out.push(test);
                                    body(out, b);
                                }
                                Arm::Otherwise { body: b, .. } => body(out, b),
                            }
                        }
                    }
                    Node::Timeline(t) => {
                        if let Some(d) = &t.duration {
                            out.push(d);
                        }
                        for track in &t.tracks {
                            for clip in &track.clips {
                                match &clip.node {
                                    ClipNode::Directive(d) => attrs(out, &d.attrs),
                                    ClipNode::Set(s) => out.push(&s.expr),
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        for shot in &doc.shots {
            body(&mut out, &shot.body);
        }
        out
    }

    #[test]
    fn fills_valid_cel_slots_and_reports_invalid() {
        let text = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.a\">\n<when test=\"1 +\">\n:line[narrator]: hi\n</when>\n<otherwise>\n:line[narrator]: bye\n</otherwise>\n</match>\n";
        let (mut doc, _) = parse(text);
        let mut arena = CelArena::default();
        let errs = fill_document(&mut arena, &mut doc);
        assert_eq!(errs.len(), 1); // the "1 +" test slot fails; "scene.a" subject parses
    }

    /// Exercises a slot in EVERY location and asserts the full walk visited each:
    /// unique non-default ids, correct count, and valid slots filled.
    #[test]
    fn walks_every_slot_location() {
        let text = concat!(
            "---\ncharacter: x\n---\n",
            "## Shot 1.\n",
            ":line[narrator]{mood=@joy}: hi\n",          // Line.attrs Ref
            "::camera{focus=@fond}\n",                    // Directive.attrs Ref
            "::set{scene.a = 1}\n",                       // top-level Set.expr
            "<branch id=\"b\" flag=@joy>\n",              // Branch.attrs Ref
            "<choice id=\"c\" label=\"l\" when=\"scene.a > 0\" pick=@joy>\n", // Choice.when + Choice.attrs Ref
            "::set{scene.b = 2}\n",                       // Set inside choice body
            "</choice>\n",
            "</branch>\n",
            "<match on=\"scene.a\">\n",                   // Match.subject
            "<when test=\"scene.a == 1\">\n",             // Arm::When test
            "::set{scene.c = 3}\n",                       // Set inside when body
            "</when>\n",
            "<otherwise>\n",
            "::set{scene.d = 4}\n",                       // Set inside otherwise body
            "</otherwise>\n",
            "</match>\n",
            "<timeline duration=\"1.4\">\n",              // Timeline.duration
            "<track channel=\"fg\">\n",
            "::cut{assetId=\"x\" cue=@joy}\n",            // clip Directive.attrs Ref
            "::set{scene.e = 5}\n",                       // clip Set.expr (ClipNode::Set)
            "</track>\n",
            "</timeline>\n",
        );
        let (mut doc, _) = parse(text);
        let mut arena = CelArena::default();
        let errs = fill_document(&mut arena, &mut doc);
        assert!(errs.is_empty(), "all fragments are valid CEL: {errs:?}");

        const EXPECTED: usize = 14;
        let slots = collect_slots(&doc);
        assert_eq!(slots.len(), EXPECTED, "hand-counted slot locations must all be present");

        // (a) every slot got a non-default, unique StableId (proves fill visited it).
        let ids: HashSet<u64> = slots.iter().map(|s| s.id.0).collect();
        assert!(slots.iter().all(|s| s.id.0 != 0), "every slot must be assigned a StableId");
        assert_eq!(ids.len(), EXPECTED, "StableIds must be unique");
        // (b) every valid slot has its AST filled.
        assert!(slots.iter().all(|s| s.ast.is_some()), "valid slots must be filled");
    }

    #[test]
    fn bianca_example_fills_all_slots_without_errors() {
        let text = include_str!("../../../docs/examples/bianca-s01ep02.lute");
        let (mut doc, _) = parse(text);
        let mut arena = CelArena::default();
        let errs = fill_document(&mut arena, &mut doc);
        assert!(errs.is_empty(), "bianca CEL is all valid: {errs:?}");

        let slots = collect_slots(&doc);
        let filled = slots.iter().filter(|s| s.ast.is_some()).count();
        assert!(filled >= 3, "expected >= 3 filled slots, got {filled}");
        assert!(slots.iter().all(|s| s.id.0 != 0), "every slot must be assigned a StableId");
    }
}
