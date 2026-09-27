//! The injection fold: stage-entity lifetime over the node stream, entering
//! `::use` bodies in document position.

use super::*;

/// Fold the injection reducer over a node slice, threading `StageState` and
/// collecting every injected command. `<branch>`/`<hub>` choices and
/// `<match>` arms are parallel paths (dsl 0.22.0 §12): each arm folds from a
/// copy of the state at the fork, and the arms' exit states meet in
/// [`StageState::join`] at the convergence — the same join `lute compile`'s
/// CFG walk applies — so a sibling arm's exit never leaks into another arm.
/// `<timeline>` clips are staged separately in `timeline_tables` and do not
/// participate in stage-entity lifetime here (see the injection reducer's
/// node-kind coverage).
///
/// A `::use` is entered too ([`fold_use`], Task 7g) — `using` carries the
/// component names on the current expansion path so a `::use` cycle
/// terminates.
pub(super) fn fold_injections(
    nodes: &[Node],
    state: &mut StageState,
    out: &mut Vec<InjectedCommand>,
    domains: &std::collections::BTreeMap<String, Domain>,
    components: &ComponentSet,
    using: &mut Vec<String>,
) {
    for (i, node) in nodes.iter().enumerate() {
        let taken = std::mem::take(state);
        let (next, emit) = lower_node(taken, node, &nodes[i + 1..], domains);
        *state = next;
        out.extend(emit);
        let arms: Vec<&[Node]> = match node {
            Node::Branch(b) => b.choices.iter().map(|c| c.body.as_slice()).collect(),
            Node::Hub(h) => h.bodies().map(|b| b.as_slice()).collect(),
            Node::Match(m) => m
                .arms
                .iter()
                .map(|arm| match arm {
                    Arm::When { body, .. } | Arm::Otherwise { body, .. } => body.as_slice(),
                })
                .collect(),
            Node::Directive(d) if d.tag == "use" => {
                fold_use(d, state, out, domains, components, using);
                continue;
            }
            _ => continue,
        };
        // Diagnostics already raised stay once, ahead of the arms' own.
        let mut diags = std::mem::take(&mut state.diags);
        let exits = arms
            .into_iter()
            .map(|body| {
                let mut arm = state.clone();
                fold_injections(body, &mut arm, out, domains, components, using);
                arm
            })
            .collect();
        let mut joined = StageState::join(state, exits);
        diags.append(&mut joined.diags);
        joined.diags = diags;
        *state = joined;
    }
}

/// Task 7g, the FOURTH instance of the same class as Task 7b's (content-line
/// attrs), Task 7c's (duplicate line codes) and Task 7e's (reachability):
/// [`fold_injections`] had exactly ONE callsite — `check()` step 7, over the
/// ROOT document's shots — and a `::use` folded as an ordinary unknown
/// directive, so its body was never entered. An `::auto` whose implicit
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
/// An unresolvable name (no `component=` attr, or one absent from the table)
/// is silently skipped — the resolution failure is already
/// `E-COMPONENT-UNKNOWN`/`E-COMPONENT-PARSE` on the import surface.
fn fold_use(
    d: &Directive,
    state: &mut StageState,
    out: &mut Vec<InjectedCommand>,
    domains: &std::collections::BTreeMap<String, Domain>,
    components: &ComponentSet,
    using: &mut Vec<String>,
) {
    let Some(name) = attr_str(d, "component") else {
        return;
    };
    let Some(def) = components.table.get(&name) else {
        return;
    };
    // A `::use` cycle is `E-COMPONENT-CYCLE` (reported by the expansion
    // checker); this fold only has to TERMINATE on it. Stack discipline,
    // exactly as `insert_shape_fields` uses for self-referential state shapes:
    // popping after the body keeps a legitimate diamond (one component reached
    // twice by disjoint paths) foldable.
    if using.iter().any(|n| n == &name) {
        return;
    }
    using.push(name);
    let mark = state.diags.len();
    // dsl 0.26.0 §3.2: a `@@p:` line stages the member this `::use` binds
    // `p` to (a nested `::use`'s arguments pass the binding through).
    let bound: Vec<Vec<Node>>;
    let bodies: Vec<&[Node]> = if def.speakers.is_empty() {
        def.body.shots.iter().map(|s| s.body.as_slice()).collect()
    } else {
        let args = crate::component_effects::use_args_for(d, def);
        bound = def
            .body
            .shots
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
        fold_injections(body, state, out, domains, components, using);
    }
    let name = using.pop().expect("pushed above");
    for diag in &mut state.diags[mark..] {
        diag.message = format!(
            "component `{name}` ({}): {}",
            def.src.display(),
            diag.message
        );
        diag.span = d.span;
        diag.fixits.clear();
    }
}
