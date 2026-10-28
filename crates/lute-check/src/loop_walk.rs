//! dsl 0.51.0 §3.4: what every flow walk shares at a `::label` and a
//! `::jump`. A walk joins the states of the jumps that reach a label into its
//! fall-through state, and at a loop head ([`crate::next_labels::LoopTable`])
//! it iterates the loop's region until the head state is stable
//! ([`head_fixpoint`]).
//!
//! **Probe passes.** A head is iterated by walking its region with
//! publication off: no diagnostic, verdict, recorded slot or emitted record,
//! and no update of the enclosing pending map. Each probe pass keeps a fresh
//! pending map of its own, so a jump inside the region carries its state to a
//! later label in it, an unguarded jump leaves no fall-through, and the back
//! edges' states arrive under the head's name; the map is dropped at the end
//! of the pass. The final pass walks the region once more from the stable
//! state and is the only one that reports.
//!
//! **Counted work.** Every walk counts its heads, probe passes and probe
//! nodes per [`WalkKind`] ([`LoopWalkStats`]), the deterministic counters the
//! `lute-bench flow-loops` cost gate reads, like 0.50's node-lint counters.

use std::cell::Cell;

use lute_manifest::core::{JUMP_DIRECTIVE, JUMP_MAX_ATTR, LABEL_DIRECTIVE, LABEL_NAME_ATTR};
use lute_syntax::ast::{Arm, AttrValue, CelSlot, Directive, Node, Section};

/// The work one walk kind spent at loop heads (dsl 0.51.0 §3.4) and, for
/// the stage walks, hub heads (dsl 0.52.0 §5.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoopWalkStats {
    /// Loop and hub heads iterated, nested heads once per enclosing probe
    /// pass.
    pub heads: u64,
    /// Probe passes over a region.
    pub probe_passes: u64,
    /// Nodes visited by probe passes, nested bodies included.
    pub probe_nodes: u64,
    /// The most probe passes any one head took.
    pub max_head_passes: u64,
    /// Heads that did not stabilize within the walk's bound and took its
    /// conservative top.
    pub widened: u64,
}

impl LoopWalkStats {
    /// The units of input the fixed point processed: the probe nodes.
    pub fn work(&self) -> u64 {
        self.probe_nodes
    }

    /// Add `other`'s counts (the maximum of the per-head maxima).
    pub fn add(&mut self, other: &LoopWalkStats) {
        self.heads += other.heads;
        self.probe_passes += other.probe_passes;
        self.probe_nodes += other.probe_nodes;
        self.max_head_passes = self.max_head_passes.max(other.max_head_passes);
        self.widened += other.widened;
    }
}

/// The flow walks that iterate loop heads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkKind {
    /// Relational facts, must/may ([`crate::fact_must`]).
    Facts,
    /// Definite assignment ([`crate::defassign`]).
    Assign,
    /// Stage state, the checker's fold.
    Stage,
    /// Stage state, the compiler's walk (through the checker's head state).
    StageCompile,
    /// Cast presence ([`crate::cast::check_presence`]): heads only, no probe.
    Presence,
}

impl WalkKind {
    /// Every kind, in report order.
    pub const ALL: [WalkKind; 5] = [
        WalkKind::Facts,
        WalkKind::Assign,
        WalkKind::Stage,
        WalkKind::StageCompile,
        WalkKind::Presence,
    ];

    /// The kind's name in reports.
    pub fn name(self) -> &'static str {
        match self {
            WalkKind::Facts => "facts",
            WalkKind::Assign => "assign",
            WalkKind::Stage => "stage",
            WalkKind::StageCompile => "stageCompile",
            WalkKind::Presence => "presence",
        }
    }

    fn index(self) -> usize {
        match self {
            WalkKind::Facts => 0,
            WalkKind::Assign => 1,
            WalkKind::Stage => 2,
            WalkKind::StageCompile => 3,
            WalkKind::Presence => 4,
        }
    }
}

thread_local! {
    /// This thread's loop work, per [`WalkKind`].
    static STATS: Cell<[LoopWalkStats; 5]> = const { Cell::new([LoopWalkStats {
        heads: 0,
        probe_passes: 0,
        probe_nodes: 0,
        max_head_passes: 0,
        widened: 0,
    }; 5]) };
}

/// The loop work this thread's `kind` walks did since the last
/// [`reset_loop_walk_stats`]: the cost gate reads it.
#[doc(hidden)]
pub fn loop_walk_stats(kind: WalkKind) -> LoopWalkStats {
    STATS.with(|s| s.get()[kind.index()])
}

/// Zero this thread's loop work counters.
#[doc(hidden)]
pub fn reset_loop_walk_stats() {
    STATS.with(|s| s.set([LoopWalkStats::default(); 5]));
}

fn count(kind: WalkKind, f: impl FnOnce(&mut LoopWalkStats)) {
    STATS.with(|s| {
        let mut all = s.get();
        f(&mut all[kind.index()]);
        s.set(all);
    });
}

/// One node visited by a probe pass of a `kind` walk.
pub(crate) fn probe_node(kind: WalkKind) {
    count(kind, |s| s.probe_nodes += 1);
}

/// One loop head a `kind` walk met (a walk without probe passes).
pub(crate) fn head_met(kind: WalkKind) {
    count(kind, |s| s.heads += 1);
}

/// How a `::jump` routes a flow walk's state (dsl 0.51.0 §3.4).
#[derive(Clone, Copy, Debug)]
pub(crate) enum JumpRoute<'d> {
    /// Unguarded, without `max`: the state goes to the label and nothing
    /// falls through.
    Away,
    /// Guarded, without `max`: a copy, its guard assumed, goes to the label;
    /// the state falls through.
    Guarded(&'d CelSlot),
    /// With `max` (and its `when`, if any): a back edge. Its copy reaches the
    /// head only in a probe pass of that head (the final pass starts from
    /// the stable head state); the state falls through, since an exhausted
    /// jump does.
    Bounded(Option<&'d CelSlot>),
}

impl<'d> JumpRoute<'d> {
    /// The route of the `::jump` `d`.
    pub(crate) fn of(d: &'d Directive) -> Self {
        if d.attrs.iter().any(|a| a.key == JUMP_MAX_ATTR) {
            JumpRoute::Bounded(d.when.as_ref())
        } else if let Some(when) = &d.when {
            JumpRoute::Guarded(when)
        } else {
            JumpRoute::Away
        }
    }
}

fn str_attr<'d>(d: &'d Directive, key: &str) -> Option<&'d str> {
    d.attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.as_str()),
            _ => None,
        })
}

/// The label `node` declares, when it is a `::label`.
pub(crate) fn label_of(node: &Node) -> Option<&str> {
    match node {
        Node::Directive(d) if d.tag == LABEL_DIRECTIVE => str_attr(d, LABEL_NAME_ATTR),
        _ => None,
    }
}

/// The label a `::jump` names.
pub(crate) fn jump_target(d: &Directive) -> Option<&str> {
    str_attr(d, "to")
}

/// The nodes a loop's region spans after its head label (dsl 0.53.0 §9.3):
/// the rest of the head's body and, when that body is a top-level section of
/// a document, whole following sections, through the **last** node holding a
/// bounded jump to the head. Walked in [`Region::parts`] order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Region<'d> {
    /// The head's body after the label, through the last back edge when it
    /// lies there.
    head: &'d [Node],
    /// The sections wholly inside the region, after the head's.
    later: &'d [Section],
    /// The prefix of the section holding the last back edge, through it
    /// (empty when that edge is in the head's body).
    last: &'d [Node],
}

impl<'d> Region<'d> {
    /// The region's node slices in document order.
    pub(crate) fn parts(self) -> impl Iterator<Item = &'d [Node]> {
        std::iter::once(self.head)
            .chain(self.later.iter().map(|s| s.body.as_slice()))
            .chain(std::iter::once(self.last))
    }
}

/// The region of the loop headed by `head`, whose label is followed in its
/// body by `rest`. `top` is the top-level sections of the scope the walk
/// walks — the document's, or inside a `::use` body the component's own
/// (else empty): when `rest` is the end of one of them, the sections after
/// it are one loop scope with it (dsl 0.53.0 §9.2) and the search runs
/// through **all** of them, so a back edge in the head's own section does
/// not stop it (several bounded jumps may loop to one head). Inside a nested
/// body no section follows. The region rules (dsl 0.51.0 §3.2) keep every
/// back edge in the head's scope; a caller inlining a component bounds
/// `rest` to the component's expansion (its labels are its own scope).
pub(crate) fn region<'d>(rest: &'d [Node], top: &'d [Section], head: &str) -> Region<'d> {
    let later = later_sections(rest, top);
    let back = |nodes: &[Node]| nodes.iter().rposition(|n| holds_back_edge(n, head));
    for (k, section) in later.iter().enumerate().rev() {
        if let Some(i) = back(&section.body) {
            return Region {
                head: rest,
                later: &later[..k],
                last: &section.body[..=i],
            };
        }
    }
    Region {
        head: &rest[..back(rest).map_or(0, |i| i + 1)],
        later: &[],
        last: &[],
    }
}

/// The sections of `top` after the one whose body `rest` ends (`rest` is a
/// suffix of every body it is taken from, so the section is the one ending
/// at the same address); empty when `rest` ends a nested body. The top-level
/// hand-off of dsl 0.53.0 §9.3 without threading the following sections
/// through every nested walk.
fn later_sections<'d>(rest: &[Node], top: &'d [Section]) -> &'d [Section] {
    let end = rest.as_ptr_range().end;
    top.iter()
        .position(|s| !s.body.is_empty() && std::ptr::eq(s.body.as_ptr_range().end, end))
        .map_or(&[], |i| &top[i + 1..])
}

fn holds_back_edge(node: &Node, head: &str) -> bool {
    match node {
        Node::Directive(d) => {
            d.tag == JUMP_DIRECTIVE
                && jump_target(d) == Some(head)
                && matches!(JumpRoute::of(d), JumpRoute::Bounded(_))
        }
        Node::Branch(b) => b
            .choices
            .iter()
            .any(|c| c.body.iter().any(|n| holds_back_edge(n, head))),
        Node::Match(m) => m.arms.iter().any(|arm| {
            let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
            body.iter().any(|n| holds_back_edge(n, head))
        }),
        Node::Hub(h) => h
            .bodies()
            .any(|b| b.iter().any(|n| holds_back_edge(n, head))),
        Node::On(o) => o.body.iter().any(|n| holds_back_edge(n, head)),
        Node::Objective(o) => o.body.iter().any(|n| holds_back_edge(n, head)),
        Node::Line(_) | Node::Set(_) | Node::Timeline(_) | Node::Assert(_) | Node::Retract(_) => {
            false
        }
    }
}

/// The operations of one walk's lattice that [`head_fixpoint`] needs.
pub(crate) trait HeadLattice<S> {
    /// The next head state from the entry, the current head `head` and the
    /// meet of the back edges a pass from `head` reached (`None`: none).
    /// A walk whose transfer is monotone joins `entry` with `back` (the
    /// same fixed point); one whose transfer is not accumulates from `head`
    /// so the iteration only descends (the stage walk, 0.52.0 §5.4).
    fn meet(&self, entry: &S, head: &S, back: Option<S>) -> S;
    /// Equal as lattice elements (the convergence test).
    fn same(&self, a: &S, b: &S) -> bool;
    /// The walk's conservative top, from the last state reached.
    fn top(&self, last: &S) -> S;
}

/// The stable state at a loop head (dsl 0.51.0 §3.4): starting from `entry`
/// (the fall-through state joined with the forward jumps to the head), each
/// probe pass walks the region from the current head state and returns the
/// meet of its back edges (`None`: no back edge was reached); the next head
/// state is [`HeadLattice::meet`] of the entry, the head and that. At most
/// `bound` passes (the walk's lattice height); a head still moving then
/// takes the walk's top.
pub(crate) fn head_fixpoint<S: Clone>(
    kind: WalkKind,
    entry: S,
    bound: usize,
    mut probe: impl FnMut(&S) -> Option<S>,
    lattice: &impl HeadLattice<S>,
) -> S {
    count(kind, |s| s.heads += 1);
    let mut head = entry.clone();
    let mut passes = 0u64;
    let mut stable = false;
    for _ in 0..bound.max(1) {
        passes += 1;
        count(kind, |s| s.probe_passes += 1);
        let back = probe(&head);
        let next = lattice.meet(&entry, &head, back);
        if lattice.same(&next, &head) {
            head = next;
            stable = true;
            break;
        }
        head = next;
    }
    count(kind, |s| s.max_head_passes = s.max_head_passes.max(passes));
    if stable {
        head
    } else {
        count(kind, |s| s.widened += 1);
        lattice.top(&head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Sets of integers under intersection; top: empty.
    struct Sets;

    impl HeadLattice<BTreeSet<u32>> for Sets {
        fn meet(
            &self,
            entry: &BTreeSet<u32>,
            _head: &BTreeSet<u32>,
            back: Option<BTreeSet<u32>>,
        ) -> BTreeSet<u32> {
            match back {
                Some(back) => entry.intersection(&back).copied().collect(),
                None => entry.clone(),
            }
        }
        fn same(&self, a: &BTreeSet<u32>, b: &BTreeSet<u32>) -> bool {
            a == b
        }
        fn top(&self, _: &BTreeSet<u32>) -> BTreeSet<u32> {
            BTreeSet::new()
        }
    }

    /// A chain that loses one element per pass stabilizes within the
    /// height bound, `|entry| + 1` passes.
    #[test]
    fn a_descending_chain_stabilizes_within_its_height() {
        reset_loop_walk_stats();
        let entry: BTreeSet<u32> = (0..5).collect();
        let head = head_fixpoint(
            WalkKind::Facts,
            entry.clone(),
            entry.len() + 1,
            |h| {
                let mut back = h.clone();
                let last = back.iter().next_back().copied();
                if let Some(last) = last {
                    back.remove(&last);
                }
                Some(back)
            },
            &Sets,
        );
        assert!(head.is_empty());
        let stats = loop_walk_stats(WalkKind::Facts);
        assert_eq!((stats.heads, stats.probe_passes, stats.widened), (1, 6, 0));
        assert_eq!(stats.max_head_passes, 6);
    }

    /// A head that never stabilizes (an oscillating back edge) takes the
    /// top once the bound is used.
    #[test]
    fn a_head_that_never_stabilizes_takes_the_top() {
        reset_loop_walk_stats();
        let entry: BTreeSet<u32> = (0..3).collect();
        let head = head_fixpoint(
            WalkKind::Stage,
            entry.clone(),
            entry.len() + 1,
            |h| {
                let mut back: BTreeSet<u32> = (0..3).collect();
                if h.contains(&0) {
                    back.remove(&0);
                }
                Some(back)
            },
            &Sets,
        );
        assert!(head.is_empty());
        let stats = loop_walk_stats(WalkKind::Stage);
        assert_eq!((stats.probe_passes, stats.widened), (4, 1));
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;

    fn sections(body: &str) -> Vec<Section> {
        let src = format!("---\nkind: scene\nid: r\n---\n## One\n\n{body}\n");
        lute_syntax::parse(&src).0.sections
    }

    /// The region of `top` in `doc`, as `(section, nodes)` per part.
    fn parts(doc: &[Section]) -> Vec<usize> {
        let at = doc[0]
            .body
            .iter()
            .position(|n| label_of(n) == Some("top"))
            .expect("a `top` label in the first section");
        region(&doc[0].body[at + 1..], doc, "top")
            .parts()
            .map(<[Node]>::len)
            .collect()
    }

    const JUMP: &str = "::jump{to=\"top\" max=\"2\"}";

    #[test]
    fn one_section_ends_at_the_back_edge() {
        let doc = sections(&format!(
            "::label{{name=\"top\"}}\n@a: x\n{JUMP}\n@a: after\n"
        ));
        assert_eq!(parts(&doc), [2, 0]);
    }

    #[test]
    fn two_sections_run_through_the_later_back_edge() {
        let doc = sections(&format!(
            "::label{{name=\"top\"}}\n@a: x\n\n## Two\n\n@a: y\n{JUMP}\n@a: after\n"
        ));
        assert_eq!(parts(&doc), [1, 2]);
    }

    #[test]
    fn a_back_edge_in_a_choice_of_a_later_section_counts() {
        let doc = sections(&format!(
            "::label{{name=\"top\"}}\n@a: x\n\n## Two\n\n@a: y\n\n## Three\n\n\
             <branch id=\"b\">\n<choice id=\"c\" text=\"C\">\n{JUMP}\n</choice>\n</branch>\n\
             @a: after\n"
        ));
        assert_eq!(parts(&doc), [1, 1, 1]);
    }

    /// Two back edges to one head: the region runs through the later one.
    #[test]
    fn two_back_edges_run_through_the_last() {
        let doc = sections(&format!(
            "::label{{name=\"top\"}}\n@a: x\n{JUMP}\n@a: y\n\n## Two\n\n@a: z\n{JUMP}\n\n## Three\n\n@a: end\n"
        ));
        assert_eq!(parts(&doc), [3, 2]);
    }

    /// A head inside a nested body has no following section.
    #[test]
    fn a_nested_head_stays_in_its_body() {
        let doc = sections(&format!(
            "<branch id=\"b\">\n<choice id=\"c\" text=\"C\">\n::label{{name=\"top\"}}\n@a: x\n{JUMP}\n\
             </choice>\n</branch>\n\n## Two\n\n{JUMP}\n"
        ));
        let Node::Branch(b) = &doc[0].body[0] else {
            panic!("a branch first")
        };
        let body = &b.choices[0].body;
        let lens: Vec<usize> = region(&body[1..], &doc, "top")
            .parts()
            .map(<[Node]>::len)
            .collect();
        assert_eq!(lens, [2, 0]);
    }
}
