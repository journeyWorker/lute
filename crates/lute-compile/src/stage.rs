//! The document walker: flatten (this task) + CFG-aware stage resolution
//! (Task 9, D9) + inline timelines (Task 10). ONE walk owns emission order.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, AttrValue, Branch, Directive, Match, Node};

use crate::cfg::{Emitter, Label};
use crate::ir::*;
use crate::lower::{lower_directive, lower_line, lower_set};
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

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
/// threading `StageState` (identity in this task; injection lands in Task 9).
pub fn walk_seq(
    em: &mut Emitter,
    nodes: &[Node],
    mut state: StageState,
    cx: &mut WalkCx<'_>,
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
                state = emit_primitive(em, node, state, lookahead(nodes, i), cx, None);
            }
            Node::Branch(b) => {
                state = walk_branch(em, b, state, cx);
            }
            Node::Match(m) => {
                state = walk_match(em, m, state, cx);
            }
            Node::Timeline(_) => {
                // Replaced in Task 10 (schedule.rs): a timeline is handled
                // INLINE in this same walk (§5 pass 5).
            }
        }
    }
    state
}

/// D9 lookahead: only CFG-reachable LINEAR successors — the rest of this
/// sequence up to (never into) the next fork. Sibling arms are unreachable.
fn lookahead(nodes: &[Node], i: usize) -> &[Node] {
    let rest = &nodes[i + 1..];
    let stop = rest
        .iter()
        .position(|n| matches!(n, Node::Branch(_) | Node::Match(_)))
        .unwrap_or(rest.len());
    &rest[..stop]
}

/// Lower one primitive node into records. Task 9 adds injection here; this
/// task emits the authored record only and passes the state through.
fn emit_primitive(
    em: &mut Emitter,
    node: &Node,
    state: StageState,
    _lookahead: &[Node],
    cx: &mut WalkCx<'_>,
    clip: Option<ClipStamp>,
) -> StageState {
    let authored = match node {
        Node::Line(l) => Some(lower_line(l)),
        Node::Directive(d) => lower_directive(d, cx.snapshot),
        Node::Set(s) => Some(lower_set(s)),
        _ => None,
    };
    if let Some(mut cmd) = authored {
        apply_source(&mut cmd, cx);
        apply_clip(&mut cmd, clip);
        em.push(cmd);
    }
    state
}

fn walk_branch(em: &mut Emitter, b: &Branch, state: StageState, cx: &mut WalkCx<'_>) -> StageState {
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
            target: l.sym(),
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
    for (c, l) in b.choices.iter().zip(&arms) {
        em.bind(*l);
        // Task 9 forks/joins here; flatten-only for now.
        let _ = walk_seq(em, &c.body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
    }
    em.bind(conv);
    state
}

fn walk_match(em: &mut Emitter, m: &Match, state: StageState, cx: &mut WalkCx<'_>) -> StageState {
    let conv = em.fresh();
    let labels: Vec<Label> = m.arms.iter().map(|_| em.fresh()).collect();
    let mut arms = Vec::new();
    let mut otherwise = None;
    for (arm, l) in m.arms.iter().zip(&labels) {
        match arm {
            Arm::When { test, .. } => arms.push(MatchArm {
                test: test.raw.clone(),
                target: l.sym(),
            }),
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
    for (arm, l) in m.arms.iter().zip(&labels) {
        let body = match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
        };
        em.bind(*l);
        let _ = walk_seq(em, body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
    }
    em.bind(conv);
    state
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
