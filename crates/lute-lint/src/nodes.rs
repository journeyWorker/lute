//! Node rows (dsl 0.50.0 §8.1, target `node`).
//!
//! One row per body node, in document order: section bodies; each top-level
//! `<quest>`, `<entry>` and `<beat>` declaration and its body; the children
//! of `<branch>`, `<hub>` (its choices, then its `<return>`), `<match>` (its
//! arms), `<choice>`, `<return>`, `<when>`/`<otherwise>`, `<objective>` and
//! `<on>`; and the `<reward>`s of a quest or objective at their source
//! position among the owner's children. A `<timeline>` is one row.
//!
//! **Authored attributes.** `node.attrs` is read from the node's head in the
//! source with [`lute_syntax::authored_attrs`], the parser's own scanner: a
//! key is present iff it was written, with its value text as written, so a
//! lifted attribute (`<reward amount="+005">`) keeps its text.
//!
//! **Cost.** The walk keeps one mutable map `tag → node` and an undo log:
//! visiting a keyed node records the entry it replaces, and leaving a body
//! restores every entry recorded in it. A row's `node.preceding` is a
//! borrowed view of that map ([`Preceding`]); a rule is evaluated while the
//! row is visited, so the view is never stored or copied. `size(node.preceding)`
//! is answered from the map; the map becomes a [`Value`] only when a rule
//! reads it whole otherwise ([`Record::materialize`]: rendering or comparing
//! it), which copies every entry for that row and is counted
//! ([`NodeWalkStats::preceding_copied`]). The sibling summaries of the body
//! being walked and of its ancestors' bodies are the only node data held at
//! once.
//!
//! The walk runs after the metric tables, because a row binds `scene`, the
//! finished document row, and only when a `node` or `pair` rule is active.
//! A [`NodeVisitor`] also sees where each body and each section or
//! top-level declaration begins and ends: a `pair` rule keeps its open
//! slots in those frames (dsl 0.51.0 §6.2).

use std::cell::Cell;
use std::collections::BTreeMap;

use lute_core_span::Span;
use lute_syntax::ast::{Arm, BundleBeat, Choice, Document, Entry, HubReturn, Node, Quest, Reward};

use crate::eval::{Record, Ref, Value};

/// Work counters of [`walk_nodes`], for the cost acceptance (dsl 0.50.0
/// §8.1, 0.51.0 §6.2): every count is per document walked and adds up
/// across documents. [`NodeWalkStats::work`] sums the units of processed
/// input — rows, head bytes scanned, attributes read, modeled ordered-map
/// steps, entries copied, characters the evaluator scanned and `pair` slot
/// operations — so a cost that grows with the scope's size shows in it.
/// The walk counts rows, attributes, head bytes and the `preceding`
/// counters; the rule evaluator counts [`NodeWalkStats::eval_chars`] and
/// the `pair` counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeWalkStats {
    /// Rows visited.
    pub rows: u64,
    /// Authored attributes read from the source.
    pub attrs: u64,
    /// Source bytes of the head lines the attribute scan reads.
    pub head_bytes: u64,
    /// `preceding` entries set (one per visited node that is not a line).
    pub preceding_writes: u64,
    /// `preceding` entries restored when a body or declaration is left.
    pub preceding_restores: u64,
    /// `preceding` lookups by tag (`"t" in node.preceding`,
    /// `node.preceding["t"]`).
    pub preceding_lookups: u64,
    /// Modeled ordered-map steps of every `preceding` write, restore,
    /// lookup and size read: ⌈log₂(entries + 1)⌉ each, the depth of a
    /// balanced search over the entries. A model of the lookup cost, not
    /// the comparisons `BTreeMap` performs (which are not observable).
    pub preceding_steps: u64,
    /// Whole `preceding` maps built for a rule that read the map whole
    /// (rendering or comparing it): zero unless a rule does.
    pub preceding_materialized: u64,
    /// Values copied by those builds: one per entry and one per attribute
    /// of each entry.
    pub preceding_copied: u64,
    /// Characters the evaluator scanned in string operations of `node` and
    /// `pair` predicates ([`crate::eval::Env::chars_scanned`]).
    pub eval_chars: u64,
    /// `pair` predicate invocations (`open`, `close`, `inside`): an event
    /// count, not a cost — the cost of each is in the other counters.
    pub pair_evals: u64,
    /// `pair` slot operations: opens, closes and slot frames pushed (one
    /// per body for a `body`-scope rule, one per section or top-level
    /// declaration for a `section`-scope rule).
    pub pair_slots: u64,
}

impl NodeWalkStats {
    /// The units of input the walk processed: rows, head bytes scanned,
    /// attributes read, modeled map steps, copied values, scanned
    /// characters and `pair` slot operations. The event counts (writes,
    /// restores, lookups, maps built, `pair` invocations) are not added
    /// again: their cost is in the others.
    pub fn work(&self) -> u64 {
        self.rows
            + self.head_bytes
            + self.attrs
            + self.preceding_steps
            + self.preceding_copied
            + self.eval_chars
            + self.pair_slots
    }

    /// Add `other`'s counts.
    pub fn add(&mut self, other: &NodeWalkStats) {
        self.rows += other.rows;
        self.attrs += other.attrs;
        self.head_bytes += other.head_bytes;
        self.preceding_writes += other.preceding_writes;
        self.preceding_restores += other.preceding_restores;
        self.preceding_lookups += other.preceding_lookups;
        self.preceding_steps += other.preceding_steps;
        self.preceding_materialized += other.preceding_materialized;
        self.preceding_copied += other.preceding_copied;
        self.eval_chars += other.eval_chars;
        self.pair_evals += other.pair_evals;
        self.pair_slots += other.pair_slots;
    }
}

/// What [`walk_nodes_with`] reports: every row, and where each body and
/// each section or top-level declaration (the unit that resets
/// `node.preceding`) begins and ends. A body's rows are its direct
/// children; a nested body begins after the row that owns it and ends
/// before that row's next sibling. A section or declaration scope holds
/// the bodies inside it.
pub trait NodeVisitor {
    /// One row, valid during the call.
    fn row(&mut self, row: &NodeRow<'_>);
    /// A body begins.
    fn body_start(&mut self) {}
    /// The body begun last ends.
    fn body_end(&mut self) {}
    /// A section or top-level declaration begins.
    fn scope_start(&mut self) {}
    /// The section or declaration begun last ends.
    fn scope_end(&mut self) {}
}

/// A [`NodeVisitor`] that sees rows only.
struct Rows<'v>(&'v mut dyn FnMut(&NodeRow<'_>));

impl NodeVisitor for Rows<'_> {
    fn row(&mut self, row: &NodeRow<'_>) {
        (self.0)(row);
    }
}

/// The modeled step count of one operation on an ordered map of `entries`
/// (see [`NodeWalkStats::preceding_steps`]).
fn steps(entries: usize) -> u64 {
    u64::from(usize::BITS - (entries + 1).leading_zeros())
}

/// One row, valid while [`walk_nodes`] visits it. It is a [`Record`]: the
/// evaluator reads its fields by reference.
#[derive(Debug)]
pub struct NodeRow<'w> {
    summary: &'w NodeSummary,
    prev: &'w NodeSummary,
    next: &'w NodeSummary,
    parent: &'w NodeSummary,
    index: usize,
    last: bool,
    depth: usize,
    section: &'w Value,
    preceding: Preceding<'w>,
    span: Span,
}

impl NodeRow<'_> {
    /// The node's span: where a finding on this row is anchored.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// The fields of a row, in the order a whole-row read lists them.
const ROW_FIELDS: [&str; 13] = [
    "kind",
    "tag",
    "speaker",
    "attrs",
    "index",
    "first",
    "last",
    "depth",
    "prev",
    "next",
    "parent",
    "preceding",
    "section",
];

impl Record for NodeRow<'_> {
    fn field(&self, key: &str) -> Option<Ref<'_>> {
        Some(match key {
            "kind" => Ref::Borrowed(&self.summary.kind),
            "tag" => Ref::Borrowed(&self.summary.tag),
            "speaker" => Ref::Borrowed(&self.summary.speaker),
            "attrs" => Ref::Borrowed(&self.summary.scoped.attrs),
            "index" => Ref::Owned(count(self.index)),
            "first" => Ref::Owned(Value::Bool(self.index == 0)),
            "last" => Ref::Owned(Value::Bool(self.last)),
            "depth" => Ref::Owned(count(self.depth)),
            "prev" => Ref::Record(self.prev),
            "next" => Ref::Record(self.next),
            "parent" => Ref::Record(self.parent),
            "preceding" => Ref::Record(&self.preceding),
            "section" => Ref::Borrowed(self.section),
            _ => return None,
        })
    }

    fn len(&self) -> usize {
        ROW_FIELDS.len()
    }

    fn materialize(&self) -> Value {
        Value::Map(
            ROW_FIELDS
                .iter()
                .filter_map(|key| Some(((*key).to_string(), ref_value(self.field(key)?))))
                .collect(),
        )
    }
}

/// `node.preceding`: a borrowed view of the walk's scope map at one row.
#[derive(Debug)]
pub struct Preceding<'w> {
    map: &'w BTreeMap<&'w str, usize>,
    arena: &'w [NodeSummary],
    counters: &'w Counters,
}

impl Preceding<'_> {
    fn count_steps(&self) {
        let c = &self.counters.steps;
        c.set(c.get() + steps(self.map.len()));
    }
}

impl Record for Preceding<'_> {
    /// `key` is a tag.
    fn field(&self, key: &str) -> Option<Ref<'_>> {
        self.counters.lookups.set(self.counters.lookups.get() + 1);
        self.count_steps();
        self.map
            .get(key)
            .map(|&i| Ref::Record(&self.arena[i].scoped as &dyn Record))
    }

    /// The number of tags, read from the map without copying it.
    fn len(&self) -> usize {
        self.count_steps();
        self.map.len()
    }

    fn materialize(&self) -> Value {
        let c = &self.counters;
        c.materialized.set(c.materialized.get() + 1);
        let mut copied = 0u64;
        let map = self
            .map
            .iter()
            .map(|(tag, &i)| {
                let attrs = &self.arena[i].scoped.attrs;
                copied += 1 + match attrs {
                    Value::Map(m) => m.len() as u64,
                    _ => 0,
                };
                ((*tag).to_string(), self.arena[i].scoped.materialize())
            })
            .collect();
        c.copied.set(c.copied.get() + copied);
        Value::Map(map)
    }
}

/// What a row shows of a node, built once when its body is entered.
#[derive(Debug)]
struct NodeSummary {
    kind: Value,
    tag: Value,
    speaker: Value,
    scoped: Scoped,
}

/// The `{ attrs }` of a `preceding` entry.
#[derive(Debug)]
struct Scoped {
    attrs: Value,
}

impl NodeSummary {
    fn new(kind: &str, tag: &str, speaker: &str, attrs: BTreeMap<String, Value>) -> Self {
        Self {
            kind: Value::Str(kind.to_string()),
            tag: Value::Str(tag.to_string()),
            speaker: Value::Str(speaker.to_string()),
            scoped: Scoped {
                attrs: Value::Map(attrs),
            },
        }
    }

    /// The summary of no node: a body edge, a section or the document.
    fn placeholder(kind: &str) -> Self {
        Self::new(kind, "", "", BTreeMap::new())
    }

    /// An arena slot whose node is not summarized (yet, or any more): no
    /// row reads it.
    fn vacant() -> Self {
        Self {
            kind: Value::Null,
            tag: Value::Null,
            speaker: Value::Null,
            scoped: Scoped { attrs: Value::Null },
        }
    }
}

/// `node.prev`, `node.next` and `node.parent`: `{ kind, tag, attrs }`.
impl Record for NodeSummary {
    fn field(&self, key: &str) -> Option<Ref<'_>> {
        match key {
            "kind" => Some(Ref::Borrowed(&self.kind)),
            "tag" => Some(Ref::Borrowed(&self.tag)),
            "attrs" => Some(Ref::Borrowed(&self.scoped.attrs)),
            _ => None,
        }
    }

    fn len(&self) -> usize {
        3
    }

    fn materialize(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("kind".to_string(), self.kind.clone()),
            ("tag".to_string(), self.tag.clone()),
            ("attrs".to_string(), self.scoped.attrs.clone()),
        ]))
    }
}

impl Record for Scoped {
    fn field(&self, key: &str) -> Option<Ref<'_>> {
        (key == "attrs").then_some(Ref::Borrowed(&self.attrs))
    }

    fn len(&self) -> usize {
        1
    }

    fn materialize(&self) -> Value {
        Value::Map(BTreeMap::from([("attrs".to_string(), self.attrs.clone())]))
    }
}

/// Counters a [`Preceding`] view bumps while a rule reads it.
#[derive(Debug, Default)]
struct Counters {
    lookups: Cell<u64>,
    steps: Cell<u64>,
    materialized: Cell<u64>,
    copied: Cell<u64>,
}

fn count(n: usize) -> Value {
    Value::Int(i64::try_from(n).expect("a count over the source is at most isize::MAX"))
}

fn ref_value(r: Ref<'_>) -> Value {
    match r {
        Ref::Borrowed(v) => v.clone(),
        Ref::Owned(v) => v,
        Ref::Record(r) => r.materialize(),
    }
}

/// A child in a body: a node, or one of the elements the AST keeps outside
/// [`Node`].
#[derive(Clone, Copy)]
enum Child<'d> {
    Node(&'d Node),
    Choice(&'d Choice),
    Return(&'d HubReturn),
    Arm(&'d Arm),
    Reward(&'d Reward),
    Quest(&'d Quest),
    Entry(&'d Entry),
    Beat(&'d BundleBeat),
}

impl<'d> Child<'d> {
    /// `node.kind` and `node.tag`: a line's tag is `""`, a directive's its
    /// tag, and an element's kind and tag are both its name.
    fn identity(self) -> (&'static str, &'d str) {
        let element = |name: &'static str| (name, name);
        match self {
            Child::Node(Node::Line(_)) => ("line", ""),
            Child::Node(Node::Directive(d)) => ("directive", d.tag.as_str()),
            Child::Node(Node::Set(_)) => ("directive", "set"),
            Child::Node(Node::Assert(_)) => ("directive", "assert"),
            Child::Node(Node::Retract(_)) => ("directive", "retract"),
            Child::Node(Node::Branch(_)) => element("branch"),
            Child::Node(Node::Hub(_)) => element("hub"),
            Child::Node(Node::Match(_)) => element("match"),
            Child::Node(Node::Timeline(_)) => element("timeline"),
            Child::Node(Node::Objective(_)) => element("objective"),
            Child::Node(Node::On(_)) => element("on"),
            Child::Choice(_) => element("choice"),
            Child::Return(_) => element("return"),
            Child::Arm(Arm::When { .. }) => element("when"),
            Child::Arm(Arm::Otherwise { .. }) => element("otherwise"),
            Child::Reward(_) => element("reward"),
            Child::Quest(_) => element("quest"),
            Child::Entry(_) => element("entry"),
            Child::Beat(_) => element("beat"),
        }
    }

    /// `node.speaker`: a line's speaker, `""` otherwise.
    fn speaker(self) -> &'d str {
        match self {
            Child::Node(Node::Line(l)) => &l.speaker,
            _ => "",
        }
    }

    fn span(self) -> Span {
        match self {
            Child::Node(n) => match n {
                Node::Line(l) => l.span,
                Node::Directive(d) => d.span,
                Node::Set(s) => s.span,
                Node::Branch(b) => b.span,
                Node::Match(m) => m.span,
                Node::Timeline(t) => t.span,
                Node::Hub(h) => h.span,
                Node::Objective(o) => o.span,
                Node::On(o) => o.span,
                Node::Assert(a) => a.span,
                Node::Retract(r) => r.span,
            },
            Child::Choice(c) => c.span,
            Child::Return(r) => r.span,
            Child::Arm(Arm::When { span, .. } | Arm::Otherwise { span, .. }) => *span,
            Child::Reward(r) => r.span,
            Child::Quest(q) => q.span,
            Child::Entry(e) => e.span,
            Child::Beat(b) => b.span,
        }
    }

    /// Whether the head carries an attribute list `node.attrs` reads.
    /// `::set`, `::assert` and `::retract` write a statement, not
    /// attributes, so theirs is `{}`.
    fn has_attrs(self) -> bool {
        !matches!(
            self,
            Child::Node(Node::Set(_) | Node::Assert(_) | Node::Retract(_))
        )
    }

    /// The node's own body, in source order.
    fn children(self) -> Vec<Child<'d>> {
        let nodes = |body: &'d [Node]| body.iter().map(Child::Node).collect::<Vec<_>>();
        match self {
            Child::Node(n) => match n {
                Node::Branch(b) => b.choices.iter().map(Child::Choice).collect(),
                Node::Hub(h) => h
                    .choices
                    .iter()
                    .map(Child::Choice)
                    .chain(h.on_return.iter().map(Child::Return))
                    .collect(),
                Node::Match(m) => m.arms.iter().map(Child::Arm).collect(),
                Node::Objective(o) => with_rewards(nodes(&o.body), &o.rewards),
                Node::On(o) => nodes(&o.body),
                Node::Line(_)
                | Node::Directive(_)
                | Node::Set(_)
                | Node::Timeline(_)
                | Node::Assert(_)
                | Node::Retract(_) => Vec::new(),
            },
            Child::Choice(c) => nodes(&c.body),
            Child::Return(r) => nodes(&r.body),
            Child::Arm(Arm::When { body, .. } | Arm::Otherwise { body, .. }) => nodes(body),
            Child::Reward(_) => Vec::new(),
            Child::Quest(q) => with_rewards(nodes(&q.body), &q.rewards),
            Child::Entry(e) => nodes(&e.body),
            Child::Beat(b) => nodes(&b.body),
        }
    }
}

/// `body` with `rewards` placed at their source position.
fn with_rewards<'d>(mut body: Vec<Child<'d>>, rewards: &'d [Reward]) -> Vec<Child<'d>> {
    if !rewards.is_empty() {
        body.extend(rewards.iter().map(Child::Reward));
        body.sort_by_key(|c| c.span().byte_start);
    }
    body
}

/// Where a body sits.
#[derive(Clone, Copy)]
struct Scope<'s> {
    /// Arena index of the enclosing element; `None` at a top level.
    parent: Option<usize>,
    /// The top level a `parent: None` body is: a section's or the document's.
    top: Top,
    depth: usize,
    section: &'s Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Top {
    Section,
    Document,
}

struct Walk<'d> {
    source: &'d str,
    /// Summaries of the bodies being walked: the current body's siblings
    /// after those of each enclosing body.
    arena: Vec<NodeSummary>,
    /// `tag → arena index` of the nearest node with that tag on the path.
    map: BTreeMap<&'d str, usize>,
    /// The entry each write replaced, since the walk began.
    undo: Vec<(&'d str, Option<usize>)>,
    edge: NodeSummary,
    section_parent: NodeSummary,
    document_parent: NodeSummary,
    counters: Counters,
    stats: NodeWalkStats,
}

/// Visit every node row of `doc` in document order. `source` is the text
/// `doc` was parsed from (`node.attrs` is read from it). `visit` sees each
/// row while its `preceding` view is valid.
pub fn walk_nodes(
    doc: &Document,
    source: &str,
    visit: &mut dyn FnMut(&NodeRow<'_>),
) -> NodeWalkStats {
    walk_nodes_with(doc, source, &mut Rows(visit))
}

/// [`walk_nodes`], reporting body and scope frames to `visitor` as well.
pub fn walk_nodes_with(
    doc: &Document,
    source: &str,
    visitor: &mut dyn NodeVisitor,
) -> NodeWalkStats {
    let mut walk = Walk {
        source,
        arena: Vec::new(),
        map: BTreeMap::new(),
        undo: Vec::new(),
        edge: NodeSummary::placeholder(""),
        section_parent: NodeSummary::placeholder("section"),
        document_parent: NodeSummary::placeholder("document"),
        counters: Counters::default(),
        stats: NodeWalkStats::default(),
    };
    for (i, section) in doc.sections.iter().enumerate() {
        let info = section_value(
            i + 1,
            &section.heading,
            section.id.as_ref().map_or("", |(id, _)| id),
        );
        let kids: Vec<Child<'_>> = section.body.iter().map(Child::Node).collect();
        let scope = Scope {
            parent: None,
            top: Top::Section,
            depth: 0,
            section: &info,
        };
        visitor.scope_start();
        walk.body(&kids, scope, false, visitor);
        visitor.scope_end();
    }
    let mut decls: Vec<Child<'_>> = doc
        .quests
        .iter()
        .map(Child::Quest)
        .chain(doc.entries.iter().map(Child::Entry))
        .chain(doc.beats.iter().map(Child::Beat))
        .collect();
    decls.sort_by_key(|c| c.span().byte_start);
    let outside = section_value(0, "", "");
    let scope = Scope {
        parent: None,
        top: Top::Document,
        depth: 0,
        section: &outside,
    };
    walk.body(&decls, scope, true, visitor);
    let mut stats = walk.stats;
    stats.preceding_lookups = walk.counters.lookups.get();
    stats.preceding_steps += walk.counters.steps.get();
    stats.preceding_materialized = walk.counters.materialized.get();
    stats.preceding_copied = walk.counters.copied.get();
    stats
}

/// `node.section`: `{ index, title, id }`, the index 1-based; `{ 0, "", "" }`
/// outside sections.
fn section_value(index: usize, title: &str, id: &str) -> Value {
    Value::Map(BTreeMap::from([
        ("index".to_string(), count(index)),
        ("title".to_string(), Value::Str(title.to_string())),
        ("id".to_string(), Value::Str(id.to_string())),
    ]))
}

impl<'d> Walk<'d> {
    /// Walk one body. `fresh` resets `preceding` before each child: the
    /// document's top-level declarations each start a new scope (dsl 0.50.0
    /// §8.1), which `visitor` sees as a scope frame around the child.
    ///
    /// A sibling is summarized when the row before it is visited (it is that
    /// row's `next`), and its summary is dropped once no later row can read
    /// it: when it is neither the next row's `prev` nor in the `preceding`
    /// map. The arena slots of a body stay in place, so map indexes are
    /// stable.
    fn body(
        &mut self,
        kids: &[Child<'d>],
        scope: Scope<'_>,
        fresh: bool,
        visitor: &mut dyn NodeVisitor,
    ) {
        visitor.body_start();
        let base = self.arena.len();
        self.arena
            .extend(std::iter::repeat_with(NodeSummary::vacant).take(kids.len()));
        if let Some(&first) = kids.first() {
            self.arena[base] = self.summary(first);
        }
        let frame = self.undo.len();
        for (i, &kid) in kids.iter().enumerate() {
            if fresh {
                self.restore(frame);
                visitor.scope_start();
            }
            let me = base + i;
            if let Some(&next) = kids.get(i + 1) {
                self.arena[me + 1] = self.summary(next);
            }
            let sibling = |j: Option<usize>| j.map_or(&self.edge, |j| &self.arena[j]);
            let parent = match (scope.parent, scope.top) {
                (Some(p), _) => &self.arena[p],
                (None, Top::Section) => &self.section_parent,
                (None, Top::Document) => &self.document_parent,
            };
            let row = NodeRow {
                summary: &self.arena[me],
                prev: sibling(i.checked_sub(1).map(|j| base + j)),
                next: sibling((i + 1 < kids.len()).then_some(me + 1)),
                parent,
                index: i,
                last: i + 1 == kids.len(),
                depth: scope.depth,
                section: scope.section,
                preceding: Preceding {
                    map: &self.map,
                    arena: &self.arena,
                    counters: &self.counters,
                },
                span: kid.span(),
            };
            visitor.row(&row);
            self.stats.rows += 1;
            let (_, tag) = kid.identity();
            if !tag.is_empty() {
                self.stats.preceding_steps += steps(self.map.len());
                let replaced = self.map.insert(tag, me);
                self.undo.push((tag, replaced));
                self.stats.preceding_writes += 1;
                if let Some(old) = replaced.filter(|&old| old >= base) {
                    self.arena[old] = NodeSummary::vacant();
                }
            }
            if let Some(prev) = i.checked_sub(1) {
                let (_, prev_tag) = kids[prev].identity();
                self.stats.preceding_steps += steps(self.map.len());
                if self.map.get(prev_tag) != Some(&(base + prev)) {
                    self.arena[base + prev] = NodeSummary::vacant();
                }
            }
            let grand = kid.children();
            if !grand.is_empty() {
                let inner = Scope {
                    parent: Some(me),
                    depth: scope.depth + 1,
                    ..scope
                };
                self.body(&grand, inner, false, visitor);
            }
            if fresh {
                visitor.scope_end();
            }
        }
        self.restore(frame);
        self.arena.truncate(base);
        visitor.body_end();
    }

    /// Undo every `preceding` write made after `frame`.
    fn restore(&mut self, frame: usize) {
        while self.undo.len() > frame {
            let Some((tag, replaced)) = self.undo.pop() else {
                break;
            };
            self.stats.preceding_steps += steps(self.map.len());
            match replaced {
                Some(i) => self.map.insert(tag, i),
                None => self.map.remove(tag),
            };
            self.stats.preceding_restores += 1;
        }
    }

    fn summary(&mut self, kid: Child<'_>) -> NodeSummary {
        let (kind, tag) = kid.identity();
        let mut attrs = BTreeMap::new();
        if kid.has_attrs() {
            let start = kid.span().byte_start;
            let head = self.source.get(start..).unwrap_or_default();
            self.stats.head_bytes += head.find('\n').unwrap_or(head.len()) as u64;
            for a in lute_syntax::authored_attrs(self.source, kid.span()) {
                attrs.insert(a.key.to_string(), Value::Str(a.raw_value.to_string()));
                self.stats.attrs += 1;
            }
        }
        NodeSummary::new(kind, tag, kid.speaker(), attrs)
    }
}
