//! Quest liveness, connectivity spans, and fixpoint contracts.

use super::*;

// --- Defect B (persona review): `completed(Q)` unreachable via a dead
// REQUIRED objective ---------------------------------------------------
//
// dsl 0.4.0 §8.2 rule C4 deliberately suppresses a standalone
// `E-QUEST-UNREACHABLE` for a quest dead via a required objective (that
// cause rides as a note on `E-OBJECTIVE-UNSATISFIABLE` instead), so
// `completed(Q)` reachability must consult the underlying objective-
// liveness signal directly, never the (C4-suppressed) diagnostic code.

/// Scalar flavor: a REQUIRED objective whose `done` predicate decides
/// false directly (`decide_slot`) must mark its quest unreachable-to-
/// complete, so a scene gated on `completed(Q)` earns `E-CONN-UNREACHABLE`
/// -- even though NO standalone `E-QUEST-UNREACHABLE` is ever emitted for
/// this cause (C4).
#[test]
fn dead_required_objective_scalar_marks_completed_gate_unreachable() {
    let dir = temp_dir("dead-required-objective-scalar");
    write(
        &dir,
        "deadquest.lute",
        "---\nkind: quest\nstate:\n  run.rank: { type: { enum: [novice, veteran, hero] }, \
         default: novice }\n---\n<quest id=\"deadQuest\" start=\"true\">\n\
         <objective id=\"becomeLegend\" done=\"run.rank == 'legendary'\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"deadQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags
            .iter()
            .any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "a scene gated on completed(Q) where Q has a dead REQUIRED (scalar) objective must be \
         E-CONN-UNREACHABLE: {v}"
    );
    // C4 unbroken: no duplicate standalone E-QUEST-UNREACHABLE anywhere.
    let files = v["files"].as_array().unwrap();
    assert!(
        !files
            .iter()
            .flat_map(|f| f["diagnostics"].as_array().unwrap())
            .any(|d| d["code"] == "E-QUEST-UNREACHABLE"),
        "C4 regression: must never emit a duplicate standalone E-QUEST-UNREACHABLE for the \
         required-objective cause: {v}"
    );
}

/// Relational flavor: a REQUIRED objective gated on a never-producible
/// relation (§4.2 second-order consequence) must ALSO mark its quest
/// unreachable-to-complete.
#[test]
fn dead_required_objective_relational_marks_completed_gate_unreachable() {
    let dir = temp_dir("dead-required-objective-relational");
    write(
        &dir,
        "deadrelquest.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         neverProduced: { args: [loc], tier: run }\n---\n<quest id=\"deadRelQuest\" start=\"true\">\n\
         <objective id=\"o\" done=\"holds('neverProduced', ['a'])\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro2\nseason: 1\nepisode: 1\n\
         after: 'completed(\"deadRelQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags
            .iter()
            .any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "a scene gated on completed(Q) where Q has a dead REQUIRED (never-producible relation) \
         objective must be E-CONN-UNREACHABLE: {v}"
    );
    assert!(
        project_diags
            .iter()
            .any(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE"),
        "the underlying relational-objective-liveness diagnostic must still fire too: {v}"
    );
}

/// An OPTIONAL dead objective must NEVER make its quest unreachable-to-
/// complete (C4's quest-level consequence is REQUIRED-only) -- the quest
/// can still complete via its other (live, required) objective, so a
/// scene gated on `completed(Q)` must stay clean.
#[test]
fn optional_dead_objective_does_not_make_quest_unreachable() {
    let dir = temp_dir("optional-dead-objective-unreachable");
    write(
        &dir,
        "mixedquest.lute",
        "---\nkind: quest\nstate:\n  run.live: { type: bool, default: true }\n---\n\
         <quest id=\"mixedQuest\" start=\"true\">\n\
         <objective id=\"deadOpt\" done=\"false\" optional/>\n\
         <objective id=\"liveReq\" done=\"run.live\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro3\nseason: 1\nepisode: 1\n\
         after: 'completed(\"mixedQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        !project_diags
            .iter()
            .any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "an OPTIONAL dead objective must never make the quest unreachable-to-complete: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let gated = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("gated.lute"))
        .unwrap();
    assert_eq!(
        gated["diagnostics"].as_array().unwrap().len(),
        0,
        "the gated scene itself must stay clean -- its route is still live: {v}"
    );
}

/// Control (unbroken by the fix): `start="false"` still marks its quest
/// unreachable-to-complete via the REAL `E-QUEST-UNREACHABLE` lifecycle
/// signal, propagating to `E-CONN-UNREACHABLE` exactly as before.
#[test]
fn start_false_quest_still_marks_completed_gate_unreachable() {
    let dir = temp_dir("start-false-completed-gate-unreachable");
    write(
        &dir,
        "questa.lute",
        "---\nkind: quest\n---\n<quest id=\"questA\" start=\"false\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"questA\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags
            .iter()
            .any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "the start=false control must still propagate to E-CONN-UNREACHABLE: {v}"
    );
    let files = v["files"].as_array().unwrap();
    assert!(
        files
            .iter()
            .flat_map(|f| f["diagnostics"].as_array().unwrap())
            .any(|d| d["code"] == "E-QUEST-UNREACHABLE"),
        "the REAL E-QUEST-UNREACHABLE lifecycle signal must still fire for this cause: {v}"
    );
}

/// A live quest (no dead objective, no dead lifecycle guard) stays
/// Reachable -- a scene gated on `completed(Q)` stays clean.
#[test]
fn live_quest_completed_gate_stays_reachable() {
    let dir = temp_dir("live-quest-completed-gate-reachable");
    write(
        &dir,
        "livequest.lute",
        "---\nkind: quest\nstate:\n  run.live: { type: bool, default: true }\n---\n\
         <quest id=\"liveQuest\" start=\"true\">\n\
         <objective id=\"o\" done=\"run.live\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro4\nseason: 1\nepisode: 1\n\
         after: 'completed(\"liveQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(
        v["project_diagnostics"].as_array().unwrap().is_empty(),
        "{v}"
    );
}

// --- Defect A (persona review): real spans on project-level connectivity
// diagnostics, instead of `0:0` ------------------------------------------

#[test]
fn episode_id_dup_span_points_at_character_line() {
    let dir = temp_dir("span-episode-id-dup");
    let scene = "---\nkind: scene\ncharacter: dupchar\nseason: 1\nepisode: 1\n---\n\
                 ## Shot 1.\n@narrator: hi\n";
    write(&dir, "a.lute", scene);
    write(&dir, "b.lute", scene);

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-EPISODE-ID-DUP")
        .unwrap_or_else(|| panic!("E-CONN-EPISODE-ID-DUP present: {v}"));
    assert_eq!(
        d["span"]["line"], 3,
        "must point at `character:` (line 3), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn unknown_node_span_points_at_after_line() {
    let dir = temp_dir("span-unknown-node");
    write(
        &dir,
        "y.lute",
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n\
         after: 'visited(\"nonexistent.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-UNKNOWN-NODE")
        .unwrap_or_else(|| panic!("E-CONN-UNKNOWN-NODE present: {v}"));
    assert_eq!(
        d["span"]["line"], 6,
        "must point at `after:` (line 6), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn cycle_span_points_at_a_cyclic_scenes_own_frontmatter() {
    let dir = temp_dir("span-cycle");
    write(
        &dir,
        "p.lute",
        "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\n\
         after: 'visited(\"q.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );
    write(
        &dir,
        "q.lute",
        "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\n\
         after: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-CYCLE")
        .unwrap_or_else(|| panic!("E-CONN-CYCLE present: {v}"));
    assert_eq!(
        d["span"]["line"], 3,
        "must point at the cyclic scene's own `character:` line (3), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn unreachable_span_points_at_gated_scenes_own_frontmatter() {
    let dir = temp_dir("span-unreachable");
    write(
        &dir,
        "questa.lute",
        "---\nkind: quest\n---\n<quest id=\"questA\" start=\"false\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"questA\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-UNREACHABLE")
        .unwrap_or_else(|| panic!("E-CONN-UNREACHABLE present: {v}"));
    assert_eq!(
        d["span"]["line"], 3,
        "must point at the gated scene's own `character:` line (3), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

// --- Defect B, round 2 (cross-model review): the fixpoint closure MUST
// iterate past round 1, and MUST NOT false-positive a still-live producer
// -----------------------------------------------------------------------

/// A 3+ node feedback chain (dsl 0.4.0 §4.2's relational-objective-
/// liveness CLOSURE): `chainQ1` has a dead REQUIRED objective (round 1) ->
/// scene `chainS` (`after: completed("chainQ1")`) becomes unreachable,
/// dropping its `::assert{chainRel(a)}` -- the SOLE producer of
/// `chainRel` (round 2) -> `chainQ2`'s REQUIRED objective
/// `done="holds(chainRel(a))"` is now dead too (round 2) -> scene `chainT`
/// (`after: completed("chainQ2")`) becomes unreachable (round 3). A
/// single-round (or two-pass) computation only catches `chainS`/`chainQ1`
/// and misses `chainQ2`/`chainT` entirely -- the fixpoint must catch all
/// four.
#[test]
fn fixpoint_closure_propagates_through_a_multi_hop_chain() {
    let dir = temp_dir("fixpoint-chain");
    write(
        &dir,
        "q1.lute",
        "---\nkind: quest\n---\n<quest id=\"chainQ1\" start=\"true\">\n\
         <objective id=\"deadReq\" done=\"false\"/>\n</quest>\n",
    );
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: chainS\nseason: 1\nepisode: 1\n\
         after: 'completed(\"chainQ1\")'\nentities:\n  loc: { members: [a] }\n\
         relations:\n  chainRel: { args: [loc], tier: run }\n---\n## Shot 1.\n\
         ::assert{ chainRel(a) }\n",
    );
    write(
        &dir,
        "q2.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         chainRel: { args: [loc], tier: run }\n---\n<quest id=\"chainQ2\" start=\"true\">\n\
         <objective id=\"checkChain\" done=\"holds('chainRel', ['a'])\"/>\n</quest>\n",
    );
    write(
        &dir,
        "t.lute",
        "---\nkind: scene\ncharacter: chainT\nseason: 1\nepisode: 1\n\
         after: 'completed(\"chainQ2\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();

    let unreachable_paths: Vec<&str> = project_diags
        .iter()
        .filter(|d| d["code"] == "E-CONN-UNREACHABLE")
        .filter_map(|d| d["path"].as_str())
        .collect();
    assert!(
        unreachable_paths.iter().any(|p| p.ends_with("s.lute")),
        "round 1: chainS must be E-CONN-UNREACHABLE (gated on the dead chainQ1): {v}"
    );
    assert!(
        unreachable_paths.iter().any(|p| p.ends_with("t.lute")),
        "round 3: chainT must ALSO be E-CONN-UNREACHABLE -- a single-round computation would \
         miss this, since chainQ2's own deadness is only provable once chainS's assert site \
         drops out in round 2: {v}"
    );
    assert!(
        project_diags.iter().any(|d| {
            d["code"] == "E-OBJECTIVE-UNSATISFIABLE"
                && d["path"].as_str().unwrap().ends_with("q2.lute")
        }),
        "round 2: chainQ2's holds('chainRel', ['a']) must be flagged dead once chainRel's sole \
         producer (chainS) drops out: {v}"
    );
}

/// False-positive guard (Fix 1): a quest with `start="true"`, a dead
/// REQUIRED objective, AND a separate OPTIONAL objective whose body
/// asserts `liveRel(a)` -- `liveRel` must stay producible (the quest
/// itself still ACTIVATES and runs its other objectives; "can never
/// COMPLETE" is not "never activates"), so a THIRD, unrelated required
/// objective gated on `holds(liveRel(a))` must NOT be flagged
/// `E-OBJECTIVE-UNSATISFIABLE`.
#[test]
fn dead_required_objective_never_drops_a_sibling_optional_objectives_live_assert() {
    let dir = temp_dir("dead-required-objective-false-positive-guard");
    write(
        &dir,
        "mixedquest2.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         liveRel: { args: [loc], tier: run }\nstate:\n  run.opt: { type: bool, default: true }\n\
         ---\n<quest id=\"mixedQuest2\" start=\"true\">\n\
         <objective id=\"deadReq\" done=\"false\"/>\n\
         <objective id=\"liveOpt\" done=\"run.opt\" optional>\n\
         ::assert{ liveRel(a) }\n</objective>\n\
         <objective id=\"checkLive\" done=\"holds('liveRel', ['a'])\"/>\n</quest>\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        !project_diags
            .iter()
            .any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "{v}"
    );
    // Load-bearing: `checkLive`'s `holds(liveRel(a))` cause is a RELATIONAL
    // one, which the project guard pass emits into `project_diags`
    // (never `files[].diagnostics` -- a per-file `check()` alone can't
    // decide it, R5-Undecided). If Fix 1 regressed (host-liveness gated on
    // the COMBINED unreachable set instead of lifecycle-only), `liveRel`
    // would wrongly go non-producible and THIS is where the false
    // `E-OBJECTIVE-UNSATISFIABLE` would land -- the per-file assertion
    // below alone can never observe it.
    assert!(
        !project_diags
            .iter()
            .any(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE"),
        "no project-wide (relational) E-OBJECTIVE-UNSATISFIABLE may fire -- `checkLive` must \
         stay live: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let f = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("mixedquest2.lute"))
        .unwrap();
    let objective_diags: Vec<&str> = f["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE")
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert_eq!(
        objective_diags.len(),
        1,
        "exactly `deadReq` must be flagged dead -- `checkLive` (holds('liveRel', ['a'])) must stay \
         live, since `liveOpt`'s `::assert{{liveRel(a)}}` is still a REAL producer (the quest \
         still activates and runs it): {v}"
    );
    assert!(
        objective_diags[0].contains("false"),
        "the ONE flagged objective must be `deadReq` (done=\"false\"), never `checkLive`: {v}"
    );
}
