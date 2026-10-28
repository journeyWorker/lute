//! The document walker: flatten (this task) + CFG-aware stage resolution
//! (Task 9, D9) + inline timelines (Task 10). ONE walk owns emission order.

use lute_check::ctx::Env;
use lute_check::{
    classify_node, lower_node_with_snapshot, Ctx, InjectKind, InjectedCommand, StageFlow,
    StageState,
};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::stamps::StampSurface;
use lute_syntax::ast::{
    Arm, AttrValue, Branch, BundleBeat, ClipNode, Directive, Entry, Hub, Match, Node, Quest,
    Timeline,
};

use crate::cfg::{Emitter, Label};
use crate::ir::{
    placeholder_from_interp, ActorCmd, BarrierCmd, BeatCmd, CelPairExt, ChoiceCmd, ChoiceOption,
    Command, EntryCmd, ForKindExt, HubCmd, HubOption, JumpCmd, MatchArm, MatchCmd, ObjectiveEntry,
    OnCmd, QuestCmd, RewardEntry, Source, Stamp, TargetKindExt,
};
use crate::lower::{
    attr_bool, attr_string, element_stamps, fact_text, lower_assert, lower_directive, lower_line,
    lower_retract, lower_set,
};
use crate::normalize::{component_scope, is_body_split, COMPONENT_BEGIN, COMPONENT_END};
use crate::schedule::schedule_timeline;
use crate::source_map::{ArmSource, ComponentBoundary, ComponentUse, SourceInfo, SourceMarker};
use lute_ir::{CelPair, ForKind, TargetKind};

/// Walk context: the read-only capability surface + the component-source
/// stack (sentinel-driven: each open expansion's name and its
/// `{component}#{n}` identity segment) + the document-order timeline counter
/// (Task 10) + the label routing of the stage state (dsl 0.51.0 §3.4).
#[derive(Debug)]
pub struct WalkCx<'a> {
    pub snapshot: &'a CapabilitySnapshot,
    pub env: &'a Env,
    pub components: Vec<(String, String, bool)>,
    pub timelines: u32,
    /// The checker's label and loop-head state function
    /// ([`lute_check::StageFlow`]): this walk binds the stage state at every
    /// `::label` from it, so it and `check()` agree.
    pub stage: StageFlow<'a>,
    /// `false` after an unguarded `::jump` without `max`, until a label a
    /// jump reaches: no route reaches the nodes between, so they inject
    /// nothing.
    pub reached: bool,
    /// dsl 0.52.0 §5.2: the checker's count of hub bodies each leaving
    /// `::jump` leaves, written on its `jump` record as `leaves`.
    pub leaves: &'a lute_check::next_labels::HubLeaves,
}

impl<'a> WalkCx<'a> {
    /// A walk over one document whose label pass (its loops and leaving
    /// jumps) is `labels` and whose `<match>` subjects resolve against
    /// `subjects`; `components` names the component table its inlined
    /// bodies come from.
    pub fn new(
        snapshot: &'a CapabilitySnapshot,
        env: &'a Env,
        labels: &'a lute_check::next_labels::LabelPass,
        subjects: lute_check::SubjectScope<'a>,
        components: Option<&'a lute_check::ComponentSet>,
    ) -> Self {
        WalkCx {
            snapshot,
            env,
            components: Vec::new(),
            timelines: 0,
            stage: StageFlow::new(
                &labels.loops,
                &env.domains,
                snapshot,
                subjects,
                components,
                lute_check::WalkKind::StageCompile,
            ),
            reached: true,
            leaves: &labels.leaves,
        }
    }
}

/// Timeline-clip stamp for records emitted inside a `<timeline>` (Task 10).
#[derive(Debug, Clone, Copy)]
pub struct ClipStamp {
    pub timeline: u32,
    pub at: f64,
    pub duration: f64,
}

/// A component sentinel, `::label` or `::jump`: the directives that scope
/// and route control, walked by [`walk_flow_directive`] in every body walker
/// ([`walk_seq`], [`walk_quest`]).
fn is_flow_directive(d: &lute_syntax::ast::Directive) -> bool {
    [
        COMPONENT_BEGIN,
        COMPONENT_END,
        lute_manifest::core::LABEL_DIRECTIVE,
        lute_manifest::core::JUMP_DIRECTIVE,
    ]
    .contains(&d.tag.as_str())
}

/// Walk one [`is_flow_directive`] node `d`; `rest` is what follows it in its
/// body. The one place a label is bound and a jump target is named, so every
/// body (scene shots, arms, handler/objective/entry/beat bodies, a quest's
/// top level) scopes component labels the same way (#207).
fn walk_flow_directive(
    em: &mut Emitter,
    node: &Node,
    d: &lute_syntax::ast::Directive,
    rest: &[Node],
    mut state: StageState,
    cx: &mut WalkCx<'_>,
) -> StageState {
    match d.tag.as_str() {
        COMPONENT_BEGIN => {
            let name = component_attr(d);
            em.marker(|| boundary_marker(d, &name));
            cx.stage.enter_component(&name);
            cx.components.push((
                name,
                component_scope(d).to_string(),
                crate::normalize::component_stable(d),
            ));
        }
        COMPONENT_END => {
            let name = cx.components.pop().map(|(name, _, _)| name);
            cx.stage.leave_component();
            em.marker(|| boundary_marker(d, name.as_deref().unwrap_or_default()));
        }
        // dsl 0.12.0: `::label{name}` emits NO record — bind the author's
        // NAMED label to whatever gets pushed NEXT (or, nothing left in this
        // body, `Emitter::finish`'s `trailing_named` — mirrors a branch/match
        // converge's `em.bind`, keyed by the author's string instead of a
        // fresh anonymous `Label`). dsl 0.51.0 §3.4: the stage state there is
        // the checker's. Inside a component expansion the name is the
        // instance's own ([`label_key`], #207).
        lute_manifest::core::LABEL_DIRECTIVE => {
            em.marker(|| directive_marker(d));
            if let Some(id) = attr_string(&d.attrs, lute_manifest::core::LABEL_NAME_ATTR) {
                bind_label(cx, &id, rest, &mut state);
                em.bind_named(label_key(cx, &id));
            }
        }
        _ => {
            state = emit_primitive(em, node, state, &[], cx, None);
            scope_jump_target(em, cx);
            mark_hub_leave(em, d, cx);
            route_jump(cx, d, &mut state);
        }
    }
    state
}

/// Walk one node sequence in document order, emitting records into `em` and
/// threading `StageState` through `lower_node` (injection) + branch/match
/// fork/join (D9). Timeline nodes are handled inline in Task 10.
pub fn walk_seq(
    em: &mut Emitter,
    nodes: &[Node],
    mut state: StageState,
    cx: &mut WalkCx<'_>,
    tail: &[Node],
    diags: &mut Vec<Diagnostic>,
) -> StageState {
    for (i, node) in nodes.iter().enumerate() {
        match node {
            Node::Directive(d) if is_flow_directive(d) => {
                state = walk_flow_directive(em, node, d, &nodes[i + 1..], state, cx);
            }
            // Fact args are ground — nothing authored, `lower_node`'s
            // wildcard skips them; lowering to `Command::Assert`/`Retract`
            // is Task 14.
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Assert(_)
            | Node::Retract(_) => {
                // Only an `::actor` entrance consumes the lookahead
                // (`entry-emotion-lookahead`); build the CFG-reachable
                // continuation just for it and pass nothing otherwise, so the
                // common line/set path never clones the tail.
                let look = if classify_node(node, cx.snapshot).is_actor() {
                    reachable_after(&nodes[i + 1..], tail)
                } else {
                    Vec::new()
                };
                state = emit_primitive(em, node, state, &look, cx, None);
                // dsl 0.52.0 §5.4: inside a hub body an unguarded `::end`
                // ends the route (the body does not complete).
                if let Node::Directive(d) = node {
                    if cx.reached && cx.stage.ends(d) {
                        cx.stage.diags.append(&mut state.diags);
                        cx.reached = false;
                    }
                }
            }
            Node::Branch(b) => {
                let cont = reachable_after(&nodes[i + 1..], tail);
                state = walk_branch(em, b, state, cx, &cont, diags);
            }
            Node::Match(m) => {
                let cont = reachable_after(&nodes[i + 1..], tail);
                state = walk_match(em, m, state, cx, &cont, diags);
            }
            Node::Timeline(tl) => {
                let cont = reachable_after(&nodes[i + 1..], tail);
                state = walk_timeline(em, tl, state, cx, &cont);
            }
            Node::Hub(h) => {
                let cont = reachable_after(&nodes[i + 1..], tail);
                state = walk_hub(em, h, state, cx, &cont, diags);
            }
            // dsl 0.2.0 §6.7: `<on>`/`<objective>` are QUEST-BODY-LEVEL
            // ONLY — grammar admission forbids them inside a scene body AND
            // inside any NESTED (`Emittable`) body within a quest (a
            // branch/match arm, an on/objective body). `walk_quest` drives
            // the quest-body-level walk directly (never through this
            // generic `walk_seq`), so this arm is structurally unreachable
            // for every `walk_seq` call site (IR addendum §6).
            Node::On(_) | Node::Objective(_) => unreachable!(
                "dsl 0.2.0 §6.7: <on>/<objective> are quest-body-level only; \
                 walk_seq is never called with a node list that can contain them"
            ),
        }
    }
    state
}

/// dsl 0.51.0 §3.4: the stage state at label `id` (`rest` follows it in its
/// body), from the checker's label and loop-head function: the walk's own
/// state joined with the jumps that reach the label, iterated at a loop head
/// over its region, which at the top level runs on into the later sections
/// (`StageFlow::top_level`, dsl 0.53.0 §9.3).
fn bind_label(cx: &mut WalkCx<'_>, id: &str, rest: &[Node], state: &mut StageState) {
    let fall = cx.reached.then(|| std::mem::take(state));
    if let Some(bound) = cx.stage.label(id, rest, fall) {
        *state = bound;
        cx.reached = true;
    }
}

/// dsl 0.51.0 §3.4: route `::jump` `d`'s state to its label; after an
/// unguarded jump without `max` nothing is reached.
fn route_jump(cx: &mut WalkCx<'_>, d: &Directive, state: &mut StageState) {
    if !cx.reached {
        return;
    }
    let mut routed = Some(std::mem::take(state));
    cx.stage.jump(d, &mut routed);
    match routed {
        Some(kept) => *state = kept,
        None => cx.reached = false,
    }
}

/// §5 pass 5, inline: schedule via `lute-check::timeline` math, thread every
/// clip through the SAME reducer in `(at, track)` order, stamp
/// `timeline`/`at`(+`duration`) on every emitted record (injected ones too),
/// append the `barrier`, and carry the post-barrier state forward. Ordering
/// is load-bearing: the node AFTER the timeline injects from the timeline's
/// resulting stage, never stale pre-timeline state.
fn walk_timeline(
    em: &mut Emitter,
    tl: &Timeline,
    mut state: StageState,
    cx: &mut WalkCx<'_>,
    cont: &[Node],
) -> StageState {
    // dsl 0.37.0 §5.2: `timing.timeline` is a zero-based ordinal.
    let ordinal = cx.timelines;
    cx.timelines += 1;
    let (clips, barrier_at) = {
        let ctx = Ctx {
            env: cx.env,
            in_match: false,
            match_subject: None,
        };
        schedule_timeline(tl, &ctx, cx.snapshot)
    };
    for sc in &clips {
        let node = match sc.node {
            ClipNode::Directive(d) => Node::Directive(d.clone()),
            ClipNode::Set(s) => Node::Set(s.clone()),
        };
        // A scheduled `::actor` entrance consumes the CFG-reachable
        // continuation for `entry-emotion-lookahead`, exactly like a linear
        // `::actor` (T9): clips carry no prose `:line`s, so the post-timeline
        // continuation is the whole lookahead. Every other clip takes none.
        let look: &[Node] = if classify_node(&node, cx.snapshot).is_actor() {
            cont
        } else {
            &[]
        };
        state = emit_primitive(
            em,
            &node,
            state,
            look,
            cx,
            Some(ClipStamp {
                timeline: ordinal,
                at: sc.at,
                duration: sc.duration,
            }),
        );
    }
    em.push(
        Command::Barrier(BarrierCmd {
            position: String::new(),
            timeline: ordinal,
            at: barrier_at,
        }),
        || SourceInfo::at(tl.span),
    );
    state
}

/// D9 lookahead / continuation: the CFG-reachable LINEAR successors of a node
/// or block — the rest of THIS sequence, then the enclosing continuation
/// (`tail`: everything reachable AFTER this sequence converges). A `<branch>`/
/// `<match>` node stays opaque here — sibling arms are unreachable, and the
/// emotion scan (`first_emotion_for`) walks only top-level `Node::Line`s, so it
/// skips fork nodes and resumes at their post-convergence successors.
fn reachable_after(rest: &[Node], tail: &[Node]) -> Vec<Node> {
    let mut out = Vec::with_capacity(rest.len() + tail.len());
    out.extend_from_slice(rest);
    out.extend_from_slice(tail);
    out
}

/// Lower one primitive node into records, threading the injection reducer:
/// `lower_node` computes the next stage state + this node's injected commands,
/// each emitted as a SEPARATE `sprite` record with provenance (§7.4). A node
/// no route reaches (dsl 0.51.0 §3.4) is emitted as authored, with no
/// injection and no state change, as `check()` stages nothing there.
fn emit_primitive(
    em: &mut Emitter,
    node: &Node,
    state: StageState,
    lookahead: &[Node],
    cx: &mut WalkCx<'_>,
    clip: Option<ClipStamp>,
) -> StageState {
    // Pure reducer step (arch #2): next stage state + this node's injections.
    let (next, injected) = if cx.reached {
        lower_node_with_snapshot(state, node, lookahead, &cx.env.domains, cx.snapshot)
    } else {
        (state, Vec::new())
    };
    let authored = match node {
        Node::Line(l) => Some(lower_line(l, cx.snapshot)),
        Node::Directive(d) => lower_directive(d, cx.snapshot, &cx.env.domains),
        Node::Set(s) => Some(lower_set(s)),
        Node::Assert(a) => Some(lower_assert(a)),
        Node::Retract(r) => Some(lower_retract(r)),
        _ => None,
    };
    // A directive that lowers to no record of its own (`::clear`, whose
    // exits are injected; `::use`) is still a step of the walk.
    if let (Node::Directive(d), None) = (node, &authored) {
        em.marker(|| directive_marker(d));
    }
    let span = node_span(node);
    // Placement (plan spec-gap note 4): an `::actor`'s injections (anchor,
    // preload) FOLLOW the authored show (§4.5); a line's posReset and a
    // scene-change's hides PRECEDE theirs.
    let auto_first = classify_node(node, cx.snapshot).is_actor();
    if auto_first {
        if let Some(cmd) = authored {
            emit_authored(em, cmd, node, cx, clip);
        }
        for ic in &injected {
            emit_stamped(em, inject_cmd(ic), cx, clip, || injected_origin(span));
        }
    } else {
        // dsl 0.24.0 §4: `::clear` lowers to its exits alone; the first one
        // carries the directive as authored, so `lute play` prints `::clear`
        // once where it happened (an empty stage emits nothing to print).
        let mut clear = match node {
            Node::Directive(d) if d.tag == lute_manifest::core::CLEAR_DIRECTIVE => {
                Some(crate::lower::authored_directive(d, None))
            }
            _ => None,
        };
        // A pending `::label` binds to the authored record, past the
        // injections emitted ahead of it (the old line `id=` did the same).
        let deferred = if authored.is_some() {
            em.defer_named()
        } else {
            Vec::new()
        };
        for ic in &injected {
            let mut cmd = inject_cmd(ic);
            if let (Some(text), Some(stamp)) = (clear.take(), cmd.stamp_mut()) {
                stamp.authored = Some(text);
            }
            emit_stamped(em, cmd, cx, clip, || injected_origin(span));
        }
        em.restore_named(deferred);
        if let Some(cmd) = authored {
            emit_authored(em, cmd, node, cx, clip);
        }
    }
    next
}

/// Push the record `node` lowered to, with where it came from.
fn emit_authored(
    em: &mut Emitter,
    cmd: Command,
    node: &Node,
    cx: &WalkCx<'_>,
    clip: Option<ClipStamp>,
) {
    let sugar = matches!(node, Node::Set(s) if em.is_into_sugar(&s.path, s.span));
    let authored_jump = matches!(cmd, Command::Jump(_));
    emit_stamped(em, cmd, cx, clip, || {
        let mut info = SourceInfo::at(node_span(node));
        info.sugar = sugar;
        info.authored_jump = authored_jump;
        match node {
            Node::Directive(d) => info.directive = Some(d.tag.clone()),
            Node::Set(s) => {
                info.write_text = Some(format!("{} {} {}", s.path, s.op, s.expr.raw.trim()));
            }
            Node::Assert(a) => info.write_text = Some(fact_text(&a.pattern)),
            Node::Retract(r) => info.write_text = Some(fact_text(&r.pattern)),
            _ => {}
        }
        info
    });
}

fn injected_origin(span: Span) -> SourceInfo {
    SourceInfo {
        injected: true,
        ..SourceInfo::at(span)
    }
}

/// The span of `node`.
fn node_span(node: &Node) -> Span {
    match node {
        Node::Line(l) => l.span,
        Node::Directive(d) => d.span,
        Node::Set(s) => s.span,
        Node::Assert(a) => a.span,
        Node::Retract(r) => r.span,
        Node::Branch(b) => b.span,
        Node::Match(m) => m.span,
        Node::Hub(h) => h.span,
        Node::Timeline(t) => t.span,
        Node::On(o) => o.span,
        Node::Objective(o) => o.span,
    }
}

/// A source-only step for directive `d`.
fn directive_marker(d: &Directive) -> SourceMarker {
    SourceMarker {
        tag: d.tag.clone(),
        span: d.span,
        component: None,
        name: None,
    }
}

/// The step a component sentinel `d` of component `name` is: the
/// expansion's begin/end, or — a template's `::body` split — where the
/// `<beat use>`'s own body starts and ends.
fn boundary_marker(d: &Directive, name: &str) -> SourceMarker {
    let component = match (d.tag == COMPONENT_BEGIN, is_body_split(d)) {
        (true, false) => ComponentBoundary::Begin,
        (false, false) => ComponentBoundary::End,
        (false, true) => ComponentBoundary::Body,
        (true, true) => ComponentBoundary::BodyEnd,
    };
    SourceMarker {
        tag: d.tag.clone(),
        span: d.span,
        component: Some(component),
        name: Some(name.to_string()),
    }
}

fn emit_stamped(
    em: &mut Emitter,
    mut cmd: Command,
    cx: &WalkCx<'_>,
    clip: Option<ClipStamp>,
    origin: impl FnOnce() -> SourceInfo,
) {
    apply_source(&mut cmd, cx);
    apply_clip(&mut cmd, clip);
    em.push(cmd, origin);
}

/// `InjectKind` → a SEPARATE `actor` record with provenance (§7.4).
fn inject_cmd(ic: &InjectedCommand) -> Command {
    let stamp = Stamp {
        provenance: Some(ic.provenance.clone()),
        ..Stamp::default()
    };
    let actor = |character: &str| ActorCmd {
        position: String::new(),
        character: character.to_string(),
        anchor: None,
        action: None,
        exit: None,
        emotion: None,
        costume: None,
        pos_reset: None,
        preload: None,
        stamp,
    };
    Command::Actor(match &ic.kind {
        InjectKind::Anchor { character, anchor } => ActorCmd {
            anchor: Some(anchor.clone()),
            ..actor(character)
        },
        InjectKind::PosReset { character } => ActorCmd {
            pos_reset: Some(true),
            ..actor(character)
        },
        InjectKind::SpriteLoad { character, emotion } => ActorCmd {
            preload: Some(true),
            emotion: Some(emotion.clone()),
            ..actor(character)
        },
        InjectKind::Hide { character } => ActorCmd {
            exit: Some(true),
            ..actor(character)
        },
    })
}

/// dsl 0.52.0 §6.2: the placeholders of a `<branch>`/`<hub>` `prompt`
/// (`attrs`, its residual list), scanned as an option label's are (spanned
/// at the prompt's value); none when the prompt is absent or has no marker.
fn prompt_placeholders(attrs: &[lute_syntax::ast::Attr]) -> Vec<crate::ir::Placeholder> {
    attrs
        .iter()
        .find(|a| a.key == "prompt")
        .map(|a| match &a.value {
            AttrValue::Str(s) => lute_syntax::scan_label_interps(s, a.value_span)
                .iter()
                .map(placeholder_from_interp)
                .collect(),
            AttrValue::Ref(_) | AttrValue::BoolTrue => Vec::new(),
        })
        .unwrap_or_default()
}

fn walk_branch(
    em: &mut Emitter,
    b: &Branch,
    state: StageState,
    cx: &mut WalkCx<'_>,
    tail: &[Node],
    diags: &mut Vec<Diagnostic>,
) -> StageState {
    let conv = em.fresh();
    let arms: Vec<Label> = b.choices.iter().map(|_| em.fresh()).collect();
    let options = b
        .choices
        .iter()
        .zip(&arms)
        .map(|(c, l)| ChoiceOption {
            id: c.id.clone(),
            text: c.text.clone(),
            line_id: String::new(),
            when: c.when.as_ref().map(CelPair::from_slot),
            target: l.sym(),
            placeholders: lute_syntax::scan_label_interps(&c.text, c.span)
                .iter()
                .map(placeholder_from_interp)
                .collect(),
            texts: Default::default(),
            stamps: element_stamps(&c.attrs, cx.snapshot, &StampSurface::Choice),
        })
        .collect();
    let timeout = attr_string(&b.attrs, "timeout").and_then(|s| s.parse::<u32>().ok());
    let mut cmd = Command::Choice(ChoiceCmd {
        position: String::new(),
        branch_id: b.id.clone(),
        selection_key: format!("scene.choices.{}", b.id),
        options,
        converge: conv.sym(),
        prompt: attr_string(&b.attrs, "prompt"),
        // dsl 0.47.0 §3.1: stamped by the addressing pass with the options'.
        prompt_line_id: None,
        prompt_texts: Default::default(),
        prompt_placeholders: prompt_placeholders(&b.attrs),
        timeout,
        default: timeout.and_then(|_| attr_string(&b.attrs, "default")),
        stamp: Stamp {
            stamps: element_stamps(&b.attrs, cx.snapshot, &StampSurface::Branch),
            ..Stamp::default()
        },
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo {
        arms: b.choices.iter().map(ArmSource::choice).collect(),
        ..SourceInfo::at(b.span)
    });
    // Fork (D9): every arm starts from the ENTRY state. Entry diagnostics are
    // drained first so per-arm clones don't duplicate them.
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let reached = cx.reached;
    let mut exits = Vec::with_capacity(b.choices.len());
    for (c, l) in b.choices.iter().zip(&arms) {
        em.bind(*l);
        let into = into_attr(c);
        em.enter_into(into.clone());
        cx.reached = reached;
        let exit = walk_seq(em, &c.body, state.clone(), cx, tail, diags);
        em.leave_into(into.as_ref());
        em.push(
            Command::Jump(JumpCmd {
                position: String::new(),
                target: conv.sym(),
                max: None,
                leaves: None,
            }),
            || SourceInfo::at(b.span),
        );
        if cx.reached {
            exits.push(exit);
        }
    }
    em.bind(conv);
    cx.reached = !exits.is_empty() || (reached && b.choices.is_empty());
    let mut joined = StageState::join(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
    joined
}

fn walk_hub(
    em: &mut Emitter,
    h: &Hub,
    state: StageState,
    cx: &mut WalkCx<'_>,
    tail: &[Node],
    diags: &mut Vec<Diagnostic>,
) -> StageState {
    // The hub id is the `id` attr (no dedicated AST field, unlike a branch).
    let id = attr_string(&h.attrs, "id").unwrap_or_default();
    let conv = em.fresh();
    let arms: Vec<Label> = h.choices.iter().map(|_| em.fresh()).collect();
    // dsl 0.28.0 §5: the `<return>` segment's label, after every option body.
    let back = h.on_return.as_ref().map(|_| em.fresh());
    let options = h
        .choices
        .iter()
        .zip(&arms)
        .map(|(c, l)| HubOption {
            id: c.id.clone(),
            text: c.text.clone(),
            line_id: String::new(),
            once: attr_bool(&c.attrs, "once").unwrap_or(false),
            exit: attr_bool(&c.attrs, "exit").unwrap_or(false),
            when: c.when.as_ref().map(CelPair::from_slot),
            target: l.sym(),
            placeholders: lute_syntax::scan_label_interps(&c.text, c.span)
                .iter()
                .map(placeholder_from_interp)
                .collect(),
            texts: Default::default(),
            stamps: element_stamps(&c.attrs, cx.snapshot, &StampSurface::Choice),
        })
        .collect();
    let mut cmd = Command::Hub(HubCmd {
        position: String::new(),
        id: id.clone(),
        selection_key: format!("scene.choices.{id}"),
        options,
        converge: conv.sym(),
        // dsl 0.23.0 §4: `<hub prompt>`, like `<branch prompt>`.
        prompt: attr_string(&h.attrs, "prompt"),
        prompt_line_id: None,
        prompt_texts: Default::default(),
        prompt_placeholders: prompt_placeholders(&h.attrs),
        on_return: back.map(super::cfg::Label::sym),
        stamp: Stamp {
            stamps: element_stamps(&h.attrs, cx.snapshot, &StampSurface::Hub),
            ..Stamp::default()
        },
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo {
        arms: h.choices.iter().map(ArmSource::choice).collect(),
        ..SourceInfo::at(h.span)
    });
    // dsl 0.52.0 §5.4: every option starts from the hub HEAD, the checker's
    // fixed point of the hub's re-presentation edge ([`StageFlow::hub`]);
    // entry diagnostics are drained first so per-arm clones don't duplicate
    // them (as in `walk_branch`).
    let mut state = state;
    let mut diags_out = std::mem::take(&mut state.diags);
    let head = cx.stage.hub(h, cx.reached.then_some(state));
    let reached = head.is_some();
    let start = head.clone().unwrap_or_default();
    let mut completed = Vec::new();
    let mut exits = Vec::new();
    for (c, l) in h.choices.iter().zip(&arms) {
        em.bind(*l);
        let into = into_attr(c);
        em.enter_into(into.clone());
        cx.reached = reached;
        cx.stage.enter_hub_body();
        let mut exit = walk_seq(em, &c.body, start.clone(), cx, tail, diags);
        cx.stage.leave_hub_body();
        em.leave_into(into.as_ref());
        diags_out.append(&mut exit.diags);
        // Flat-VM contract (A2 §7): an EXIT arm ends in a forward Jump→converge,
        // exactly like a `<choice>` arm; a NON-exit arm emits NO trailing jump —
        // its completion returns control to the hub loop head, a RUNTIME property
        // of the `hub` kind. No backward jump is emitted (D2/§3.2 stays flat,
        // forward-only, "reduces to data").
        let is_exit = StageFlow::exit_option(c);
        if is_exit {
            em.push(
                Command::Jump(JumpCmd {
                    position: String::new(),
                    target: conv.sym(),
                    max: None,
                    leaves: None,
                }),
                || SourceInfo::at(h.span),
            );
        }
        if cx.reached {
            if is_exit {
                exits.push(exit);
            } else {
                completed.push(exit);
            }
        }
    }
    // dsl 0.28.0 §5: the `<return>` segment, bounded like an option body and
    // likewise ending in NO jump — the runtime runs it after a non-`exit`
    // option's segment, then judges and presents the hub again. dsl 0.52.0
    // §5.4: it starts from the completion of the non-`exit` options, and no
    // route reaches it when none completes.
    if let (Some(r), Some(l)) = (&h.on_return, back) {
        let entry = StageFlow::completion(completed);
        em.bind(l);
        cx.reached = entry.is_some();
        cx.stage.enter_hub_body();
        let mut exit = walk_seq(em, &r.body, entry.unwrap_or_default(), cx, tail, diags);
        cx.stage.leave_hub_body();
        diags_out.append(&mut exit.diags);
    }
    em.bind(conv);
    // After the hub: `join(H, [H, the exit options' exit states])`, `H`
    // standing for the presentation with nothing eligible.
    let joined = match head {
        Some(head) => {
            let mut all = vec![head.clone()];
            all.extend(exits);
            Some(StageState::join(&head, all))
        }
        None => StageFlow::completion(exits),
    };
    cx.reached = joined.is_some();
    let mut joined = joined.unwrap_or_default();
    diags_out.append(&mut joined.diags);
    joined.diags = diags_out;
    joined
}

fn walk_match(
    em: &mut Emitter,
    m: &Match,
    state: StageState,
    cx: &mut WalkCx<'_>,
    tail: &[Node],
    diags: &mut Vec<Diagnostic>,
) -> StageState {
    let conv = em.fresh();
    let labels: Vec<Label> = m.arms.iter().map(|_| em.fresh()).collect();
    let mut arms = Vec::new();
    let mut otherwise = None;
    for (arm, l) in m.arms.iter().zip(&labels) {
        match arm {
            Arm::When { is, test, span, .. } => {
                let is_raw = is.as_ref().map(|p| p.raw.as_str());
                // An `is` literal compares against the subject; with none there
                // is nothing to compare, and an engine would read the arm as
                // unknown. The checker refuses this first.
                if let (Some(p), true) = (is, m.subject.raw.trim().is_empty()) {
                    diags.push(arm_diag(
                        "E-MATCH-NO-SUBJECT",
                        format!(
                            "`is=\"{}\"` compares against the `<match subject>`; this \
                             `<match>` has none — add `subject=`, or write `test=`",
                            p.raw.trim()
                        ),
                        p.span,
                    ));
                    continue;
                }

                let subject_is_double = cx
                    .env
                    .state
                    .get(m.subject.raw.trim())
                    .is_some_and(|decl| matches!(decl.ty, lute_manifest::types::Type::Double));
                let pair = match crate::expr::synth_arm_expr_typed(
                    is_raw,
                    &test.raw,
                    &m.subject.raw,
                    subject_is_double,
                ) {
                    crate::expr::ArmExpr::Lowered(Some(expr)) if is_raw.is_some() => {
                        let authored = is_raw.map(|raw| {
                            if test.raw.trim().is_empty() {
                                format!("is={raw}")
                            } else {
                                format!("is={raw} test={}", test.raw.trim())
                            }
                        });
                        CelPair::from_expr(expr, authored)
                    }
                    crate::expr::ArmExpr::Lowered(Some(_)) => CelPair::from_slot(test),
                    crate::expr::ArmExpr::Lowered(None) => {
                        diags.push(arm_diag(
                            "E-COMPILE-INTERNAL",
                            format!(
                                "checked <match subject=\"{}\"> arm condition did not lower to \
                                 portable CEL",
                                m.subject.raw.trim()
                            ),
                            *span,
                        ));
                        continue;
                    }
                    crate::expr::ArmExpr::UnsetOnCompoundSubject => {
                        diags.push(arm_diag(
                            "E-WHEN-UNSET-SUBJECT",
                            "`<when is=\"unset\">` on a non-path <match> subject \
                             cannot be lowered to an executable expr (dsl §7.3.1 / \
                             IR A13)"
                                .to_string(),
                            *span,
                        ));
                        continue;
                    }
                };
                arms.push(MatchArm {
                    is: is_raw.map(|raw| raw.trim().to_string()),
                    test: pair,
                    target: l.sym(),
                });
            }
            Arm::Otherwise { .. } => otherwise = Some(l.sym()),
        }
    }
    // dsl 0.26.0 §4: a guarded `::use`'s match reports the use as authored.
    let authored = m.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
        (crate::normalize::AUTHORED_ATTR, AttrValue::Str(s)) => Some(s.clone()),
        _ => None,
    });
    // dsl 0.28.0 §3: the per-member dispatch of a `::use{… who=
    // occasion.target}` is the compiler's, not a decision the author wrote.
    let target_use = m
        .attrs
        .iter()
        .any(|a| a.key == crate::normalize::TARGET_USE_ATTR);
    let mut cmd = Command::Match(MatchCmd {
        position: String::new(),
        subject: (!m.subject.raw.trim().is_empty()).then(|| CelPair::from_slot(&m.subject)),
        arms,
        otherwise,
        converge: conv.sym(),
        stamp: Stamp {
            authored,
            provenance: target_use.then(|| lute_check::Provenance {
                by: "occasion-target-use".to_string(),
                explanation: "plays the `::use` for the member `occasion.target` is bound to"
                    .to_string(),
            }),
            ..Stamp::default()
        },
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo {
        authored_id: m.subject.authored.clone(),
        arms: m
            .arms
            .iter()
            .map(|arm| match arm {
                Arm::When { is, test, span, .. } => ArmSource::when(*span, is.as_ref(), test),
                Arm::Otherwise { span, .. } => ArmSource::otherwise(*span),
            })
            .collect(),
        guard: is_guard_desugar(m),
        component: component_use(cx),
        injected: target_use,
        ..SourceInfo::at(m.span)
    });
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let reached = cx.reached;
    let mut exits = Vec::with_capacity(m.arms.len());
    for (arm, l) in m.arms.iter().zip(&labels) {
        let body = match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
        };
        em.bind(*l);
        cx.reached = reached;
        let exit = walk_seq(em, body, state.clone(), cx, tail, diags);
        em.push(
            Command::Jump(JumpCmd {
                position: String::new(),
                target: conv.sym(),
                max: None,
                leaves: None,
            }),
            || SourceInfo::at(m.span),
        );
        if cx.reached {
            exits.push(exit);
        }
    }
    em.bind(conv);
    cx.reached = !exits.is_empty() || (reached && m.arms.is_empty());
    let mut joined = StageState::join(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
    joined
}

/// A choice's `into="<path>"` attr — the `::set` normalize synthesized
/// from it carries this attr's span (`normalize::synth_into`).
fn into_attr(c: &lute_syntax::ast::Choice) -> Option<(String, Span)> {
    c.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
        ("into", AttrValue::Str(p)) => Some((p.clone(), a.span)),
        _ => None,
    })
}

/// Is `m` the one-arm match a `when=` guard desugars to
/// ([`crate::normalize::guard_match`])? Its arms carry the match's own span;
/// an authored arm is an element of its own, never spanning its `<match>`.
fn is_guard_desugar(m: &Match) -> bool {
    matches!(
        m.arms.as_slice(),
        [Arm::When { is: None, span: when, .. }, Arm::Otherwise { body, span: other, .. }]
            if *when == m.span && *other == m.span && body.is_empty()
    )
}

/// A compile-stage Error at a `<when>` arm.
fn arm_diag(code: &str, message: String, span: lute_core_span::Span) -> Diagnostic {
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

/// `source { component }` from the sentinel-driven stack (§4.3, D8), plus
/// the identity scope the addressing pass mints the record's ids under.
fn apply_source(cmd: &mut Command, cx: &WalkCx<'_>) {
    if let Some(ComponentUse {
        name,
        scope,
        stable,
    }) = component_use(cx)
    {
        if let Some(stamp) = cmd.stamp_mut() {
            stamp.source = Some(Source {
                component: name,
                scope,
                stable,
            });
        }
    }
}

fn component_use(cx: &WalkCx<'_>) -> Option<ComponentUse> {
    let (name, _, _) = cx.components.last()?;
    let scope = cx
        .components
        .iter()
        .map(|(_, segment, _)| segment.as_str())
        .collect::<Vec<_>>()
        .join(".");
    let stable = cx.components.iter().all(|(_, _, stable)| *stable);
    Some(ComponentUse {
        name: name.clone(),
        scope,
        stable,
    })
}

/// The document-wide name of label `id` (#207): inside a component
/// expansion, qualified by the instance's scope path, so each `::use`
/// instance's jumps reach its own labels (the checker resolves a component's
/// labels in its own document). Outside a component, the authored name.
fn label_key(cx: &WalkCx<'_>, id: &str) -> String {
    match component_use(cx) {
        Some(ComponentUse { scope, .. }) => format!("{scope}::{id}"),
        None => id.to_string(),
    }
}

/// Qualify the `#<label>` target of the `jump` record just emitted the way
/// [`label_key`] names its label.
fn scope_jump_target(em: &mut Emitter, cx: &WalkCx<'_>) {
    if cx.components.is_empty() {
        return;
    }
    if let Some(Command::Jump(jump)) = em.recs.last_mut().map(|r| &mut r.cmd) {
        if let Some(id) = jump.target.strip_prefix('#') {
            jump.target = format!("#{}", label_key(cx, id));
        }
    }
}

/// dsl 0.52.0 §5.2: write the checker's `leaves` count
/// ([`lute_check::next_labels::HubLeaves`]) on the `jump` record just
/// emitted for the authored `::jump` `d`. A jump inside a component
/// expansion never leaves a hub (a component admits no `<hub>`), and its
/// span is in the component's file, so it is not looked up.
fn mark_hub_leave(em: &mut Emitter, d: &Directive, cx: &WalkCx<'_>) {
    if !cx.components.is_empty() {
        return;
    }
    let Some(leaves) = cx.leaves.of(d) else {
        return;
    };
    if let Some(Command::Jump(jump)) = em.recs.last_mut().map(|r| &mut r.cmd) {
        jump.leaves = Some(leaves);
    }
}

/// `timing.timeline`/`at`/`duration` on timeline-clip records (dsl 0.37.0 §5.2).
fn apply_clip(cmd: &mut Command, clip: Option<ClipStamp>) {
    let Some(c) = clip else { return };
    if let Some(stamp) = cmd.stamp_mut() {
        stamp.timing.timeline = Some(c.timeline);
        stamp.timing.at = Some(c.at);
        if c.duration > 0.0 {
            stamp.timing.duration = Some(c.duration);
        }
    }
}

fn component_attr(d: &Directive) -> String {
    d.attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// One `<reward>`'s wire entry, stamped with what its kind declares in
/// `snapshot` ([`RewardEntry::from_ast`]).
fn reward_entry(
    reward: &lute_syntax::ast::Reward,
    snapshot: &CapabilitySnapshot,
    owner_is_quest: bool,
) -> RewardEntry {
    let kind = snapshot.reward_kinds.get(reward.kind.trim());
    RewardEntry::from_ast(reward, kind, &snapshot.occasions, owner_is_quest)
}

/// Walk a `<quest>` declaration (dsl 0.2.0 §6.3, IR addendum §3): emits the
/// `quest` declaration head FIRST (objective table inlined, symbolic `body`
/// targets for non-empty objective bodies via `em.fresh()` — the quest
/// record must be the FIRST record, IR addendum §3.1), then walks the WHOLE
/// `quest.body` in document order — `<objective>`/`<on>` bind their
/// pre-allocated/fresh label and recurse into `walk_seq` for their body.
/// Nothing else sits at a quest's top level: a quest runs only its handler
/// and objective bodies, so the checker refuses any other node there (dsl
/// 0.52.0 §4; `<hub>`/`<timeline>` everywhere in a quest, dsl 0.2.0 §6.7)
/// and gates the document out (D6), so those arms are unreachable here.
pub fn walk_quest(
    em: &mut Emitter,
    quest: &Quest,
    cx: &mut WalkCx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    // Pass 1: pre-scan the quest body's TOP-LEVEL objectives (grammar
    // admission guarantees they appear only directly here, never nested,
    // dsl 0.2.0 §6.7) — allocate a fresh label for each non-empty body and
    // build the inlined `objectives[]` table BEFORE the quest record is
    // pushed.
    let mut obj_labels: Vec<Option<Label>> = Vec::new();
    let mut objectives: Vec<ObjectiveEntry> = Vec::new();
    for node in &quest.body {
        if let Node::Objective(o) = node {
            let label = (!o.body.is_empty()).then(|| em.fresh());
            objectives.push(ObjectiveEntry {
                id: o.id.clone(),
                title: o.title.as_ref().map(|(t, _)| t.clone()),
                title_line_id: o.title.as_ref().map(|_| format!("{}.{}", quest.id, o.id)),
                done: CelPair::from_slot(&o.done),
                visible_when: o.visible_when.as_ref().map(CelPair::from_slot),
                optional: o.optional,
                body: label.map(Label::sym),
                // Subquest reference (2026-08-31 subquest design §3): the
                // child id this objective is bound to. `normalize_document`
                // has already synthesized `done.raw` from it (§2.1 —
                // `quest.<child>.state == 'complete'`); the field itself
                // rides along so cross-artifact consumers can derive the
                // journal tree without re-parsing CEL. Preserved verbatim
                // for a non-subquest objective (`None`) — `serde` omits it
                // there, keeping the byte-stable output of every existing
                // artifact (see `ir::ObjectiveEntry.quest`).
                quest: o.quest.clone(),
                // dsl 0.16.0 §2/§3: owner-declared `<reward/>` entries in
                // declaration order, pure data (no handler synthesis — the
                // inverse of the 0.14.0 subquest pass above). Empty when
                // the objective authored no `<reward/>` — the wire field
                // is then omitted, preserving byte-identity for every
                // pre-0.16.0 objective (`skip_serializing_if`).
                rewards: o
                    .rewards
                    .iter()
                    .map(|r| reward_entry(r, cx.snapshot, false))
                    .collect(),
                // dsl 0.21.0 §7a.2: the occasion the objective is judged at;
                // `None` (omitted) keeps every other objective byte-identical.
                on: o.on.as_ref().map(|(on, _)| on.clone()),
                // dsl 0.23.0 §2: the deadline condition (expanded like
                // `done`) and the occasion target; both omitted when absent.
                by: o.by.as_ref().map(CelPair::from_slot),
                target: o.target.as_ref().map(|(t, _)| t.clone()),
                // dsl 0.24.0 §2.1: the place-bound deadline, omitted when absent.
                until: o.until.as_ref().map(CelPair::from_slot),
            });
            obj_labels.push(label);
        }
    }
    let mut cmd = Command::Quest(QuestCmd {
        position: String::new(),
        id: quest.id.clone(),
        title: quest.title.clone(),
        title_line_id: quest.title.as_ref().map(|_| format!("{}.title", quest.id)),
        start: quest.start.as_ref().map(CelPair::from_slot),
        fail: quest.fail.as_ref().map(CelPair::from_slot),
        objectives,
        // dsl 0.16.0 §2/§3: quest-level `<reward/>` entries in declaration
        // order. `outcome="failed"` is preserved here (only ever legal on a
        // quest-level entry per spec §2). Empty ⇒ field omitted, so a
        // rewardless quest artifact stays byte-identical to pre-0.16.0
        // output (see `ir::QuestCmd.rewards`).
        rewards: quest
            .rewards
            .iter()
            .map(|r| reward_entry(r, cx.snapshot, true))
            .collect(),
        // dsl 0.22.0 §7, 0.27.0 §5: only `tier="run"` / `"season:<name>"`
        // are serialized (`E-ATTR-TYPE` / `E-SEASON-DECL` gated the rest).
        tier: quest.tier.as_ref().and_then(|(t, _)| {
            if t == "run" {
                Some(crate::ir::QuestTier::Run)
            } else {
                lute_manifest::season::season_ref(t)
                    .map(|name| crate::ir::QuestTier::Season(name.to_string()))
            }
        }),
        // dsl 0.24.0 §2: only the non-default modes are serialized
        // (`E-ATTR-TYPE` already gated any other value).
        activate: quest
            .activates_on_accept()
            .then_some(crate::ir::QuestActivate::Accept),
        complete: quest
            .completes_on_any()
            .then_some(crate::ir::QuestComplete::Any),
        // dsl 0.25.0 §5: the engine may accept it outside any document.
        accept: quest
            .accepted_externally()
            .then_some(crate::ir::QuestAccept::External),
        // dsl 0.27.0 §5: back to `unset` when the condition goes false→true.
        rearm: quest.rearm.as_ref().map(CelPair::from_slot),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo::at(quest.span));

    // Pass 2: real document-order walk. Each objective-body / on-arm body is
    // an INDEPENDENT, temporally-disconnected trigger (not "one of N
    // alternatives now" like a branch/match arm) — `walk_body` starts it from
    // a FRESH `StageState::default()`, discarded afterward. Nothing else sits
    // at a quest's top level (dsl 0.52.0 §4, gated by the checker, D6).
    cx.reached = true;
    let mut obj_idx = 0usize;
    // Empty-`<on>`-body labels (F5, final review P1): `OnCmd.body` is a
    // REQUIRED target (String, not Option, IR addendum §3.3), so an EMPTY
    // `<on>` body still must resolve somewhere. It must NOT bind to
    // whatever record happens to follow it in this document-order walk
    // (that dangles the handler onto unrelated content) — it must resolve
    // to the quest unit's ONE-PAST-END converge, exactly like `walk_branch`
    // binds `conv` only after ALL arms are walked. So these labels are
    // collected here and bound ONLY after the whole pass-2 loop finishes;
    // with nothing left to push, they trail past the unit's last record and
    // `address::assign_addresses` resolves them all to that same
    // `addr_of(shot, recs.len())` past-end address.
    let mut empty_on_labels: Vec<Label> = Vec::new();
    for node in &quest.body {
        match node {
            Node::Objective(o) => {
                if let Some(label) = obj_labels[obj_idx] {
                    em.bind(label);
                    walk_body(em, &o.body, cx, diags);
                }
                obj_idx += 1;
            }
            Node::On(on) => {
                let label = em.fresh();
                let mut on_cmd = Command::On(OnCmd {
                    position: String::new(),
                    event: on.event.clone(),
                    when: on.when.as_ref().map(CelPair::from_slot),
                    body: label.sym(),
                    target: on.target.as_ref().map(|(t, _)| t.clone()),
                    stamp: Stamp::default(),
                });
                apply_source(&mut on_cmd, cx);
                em.push(on_cmd, || SourceInfo::at(on.span));
                if on.body.is_empty() {
                    empty_on_labels.push(label);
                } else {
                    em.bind(label);
                    walk_body(em, &on.body, cx, diags);
                }
            }
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Assert(_)
            | Node::Retract(_)
            | Node::Branch(_)
            | Node::Match(_)
            | Node::Timeline(_)
            | Node::Hub(_) => {
                unreachable!(
                    "dsl 0.52.0 §4: a quest's top level admits only <on>/<objective> \
                     — the checker gates this out (D6)"
                )
            }
        }
    }
    // All empty-`<on>`-body labels converge here, AFTER every top-level
    // sibling has been walked — the unit's one-past-end (F5).
    for label in empty_on_labels {
        em.bind(label);
    }
}

/// Walk one `<entry>` declaration (dsl 0.19.0 §4/§7): the `entry` head record
/// FIRST, then its body segment — addressed and terminated exactly as an
/// `<on>` body in [`walk_quest`]: a fresh label bound to the first body
/// record, the body walked by [`walk_seq`] from a FRESH `StageState` (an
/// entry is presented on its own, never threaded from a sibling), and an
/// EMPTY body's label left trailing so the addressing pass resolves it to
/// the entry unit's one-past-end. The checker admits only lines, `<match>`,
/// `::set`/`::assert`/`::retract` in an entry body (D6), so `walk_seq`'s
/// generic lowering is exactly right.
///
/// `series`/`order` are the entry's RESOLVED position
/// ([`lute_check::resolve_entry_series`], dsl 0.19.0 §2.1/§7) — a
/// document-level `series:` supplies them by place in the file; `order` is
/// the checker-validated non-negative integer (`E-ENTRY-ATTR` gates anything
/// else). `titleLineId` is `{entryId}.title`, the quest `titleLineId`
/// convention.
pub fn walk_entry(
    em: &mut Emitter,
    entry: &Entry,
    series: &lute_check::EntrySeries<'_>,
    cx: &mut WalkCx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let label = em.fresh();
    let text = |v: &Option<(String, lute_core_span::Span)>| v.as_ref().map(|(s, _)| s.clone());
    let advances = lute_check::advances_from_attr(entry.advances.as_ref(), &mut Vec::new());
    let mut cmd = Command::Entry(EntryCmd {
        position: String::new(),
        id: entry.id.clone(),
        target: text(&entry.target),
        category: text(&entry.category),
        title: text(&entry.title),
        title_line_id: entry.title.as_ref().map(|_| format!("{}.title", entry.id)),
        series: series.series.map(str::to_string),
        order: series.order,
        when: entry.when.as_ref().map(CelPair::from_slot),
        body: label.sym(),
        // dsl 0.21.0 §3.2: an entry beat's occasion + priority, verbatim /
        // parsed (`E-BEAT-ATTR` already gated a non-integer priority).
        on: text(&entry.on),
        priority: entry
            .priority
            .as_ref()
            .and_then(|(p, _)| lute_check::parse_beat_priority(p)),
        // dsl 0.22.0 §7, 0.24.0 §1, 0.27.0 §5: `once="run"|"user"|"day"|
        // "slot"|"week"|"season:<name>"` (`E-BEAT-ATTR` gated the rest). A
        // `spentBy` entry stays spent for its `once` period, `run` unless
        // written — recorded, as a bundle beat's is.
        once: match entry.once.as_ref() {
            Some((o, _)) => lute_ir::BeatOnce::parse(o),
            None => entry.spent_by.as_ref().map(|_| lute_ir::BeatOnce::Run),
        },
        share: text(&entry.share),
        target_kind: entry.on.as_ref().and_then(|(on, _)| {
            TargetKind::resolve(
                on,
                entry.target.as_ref().map(|(t, _)| t.as_str()),
                &cx.snapshot.occasions,
                &cx.env.rel_vocab.kinds,
            )
        }),
        for_kind: entry.on.as_ref().and_then(|(on, _)| {
            ForKind::resolve(
                on,
                entry.for_kind.as_ref().map(|(f, _)| f.as_str()),
                entry.target.is_some(),
                &cx.snapshot.occasions,
                &cx.env.rel_vocab.kinds,
            )
        }),
        // dsl 0.27.0 §5: already `@def`-expanded, like `when`.
        spent_by: entry.spent_by.as_ref().map(CelPair::from_slot),
        advances,
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo::at(entry.span));
    em.bind(label);
    walk_body(em, &entry.body, cx, diags);
}

/// Walk one bundle `<beat>` (dsl 0.23.0 §4): the `beat` head record FIRST,
/// then its body segment, addressed exactly like an entry's
/// ([`walk_entry`]) — a fresh label bound to the first body record (an empty
/// body's label trails to the unit's one-past-end). The body is a scene
/// body, walked by [`walk_seq`] from a FRESH `StageState`: a beat is staged
/// on its own, from an empty stage, as a scene is. `key` is the canonical
/// `<document id>.<beat id>`; `when` is already `@def`-expanded
/// (`expand_document`), and every attribute is checker-validated (D6), so
/// the resolution helpers' defaults never mask a fault.
pub fn walk_bundle_beat(
    em: &mut Emitter,
    beat: &BundleBeat,
    key: &str,
    cx: &mut WalkCx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let label = em.fresh();
    let title = beat.title.as_ref().map(|(t, _)| t.clone());
    let advances = lute_check::advances_from_attr(beat.advances.as_ref(), &mut Vec::new());
    let mut cmd = Command::Beat(BeatCmd {
        position: String::new(),
        id: key.to_string(),
        on: beat
            .on
            .as_ref()
            .map(|(on, _)| on.clone())
            .unwrap_or_default(),
        target: beat.target.as_ref().map(|(t, _)| t.clone()),
        title_line_id: title.as_ref().map(|_| format!("{key}.title")),
        title,
        when: beat.when.as_ref().map(CelPair::from_slot),
        priority: lute_check::bundle_beat_priority(beat),
        once: lute_check::bundle_beat_once(beat),
        also: lute_check::bundle_beat_also(beat),
        share: beat.share.as_ref().map(|(k, _)| k.clone()),
        after: beat.after.as_ref().map(|(a, _)| a.clone()),
        target_kind: beat.on.as_ref().and_then(|(on, _)| {
            TargetKind::resolve(
                on,
                beat.target.as_ref().map(|(t, _)| t.as_str()),
                &cx.snapshot.occasions,
                &cx.env.rel_vocab.kinds,
            )
        }),
        for_kind: beat.on.as_ref().and_then(|(on, _)| {
            ForKind::resolve(
                on,
                beat.for_kind.as_ref().map(|(f, _)| f.as_str()),
                beat.target.is_some(),
                &cx.snapshot.occasions,
                &cx.env.rel_vocab.kinds,
            )
        }),
        spent_by: beat.spent_by.as_ref().map(CelPair::from_slot),
        advances,
        body: label.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd, || SourceInfo::at(beat.span));
    em.bind(label);
    walk_body(em, &beat.body, cx, diags);
}

/// Walk a body the engine starts on its own (an objective or `<on>` body, a
/// lore entry, a bundle beat): from an empty stage, reached.
fn walk_body(em: &mut Emitter, body: &[Node], cx: &mut WalkCx<'_>, diags: &mut Vec<Diagnostic>) {
    let reached = std::mem::replace(&mut cx.reached, true);
    walk_seq(em, body, StageState::default(), cx, &[], diags);
    cx.reached = reached;
}
