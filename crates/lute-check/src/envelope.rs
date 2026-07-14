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

use lute_syntax::ast::{
    Arm, AttrValue, Branch, Choice, ClipNode, Hub, Match, Node, Objective, On, Timeline,
};

use crate::defassign::Assigned;
use crate::meta::Namespace;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use crate::defassign::check_definite_assignment;
    use crate::meta::StateSchema;
    use crate::Ctx;
    use lute_cel::{fill_document, CelArena};
    use lute_syntax::parse;
    use std::sync::LazyLock;

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
}
