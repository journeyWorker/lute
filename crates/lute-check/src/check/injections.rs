//! The injection fold: stage-entity lifetime over the node stream, entering
//! `::use` bodies in document position.

use super::{
    attr_str, lower_node_with_snapshot, Arm, Directive, InjectedCommand, Node, StageState,
};
use crate::inject::StageFlow;

/// Fold the injection reducer over a node slice, threading `StageState` and
/// collecting every injected command. `<branch>` choices and `<match>` arms
/// are parallel paths (dsl 0.22.0 §12): each arm folds from a copy of the
/// state at the fork, and the arms' exit states meet in
/// [`StageState::join`] at the convergence — the same join `lute compile`'s
/// CFG walk applies — so a sibling arm's exit never leaks into another arm.
/// A `<hub>`'s options fold from its hub head instead ([`fold_hub`], dsl
/// 0.52.0 §5.4): a hub presents again after a non-exit option.
/// `<timeline>` clips are staged separately in `timeline_tables` and do not
/// participate in stage-entity lifetime here (see the injection reducer's
/// node-kind coverage).
///
/// dsl 0.51.0 §3.4: `flow` routes every `::jump` to its label and binds the
/// state at every `::label` ([`StageFlow::label`], the function `lute
/// compile` binds its label and loop-head states from). `None` is a point no
/// route reaches (after an unguarded jump without `max`): nothing is staged
/// there, and a label a jump reaches revives it. The reducer's diagnostics
/// collect in [`StageFlow::diags`].
///
/// A `::use` is entered too ([`fold_use`], Task 7g) — `flow.using` carries
/// the component names on the current expansion path so a `::use` cycle
/// terminates.
pub(crate) fn fold_injections(
    nodes: &[Node],
    state: &mut Option<StageState>,
    out: &mut Vec<InjectedCommand>,
    flow: &mut StageFlow<'_>,
) {
    for (i, node) in nodes.iter().enumerate() {
        flow.visit();
        if let Some(id) = crate::loop_walk::label_of(node) {
            *state = flow.label(id, &nodes[i + 1..], state.take());
            continue;
        }
        if let Node::Directive(d) = node {
            if d.tag == lute_manifest::core::JUMP_DIRECTIVE {
                flow.jump(d, state);
                continue;
            }
        }
        if let Node::Hub(h) = node {
            fold_hub(h, state, out, flow);
            continue;
        }
        if let Some(taken) = state.take() {
            let (mut next, emit) =
                lower_node_with_snapshot(taken, node, &nodes[i + 1..], flow.domains, flow.snapshot);
            flow.diags.append(&mut next.diags);
            *state = Some(next);
            out.extend(emit);
        }
        if let Node::Directive(d) = node {
            if flow.ends(d) {
                if let Some(mut ended) = state.take() {
                    flow.diags.append(&mut ended.diags);
                }
            }
        }
        let arms: Vec<&[Node]> = match node {
            Node::Branch(b) => b.choices.iter().map(|c| c.body.as_slice()).collect(),
            Node::Match(m) => m
                .arms
                .iter()
                .map(|arm| match arm {
                    Arm::When { body, .. } | Arm::Otherwise { body, .. } => body.as_slice(),
                })
                .collect(),
            Node::Directive(d) if d.tag == "use" => {
                fold_use(d, state, out, flow);
                continue;
            }
            _ => continue,
        };
        if arms.is_empty() {
            continue;
        }
        let exits: Vec<StageState> = arms
            .into_iter()
            .filter_map(|body| {
                let mut arm = state.clone();
                fold_injections(body, &mut arm, out, flow);
                arm
            })
            .collect();
        // No arm left by its end: the walk resumes only at a label — unless
        // control may pass the node without taking an arm, which keeps the
        // stage it entered with (0.50 joins only the arms otherwise).
        if !exits.is_empty() {
            *state = Some(StageState::join(&StageState::default(), exits));
        } else if !flow.may_take_no_arm(node) {
            *state = None;
        }
    }
}

/// dsl 0.52.0 §5.4: a `<hub>` from the state `state` reaching it. Every
/// option folds from the hub head ([`StageFlow::hub`]); the `<return>` body
/// from the completion of the non-exit options, and not at all when none
/// completes (its nodes are then unreachable, as at runtime); after the hub
/// the state is `join(H, [H, the exit options' exit states])` — `H` stands
/// for the presentation with nothing eligible.
fn fold_hub(
    h: &lute_syntax::ast::Hub,
    state: &mut Option<StageState>,
    out: &mut Vec<InjectedCommand>,
    flow: &mut StageFlow<'_>,
) {
    let head = flow.hub(h, state.take());
    let mut completed = Vec::new();
    let mut exits = Vec::new();
    for c in &h.choices {
        let mut arm = head.as_ref().map(StageState::bare);
        flow.enter_hub_body();
        fold_injections(&c.body, &mut arm, out, flow);
        flow.leave_hub_body();
        if StageFlow::exit_option(c) {
            exits.extend(arm);
        } else {
            completed.extend(arm);
        }
    }
    if let Some(r) = &h.on_return {
        let mut back = StageFlow::completion(completed);
        flow.enter_hub_body();
        fold_injections(&r.body, &mut back, out, flow);
        flow.leave_hub_body();
    }
    *state = match head {
        Some(head) => {
            let mut all = vec![head.bare()];
            all.extend(exits);
            Some(StageState::join(&head, all))
        }
        None => StageFlow::completion(exits),
    };
}

/// Task 7g, the FOURTH instance of the same class as Task 7b's (content-line
/// attrs), Task 7c's (duplicate line codes) and Task 7e's (reachability):
/// [`fold_injections`] had exactly ONE callsite — `check()` step 7, over the
/// ROOT document's shots — and a `::use` folded as an ordinary unknown
/// directive, so its body was never entered. An `::actor` whose implicit
/// `anchor`-domain read has nothing to read therefore checked CLEAN through a
/// `::use` while the identical directive reported `E-DOMAIN-UNKNOWN` at scene
/// level AND when that same component file was checked STANDALONE.
/// `lute compile`'s own CFG walk DID derive it (it folds the NORMALIZED tree,
/// body already inlined) but discarded it — see the `state.diags.clear()`
/// comment in `lute-compile/src/lib.rs` — so the diagnostic was reported by no
/// tool at all. (The original instance was `W-INJECT-CONFLICT`, removed in
/// 0.10.0 §12.3; the seam is structural and outlives the code.)
///
/// THE SEAM (the one decision here): the body folds IN DOCUMENT POSITION,
/// against the `StageState` INHERITED at this `::use` — the same reducer, the
/// same threaded environment the enclosing walk already carries, which is
/// exactly the context `lute compile` folds the inlined body in. A per-body
/// fold against a FRESH `StageState` (the shape Tasks 7c/7e use, because
/// their passes are position-independent) would be wrong HERE: the reducer's
/// entrance rules are stage-state dependent — `lower_auto` returns early when
/// the character is already on stage — so an empty entry state would INVENT
/// diagnostics that do not exist in context, trading an old divergence for a
/// new one.
///
/// Body diagnostics are re-anchored the way every other component-body
/// diagnostic is (component name + source path prefix, cleared fixits, a span
/// this document can represent) — but at THIS `::use` directive rather than
/// the scene frontmatter `validate_components` uses: the conflict is a
/// property of this invocation SITE, not of the component file in isolation,
/// so the site is the only honest anchor. A nested `::use` prefixes again,
/// naming the whole expansion path.
///
/// dsl 0.51.0 §3.4: the body's labels and loops are its own document's
/// ([`StageFlow::enter_component`]).
///
/// An unresolvable name (no `component=` attr, or one absent from the table)
/// is silently skipped — the resolution failure is already
/// `E-COMPONENT-UNKNOWN`/`E-COMPONENT-PARSE` on the import surface.
fn fold_use(
    d: &Directive,
    state: &mut Option<StageState>,
    out: &mut Vec<InjectedCommand>,
    flow: &mut StageFlow<'_>,
) {
    let Some(name) = attr_str(d, "component") else {
        return;
    };
    let Some(def) = flow.components.and_then(|set| set.table.get(&name)) else {
        return;
    };
    // A `::use` cycle is `E-COMPONENT-CYCLE` (reported by the expansion
    // checker); this fold only has to TERMINATE on it. Stack discipline,
    // exactly as `insert_shape_fields` uses for self-referential state shapes:
    // popping after the body keeps a legitimate diamond (one component reached
    // twice by disjoint paths) foldable.
    if flow.using.iter().any(|n| n == &name) {
        return;
    }
    flow.enter_component(&name);
    flow.using.push(name);
    let mark = flow.diags.len();
    // dsl 0.26.0 §3.2: a `@@p:` line stages the member this `::use` binds
    // `p` to (a nested `::use`'s arguments pass the binding through).
    let bound: Vec<Vec<Node>>;
    let bodies: Vec<&[Node]> = if def.speakers.is_empty() {
        def.body
            .sections
            .iter()
            .map(|s| s.body.as_slice())
            .collect()
    } else {
        let args = crate::component_effects::use_args_for(d, def);
        bound = def
            .body
            .sections
            .iter()
            .map(|s| {
                let mut body = s.body.clone();
                crate::component_effects::bind_speaker_params(&mut body, &args, &def.params);
                body
            })
            .collect();
        bound.iter().map(Vec::as_slice).collect()
    };
    for body in bodies {
        fold_injections(body, state, out, flow);
    }
    let name = flow.using.pop().expect("pushed above");
    flow.leave_component();
    for diag in &mut flow.diags[mark..] {
        diag.message = format!(
            "component `{name}` ({}): {}",
            def.src.display(),
            diag.message
        );
        diag.span = d.span;
        diag.fixits.clear();
    }
}
