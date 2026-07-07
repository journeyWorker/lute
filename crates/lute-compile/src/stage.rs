//! The document walker: flatten (this task) + CFG-aware stage resolution
//! (Task 9, D9) + inline timelines (Task 10). ONE walk owns emission order.

use lute_check::ctx::Env;
use lute_check::{lower_node, Ctx, InjectKind, InjectedCommand, StageState};
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, AttrValue, Branch, ClipNode, Directive, Hub, Match, Node, Timeline};

use crate::cfg::{Emitter, Label};
use crate::ir::*;
use crate::lower::{attr_bool, attr_string, lower_directive, lower_line, lower_set};
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};
use crate::schedule::schedule_timeline;

/// Walk context: the read-only capability surface + the component-source
/// stack (sentinel-driven) + the document-order timeline counter (Task 10).
pub struct WalkCx<'a> {
    pub snapshot: &'a CapabilitySnapshot,
    pub env: &'a Env,
    pub components: Vec<String>,
    pub timelines: u32,
}

/// Timeline-clip stamp for records emitted inside a `<timeline>` (Task 10).
#[derive(Clone, Copy)]
pub struct ClipStamp {
    pub timeline: u32,
    pub at: f64,
    pub duration: f64,
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
            Node::Directive(d) if d.tag == COMPONENT_BEGIN => {
                cx.components.push(component_attr(d));
            }
            Node::Directive(d) if d.tag == COMPONENT_END => {
                cx.components.pop();
            }
            Node::Line(_) | Node::Directive(_) | Node::Set(_) => {
                // Only an `::auto` entrance consumes the lookahead
                // (`entry-emotion-lookahead`); build the CFG-reachable
                // continuation just for it and pass nothing otherwise, so the
                // common line/set path never clones the tail.
                let look = if matches!(node, Node::Directive(d) if d.tag == "auto") {
                    reachable_after(&nodes[i + 1..], tail)
                } else {
                    Vec::new()
                };
                state = emit_primitive(em, node, state, &look, cx, None);
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
        }
    }
    state
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
    cx.timelines += 1;
    let ordinal = cx.timelines;
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
        // A scheduled `::auto` entrance consumes the CFG-reachable
        // continuation for `entry-emotion-lookahead`, exactly like a linear
        // `::auto` (T9): clips carry no prose `:line`s, so the post-timeline
        // continuation is the whole lookahead. Every other clip takes none.
        let look: &[Node] = if matches!(&node, Node::Directive(d) if d.tag == "auto") {
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
    em.push(Command::Barrier(BarrierCmd {
        addr: String::new(),
        timeline: ordinal,
        at: barrier_at,
    }));
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
/// each emitted as a SEPARATE `sprite` record with provenance (§7.4).
fn emit_primitive(
    em: &mut Emitter,
    node: &Node,
    state: StageState,
    lookahead: &[Node],
    cx: &mut WalkCx<'_>,
    clip: Option<ClipStamp>,
) -> StageState {
    // Pure reducer step (arch #2): next stage state + this node's injections.
    let (next, injected) = lower_node(state, node, lookahead);
    let authored = match node {
        Node::Line(l) => Some(lower_line(l)),
        Node::Directive(d) => lower_directive(d, cx.snapshot),
        Node::Set(s) => Some(lower_set(s)),
        _ => None,
    };
    // Placement (plan spec-gap note 4): an `::auto`'s injections (anchor,
    // preload) FOLLOW the authored show (§4.5); a line's posReset and a
    // scene-change's hides PRECEDE theirs.
    let auto_first = matches!(node, Node::Directive(d) if d.tag == "auto");
    if auto_first {
        if let Some(cmd) = authored {
            emit_stamped(em, cmd, cx, clip);
        }
        for ic in &injected {
            emit_stamped(em, inject_cmd(ic), cx, clip);
        }
    } else {
        for ic in &injected {
            emit_stamped(em, inject_cmd(ic), cx, clip);
        }
        if let Some(cmd) = authored {
            emit_stamped(em, cmd, cx, clip);
        }
    }
    next
}

fn emit_stamped(em: &mut Emitter, mut cmd: Command, cx: &WalkCx<'_>, clip: Option<ClipStamp>) {
    apply_source(&mut cmd, cx);
    apply_clip(&mut cmd, clip);
    em.push(cmd);
}

/// `InjectKind` → a SEPARATE `sprite` record with provenance (§7.4).
fn inject_cmd(ic: &InjectedCommand) -> Command {
    let stamp = Stamp {
        provenance: Some(ic.provenance.clone()),
        ..Stamp::default()
    };
    let sprite = |character: &str| SpriteCmd {
        addr: String::new(),
        character: character.to_string(),
        anchor: None,
        action: None,
        exit: None,
        pos_reset: None,
        preload: None,
        emotion: None,
        costume: None,
        stamp,
    };
    Command::Sprite(match &ic.kind {
        InjectKind::Anchor { character, anchor } => SpriteCmd {
            anchor: Some(anchor.clone()),
            ..sprite(character)
        },
        InjectKind::PosReset { character } => SpriteCmd {
            pos_reset: Some(true),
            ..sprite(character)
        },
        InjectKind::SpriteLoad { character, emotion } => SpriteCmd {
            preload: Some(true),
            emotion: Some(emotion.clone()),
            ..sprite(character)
        },
        InjectKind::Hide { character } => SpriteCmd {
            exit: Some(true),
            ..sprite(character)
        },
    })
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
            label: c.label.clone(),
            line_id: String::new(),
            when: c.when.as_ref().map(|w| w.raw.clone()),
            expr: c.when.as_ref().and_then(|w| crate::expr::lower_expr(&w.raw)),
            target: l.sym(),
            placeholders: lute_syntax::scan_label_interps(&c.label, c.span)
                .iter()
                .map(placeholder_from_interp)
                .collect(),
        })
        .collect();
    let mut cmd = Command::Choice(ChoiceCmd {
        addr: String::new(),
        branch_id: b.id.clone(),
        record_key: format!("scene.choices.{}", b.id),
        options,
        converge: conv.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd);
    // Fork (D9): every arm starts from the ENTRY state. Entry diagnostics are
    // drained first so per-arm clones don't duplicate them.
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let mut exits = Vec::with_capacity(b.choices.len());
    for (c, l) in b.choices.iter().zip(&arms) {
        em.bind(*l);
        let exit = walk_seq(em, &c.body, state.clone(), cx, tail, diags);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
        exits.push(exit);
    }
    em.bind(conv);
    let mut joined = join_states(&state, exits);
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
    let options = h
        .choices
        .iter()
        .zip(&arms)
        .map(|(c, l)| HubOption {
            id: c.id.clone(),
            label: c.label.clone(),
            line_id: String::new(),
            once: attr_bool(&c.attrs, "once").unwrap_or(false),
            exit: attr_bool(&c.attrs, "exit").unwrap_or(false),
            when: c.when.as_ref().map(|w| w.raw.clone()),
            expr: c.when.as_ref().and_then(|w| crate::expr::lower_expr(&w.raw)),
            target: l.sym(),
            placeholders: lute_syntax::scan_label_interps(&c.label, c.span)
                .iter()
                .map(placeholder_from_interp)
                .collect(),
        })
        .collect();
    let mut cmd = Command::Hub(HubCmd {
        addr: String::new(),
        id: id.clone(),
        record_key: format!("scene.choices.{id}"),
        options,
        converge: conv.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd);
    // Fork (D9): every arm starts from the ENTRY state; entry diagnostics are
    // drained first so per-arm clones don't duplicate them (as in `walk_branch`).
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let mut exits = Vec::with_capacity(h.choices.len());
    for (c, l) in h.choices.iter().zip(&arms) {
        em.bind(*l);
        let exit = walk_seq(em, &c.body, state.clone(), cx, tail, diags);
        // Flat-VM contract (A2 §7): an EXIT arm ends in a forward Jump→converge,
        // exactly like a `<choice>` arm; a NON-exit arm emits NO trailing jump —
        // its completion returns control to the hub loop head, a RUNTIME property
        // of the `hub` kind. No backward jump is emitted (D2/§3.2 stays flat,
        // forward-only, "reduces to data").
        if attr_bool(&c.attrs, "exit").unwrap_or(false) {
            em.push(Command::Jump(JumpCmd {
                addr: String::new(),
                target: conv.sym(),
            }));
        }
        exits.push(exit);
    }
    em.bind(conv);
    // Hub arms have no dominance relation (like `<match>` arms) — arm writes are
    // may-writes at hub exit; `join_states` applies the conservative join.
    let mut joined = join_states(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
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
            Arm::When {
                is, test, span, ..
            } => {
                let expr = match crate::expr::synth_arm_expr(
                    is.as_ref().map(|p| p.raw.as_str()),
                    &test.raw,
                    &m.subject.raw,
                ) {
                    crate::expr::ArmExpr::Lowered(expr) => expr,
                    crate::expr::ArmExpr::UnsetOnCompoundSubject => {
                        // A13 rule 5: `<when is="unset">` lowers to `!isSet(path)`,
                        // which needs a bare-path subject. A compound subject cannot
                        // be lowered — surface a compile error rather than silently
                        // dropping the arm (its `expr` stays `None`).
                        diags.push(Diagnostic {
                            code: "E-WHEN-UNSET-SUBJECT".to_string(),
                            severity: Severity::Error,
                            message: "`<when is=\"unset\">` on a non-path <match> subject \
                                      cannot be lowered to an executable expr (dsl §7.3.1 / \
                                      IR A13)"
                                .to_string(),
                            span: *span,
                            layer: Layer::Logic,
                            fixits: Vec::new(),
                            provenance: None,
                        });
                        None
                    }
                };
                arms.push(MatchArm {
                    test: test.raw.clone(),
                    target: l.sym(),
                    expr,
                });
            }
            Arm::Otherwise { .. } => otherwise = Some(l.sym()),
        }
    }
    let mut cmd = Command::Match(MatchCmd {
        addr: String::new(),
        subject: m.subject.raw.clone(),
        arms,
        otherwise,
        converge: conv.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd);
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let mut exits = Vec::with_capacity(m.arms.len());
    for (arm, l) in m.arms.iter().zip(&labels) {
        let body = match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
        };
        em.bind(*l);
        let exit = walk_seq(em, body, state.clone(), cx, tail, diags);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
        exits.push(exit);
    }
    em.bind(conv);
    let mut joined = join_states(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
    joined
}

/// §7.3 conservative convergence join. Per character: identical `SpriteState`
/// in EVERY arm → carried; differing or partial → dropped (that encodes
/// `Unknown`: a later plain line assumes no pose — no false posReset — and a
/// later `::auto` is a fresh show → anchor + preload). `dirty` survives for a
/// carried character if ANY surviving arm exit marks it dirty (a redundant
/// posReset beats a missing one — e.g. a `variant`/`dialogMotion`-only line
/// dirties without changing `SpriteState`); `bg`/`music` carry only when
/// identical across arms. Exits' diagnostics concatenate in arm order.
pub fn join_states(entry: &StageState, mut exits: Vec<StageState>) -> StageState {
    let Some(first) = exits.first().cloned() else {
        return entry.clone();
    };
    let mut joined = StageState::default();
    for e in &mut exits {
        joined.diags.append(&mut e.diags);
    }
    'chars: for (ch, sprite) in &first.on_stage {
        for e in &exits[1..] {
            if e.on_stage.get(ch) != Some(sprite) {
                continue 'chars;
            }
        }
        joined.on_stage.insert(ch.clone(), sprite.clone());
    }
    let kept: Vec<String> = joined.on_stage.keys().cloned().collect();
    for ch in kept {
        if exits.iter().any(|e| e.dirty.contains(&ch)) {
            joined.dirty.insert(ch);
        }
    }
    joined.bg = if exits.iter().all(|e| e.bg == first.bg) {
        first.bg.clone()
    } else {
        None
    };
    joined.music = if exits.iter().all(|e| e.music == first.music) {
        first.music.clone()
    } else {
        None
    };
    joined
}

/// `source { component }` from the sentinel-driven stack (§4.3, D8).
fn apply_source(cmd: &mut Command, cx: &WalkCx<'_>) {
    if let Some(name) = cx.components.last() {
        if let Some(stamp) = cmd.stamp_mut() {
            stamp.source = Some(Source {
                component: name.clone(),
            });
        }
    }
}

/// `timeline`/`at`/`duration` stamps on timeline-clip records (§4.3, Task 10).
fn apply_clip(cmd: &mut Command, clip: Option<ClipStamp>) {
    let Some(c) = clip else { return };
    if let Some(stamp) = cmd.stamp_mut() {
        stamp.timeline = Some(c.timeline);
        stamp.at = Some(c.at);
        if c.duration > 0.0 {
            stamp.duration = Some(c.duration);
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
