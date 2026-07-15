//! `lute scenario` acceptance (connectivity T14, dsl §5:571-584): the
//! project-wide, read-only reporting surface over everything §4 computes —
//! bare `lute scenario` prints the assembled topological graph, `reach
//! <nodeId>` reports a node's reachability verdict, and `envelope <nodeId>`
//! (or `envelope quest:<id>`) prints the Guaranteed/Possible tables plus the
//! `Possible \ Guaranteed` warning-grade reads T11 computes-and-drops from
//! default `check-project` (dsl §6). ALSO: a `check-project` regression
//! confirming the full §6 diagnostics table is wired (`E-CONN-UNKNOWN-NODE`
//! exits non-zero) — connectivity T14's "Also confirm" step.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (matches every other `lute-cli` integration test's
/// own helper — each integration test binary is compiled separately, so
/// this is intentionally duplicated rather than shared).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A minimal valid core-only `lute.project.yaml` (mirrors
/// `check_project_multi.rs`'s own helper) — used by the cross-root
/// duplicate-node fixture to force TWO independently resolved project
/// roots under one walked directory.
fn core_only_project_yaml() -> String {
    "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n".to_string()
}

/// A scene doc setting `run.a` unconditionally on entry (no `after`) — the
/// upstream node the guaranteed-envelope test's `B` scene routes through.
fn scene_sets_run_a(character: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         state:\n  run.a: {{ type: number }}\n---\n## Shot 1.\n::set{{run.a = 1}}\n"
    )
}

/// A scene doc declaring `after: visited(<after_key>)` with no reads of its
/// own — used for the bare-graph/reach/guaranteed-envelope fixtures.
fn scene_after(character: &str, after_key: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         after: 'visited(\"{after_key}\")'\n---\n## Shot 1.\n@narrator: hi\n"
    )
}

/// A scene doc setting `run.z` unconditionally on entry (no `after`) — the
/// route that DOES set `run.z`, for the `Possible \ Guaranteed` fixture
/// (paired with a sibling route that never sets it).
fn scene_sets_run_z(character: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         state:\n  run.z: {{ type: number }}\n---\n## Shot 1.\n::set{{run.z = 1}}\n"
    )
}

/// A scene doc declaring `after: visited(a) || visited(b)` that reads
/// `run.z` via a plain `::set` RHS — the entry-dependent read shape needed
/// to trigger a concrete `Possible \ Guaranteed` warning READ (contract #2),
/// mirrors `check_project.rs`'s own `scene_reading_run_z` fixture.
fn scene_reading_run_z_after_or(character: &str, a_key: &str, b_key: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         after: 'visited(\"{a_key}\") || visited(\"{b_key}\")'\n\
         state:\n  run.z: {{ type: number }}\n  run.out: {{ type: number }}\n---\n\
         ## Shot 1.\n::set{{run.out = run.z}}\n"
    )
}

/// A self-contained, otherwise-CLEAN `kind: quest` doc declaring exactly one
/// quest id, with NO `after` attribute — mirrors `check_project.rs`'s own
/// `clean_quest_doc` helper.
fn quest_no_after(quest_id: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  run.done: {{ type: bool, default: false }}\n---\n\
         <quest id=\"{quest_id}\">\n<objective id=\"o\" done=\"run.done\"/>\n</quest>\n"
    )
}

// --- brief Step 1 tests --------------------------------------------------

#[test]
fn scenario_envelope_reports_guaranteed_for_scene() {
    // A (entry, sets run.a unconditionally) <- B (after visited(A key)).
    // `run.a` is B's ONLY route's unconditional write, so `run.a ∈
    // Guaranteed(B)` — the envelope table must print it under a header
    // carrying the §2.6 "under your declared routes" qualifier.
    let dir = temp_dir("scenario-envelope-guaranteed");
    write(&dir, "a.lute", &scene_sets_run_a("a"));
    write(&dir, "b.lute", &scene_after("b", "a.s01ep01"));

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "b.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("run.a"), "{out_text}");
    assert!(out_text.contains("under your declared routes"), "{out_text}");
}

#[test]
fn scenario_envelope_quest_without_after_shows_defaults_note() {
    let dir = temp_dir("scenario-envelope-quest-defaults");
    write(&dir, "q.lute", &quest_no_after("someQuest"));

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "quest:someQuest"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("declaring `after`"), "{out_text}");
}

#[test]
fn scenario_envelope_scene_falls_back_to_dd_floor_when_graph_cycle_empties_envs() {
    // p <-> q form a prerequisite cycle -- `assemble_graph` empties the
    // WHOLE root's `topo_order`, so `envelope::propagate` never inserts
    // an `Env` for `p` (or `q`) into `envs` at all. `scenario envelope
    // p.s01ep01` must fall back to the D/D schema-default floor (mirrors
    // T12's quest fallback), never empty tables.
    let dir = temp_dir("scenario-envelope-cycle-floor");
    let p = "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\n\
             after: 'visited(\"q.s01ep01\")'\nstate:\n  run.known: { type: number, default: 0 }\n\
             ---\n## Shot 1.\n@narrator: hi\n";
    let q = "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\n\
             after: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "p.lute", p);
    write(&dir, "q.lute", q);

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "p.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    // Anchor on the actual table HEADINGS ("Guaranteed (safe to read" /
    // "Possible (set on"), not bare "Guaranteed"/"Possible" -- the prepended
    // E-CONN-CYCLE note now also contains the words "Guaranteed/Possible".
    let after_heading = out_text
        .split_once("Guaranteed (safe to read")
        .unwrap_or_else(|| panic!("no Guaranteed heading: {out_text}"))
        .1;
    let guaranteed_block = after_heading
        .split_once("Possible (set on")
        .unwrap_or_else(|| panic!("no Possible heading after Guaranteed: {out_text}"))
        .0;
    assert!(
        guaranteed_block.contains("run.known"),
        "a cycle-emptied envs entry must fall back to the D/D schema-default floor, not empty \
         tables, got Guaranteed block:\n{guaranteed_block}\nfull output:\n{out_text}"
    );
}

#[test]
fn scenario_envelope_cyclic_project_announces_e_conn_cycle_degraded() {
    // C-honesty (persona review): on a project WITH a prerequisite cycle,
    // `reach` already prints an explicit `E-CONN-CYCLE` verdict, but
    // `envelope` used to print only the D/D floor tables with NO indication
    // the emptiness/floor was CYCLE-caused -- silently indistinguishable
    // from a genuinely-empty envelope. `envelope` must now announce the
    // cycle-degraded state explicitly, mirroring `reach`'s wording, and
    // (like `reach`'s cyclic case) still exit 0.
    let dir = temp_dir("scenario-envelope-cycle-announce");
    let p = "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\n\
             after: 'visited(\"q.s01ep01\")'\nstate:\n  run.known: { type: number, default: 0 }\n\
             ---\n## Shot 1.\n@narrator: hi\n";
    let q = "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\n\
             after: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "p.lute", p);
    write(&dir, "q.lute", q);

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "p.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "cyclic envelope must exit 0 like reach: {out_text}");
    // The explicit cycle note, mirroring `reach`'s E-CONN-CYCLE wording.
    assert!(out_text.contains("E-CONN-CYCLE"), "envelope must name the cycle code: {out_text}");
    assert!(
        out_text.contains("cycle"),
        "envelope must explain the emptiness is cycle-caused, not silently empty: {out_text}"
    );
    // Cross-check the same project's `reach` DOES announce the cycle, so the
    // two views are now consistent rather than reach-only.
    let reach_out = run(&["scenario", dir.to_str().unwrap(), "reach", "p.s01ep01"]);
    assert!(stdout(&reach_out).contains("E-CONN-CYCLE"), "{}", stdout(&reach_out));
}

#[test]
fn scenario_envelope_header_carries_pre_entry_label() {
    // Persona review comprehension nit: the Guaranteed/Possible tables report
    // state available when control REACHES the node, BEFORE the node's own
    // writes -- both personas misread it as the node's own effects. The
    // header must carry an explicit pre-entry label.
    let dir = temp_dir("scenario-envelope-pre-entry");
    write(&dir, "a.lute", &scene_sets_run_a("a"));
    write(&dir, "b.lute", &scene_after("b", "a.s01ep01"));

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "b.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("pre-entry"), "header must carry the pre-entry label: {out_text}");
    assert!(
        out_text.contains("before its own writes"),
        "header must clarify the tables are before the node's own writes: {out_text}"
    );
}

#[test]
fn connectivity_t15_corpus_example_envelope_shows_guaranteed_cross_scene_read() {
    // Connectivity T15 grounding: docs/examples/connected-outro.lute
    // declares `after: visited("kestrel.s01ep01")` (connected-intro.lute's
    // canonical key) and reads `run.sawOverlook` -- a run-tier path with NO
    // schema default, guaranteed ONLY because the sole declared route
    // unconditionally `::set`s it. `lute scenario envelope` must report it
    // under the Guaranteed table, carrying the §2.6 declared-routes
    // qualifier.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    let out = run(&["scenario", examples.to_str().unwrap(), "envelope", "kestrel.s01ep02"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("under your declared routes"), "{out_text}");
    // Pin the path to the GUARANTEED section specifically -- the printer
    // always emits a "Guaranteed" heading and `run.sawOverlook` ALSO
    // appears in the (superset) "Possible" section regardless of whether
    // it is guaranteed, so a bare substring check on the whole output
    // would stay green even if the path dropped OUT of Guaranteed into
    // Possible-only. Slice the block between the "Guaranteed" heading and
    // the next ("Possible") heading and require the path inside THAT
    // slice.
    let after_heading = out_text
        .split_once("Guaranteed")
        .unwrap_or_else(|| panic!("no Guaranteed heading: {out_text}"))
        .1;
    let guaranteed_block = after_heading
        .split_once("Possible")
        .unwrap_or_else(|| panic!("no Possible heading after Guaranteed: {out_text}"))
        .0;
    assert!(
        guaranteed_block.contains("run.sawOverlook"),
        "run.sawOverlook must be listed WITHIN the Guaranteed section specifically \
         (cross-scene proof, not merely Possible), got Guaranteed block:\n{guaranteed_block}\n\
         full output:\n{out_text}"
    );
}

// --- additional T14 contract coverage ------------------------------------

#[test]
fn bare_scenario_prints_nonempty_topological_graph() {
    let dir = temp_dir("scenario-bare-graph");
    write(&dir, "a.lute", &scene_sets_run_a("a"));
    write(&dir, "b.lute", &scene_after("b", "a.s01ep01"));

    let out = run(&["scenario", dir.to_str().unwrap()]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(!out_text.trim().is_empty(), "bare scenario must print a nonempty graph");
    assert!(out_text.contains("scene(a.s01ep01)"), "{out_text}");
    assert!(out_text.contains("scene(b.s01ep01)"), "{out_text}");
    // Topological ordering: A (no prerequisite) must be reported before B
    // (after visited(A)) — a real edge, not just node names dumped flat.
    let a_pos = out_text.find("scene(a.s01ep01)").unwrap();
    let b_pos = out_text.find("scene(b.s01ep01)").unwrap();
    assert!(a_pos < b_pos, "A must precede B in topological order: {out_text}");
}

#[test]
fn scenario_reach_prints_reachability_verdict() {
    let dir = temp_dir("scenario-reach");
    write(&dir, "a.lute", &scene_sets_run_a("a"));
    write(&dir, "b.lute", &scene_after("b", "a.s01ep01"));

    let out = run(&["scenario", dir.to_str().unwrap(), "reach", "b.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("Reachable"), "{out_text}");
    // The reachability CLAIM itself carries the §2.6 declared-routes hedge.
    assert!(out_text.contains("under your declared routes"), "{out_text}");
    // The declared prerequisite is shown structurally (not flattened away).
    assert!(out_text.contains("visited(\"a.s01ep01\")"), "{out_text}");
}

#[test]
fn scenario_envelope_explicitly_lists_possible_not_guaranteed_warning_read() {
    // Contract #2: T11 computes-and-drops the `Possible \ Guaranteed`
    // warning-grade read from default `check-project`; T14 must RE-derive
    // and print it explicitly for the requested node, not merely dump the
    // two sets and expect a diff. `a` unconditionally sets `run.z`, `b`
    // never does; `x after: visited(a) || visited(b)` reads `run.z`, so
    // `run.z ∈ Possible(x) \ Guaranteed(x)` — a concrete warning READ.
    let dir = temp_dir("scenario-envelope-warning-read");
    write(&dir, "a.lute", &scene_sets_run_z("a"));
    write(
        &dir,
        "b.lute",
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n",
    );
    write(&dir, "x.lute", &scene_reading_run_z_after_or("x", "a.s01ep01", "b.s01ep01"));

    // Default `check-project` must NOT surface this warning grade at all
    // (T11's compute-and-drop default stays unchanged).
    let cp = run(&["check-project", dir.to_str().unwrap()]);
    assert!(cp.status.success(), "{}", stdout(&cp));
    let cp_text = stdout(&cp);
    assert!(!cp_text.contains("E-STATE-MAYBE-UNAVAILABLE"), "{cp_text}");

    // `lute scenario envelope` explicitly surfaces it for the requested node.
    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "x.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("run.z"), "{out_text}");
    assert!(
        out_text.contains("not yet guaranteed") || out_text.contains("Possible"),
        "must explicitly name the Possible\\Guaranteed warning class: {out_text}"
    );
}

// --- "Also confirm" (Step 3): check-project's full §6 diagnostics table --

#[test]
fn check_project_unknown_node_exits_nonzero_with_e_conn_unknown_node() {
    let dir = temp_dir("scenario-check-project-unknown-node");
    write(&dir, "x.lute", &scene_after("x", "doesNotExist.s01ep01"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    let out_text = stdout(&out);
    assert!(!out.status.success(), "unknown node must exit non-zero: {out_text}");
    assert!(out_text.contains("E-CONN-UNKNOWN-NODE"), "{out_text}");
}

// --- Main review fixes -----------------------------------------------

#[test]
fn scenario_reach_on_cross_root_duplicate_node_id_is_ambiguous_not_silently_picked() {
    // Two INDEPENDENT project roots (subA/subB, each its own
    // `lute.project.yaml`), each declaring a scene with the SAME
    // character/season/episode triad -- the SAME canonical scene key
    // `c.s01ep01` in both. A scene/quest id is only unique WITHIN one
    // resolved root (dsl §2.3/§6.3); `lute scenario` must reject this as
    // ambiguous rather than silently reporting only the first root.
    let dir = temp_dir("scenario-cross-root-dup");
    write(&dir, "lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subA/lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subA/scene.lute", &scene_sets_run_a("c"));
    write(&dir, "subB/lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subB/scene.lute", &scene_sets_run_a("c"));
    let sub_a = dir.join("subA").to_string_lossy().into_owned();
    let sub_b = dir.join("subB").to_string_lossy().into_owned();

    let reach_out = run(&["scenario", dir.to_str().unwrap(), "reach", "c.s01ep01"]);
    assert!(!reach_out.status.success(), "{}", stdout(&reach_out));
    let reach_err = stderr(&reach_out);
    assert!(reach_err.contains("ambiguous"), "{reach_err}");
    assert!(reach_err.contains("2"), "must name the match count: {reach_err}");
    // The fix's observable contract is naming EVERY matching root, not
    // merely a count -- assert BOTH root paths appear, not just one.
    assert!(reach_err.contains(&sub_a), "must name subA's root path: {reach_err}");
    assert!(reach_err.contains(&sub_b), "must name subB's root path: {reach_err}");

    let env_out = run(&["scenario", dir.to_str().unwrap(), "envelope", "c.s01ep01"]);
    assert!(!env_out.status.success(), "{}", stdout(&env_out));
    let env_err = stderr(&env_out);
    assert!(env_err.contains("ambiguous"), "{env_err}");
    assert!(env_err.contains(&sub_a), "must name subA's root path: {env_err}");
    assert!(env_err.contains(&sub_b), "must name subB's root path: {env_err}");
}

#[test]
fn scenario_reach_referenced_undeclared_node_reports_unknown_not_reachable_or_cycle() {
    // `completed("missingQuest")` and `visited("missingScene")` target ids
    // that are NEVER declared anywhere in the project. Before the fix, the
    // referenced-node verdict list mislabeled an undeclared quest as a
    // "plain quest, reachable" and an undeclared scene as evidence of an
    // `E-CONN-CYCLE`. Both must instead read Unknown, consistent with
    // `E-CONN-UNKNOWN-NODE`.
    let dir = temp_dir("scenario-undeclared-reference");
    write(&dir, "x.lute", &scene_after("x", "__never_declared_scene__"));
    // Rewrite `x.lute` to reference a quest instead via `completed(...)`
    // is covered by a separate doc so both atom kinds are exercised.
    write(
        &dir,
        "q.lute",
        "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\n\
         after: 'completed(\"missingQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out_scene = run(&["scenario", dir.to_str().unwrap(), "reach", "x.s01ep01"]);
    let text_scene = stdout(&out_scene);
    assert!(out_scene.status.success(), "{text_scene}");
    assert!(text_scene.contains("scene(__never_declared_scene__)"), "{text_scene}");
    let referenced_line_scene = text_scene
        .lines()
        .find(|l| l.contains("scene(__never_declared_scene__)"))
        .unwrap();
    assert!(referenced_line_scene.contains("Unknown"), "{referenced_line_scene}");
    assert!(!referenced_line_scene.contains("E-CONN-CYCLE"), "{referenced_line_scene}");

    let out_quest = run(&["scenario", dir.to_str().unwrap(), "reach", "q.s01ep01"]);
    let text_quest = stdout(&out_quest);
    assert!(out_quest.status.success(), "{text_quest}");
    let referenced_line_quest =
        text_quest.lines().find(|l| l.contains("quest(missingQuest)")).unwrap();
    assert!(referenced_line_quest.contains("Unknown"), "{referenced_line_quest}");
    assert!(!referenced_line_quest.contains("Reachable"), "{referenced_line_quest}");
}

#[test]
fn scenario_envelope_labels_scene_and_quest_possible_guaranteed_differently() {
    // Scene envelope's Possible\Guaranteed section is the T11 warning-grade
    // READ-SITE class (suppressed by default in `check-project`); a quest
    // envelope's is plain SET-difference inventory (`check_envelope` is
    // scene-only, so there is no read-site data for a quest at all). The
    // two sections must NOT share the same "suppressed... warning-grade
    // reads" wording.
    let dir = temp_dir("scenario-envelope-label-scene");
    write(&dir, "a.lute", &scene_sets_run_a("a"));
    write(&dir, "b.lute", &scene_after("b", "a.s01ep01"));
    let out_scene = run(&["scenario", dir.to_str().unwrap(), "envelope", "b.s01ep01"]);
    let text_scene = stdout(&out_scene);
    assert!(out_scene.status.success(), "{text_scene}");
    assert!(text_scene.contains("warning-grade reads"), "{text_scene}");

    let qdir = temp_dir("scenario-envelope-label-quest");
    write(&qdir, "q.lute", &quest_no_after("labelQuest"));
    let out_quest = run(&["scenario", qdir.to_str().unwrap(), "envelope", "quest:labelQuest"]);
    let text_quest = stdout(&out_quest);
    assert!(out_quest.status.success(), "{text_quest}");
    assert!(!text_quest.contains("warning-grade reads"), "{text_quest}");
    assert!(text_quest.contains("inventory only"), "{text_quest}");
}

#[test]
fn scenario_envelope_ambiguous_quest_id_prints_ambiguity_note_not_arbitrary_pick() {
    // Two `<quest id="dup">` declarations in ONE resolved root (distinct
    // files, no import edge -- already an `E-QUEST-ID-DUP` case elsewhere).
    // `envelope quest:dup` must not silently report either declaration's
    // envelope as if it were unambiguous.
    let dir = temp_dir("scenario-ambiguous-quest-envelope");
    write(
        &dir,
        "q1.lute",
        "---\nkind: quest\nstate:\n  run.a: { type: bool, default: false }\n---\n\
         <quest id=\"dup\">\n<objective id=\"o\" done=\"run.a\"/>\n</quest>\n",
    );
    write(
        &dir,
        "q2.lute",
        "---\nkind: quest\nstate:\n  run.b: { type: bool, default: false }\n---\n\
         <quest id=\"dup\">\n<objective id=\"o\" done=\"run.b\"/>\n</quest>\n",
    );

    let out = run(&["scenario", dir.to_str().unwrap(), "envelope", "quest:dup"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    assert!(out_text.contains("ambiguous"), "{out_text}");
    assert!(out_text.contains("E-QUEST-ID-DUP"), "{out_text}");
    // The exact regression this test guards against: printing the note
    // AFTER (or alongside) one declaration's envelope table would also
    // satisfy the two assertions above -- assert NO envelope table (either
    // declaration's `run.a`/`run.b` "Guaranteed" section) is shown at all.
    assert!(!out_text.contains("Guaranteed"), "{out_text}");
    assert!(!out_text.contains("run.a") && !out_text.contains("run.b"), "{out_text}");
}

#[test]
fn scenario_reach_and_envelope_on_duplicate_scene_key_prints_ambiguity_note_not_arbitrary_pick() {
    // Symmetric to the duplicate-quest-id fix above: two scene documents
    // computing the SAME canonical key (`E-CONN-EPISODE-ID-DUP`, T3) within
    // ONE resolved root. `key_set[key]` internally anchors at the FIRST
    // occurrence (mirrors `assemble_graph`'s own precedent) -- but `lute
    // scenario` must never surface that internal pick to the user as if it
    // were the single authoritative answer; both `reach` and `envelope`
    // must refuse with an explicit ambiguity note instead.
    let dir = temp_dir("scenario-ambiguous-scene-key");
    write(
        &dir,
        "a1.lute",
        "---\nkind: scene\ncharacter: dup\nseason: 1\nepisode: 1\n\
         state:\n  run.x: { type: bool, default: false }\n---\n\
         ## Shot 1.\n::set{run.x = true}\n",
    );
    write(
        &dir,
        "a2.lute",
        "---\nkind: scene\ncharacter: dup\nseason: 1\nepisode: 1\n\
         state:\n  run.y: { type: bool, default: false }\n---\n\
         ## Shot 1.\n::set{run.y = true}\n",
    );

    let reach_out = run(&["scenario", dir.to_str().unwrap(), "reach", "dup.s01ep01"]);
    let reach_text = stdout(&reach_out);
    assert!(reach_out.status.success(), "{reach_text}");
    assert!(reach_text.contains("ambiguous"), "{reach_text}");
    assert!(reach_text.contains("E-CONN-EPISODE-ID-DUP"), "{reach_text}");
    // The exact regression this guards against: printing the note
    // AFTER/alongside the normal reach verdict/formula output would also
    // satisfy the assertions above -- assert neither appears at all.
    assert!(!reach_text.contains("verdict:"), "{reach_text}");
    assert!(!reach_text.contains("after:"), "{reach_text}");

    let env_out = run(&["scenario", dir.to_str().unwrap(), "envelope", "dup.s01ep01"]);
    let env_text = stdout(&env_out);
    assert!(env_out.status.success(), "{env_text}");
    assert!(env_text.contains("ambiguous"), "{env_text}");
    assert!(env_text.contains("E-CONN-EPISODE-ID-DUP"), "{env_text}");
    // Neither file's own data (nor a normal envelope table at all) leaks
    // through as if it were authoritative -- the exact regression this
    // test is named to prevent.
    assert!(!env_text.contains("Guaranteed"), "{env_text}");
    assert!(
        !env_text.contains("run.x") && !env_text.contains("run.y"),
        "must not silently show one file's data: {env_text}"
    );
}

#[test]
fn scenario_reach_formula_rendering_escapes_embedded_quote_and_backslash() {
    // `format_prereq` renders the PARSED atom id back to CEL-like text
    // (Main review): a raw `format!("\"{id}\"")` interpolation would
    // render an id containing an embedded `"`/`\` verbatim, breaking the
    // printed structure's own quoting into something malformed/misleading
    // (`visited("has"quote\slash")` — the closing `"` after `has` looks
    // like the STRING'S OWN closing quote, not part of the id). The CEL
    // `after` value below is itself properly CEL-escaped
    // (`\"` -> `"`, `\\` -> `\`), so `parse_prereq` resolves the atom id to
    // the literal Rust string `has"quote\slash` — the rendered output must
    // re-escape it the same way, not leak it raw.
    let dir = temp_dir("scenario-formula-escaping");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         after: 'visited(\"has\\\"quote\\\\slash\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["scenario", dir.to_str().unwrap(), "reach", "x.s01ep01"]);
    let out_text = stdout(&out);
    assert!(out.status.success(), "{out_text}");
    // Properly escaped: a backslash before the embedded quote, and a
    // doubled backslash for the literal backslash in the id.
    assert!(
        out_text.contains(r#"visited("has\"quote\\slash")"#),
        "must render the atom id properly escaped, not raw: {out_text}"
    );
}

// --- whole-branch review fix: `quest:` prefix / scene-key collision -------

#[test]
fn scene_key_beginning_with_quest_prefix_is_selectable_via_explicit_scene_prefix() {
    // A scene's canonical key (`{character}.{episodeId}`) is an
    // unvalidated, author-controlled string -- `character` accepts any
    // YAML scalar, no charset restriction -- so it CAN literally begin
    // with `quest:` (colliding with the CLI's OWN `quest:<id>` selector
    // syntax). `quest:<id>` stays authoritative for an explicit quest
    // lookup (never silently re-tried as a scene), so such a scene must
    // be reachable via the symmetric explicit `scene:<key>` prefix
    // instead -- never permanently unselectable.
    let dir = temp_dir("scenario-quest-prefix-scene-key");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: \"quest:zzz\"\nseason: 1\nepisode: 1\n\
         state:\n  run.known: { type: bool, default: true }\n---\n\
         ## Shot 1.\n@narrator: hi\n",
    );

    let reach_out =
        run(&["scenario", dir.to_str().unwrap(), "reach", "scene:quest:zzz.s01ep01"]);
    let reach_text = stdout(&reach_out);
    assert!(reach_out.status.success(), "{reach_text}");
    assert!(reach_text.contains("scene(quest:zzz.s01ep01)"), "{reach_text}");
    assert!(reach_text.contains("Reachable"), "{reach_text}");

    let env_out =
        run(&["scenario", dir.to_str().unwrap(), "envelope", "scene:quest:zzz.s01ep01"]);
    let env_text = stdout(&env_out);
    assert!(env_out.status.success(), "{env_text}");
    assert!(env_text.contains("scene(quest:zzz.s01ep01)"), "{env_text}");
    assert!(env_text.contains("run.known"), "{env_text}");

    // Without the explicit `scene:` prefix, the SAME text is treated as
    // an authoritative (never-guessed) quest lookup -- and since no quest
    // `zzz.s01ep01` is declared, it must fail as unknown, NOT silently
    // fall through to the colliding scene.
    let bare_out = run(&["scenario", dir.to_str().unwrap(), "reach", "quest:zzz.s01ep01"]);
    assert!(!bare_out.status.success(), "{}", stdout(&bare_out));
    assert!(stderr(&bare_out).contains("unknown node"), "{}", stderr(&bare_out));
}

#[test]
fn scenario_bare_nodeid_matching_both_scene_key_and_quest_id_is_rejected_as_ambiguous() {
    // A BARE (unprefixed) nodeId that matches BOTH a declared scene key
    // and a declared quest id in the same root is genuinely ambiguous --
    // neither kind is silently preferred; the user must disambiguate with
    // an explicit `scene:`/`quest:` prefix.
    let dir = temp_dir("scenario-bare-both-match");
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: dup\nseason: 1\nepisode: 1\n---\n\
         ## Shot 1.\n@narrator: hi\n",
    );
    write(&dir, "q.lute", &quest_no_after("dup.s01ep01"));

    let out = run(&["scenario", dir.to_str().unwrap(), "reach", "dup.s01ep01"]);
    assert!(!out.status.success(), "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("ambiguous"), "{err}");
    assert!(err.contains("scene:dup.s01ep01"), "{err}");
    assert!(err.contains("quest:dup.s01ep01"), "{err}");
}

/// Defect B propagation: `lute scenario reach` must ALSO report a quest
/// with a dead REQUIRED objective as `Unreachable` (both directly, and
/// transitively through a scene's `completed(Q)` gate) -- the exact
/// surface `check-project`'s own `E-CONN-UNREACHABLE` shares its
/// `reach`/`unreachable_quests` data with.
#[test]
fn scenario_reach_reports_unreachable_for_dead_required_objective_quest() {
    let dir = temp_dir("scenario-reach-dead-required-objective");
    write(
        &dir,
        "deadquest.lute",
        "---\nkind: quest\n---\n<quest id=\"deadQuest\" start=\"true\">\n\
         <objective id=\"o\" done=\"false\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"deadQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out_quest = run(&["scenario", dir.to_str().unwrap(), "reach", "quest:deadQuest"]);
    let text_quest = stdout(&out_quest);
    assert!(text_quest.contains("Unreachable"), "{text_quest}");
    assert!(text_quest.contains("E-OBJECTIVE-UNSATISFIABLE"), "{text_quest}");

    let out_scene = run(&["scenario", dir.to_str().unwrap(), "reach", "scene:repro.s01ep01"]);
    let text_scene = stdout(&out_scene);
    assert!(text_scene.contains("Unreachable"), "{text_scene}");
    assert!(
        text_scene.contains("E-CONN-UNREACHABLE"),
        "the gated scene's own verdict must be E-CONN-UNREACHABLE: {text_scene}"
    );
}
