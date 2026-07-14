//! Per-document guaranteed (`G`) / possible (`P`) write sets (connectivity
//! layer T8, dsl §4.3 "§C effect summary"). `G` is [`defassign`]'s own
//! end-of-document `Assigned` set — the must-write join (`intersect_all`) it
//! already computes to drive `E-MAYBE-UNSET` — filtered to the two monotonic
//! tiers the envelope lattice tracks. `P` is an INDEPENDENT flat,
//! path-insensitive scan of every `::set`/persist-sugar target reachable
//! ANYWHERE in the node stream: every `<branch>`/`<match>`/`<hub>`/`<on>`/
//! `<objective>` arm counts, including the non-dominating (may-only) writes
//! `defassign`'s `intersect_all` deliberately drops from `G` — so `P` is a
//! strict superset-shaped view of `G` (`P ⊇ G`), never derived from it.
//!
//! ## Tier scope (dsl §9.1, §4.3)
//! Both `G` and `P` are filtered to `run.*`/`user.*` only. The envelope
//! lattice assumes MONOTONIC writes; `quest.<id>.*` is engine-reserved
//! scratch that MAY be cleared mid-run (dsl 0.2.0 §5), so it is deliberately
//! excluded, along with `scene.*`/`app.*` (out of scope for this analysis).
//!
//! ## `::assert`/`::retract` are out of scope
//! A `::assert{…}`/`::retract{…}` target is a relational FACT (dsl 0.3.0 §5),
//! not a scalar `state:` path — `Node::Assert`/`Node::Retract` carry no
//! `state:` path at all, so they never contribute to either set here.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use lute_syntax::ast::{
    Arm, AttrValue, Branch, CelSlot, Choice, ClipNode, Hub, Match, Node, Objective, On, Quest,
    Timeline,
};

use crate::ctx::Env;
use crate::defassign::{check_definite_assignment, Assigned};
use crate::meta::{Namespace, StateSchema};
use crate::Ctx;

/// `true` when `path` resolves to the `run.*`/`user.*` tier — the two
/// monotonic namespaces the envelope lattice tracks (dsl §4.3). Every other
/// tier (`scene.*`/`app.*`/`quest.*`) and any non-state-path string returns
/// `false`.
fn in_envelope_scope(path: &str) -> bool {
    matches!(
        crate::meta::namespace_of(path),
        Some(Namespace::Run) | Some(Namespace::User)
    )
}

fn insert_in_scope(out: &mut BTreeSet<String>, path: &str) {
    if in_envelope_scope(path) {
        out.insert(path.to_string());
    }
}

/// The guaranteed-write set `G` for a document (dsl §4.3): every path
/// [`crate::defassign::check_definite_assignment`] proved assigned on EVERY
/// execution path through the whole-document node stream, filtered to
/// `run.*`/`user.*`. Reuses `defassign`'s own computed set verbatim — no new
/// lattice, this is purely the tier filter.
pub fn guaranteed(assigned: &Assigned) -> BTreeSet<String> {
    assigned.iter().filter(|p| in_envelope_scope(p)).cloned().collect()
}

/// The possible-write set `P` for a document (dsl §4.3): every `::set` /
/// `<choice persist>` target reachable ANYWHERE in `nodes`, path-insensitively
/// (every branch/match/hub/on/objective arm counts, regardless of whether it
/// dominates), filtered to `run.*`/`user.*`. Independent of
/// `check_definite_assignment` — a write that occurs in only ONE arm of a
/// `<branch>`/`<match>`/`<hub>` (never proven by `intersect_all`, so absent
/// from `G`) still appears here.
pub fn possible_writes(nodes: &[Node]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    scan_nodes(nodes, &mut out);
    out
}

fn scan_nodes(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Set(set) => insert_in_scope(out, &set.path),
            Node::Branch(branch) => scan_branch(branch, out),
            Node::Hub(hub) => scan_hub(hub, out),
            Node::Match(m) => scan_match(m, out),
            Node::On(on) => scan_on(on, out),
            Node::Objective(o) => scan_objective(o, out),
            Node::Timeline(tl) => scan_timeline(tl, out),
            // Content lines carry only `{{…}}` reads; directives carry no
            // state write; `::assert`/`::retract` target relational facts,
            // not `state:` paths (dsl 0.3.0 §5) — none contribute to `P`.
            Node::Line(_) | Node::Directive(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

fn scan_branch(branch: &Branch, out: &mut BTreeSet<String>) {
    for choice in &branch.choices {
        scan_choice(choice, out);
    }
}

fn scan_hub(hub: &Hub, out: &mut BTreeSet<String>) {
    for choice in &hub.choices {
        scan_choice(choice, out);
    }
}

fn scan_choice(choice: &Choice, out: &mut BTreeSet<String>) {
    scan_choice_persist(choice, out);
    scan_nodes(&choice.body, out);
}

fn scan_match(m: &Match, out: &mut BTreeSet<String>) {
    for arm in &m.arms {
        match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => scan_nodes(body, out),
        }
    }
}

fn scan_on(on: &On, out: &mut BTreeSet<String>) {
    scan_nodes(&on.body, out);
}

fn scan_objective(o: &Objective, out: &mut BTreeSet<String>) {
    scan_nodes(&o.body, out);
}

fn scan_timeline(tl: &Timeline, out: &mut BTreeSet<String>) {
    for track in &tl.tracks {
        for clip in &track.clips {
            if let ClipNode::Set(set) = &clip.node {
                insert_in_scope(out, &set.path);
            }
        }
    }
}

/// A `<choice persist="run" into="run.<path>" [value=…]>` (dsl §11.1.1) is
/// EXACTLY a `::set{run.<path> = value}` appended to the arm when the choice
/// is selected — the engine materializes the write (`check_choice_persist`,
/// check.rs owns validating the sugar's well-formedness; this only extracts
/// the target for `P`). `persist` is REQUIRED to be `"run"` (Rule 1,
/// §11.1.1), so `into` is always `run.*` when present — the tier filter is
/// applied anyway for a malformed/unvalidated document.
fn scan_choice_persist(choice: &Choice, out: &mut BTreeSet<String>) {
    let persists = choice.attrs.iter().any(|a| a.key == "persist");
    if !persists {
        return;
    }
    if let Some(into) = choice.attrs.iter().find(|a| a.key == "into") {
        if let AttrValue::Str(path) = &into.value {
            insert_in_scope(out, path);
        }
    }
}

/// A throwaway [`Ctx`] for [`body_guaranteed`]'s inner
/// [`check_definite_assignment`] call: that pass reads `schema` as an
/// explicit argument and never reads its own `_ctx` parameter, so a fresh
/// default `Env` is sufficient — this never observes the real project's
/// `defs`/`rel_vocab`/etc., nor needs to.
fn body_ctx() -> Ctx<'static> {
    static ENV: LazyLock<Env> = LazyLock::new(Env::default);
    Ctx { env: &ENV, in_match: false, match_subject: None }
}

/// `true` when an `<on>` handler is guaranteed to fire whenever its event
/// occurs (dsl §4.3, connectivity T9): no `when` guard at all, or a `when`
/// that is a literal CEL `true` constant. Any other `when` MAY evaluate
/// false at runtime — the handler simply does not run that time — so its
/// body's writes are may-only, never a sound member of
/// [`writes_on_complete`]'s guaranteed union.
fn is_unconditional(when: &Option<CelSlot>) -> bool {
    match when {
        None => true,
        Some(slot) => slot.raw.trim() == "true",
    }
}

/// The guaranteed-write must-set for ONE body's node stream taken ALONE (dsl
/// §4.3): reruns T8's write-only definite-assignment pass
/// (`check_definite_assignment`) as if `nodes` were a whole document on its
/// own — so a `<branch>`/`<match>` nested in the body still intersects its
/// own arms exactly as `guaranteed`/`G` does for a whole document — then
/// applies the same `run.*`/`user.*` tier filter. Diagnostics are
/// discarded: the body was already validated by the real
/// `check_definite_assignment` pass over the whole quest document; this is
/// a read-only re-derivation of its guaranteed-write set, not a second
/// source of diagnostics.
fn body_guaranteed(nodes: &[Node], schema: &StateSchema) -> BTreeSet<String> {
    let (_diags, writes) = check_definite_assignment(nodes, schema, &body_ctx());
    guaranteed(&writes)
}

/// `writesOnComplete(Q)` (dsl §4.3): the write set a quest `q` GUARANTEES
/// once it completes. Union, across each REQUIRED objective's body plus an
/// UNCONDITIONAL `questComplete` `<on>` handler's body, of each body's own
/// guaranteed-write must-set (`body_guaranteed` — INTERSECT within a body,
/// since a body may itself contain a `<branch>`/`<match>`, dsl §6.7).
/// OPTIONAL objectives (`Objective::optional`) are skipped before their
/// body is ever walked — they need not fire for completion (dsl 0.2.0
/// §6.4), so crediting their writes would be unsound. A `questComplete`
/// handler guarded by a `when` that isn't provably `true`
/// (`is_unconditional`) is skipped the same way: its guard MAY be false
/// exactly when the quest completes, so its writes are may-only, not
/// guaranteed. The bodies that DO qualify each fire independently and
/// unconditionally-once on completion (dsl §6.3/§6.4), so nothing narrows
/// between them — UNION across them. Filtered to `run.*`/`user.*` (same
/// tier scope as `guaranteed`/`possible_writes`; `body_guaranteed` already
/// applies it per body, so the union stays in scope for free). `Objective`/
/// `On` nodes only ever appear directly in a quest's own body, never nested
/// (grammar admission, dsl 0.2.0 §6.7 — mirrors `match_check::check_quest`'s
/// and `project_check`'s own top-level-only objective scans), so a single
/// top-level pass over `q.body` is exhaustive.
pub fn writes_on_complete(q: &Quest, schema: &StateSchema) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for node in &q.body {
        match node {
            Node::Objective(o) if !o.optional => out.extend(body_guaranteed(&o.body, schema)),
            Node::On(on) if on.event == "questComplete" && is_unconditional(&on.when) => {
                out.extend(body_guaranteed(&on.body, schema));
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_cel::{fill_document, CelArena};
    use lute_syntax::parse;

    fn shot_nodes(src: &str) -> Vec<Node> {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        doc.shots.into_iter().next().map(|s| s.body).unwrap_or_default()
    }

    fn quest_nodes(src: &str) -> Vec<Node> {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        doc.quests.into_iter().next().map(|q| q.body).unwrap_or_default()
    }

    /// Mirrors `defassign::tests::fixture` — parses + fills CEL + lifts the
    /// inline `state:` schema, needed only by the `guaranteed()`/divergence
    /// tests below (`possible_writes` needs no schema).
    fn fixture(src: &str) -> (Vec<Node>, StateSchema) {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        let (meta, _md) = crate::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let nodes = doc
            .shots
            .into_iter()
            .next()
            .map(|s| s.body)
            .unwrap_or_default();
        (nodes, meta.state)
    }

    /// Like `fixture`, but returns the first `<quest>` declaration itself
    /// (not just its flattened node stream) plus the doc's `state:` schema —
    /// `writes_on_complete` needs the `Quest` struct to read
    /// `Objective::optional`/`On::event`/`On::when`, which the flat node
    /// list `quest_nodes` returns loses.
    fn quest_fixture(src: &str) -> (Quest, StateSchema) {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        let (meta, _md) = crate::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let quest = doc
            .quests
            .into_iter()
            .next()
            .expect("quest_fixture src must declare a <quest>");
        (quest, meta.state)
    }

    fn ctx() -> Ctx<'static> {
        static ENV: LazyLock<Env> = LazyLock::new(Env::default);
        Ctx {
            env: &ENV,
            in_match: false,
            match_subject: None,
        }
    }

    #[test]
    fn possible_writes_collects_across_branch_hub_match_run_user_only() {
        // `run.a` (branch choice), `user.b` (nested match `<when>`) collected;
        // `scene.skip` (match `<otherwise>`), `quest.q1.d` (top-level `::set`)
        // excluded by the run/user tier filter.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\">\n::set{run.a = 1}\n<match on=\"run.a\">\n<when test=\"run.a > 0\">\n::set{user.b = 2}\n</when>\n<otherwise>\n::set{scene.skip = 1}\n</otherwise>\n</match>\n</choice>\n<choice id=\"c2\" label=\"L2\">\n@narrator: skip\n</choice>\n</branch>\n<hub id=\"h\">\n<choice id=\"hc\" label=\"HL\">\n::set{run.c = 3}\n</choice>\n</hub>\n::set{quest.q1.d = 4}\n";
        let nodes = shot_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(
            p,
            BTreeSet::from(["run.a".to_string(), "run.c".to_string(), "user.b".to_string()])
        );
    }

    #[test]
    fn possible_writes_excludes_assert_and_includes_persist_sugar() {
        // `<choice persist="run" into="run.p">` is sugar for a `::set` — counts.
        // `::assert{…}` targets a relational fact, not a state path — excluded.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" persist=\"run\" into=\"run.p\">\n::assert{ seen(x) }\n</choice>\n</branch>\n";
        let nodes = shot_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(p, BTreeSet::from(["run.p".to_string()]));
    }

    #[test]
    fn possible_writes_collects_across_on_and_objective_arms() {
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n::set{run.done = 1}\n</on>\n<objective id=\"o\" done=\"run.done\">\n::set{user.progress = 1}\n</objective>\n</quest>\n";
        let nodes = quest_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(
            p,
            BTreeSet::from(["run.done".to_string(), "user.progress".to_string()])
        );
    }

    #[test]
    fn guaranteed_filters_assigned_to_run_user_tier() {
        let mut assigned: Assigned = Assigned::new();
        assigned.insert("scene.s".to_string());
        assigned.insert("run.r".to_string());
        assigned.insert("user.u".to_string());
        assigned.insert("app.a".to_string());
        assigned.insert("quest.q1.state".to_string());
        assert_eq!(
            guaranteed(&assigned),
            BTreeSet::from(["run.r".to_string(), "user.u".to_string()])
        );
    }

    #[test]
    fn possible_writes_diverges_from_guaranteed_for_guarded_only_write() {
        // `run.a` is written in exactly ONE `<branch>` arm (guarded by
        // `run.flag`), never on the sibling `c2` arm — `intersect_all` drops
        // it from `check_definite_assignment`'s final `Assigned` set, so it
        // must be ABSENT from `guaranteed(G)`. `possible_writes` walks every
        // arm regardless of dominance, so `run.a` MUST still be present —
        // proving `P` captures may-only writes `G` deliberately discards.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.a: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" when=\"run.flag\">\n::set{run.a = 1}\n</choice>\n<choice id=\"c2\" label=\"L2\">\n@narrator: skip\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned) = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            !g.contains("run.a"),
            "guarded-only write must NOT survive intersect_all into G, got {g:?}"
        );
        assert!(
            p.contains("run.a"),
            "guarded-only write must still be a possible write in P, got {p:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn guaranteed_excludes_arm_guard_proof_with_no_matching_write() {
        // RevT8 P1: both arms of an exhaustive bool match guard-prove
        // `run.x` (`test="isSet(run.x)"`, arm-level, dominating) but NEITHER
        // writes it. Before the fix `defassign`'s mixed `Assigned` exported
        // this guard-proof into `G`, while `possible_writes` (writes only)
        // never saw it -> `G ⊄ P`. `run.x` must now be ABSENT from `G`, and
        // `P` must remain a superset (both empty for this path here).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"run.flag\">\n<when is=\"true\" test=\"isSet(run.x)\">\n@narrator: a\n</when>\n<when is=\"false\" test=\"isSet(run.x)\">\n@narrator: b\n</when>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned) = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(
            errs.is_empty(),
            "guard-proven read should not flag E-MAYBE-UNSET, got {errs:?}"
        );

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            !g.contains("run.x"),
            "a guard-proof with no write must NOT enter G, got {g:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn guaranteed_includes_persist_target_written_on_every_arm() {
        // RevT8 P1 Fix 2: `<choice persist>` is an arm-flow WRITE — an
        // exhaustive per-arm persist of `run.x` must join `G`, exactly like
        // an exhaustive `::set`, and `P` (which already counted persist
        // sugar) must stay a superset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" when=\"run.flag\" persist=\"run\" into=\"run.x\" value=\"1\">\n@narrator: a\n</choice>\n<choice id=\"c2\" label=\"L2\" persist=\"run\" into=\"run.x\" value=\"2\">\n@narrator: b\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned) = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            g.contains("run.x"),
            "an exhaustive per-arm persist should join G, got {g:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn writes_on_complete_intersects_within_body_and_unions_across_bodies() {
        // `run.done` is written on BOTH `<branch>` arms of the required
        // objective's body — `body_guaranteed`'s own `intersect_all` walk
        // must keep it. `run.b` is written on only ONE arm (`c1`) — it must
        // NOT survive that same intersect. The unconditional `questComplete`
        // `<on>` body's `run.flag` write is a SEPARATE body — union, not
        // intersect, brings it into the result alongside `run.done`.
        let src = "---\nkind: quest\nstate:\n  run.done: { type: number }\n  run.b: { type: number }\n  run.flag: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.done\">\n<branch id=\"br\">\n<choice id=\"c1\" label=\"L1\">\n::set{run.done = 1}\n::set{run.b = 1}\n</choice>\n<choice id=\"c2\" label=\"L2\">\n::set{run.done = 1}\n</choice>\n</branch>\n</objective>\n<on event=\"questComplete\">\n::set{run.flag = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.done"), "both-arm write must be guaranteed, got {w:?}");
        assert!(w.contains("run.flag"), "unconditional questComplete body must union in, got {w:?}");
        assert!(!w.contains("run.b"), "one-arm-only write must NOT survive intersect, got {w:?}");
    }

    #[test]
    fn writes_on_complete_excludes_optional_objective() {
        // `o2` is OPTIONAL — its unconditional `run.opt` write must be
        // excluded from the union entirely; `o1` is required and still
        // contributes `run.req`.
        let src = "---\nkind: quest\nstate:\n  run.req: { type: number }\n  run.opt: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.req\">\n::set{run.req = 1}\n</objective>\n<objective id=\"o2\" done=\"run.opt\" optional>\n::set{run.opt = 1}\n</objective>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.req"), "required objective write must be present, got {w:?}");
        assert!(!w.contains("run.opt"), "optional objective write must be excluded, got {w:?}");
    }

    #[test]
    fn writes_on_complete_filters_to_run_user_tier() {
        // `quest.q.scratch` (required objective body) and `scene.bar`
        // (questComplete body) are both unconditional writes, but neither
        // tier is in envelope scope (dsl §4.3) — only `run.keep` may survive.
        let src = "---\nkind: quest\nstate:\n  run.keep: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.keep\">\n::set{run.keep = 1}\n::set{quest.q.scratch = 1}\n</objective>\n<on event=\"questComplete\">\n::set{scene.bar = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert_eq!(w, BTreeSet::from(["run.keep".to_string()]));
    }

    #[test]
    fn writes_on_complete_excludes_guarded_quest_complete_handler() {
        // `<on event="questComplete" when="run.flag">` MAY not fire (the
        // guard could be false right when the quest completes) — its
        // `run.g` write is may-only, never guaranteed.
        let src = "---\nkind: quest\nstate:\n  run.flag: { type: number }\n  run.g: { type: number }\n---\n<quest id=\"q\">\n<on event=\"questComplete\" when=\"run.flag\">\n::set{run.g = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(!w.contains("run.g"), "guarded questComplete write must NOT be guaranteed, got {w:?}");
    }

    #[test]
    fn writes_on_complete_includes_unconditional_quest_complete_handler() {
        // No `when` at all — the handler always fires on `questComplete`, so
        // its write IS guaranteed.
        let src = "---\nkind: quest\nstate:\n  run.g: { type: number }\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n::set{run.g = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.g"), "unconditional questComplete write must be guaranteed, got {w:?}");
    }
}
