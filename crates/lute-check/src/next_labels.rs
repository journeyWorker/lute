//! dsl 0.12.0 / 0.37.0 §3.5 / 0.51.0 §3.1–§3.2 whole-document label pass:
//! `::label{name}` registers a DOCUMENT-WIDE jump label; `::jump{to}`
//! resolves against that ONE table. Diagnostics: `E-LABEL-DUP` (a label name
//! reused anywhere in the document), `E-JUMP-UNDEFINED` (a `to=` naming no
//! label anywhere in the document), `E-JUMP-BACKWARD` (a `to=` naming a label
//! at or before the `::jump` site's own document position, on a jump without
//! `max`), and the bounded-jump rules of dsl 0.51.0: `E-JUMP-MAX`,
//! `E-JUMP-LOOP-SCOPE`, `E-JUMP-INTO-LOOP` and `E-JUMP-LOOP-CROSS`. The same
//! pass builds the document's [`LoopTable`], which the flow walks consult.
//!
//! Position is a MONOTONIC counter, depth first, that ticks at EVERY node: a
//! label or a jump when it is recorded, every line, ordinary directive,
//! `::set`, `::assert`, `::retract` and `<timeline>`, and every `<branch>`,
//! `<hub>`, `<match>`, `<on>` and `<objective>` before its bodies are walked
//! ([`Collector::walk`]). The order is the one `lute-compile::stage::walk_seq`
//! flattens records in (top-level nodes in order; a `<branch>`/`<hub>` choice
//! body, or a `<match>` arm body, recursed in order), so "forward" here means
//! exactly what `lute-compile::address`'s addr-lexicographic order will mean
//! once compiled. A loop's region (dsl 0.51.0 §3.2) is every node at a
//! position from its head label to its jump.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::core::{JUMP_MAX_ATTR, LABEL_NAME_ATTR};
use lute_syntax::ast::{Arm, Attr, AttrValue, Directive, Document, Node};

/// `E-LABEL-DUP` (dsl 0.12.0): a label name (`::label{name}`) reused anywhere
/// in the document — one namespace.
pub const E_MARK_DUP: &str = "E-LABEL-DUP";

/// `E-JUMP-UNDEFINED` (dsl 0.12.0): `::jump{to}` names no label anywhere in
/// the document.
pub const E_NEXT_UNDEFINED: &str = "E-JUMP-UNDEFINED";

/// `E-JUMP-BACKWARD` (dsl 0.12.0, 0.51.0 §3.1): `::jump{to}` without `max`
/// names a label at or before its own document position.
pub const E_NEXT_BACKWARD: &str = "E-JUMP-BACKWARD";

/// `E-JUMP-MAX` (dsl 0.51.0 §3.1): `max` is an `@ref` or an integer outside
/// 1..=99, or sits on a forward jump.
pub const E_JUMP_MAX: &str = "E-JUMP-MAX";

/// `E-JUMP-LOOP-SCOPE` (dsl 0.51.0 §3.2): a loop leaves the inline bodies
/// its jump is in, or its region spans an `<on>`/`<objective>`.
pub const E_JUMP_LOOP_SCOPE: &str = "E-JUMP-LOOP-SCOPE";

/// `E-JUMP-INTO-LOOP` (dsl 0.51.0 §3.2): a forward jump from outside a loop
/// lands inside it, after its head.
pub const E_JUMP_INTO_LOOP: &str = "E-JUMP-INTO-LOOP";

/// `E-JUMP-LOOP-CROSS` (dsl 0.51.0 §3.2): two loop regions overlap without
/// nesting.
pub const E_JUMP_LOOP_CROSS: &str = "E-JUMP-LOOP-CROSS";

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
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

/// `max` on `::jump` (dsl 0.51.0 §3.1): how many times a backward jump may be
/// taken, a literal integer from 1 to [`JumpMax::LIMIT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JumpMax(u8);

impl JumpMax {
    /// The largest bound.
    pub const LIMIT: u8 = 99;

    /// `value` as a bound: `None` unless it is a literal integer in
    /// 1..=[`JumpMax::LIMIT`] (an `@ref` never is: a loop bound is static).
    pub fn parse(value: &AttrValue) -> Option<Self> {
        match value {
            AttrValue::Str(s) => s.parse::<i64>().ok().and_then(Self::new),
            AttrValue::Ref(_) | AttrValue::BoolTrue => None,
        }
    }

    fn new(n: i64) -> Option<Self> {
        u8::try_from(n)
            .ok()
            .filter(|n| (1..=Self::LIMIT).contains(n))
            .map(Self)
    }

    /// The bound as a number.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// What a `::jump`'s `max` attribute says.
#[derive(Clone, Copy)]
enum MaxAttr {
    /// No `max`: a forward jump.
    Absent,
    /// A valid bound, at the value's span.
    Bound(JumpMax, Span),
    /// An `@ref` or an integer outside 1..=99 (`E-JUMP-MAX`).
    Invalid(Span),
    /// Not an integer: the attribute checker's `E-ATTR-TYPE` (the declared
    /// `int`), so this pass adds nothing.
    NotInt,
}

impl MaxAttr {
    fn of(attrs: &[Attr]) -> Self {
        let Some(attr) = attrs.iter().find(|a| a.key == JUMP_MAX_ATTR) else {
            return Self::Absent;
        };
        let span = attr.value_span;
        match &attr.value {
            AttrValue::Ref(_) => Self::Invalid(span),
            AttrValue::BoolTrue => Self::NotInt,
            AttrValue::Str(s) => match s.parse::<i64>() {
                Err(_) => Self::NotInt,
                Ok(n) => JumpMax::new(n).map_or(Self::Invalid(span), |max| Self::Bound(max, span)),
            },
        }
    }
}

/// The kind of body a node sits in, for the loop scope rule (dsl 0.51.0
/// §3.2). Only a `<choice>` body of a `<branch>` and a `<match>` arm body are
/// laid out inline; every other body is its own runtime range.
#[derive(Clone, Copy)]
enum BodyKind<'d> {
    Section(&'d str),
    Quest(&'d str),
    Entry(&'d str),
    Beat(&'d str),
    Choice,
    Arm,
    HubOption,
    HubReturn,
    On(&'d str),
    Objective(&'d str),
}

impl BodyKind<'_> {
    fn inline(self) -> bool {
        matches!(self, Self::Choice | Self::Arm)
    }

    /// The `E-JUMP-LOOP-SCOPE` message for a loop of `jump` back to `head`
    /// that leaves this (non-inline) body.
    fn leave_message(self, jump: &str, head: &str) -> String {
        let (body, stays, also) = match self {
            Self::Section(heading) => (format!("the section `## {heading}`"), "one section", ""),
            Self::Quest(id) => (format!("`<quest id=\"{id}\">`"), "one quest", ""),
            Self::Entry(id) => (format!("`<entry id=\"{id}\">`"), "one entry", ""),
            Self::Beat(id) => (format!("`<beat id=\"{id}\">`"), "one beat", ""),
            Self::HubOption => (
                "the `<hub>` option".to_string(),
                "one option body",
                ", or use the hub itself to repeat",
            ),
            Self::HubReturn => (
                "the `<hub>` `<return>` body".to_string(),
                "that body",
                ", or use the hub itself to repeat",
            ),
            Self::On(event) => (
                format!("the `<on event=\"{event}\">` handler"),
                "one handler body",
                "",
            ),
            Self::Objective(id) => (
                format!("`<objective id=\"{id}\">`"),
                "the objective's body",
                "",
            ),
            Self::Choice | Self::Arm => unreachable!("inline bodies never end a loop's scope"),
        };
        format!(
            "`{jump}` loops back to a label outside {body} it is in; a loop stays inside \
             {stays} — put `::label{{name=\"{head}\"}}` inside it{also}"
        )
    }
}

/// One body: its kind and the body that encloses it (`None` for a section,
/// quest, entry or beat).
struct Body<'d> {
    kind: BodyKind<'d>,
    parent: Option<usize>,
}

/// An `<on>` or `<objective>`: a separate runtime range a loop region may not
/// span (dsl 0.51.0 §3.2).
struct Separate<'d> {
    pos: u64,
    what: String,
    body: &'d str,
}

/// One label's DEFINING position — first occurrence only; a repeat is
/// `E-LABEL-DUP` at the repeat's own span and never overwrites the table.
struct LabelSite {
    pos: u64,
    body: usize,
}

/// One `::jump{to}` site: its own document position, the target id, the span
/// to anchor its diagnostics at (the `to` value), the directive, the body it
/// sits in and its `max`.
struct NextSite<'d> {
    pos: u64,
    to: &'d str,
    span: Span,
    directive: Span,
    body: usize,
    max: MaxAttr,
}

impl NextSite<'_> {
    /// The jump as written, for messages: `::jump{to="x" max="3"}`.
    fn render(&self) -> String {
        match self.max {
            MaxAttr::Bound(max, _) => format!("::jump{{to=\"{}\" max=\"{}\"}}", self.to, max.get()),
            MaxAttr::Absent | MaxAttr::Invalid(_) | MaxAttr::NotInt => {
                format!("::jump{{to=\"{}\"}}", self.to)
            }
        }
    }
}

struct Collector<'d> {
    pos: u64,
    labels: BTreeMap<&'d str, LabelSite>,
    dups: Vec<Diagnostic>,
    nexts: Vec<NextSite<'d>>,
    bodies: Vec<Body<'d>>,
    separate: Vec<Separate<'d>>,
}

impl<'d> Collector<'d> {
    fn tick(&mut self) {
        self.pos += 1;
    }

    /// A new body of `kind` inside `parent`.
    fn open(&mut self, kind: BodyKind<'d>, parent: Option<usize>) -> usize {
        self.bodies.push(Body { kind, parent });
        self.bodies.len() - 1
    }

    fn record_label(&mut self, id: &'d str, span: Span, attrs: &[Attr], body: usize) {
        if let Some(message) = lute_manifest::ident::name_fault("label name", id) {
            let at = attr_span(attrs, LABEL_NAME_ATTR).unwrap_or(span);
            self.dups
                .push(diag(crate::cel_paths::E_PATH_IDENT, message, at));
        }
        if self.labels.contains_key(id) {
            self.dups.push(diag(
                E_MARK_DUP,
                format!("label `{id}` is already declared elsewhere in this document"),
                span,
            ));
        } else {
            self.labels.insert(
                id,
                LabelSite {
                    pos: self.pos,
                    body,
                },
            );
        }
        self.tick();
    }

    fn record_next(&mut self, d: &'d Directive, body: usize) {
        if let Some(to) = attr_str(&d.attrs, "to") {
            let span = attr_span(&d.attrs, "to").unwrap_or(d.span);
            self.nexts.push(NextSite {
                pos: self.pos,
                to,
                span,
                directive: d.span,
                body,
                max: MaxAttr::of(&d.attrs),
            });
        }
        self.tick();
    }

    /// Mirrors `lute-compile::stage::walk_seq`'s recursion shape (top-level
    /// nodes, then each `<branch>`/`<hub>` choice body / `<match>` arm body
    /// in order) closely enough that RELATIVE label/next ordering here
    /// agrees with the compiled addr-lexicographic order — `<timeline>`
    /// clips carry no `Node`s (a `::label`/`::jump` inside one is rejected
    /// outright by `check.rs`'s timeline-clip loop, mirroring `::end`), so
    /// they are ticked once as an opaque leaf and never recursed into.
    /// `body` is the body `nodes` sit in.
    fn walk(&mut self, nodes: &'d [Node], body: usize) {
        for node in nodes {
            match node {
                Node::Directive(d) if d.tag == lute_manifest::core::LABEL_DIRECTIVE => {
                    match attr_str(&d.attrs, LABEL_NAME_ATTR) {
                        Some(id) => self.record_label(id, d.span, &d.attrs, body),
                        None => self.tick(),
                    }
                }
                Node::Directive(d) if d.tag == lute_manifest::core::JUMP_DIRECTIVE => {
                    self.record_next(d, body);
                }
                Node::Line(_)
                | Node::Directive(_)
                | Node::Set(_)
                | Node::Assert(_)
                | Node::Retract(_)
                | Node::Timeline(_) => {
                    self.tick();
                }
                Node::Branch(b) => {
                    self.tick();
                    for c in &b.choices {
                        let inner = self.open(BodyKind::Choice, Some(body));
                        self.walk(&c.body, inner);
                    }
                }
                Node::Hub(h) => {
                    self.tick();
                    for c in &h.choices {
                        let inner = self.open(BodyKind::HubOption, Some(body));
                        self.walk(&c.body, inner);
                    }
                    if let Some(r) = &h.on_return {
                        let inner = self.open(BodyKind::HubReturn, Some(body));
                        self.walk(&r.body, inner);
                    }
                }
                Node::Match(m) => {
                    self.tick();
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body: nodes, .. } | Arm::Otherwise { body: nodes, .. } => {
                                let inner = self.open(BodyKind::Arm, Some(body));
                                self.walk(nodes, inner);
                            }
                        }
                    }
                }
                Node::On(o) => {
                    self.separate.push(Separate {
                        pos: self.pos,
                        what: format!("`<on event=\"{}\">`", o.event),
                        body: "handler's",
                    });
                    self.tick();
                    let inner = self.open(BodyKind::On(&o.event), Some(body));
                    self.walk(&o.body, inner);
                }
                Node::Objective(o) => {
                    self.separate.push(Separate {
                        pos: self.pos,
                        what: format!("`<objective id=\"{}\">`", o.id),
                        body: "objective's",
                    });
                    self.tick();
                    let inner = self.open(BodyKind::Objective(&o.id), Some(body));
                    self.walk(&o.body, inner);
                }
            }
        }
    }

    /// The non-inline body a loop of the jump in `jump_body` back to a label
    /// in `label_body` leaves, if any (dsl 0.51.0 §3.2 scope rule): the label
    /// must sit in the jump's own body or one enclosing it, through `<choice>`
    /// bodies of a `<branch>` and `<match>` arm bodies only.
    fn left_body(&self, jump_body: usize, label_body: usize) -> Option<BodyKind<'d>> {
        let mut at = jump_body;
        while at != label_body {
            let body = &self.bodies[at];
            if !body.kind.inline() {
                return Some(body.kind);
            }
            at = body
                .parent
                .expect("an inline body (a choice or an arm) always sits in an enclosing body");
        }
        None
    }
}

/// One bounded backward jump's loop (dsl 0.51.0 §3.2): its head label, its
/// jump, the jump's bound and the innermost loop that encloses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopRegion {
    head: String,
    jump: Span,
    max: JumpMax,
    /// The head's and the jump's positions: the region is `first..=last`.
    first: u64,
    last: u64,
    enclosing: Option<usize>,
}

impl LoopRegion {
    /// The name of the head label.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// The span of the `::jump` directive that closes the loop.
    pub fn jump(&self) -> Span {
        self.jump
    }

    /// How many times the jump may be taken.
    pub fn max(&self) -> JumpMax {
        self.max
    }

    /// The index, in [`LoopTable::regions`], of the innermost region that
    /// encloses this one; `None` at the top of the nesting.
    pub fn enclosing(&self) -> Option<usize> {
        self.enclosing
    }

    fn contains(&self, pos: u64) -> bool {
        (self.first..=self.last).contains(&pos)
    }
}

/// Every loop of one document (dsl 0.51.0 §3.2), in the document order of
/// their jumps, computed once by [`check_next_labels`]. The table holds only
/// loops that satisfy every structural rule: each stays inside one inline
/// body chain, spans no `<on>`/`<objective>`, and any two regions nest or are
/// disjoint. A bounded jump that breaks a rule is reported and left out, so a
/// flow walk can treat each head as one join point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopTable {
    regions: Vec<LoopRegion>,
}

impl LoopTable {
    /// Links each region to the innermost region enclosing it.
    fn nest(mut regions: Vec<LoopRegion>) -> Self {
        let enclosing: Vec<Option<usize>> = regions
            .iter()
            .enumerate()
            .map(|(i, r)| {
                regions
                    .iter()
                    .enumerate()
                    .filter(|(k, o)| *k != i && o.first <= r.first && r.last <= o.last)
                    .min_by_key(|(_, o)| o.last - o.first)
                    .map(|(k, _)| k)
            })
            .collect();
        for (region, outer) in regions.iter_mut().zip(enclosing) {
            region.enclosing = outer;
        }
        Self { regions }
    }

    /// Every loop, in the document order of its jump.
    pub fn regions(&self) -> &[LoopRegion] {
        &self.regions
    }

    /// `true` when the document has no loop.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// `true` when `label` heads at least one loop.
    pub fn is_head(&self, label: &str) -> bool {
        self.regions.iter().any(|r| r.head == label)
    }

    /// The loop `jump` closes, when it is a bounded backward jump in this
    /// table.
    pub fn back_edge(&self, jump: &Directive) -> Option<&LoopRegion> {
        self.regions
            .iter()
            .find(|r| r.jump.byte_start == jump.span.byte_start)
    }
}

/// What the label pass finds in one document: its diagnostics and its loops.
#[derive(Debug)]
pub struct LabelPass {
    pub diagnostics: Vec<Diagnostic>,
    pub loops: LoopTable,
}

/// The `E-JUMP-MAX` message for a `max` that is no literal integer in 1..=99.
fn max_range_message() -> String {
    format!(
        "`max` on `::jump` is the most times it may be taken: a literal integer from 1 to {} \
         (a loop bound is static, so it cannot be `@ref`)",
        JumpMax::LIMIT
    )
}

/// dsl 0.12.0 / 0.51.0 whole-document pass: `E-LABEL-DUP` /
/// `E-JUMP-UNDEFINED` / `E-JUMP-BACKWARD`, the bounded-jump rules
/// (`E-JUMP-MAX`, `E-JUMP-LOOP-SCOPE`, `E-JUMP-INTO-LOOP`,
/// `E-JUMP-LOOP-CROSS`, judged in one pass after collection) and the
/// document's [`LoopTable`]. Walks `doc.shots`, `doc.quests`, then the lore
/// addressing units — `doc.entries` (dsl 0.19.0) and `doc.beats` (dsl 0.23.0
/// §4) interleaved in source order, as the artifact lays them out — each
/// recursively — mirrors `reachability::check_reachability_in`'s own walk
/// shape. The label NAMESPACE is document-wide: ids are NOT reset between
/// shots/quests — a mark in shot 1 and a `::jump` in shot 4 resolve against
/// the SAME table shot 1 populated (dsl 0.12.0: "a single table" — this is
/// also what lets a guarded `::jump` join a LATER shot,
/// `lute-compile::address`'s document-wide named-label resolution pass).
pub fn check_next_labels(doc: &Document) -> LabelPass {
    let mut c = collect(doc);
    let mut diags = std::mem::take(&mut c.dups);
    // Bounded backward jumps, as (jump, head) site pairs.
    let mut bounded: Vec<(&NextSite, &LabelSite)> = Vec::new();
    for next in &c.nexts {
        let Some(label) = c.labels.get(next.to) else {
            // Round-6 T3-60: a target that names a heading, a choice or
            // a menu says what it is; otherwise the nearest label.
            let hint = match not_a_mark(doc, next.to) {
                Some(why) => format!(" — {why}"),
                None => lute_manifest::suggest::did_you_mean(next.to, c.labels.keys().copied()),
            };
            diags.push(diag(
                E_NEXT_UNDEFINED,
                format!(
                    "`::jump` targets `{}`, which no `::label` in this document declares{hint}",
                    next.to
                ),
                next.span,
            ));
            if let MaxAttr::Invalid(at) = next.max {
                diags.push(diag(E_JUMP_MAX, max_range_message(), at));
            }
            continue;
        };
        if label.pos <= next.pos {
            match next.max {
                MaxAttr::Absent => diags.push(diag(
                    E_NEXT_BACKWARD,
                    format!(
                        "`::jump` targets label `{to}`, which is not forward of this `::jump` in \
                         document order — `::jump` only jumps forward unless it carries `max`: \
                         `::jump{{to=\"{to}\" max=\"3\"}}` repeats at most 3 times; to offer \
                         choices again, use a `<hub>` (it asks until an `exit` choice is taken)",
                        to = next.to
                    ),
                    next.span,
                )),
                MaxAttr::Invalid(at) => diags.push(diag(E_JUMP_MAX, max_range_message(), at)),
                MaxAttr::NotInt => {}
                MaxAttr::Bound(..) => bounded.push((next, label)),
            }
        } else if let MaxAttr::Bound(_, at) | MaxAttr::Invalid(at) = next.max {
            diags.push(diag(
                E_JUMP_MAX,
                format!(
                    "`max` bounds a backward jump; `::jump{{to=\"{}\"}}` already goes forward, so \
                     `max` means nothing here — remove it",
                    next.to
                ),
                at,
            ));
        }
    }

    // Scope: the loop stays inside one inline body chain and spans no
    // separately run `<on>`/`<objective>`.
    let mut scoped: Vec<(&NextSite, LoopRegion)> = Vec::new();
    for (next, label) in bounded {
        if let Some(left) = c.left_body(next.body, label.body) {
            diags.push(diag(
                E_JUMP_LOOP_SCOPE,
                left.leave_message(&next.render(), next.to),
                next.span,
            ));
            continue;
        }
        if let Some(over) = c
            .separate
            .iter()
            .find(|s| label.pos < s.pos && s.pos < next.pos)
        {
            diags.push(diag(
                E_JUMP_LOOP_SCOPE,
                format!(
                    "`{}` loops back over {}, which runs on its own; put the loop inside the {} \
                     body or before it",
                    next.render(),
                    over.what,
                    over.body
                ),
                next.span,
            ));
            continue;
        }
        let MaxAttr::Bound(max, _) = next.max else {
            unreachable!("only a jump with a valid `max` is collected as bounded")
        };
        scoped.push((
            next,
            LoopRegion {
                head: next.to.to_string(),
                jump: next.directive,
                max,
                first: label.pos,
                last: next.pos,
                enclosing: None,
            },
        ));
    }

    // Nest, never cross: `scoped` is in jump order, so an earlier region
    // ends first, and a later one crosses it when it starts inside it but
    // after its head. Reported once, at the later jump, which stays out of
    // the table.
    let mut regions = Vec::new();
    for (i, (next, region)) in scoped.iter().enumerate() {
        let crossed = scoped[..i]
            .iter()
            .find(|(_, earlier)| earlier.first < region.first && region.first <= earlier.last);
        match crossed {
            Some((earlier_next, earlier)) => diags.push(diag(
                E_JUMP_LOOP_CROSS,
                format!(
                    "the loop back to `{}` (line {}) and the loop back to `{}` (line {}) overlap \
                     without nesting; close the inner loop before the outer one jumps back",
                    earlier.head, earlier_next.directive.line, region.head, next.directive.line
                ),
                next.span,
            )),
            None => regions.push(region.clone()),
        }
    }
    let loops = LoopTable::nest(regions);

    // No entry into a region: a forward jump from outside a loop lands at
    // its head, never after it. The outermost loop entered is named.
    for next in &c.nexts {
        let Some(label) = c.labels.get(next.to) else {
            continue;
        };
        if label.pos <= next.pos {
            continue;
        }
        let entered = loops
            .regions
            .iter()
            .filter(|r| r.first < label.pos && r.contains(label.pos) && !r.contains(next.pos))
            .min_by_key(|r| r.first);
        if let Some(region) = entered {
            diags.push(diag(
                E_JUMP_INTO_LOOP,
                format!(
                    "`{}` enters the loop of `::jump{{to=\"{head}\" max=\"{}\"}}` (line {}) after \
                     its head `{head}`; a loop is entered only at its head — jump to `{head}` \
                     instead",
                    next.render(),
                    region.max.get(),
                    region.jump.line,
                    head = region.head
                ),
                next.span,
            ));
        }
    }
    LabelPass {
        diagnostics: diags,
        loops,
    }
}

/// Why `to` is no jump target when it names something else in `doc` — a
/// `## ` heading, a `<branch>`/`<hub>` id, a choice id — or Ink's `END`;
/// `None` when it names nothing (a typo, answered by did-you-mean). A
/// heading matches ignoring case and spacing (`## Lamp Room` for
/// `lampRoom`).
fn not_a_mark(doc: &Document, to: &str) -> Option<String> {
    const TARGET: &str = "a `::jump` target is a `::label{name=\"…\"}` later in this document";
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
    if let Some(shot) = doc.sections.iter().find(|s| fold(&s.heading) == fold(to)) {
        return Some(format!(
            "`{to}` is the `## {}` heading, not a label; {TARGET}, so put `::label{{name=\"{to}\"}}` \
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
                return Some(format!("`{to}` is the id of {menu}, not a label"));
            }
            choices
                .iter()
                .find_map(|c| {
                    if c.id == to {
                        Some(format!("`{to}` is a choice id in {menu}, not a label"))
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
        .sections
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

/// dsl 0.27.0 (round-5 T3-7): every label name some `::jump{to}` in `doc`
/// names — the `::label`s a walk can enter by a jump, so content from one of
/// them on is reachable even after an `::end`.
pub(crate) fn next_targets(doc: &Document) -> BTreeSet<String> {
    collect(doc)
        .nexts
        .into_iter()
        .map(|n| n.to.to_string())
        .collect()
}

/// `node` is — or holds, at any depth — a `::label{name}` named in
/// `targets`: a walk can enter it by a jump.
pub(crate) fn holds_label(node: &Node, targets: &BTreeSet<String>) -> bool {
    let named =
        |attrs: &[Attr]| attr_str(attrs, LABEL_NAME_ATTR).is_some_and(|id| targets.contains(id));
    let any = |nodes: &[Node]| nodes.iter().any(|n| holds_label(n, targets));
    match node {
        Node::Directive(d) => d.tag == lute_manifest::core::LABEL_DIRECTIVE && named(&d.attrs),
        Node::Line(_) => false,
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

/// The document's labels, `::jump` sites and bodies, in [`Collector::walk`]
/// order.
fn collect(doc: &Document) -> Collector<'_> {
    let mut c = Collector {
        pos: 0,
        labels: BTreeMap::new(),
        dups: Vec::new(),
        nexts: Vec::new(),
        bodies: Vec::new(),
        separate: Vec::new(),
    };
    for shot in &doc.sections {
        let body = c.open(BodyKind::Section(&shot.heading), None);
        c.walk(&shot.body, body);
    }
    for quest in &doc.quests {
        let body = c.open(BodyKind::Quest(&quest.id), None);
        c.walk(&quest.body, body);
    }
    let mut units: Vec<(usize, BodyKind<'_>, &[Node])> = doc
        .entries
        .iter()
        .map(|e| (e.span.byte_start, BodyKind::Entry(&e.id), e.body.as_slice()))
        .chain(
            doc.beats
                .iter()
                .map(|b| (b.span.byte_start, BodyKind::Beat(&b.id), b.body.as_slice())),
        )
        .collect();
    units.sort_by_key(|(start, _, _)| *start);
    for (_, kind, nodes) in units {
        let body = c.open(kind, None);
        c.walk(nodes, body);
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

    fn check(d: &Document) -> Vec<Diagnostic> {
        check_next_labels(d).diagnostics
    }

    fn codes(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn clean_forward_mark_and_next_is_clean() {
        let d = doc("@narrator: hi\n::jump{to=\"x\"}\n::label{name=\"x\"}\n@narrator: there\n");
        let pass = check_next_labels(&d);
        assert!(pass.diagnostics.is_empty());
        assert!(pass.loops.is_empty());
    }

    /// dsl 0.37.0 §3.5: a line `id=` is no label — only `::label{name}` is.
    #[test]
    fn line_id_is_not_a_target() {
        let d = doc("::jump{to=\"x\"}\n@narrator{id=\"x\"}: there\n");
        assert_eq!(codes(&check(&d)), ["E-JUMP-UNDEFINED"]);
    }

    #[test]
    fn undefined_target_errors() {
        let d = doc("::jump{to=\"nope\"}\n");
        assert_eq!(codes(&check(&d)), ["E-JUMP-UNDEFINED"]);
    }

    // A misspelled target names the document's closest mark; a target
    // nothing resembles gets no guess.
    #[test]
    fn undefined_target_suggests_the_nearest_mark() {
        let d = doc("::jump{to=\"endng\"}\n::label{name=\"ending\"}\n@narrator: bye\n");
        let diags = check(&d);
        assert_eq!(codes(&diags), ["E-JUMP-UNDEFINED"]);
        assert!(
            diags[0].message.contains("did you mean `ending`?"),
            "{}",
            diags[0].message
        );
        let d = doc("::jump{to=\"nowhere\"}\n::label{name=\"ending\"}\n@narrator: bye\n");
        assert!(!check(&d)[0].message.contains("did you mean"));
    }

    // Round-6 T3-60: a backward jump says what loops instead; dsl 0.51.0
    // §3.1: and how `max` makes it legal.
    #[test]
    fn backward_target_errors() {
        let d = doc("::label{name=\"x\"}\n@narrator: hi\n::jump{to=\"x\"}\n");
        let diags = check(&d);
        assert_eq!(codes(&diags), ["E-JUMP-BACKWARD"]);
        let m = &diags[0].message;
        assert!(m.contains("`<hub>`"), "{m}");
        assert!(
            m.contains("only jumps forward unless it carries `max`: `::jump{to=\"x\" max=\"3\"}`"),
            "{m}"
        );
    }

    // Round-6 T3-60: a target naming a heading, a choice, or Ink's `END`
    // says what that id is instead of only "undefined".
    #[test]
    fn undefined_target_names_what_the_id_is() {
        let message = |src: &str| check(&doc(src))[0].message.clone();
        let m = message("::jump{to=\"gallery\"}\n\n## Gallery\n\n@narrator: fog\n");
        assert!(m.contains("`## Gallery` heading"), "{m}");
        let m = message(
            "<branch id=\"door\">\n  <choice id=\"inside\" text=\"In\">\n    @narrator: in\n  \
             </choice>\n  <choice id=\"stay\" text=\"Stay\">\n    ::jump{to=\"inside\"}\n  \
             </choice>\n</branch>\n",
        );
        assert!(m.contains("choice id in `<branch id=\"door\">`"), "{m}");
        // Ink's END ends the story (`terminal:`); DONE ends the scene.
        assert!(message("::jump{to=\"END\"}\n").contains("`terminal:`"));
        assert!(message("::jump{to=\"DONE\"}\n").contains("a scene ends with `::end`"));
    }

    #[test]
    fn duplicate_mark_ids_error() {
        let d = doc("::label{name=\"x\"}\n::label{name=\"x\"}\n");
        assert_eq!(codes(&check(&d)), ["E-LABEL-DUP"]);
    }

    #[test]
    fn duplicate_label_across_sections_errors() {
        let d = doc("::label{name=\"x\"}\n@narrator: hi\n\n## Two\n\n::label{name=\"x\"}\n");
        assert_eq!(codes(&check(&d)), ["E-LABEL-DUP"]);
    }

    // --- dsl 0.51.0 §3.1–§3.2: bounded backward jumps -----------------------

    #[test]
    fn jump_max_accepts_literal_integers_from_1_to_99() {
        let parse = |s: &str| JumpMax::parse(&AttrValue::Str(s.to_string())).map(JumpMax::get);
        assert_eq!(parse("1"), Some(1));
        assert_eq!(parse("99"), Some(99));
        for bad in ["0", "100", "-1", "two", "", "300"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
        assert_eq!(JumpMax::parse(&AttrValue::BoolTrue), None);
    }

    /// The #159 retry menu: a bounded jump out of a `<choice>` body back to
    /// a label in the enclosing section is one loop.
    #[test]
    fn a_bounded_retry_loop_is_one_region() {
        let d = doc(
            "::label{name=\"again\"}\n@mira: Ready?\n<branch id=\"retry\" prompt=\"Again?\">\n  \
             <choice id=\"yes\" text=\"Yes\">\n    ::jump{to=\"again\" max=\"3\"}\n  </choice>\n  \
             <choice id=\"no\" text=\"No\">\n    @theo: Fine.\n  </choice>\n</branch>\n",
        );
        let pass = check_next_labels(&d);
        assert!(pass.diagnostics.is_empty(), "{:?}", pass.diagnostics);
        let [region] = pass.loops.regions() else {
            panic!("one loop: {:?}", pass.loops)
        };
        assert_eq!((region.head(), region.max().get()), ("again", 3));
        assert_eq!(region.enclosing(), None);
        assert!(pass.loops.is_head("again"));
        assert!(!pass.loops.is_head("yes"));
        let jump = d.sections[0]
            .body
            .iter()
            .find_map(|n| match n {
                Node::Branch(b) => b.choices[0].body.iter().find_map(|n| match n {
                    Node::Directive(j) => Some(j),
                    _ => None,
                }),
                _ => None,
            })
            .expect("the jump in the `yes` choice");
        assert_eq!(pass.loops.back_edge(jump), Some(region));
    }

    #[test]
    fn nested_and_same_head_regions_nest() {
        let d = doc(
            "::label{name=\"outer\"}\n@a: one\n::label{name=\"inner\"}\n@a: two\n\
             ::jump{to=\"inner\" max=\"2\"}\n::jump{to=\"inner\" max=\"4\"}\n\
             ::jump{to=\"outer\" max=\"2\"}\n",
        );
        let pass = check_next_labels(&d);
        assert!(pass.diagnostics.is_empty(), "{:?}", pass.diagnostics);
        let heads: Vec<(&str, Option<usize>)> = pass
            .loops
            .regions()
            .iter()
            .map(|r| (r.head(), r.enclosing()))
            .collect();
        // The shorter same-head loop nests in the longer one, which nests in
        // `outer`.
        assert_eq!(
            heads,
            [("inner", Some(1)), ("inner", Some(2)), ("outer", None)]
        );
    }

    #[test]
    fn crossing_regions_are_reported_at_the_later_jump_and_left_out() {
        let d = doc(
            "::label{name=\"a\"}\n@x: one\n::label{name=\"b\"}\n@x: two\n\
             ::jump{to=\"a\" max=\"2\"}\n@x: three\n::jump{to=\"b\" max=\"2\"}\n",
        );
        let pass = check_next_labels(&d);
        assert_eq!(codes(&pass.diagnostics), ["E-JUMP-LOOP-CROSS"]);
        let m = &pass.diagnostics[0].message;
        assert!(
            m.contains("the loop back to `a` (line 14) and the loop back to `b` (line 16)"),
            "{m}"
        );
        assert_eq!(pass.diagnostics[0].span.line, 16);
        let heads: Vec<&str> = pass.loops.regions().iter().map(LoopRegion::head).collect();
        assert_eq!(heads, ["a"]);
    }

    #[test]
    fn disjoint_regions_and_exits_are_legal() {
        let d = doc(
            "::label{name=\"a\"}\n@x: one\n<match subject=\"run.done\">\n  <when is=\"true\">\n    \
             ::jump{to=\"out\"}\n  </when>\n</match>\n::jump{to=\"a\" max=\"2\"}\n\
             ::label{name=\"b\"}\n@x: two\n::jump{to=\"b\" max=\"2\"}\n::label{name=\"out\"}\n\
             @x: done\n",
        );
        let pass = check_next_labels(&d);
        assert!(pass.diagnostics.is_empty(), "{:?}", pass.diagnostics);
        assert_eq!(pass.loops.regions().len(), 2);
    }

    #[test]
    fn a_forward_jump_into_a_region_after_its_head_is_refused() {
        let d = doc(
            "::jump{to=\"mid\"}\n::label{name=\"attempt\"}\n@x: one\n::label{name=\"mid\"}\n\
             @x: two\n::jump{to=\"attempt\" max=\"2\"}\n",
        );
        let diags = check(&d);
        assert_eq!(codes(&diags), ["E-JUMP-INTO-LOOP"]);
        assert_eq!(
            diags[0].message,
            "`::jump{to=\"mid\"}` enters the loop of `::jump{to=\"attempt\" max=\"2\"}` (line 15) \
             after its head `attempt`; a loop is entered only at its head — jump to `attempt` \
             instead"
        );
        // To the head itself: legal.
        let d = doc(
            "::jump{to=\"attempt\"}\n@x: skipped\n::label{name=\"attempt\"}\n@x: one\n\
             ::jump{to=\"attempt\" max=\"2\"}\n",
        );
        assert!(check(&d).is_empty());
        // From inside the region: legal.
        let d = doc(
            "::label{name=\"attempt\"}\n::jump{to=\"mid\"}\n@x: one\n::label{name=\"mid\"}\n\
             ::jump{to=\"attempt\" max=\"2\"}\n",
        );
        assert!(check(&d).is_empty());
    }

    #[test]
    fn max_out_of_range_or_forward_is_e_jump_max() {
        let back = |max: &str| {
            check(&doc(&format!(
                "::label{{name=\"x\"}}\n@a: hi\n::jump{{to=\"x\" max={max}}}\n"
            )))
        };
        for bad in ["\"0\"", "\"100\"", "\"-1\"", "@run.n"] {
            let diags = back(bad);
            assert_eq!(codes(&diags), ["E-JUMP-MAX"], "{bad}");
            assert!(
                diags[0].message.contains("a literal integer from 1 to 99"),
                "{}",
                diags[0].message
            );
        }
        // Not an integer at all: the attribute checker's `E-ATTR-TYPE` only.
        assert!(back("\"two\"").is_empty());
        let diags = check(&doc(
            "::jump{to=\"later\" max=\"2\"}\n::label{name=\"later\"}\n",
        ));
        assert_eq!(codes(&diags), ["E-JUMP-MAX"]);
        assert_eq!(
            diags[0].message,
            "`max` bounds a backward jump; `::jump{to=\"later\"}` already goes forward, so `max` \
             means nothing here — remove it"
        );
    }

    #[test]
    fn a_loop_stays_inside_inline_bodies() {
        let message = |src: &str| {
            let pass = check_next_labels(&doc(src));
            assert!(pass.loops.is_empty(), "{src}");
            let [d] = pass.diagnostics.as_slice() else {
                panic!("one diagnostic for {src}: {:?}", pass.diagnostics)
            };
            assert_eq!(d.code, "E-JUMP-LOOP-SCOPE");
            d.message.clone()
        };
        let hub_option = message(
            "::label{name=\"attempt\"}\n<hub id=\"h\">\n  <choice id=\"go\" text=\"Go\">\n    \
             ::jump{to=\"attempt\" max=\"2\"}\n  </choice>\n  <choice id=\"out\" text=\"Out\" \
             exit>\n    @a: bye\n  </choice>\n</hub>\n",
        );
        assert_eq!(
            hub_option,
            "`::jump{to=\"attempt\" max=\"2\"}` loops back to a label outside the `<hub>` option it \
             is in; a loop stays inside one option body — put `::label{name=\"attempt\"}` inside \
             it, or use the hub itself to repeat"
        );
        let hub_return = message(
            "::label{name=\"attempt\"}\n<hub id=\"h\">\n  <choice id=\"out\" text=\"Out\" exit>\n    \
             @a: bye\n  </choice>\n  <return>\n    ::jump{to=\"attempt\" max=\"2\"}\n  </return>\n\
             </hub>\n",
        );
        assert!(
            hub_return.contains("outside the `<hub>` `<return>` body"),
            "{hub_return}"
        );
        let section = message(
            "::label{name=\"attempt\"}\n@a: hi\n\n## Two\n\n::jump{to=\"attempt\" max=\"2\"}\n",
        );
        assert!(
            section.contains("outside the section `## Two`"),
            "{section}"
        );
        // Through a `<choice>` body and a `<match>` arm: legal.
        let d = doc(
            "::label{name=\"attempt\"}\n<branch id=\"b\">\n  <choice id=\"c\" text=\"C\">\n    \
             <match subject=\"run.done\">\n      <when is=\"false\">\n        \
             ::jump{to=\"attempt\" max=\"2\"}\n      </when>\n    </match>\n  </choice>\n</branch>\n",
        );
        assert!(check(&d).is_empty());
        // A loop inside one hub option body: legal.
        let d = doc(
            "<hub id=\"h\">\n  <choice id=\"go\" text=\"Go\">\n    ::label{name=\"attempt\"}\n    \
             @a: try\n    ::jump{to=\"attempt\" max=\"2\"}\n  </choice>\n  <choice id=\"out\" \
             text=\"Out\" exit>\n    @a: bye\n  </choice>\n</hub>\n",
        );
        assert_eq!(check_next_labels(&d).loops.regions().len(), 1);
    }
}
