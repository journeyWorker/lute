//! 0.49.0 §6: connectivity and constraints. #181 (a start cycle is
//! `E-QUEST-START-CYCLE`, every edge is drawn, an externally entered cycle is
//! no deadlock), #182 (`E-QUEST-START-SELF`; a self-locked quest and its
//! dependents are not `Reachable`), #183 (`constraints` on an invalid
//! manifest), #184 (one reach claim; reasons; no self counterexample), #185
//! (no `related` on a holding constraint). Each project is the issue's repro.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const SCHEMA: &str =
    "state:\n  run.flag: { type: bool, default: false }\ncast:\n  mira: { name: Mira }\n";
const PROFILES: &str =
    "profiles:\n  game:\n    plugins: {}\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n";

fn quest(id: &str, start: &str) -> String {
    format!(
        "<quest id=\"{id}\" tier=\"run\" title=\"Q\" start=\"{start}\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n\n"
    )
}

/// A project with `quests` in one quest document, `constraints` appended to
/// the manifest, and `extra` files.
fn project(
    name: &str,
    quests: &[(&str, &str)],
    constraints: &str,
    extra: &[(&str, &str)],
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(name);
    let body: String = quests.iter().map(|(id, start)| quest(id, start)).collect();
    let mut files = vec![
        (
            "lute.project.yaml".to_string(),
            format!("defaultProfile: game\n{PROFILES}{constraints}"),
        ),
        ("schema/world.schema.yaml".to_string(), SCHEMA.to_string()),
        (
            "quests/q.lute".to_string(),
            format!("---\nkind: quest\nid: acme.quests\n---\n\n{body}"),
        ),
    ];
    files.extend(extra.iter().map(|(p, t)| (p.to_string(), t.to_string())));
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn json(dir: &Path, args: &[&str]) -> (i32, serde_json::Value) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout))),
    )
}

const ENTRY: (&str, &str) = (
    "scenes/entry.lute",
    "---\nkind: scene\nid: entry\n---\n\n## E {#e}\n\n@mira: E.\n",
);

// ---- #181 -------------------------------------------------------------

#[test]
fn a_two_quest_start_cycle_is_an_error_with_both_edges_drawn() {
    let dir = project(
        "connectivity-049-181",
        &[
            ("cycA", "quest.cycB.state == 'active'"),
            ("cycB", "quest.cycA.state == 'complete'"),
            ("after", "quest.cycA.state == 'complete'"),
        ],
        "",
        &[],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "./quests/q.lute:6:46: error [E-QUEST-START-CYCLE] quest start cycle: quest(cycA) -> \
             quest(cycB) -> quest(cycA) — each `start` waits for the next and no source outside \
             the cycle starts any of them"
        ),
        "{out}"
    );
    assert_eq!(out.matches("E-QUEST-START-CYCLE").count(), 1, "{out}");

    let (_, out) = run(&dir, &["scenario", "--project", "."]);
    assert!(
        out.contains("    quest(cycA) -> quest(cycB) [start]\n"),
        "{out}"
    );
    assert!(
        out.contains("    quest(cycB) -> quest(cycA) [start]\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "    (unlayered -- part of a start cycle, E-QUEST-START-CYCLE): quest(cycA), quest(cycB)\n"
        ),
        "{out}"
    );
    assert!(!out.contains("layer 0: quest(cycB)"), "{out}");

    let reason = "Unknown — this quest's `start` waits on a start cycle (E-QUEST-START-CYCLE): \
                  quest(cycA) -> quest(cycB) -> quest(cycA); it can never start under your \
                  declared routes. [unknown]";
    let (_, out) = run(
        &dir,
        &[
            "scenario",
            "--project",
            ".",
            "reach",
            "--node",
            "quest:cycA",
        ],
    );
    assert!(out.contains(&format!("  verdict: {reason}")), "{out}");
    // Downstream only through the cycle: the same reason.
    let (_, out) = run(
        &dir,
        &[
            "scenario",
            "--project",
            ".",
            "reach",
            "--node",
            "quest:after",
        ],
    );
    assert!(out.contains(&format!("  verdict: {reason}")), "{out}");

    let (_, graph) = json(&dir, &["scenario", "--project", ".", "--json"]);
    let nodes = graph["roots"][0]["nodes"].as_array().unwrap();
    let start_cycle: Vec<&str> = nodes
        .iter()
        .filter(|n| n["startCycle"] == true)
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(start_cycle, ["quest(cycA)", "quest(cycB)"], "{graph}");
    let (_, dot) = run(&dir, &["scenario", "--project", ".", "--format", "dot"]);
    assert_eq!(dot.matches("class=\"startCycle\"").count(), 2, "{dot}");
}

#[test]
fn an_externally_entered_cycle_is_no_deadlock_in_either_order() {
    // The quest with the outside source sorts after (`cycB`) and before
    // (`cycA`) the other, so the closing edge lands on either side.
    for (name, quests) in [
        (
            "connectivity-049-ext-b",
            [
                ("cycA", "quest.cycB.state == 'active'"),
                ("cycB", "quest.cycA.state == 'active' || visited('entry')"),
            ],
        ),
        (
            "connectivity-049-ext-a",
            [
                ("cycA", "quest.cycB.state == 'active' || visited('entry')"),
                ("cycB", "quest.cycA.state == 'active'"),
            ],
        ),
    ] {
        let dir = project(name, &quests, "", &[ENTRY]);
        let (code, out) = run(&dir, &["check", "--project", "."]);
        assert_eq!(code, 0, "{name}: {out}");
        assert!(!out.contains("E-QUEST-START-CYCLE"), "{name}: {out}");
        let (_, out) = run(&dir, &["scenario", "--project", "."]);
        assert!(
            out.contains("    quest(cycA) -> quest(cycB) [start]\n"),
            "{name}: {out}"
        );
        assert!(
            out.contains("    quest(cycB) -> quest(cycA) [start]\n"),
            "{name}: {out}"
        );
        assert!(out.contains("    layer 0: scene(entry)"), "{name}: {out}");
        assert!(!out.contains("unlayered"), "{name}: {out}");
        for node in ["quest:cycA", "quest:cycB"] {
            let (_, out) = run(
                &dir,
                &["scenario", "--project", ".", "reach", "--node", node],
            );
            assert!(
                out.contains("  verdict: Reachable — a satisfiable route exists"),
                "{name} {node}: {out}"
            );
        }
    }
}

// ---- #182 -------------------------------------------------------------

#[test]
fn a_start_that_reads_its_own_quest_is_an_error_and_never_reachable() {
    let constraints = "constraints:\n  - id: locked-reachable\n    kind: reachable\n    node: quest:locked\n  - id: next-reachable\n    kind: reachable\n    node: quest:next\n";
    let dir = project(
        "connectivity-049-182",
        &[
            ("locked", "quest.locked.state == 'active'"),
            ("next", "quest.locked.state == 'complete'"),
        ],
        constraints,
        &[],
    );
    // Single-file: the per-document check.
    let (code, out) = run(&dir, &["check", "quests/q.lute", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "quests/q.lute:6:48: error [E-QUEST-START-SELF] `start` of quest `locked` reads its \
             own state (`quest.locked.state`), which has no value before the quest starts, so \
             this condition can never start it as intended; a quest that only the host grants \
             is written `accept=\"external\"` without a `start`"
        ),
        "{out}"
    );
    let (_, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(out.matches("E-QUEST-START-SELF").count(), 1, "{out}");
    assert!(!out.contains("E-QUEST-START-CYCLE"), "{out}");

    let (_, out) = run(&dir, &["scenario", "--project", "."]);
    assert!(
        out.contains("    quest(locked) -> quest(locked) [start]\n"),
        "{out}"
    );
    let (_, out) = run(
        &dir,
        &[
            "scenario",
            "--project",
            ".",
            "reach",
            "--node",
            "quest:locked",
        ],
    );
    assert!(
        out.contains(
            "  verdict: Unknown — this quest's `start` reads its own state (E-QUEST-START-SELF)"
        ),
        "{out}"
    );
    let (_, out) = run(
        &dir,
        &[
            "scenario",
            "--project",
            ".",
            "reach",
            "--node",
            "quest:next",
        ],
    );
    assert!(
        out.contains(
            "  verdict: Unknown — this quest's `start` waits on quest(locked), whose `start` \
             reads its own state (E-QUEST-START-SELF)"
        ),
        "{out}"
    );
    let (code, out) = run(&dir, &["constraints", "--project", "."]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("locked-reachable: reachable unknown [unknown]\n"),
        "{out}"
    );
    assert!(
        out.contains("next-reachable: reachable unknown [unknown]\n"),
        "{out}"
    );
    assert!(!out.contains("holds"), "{out}");
}

#[test]
fn a_self_read_in_a_non_anchoring_conjunct_is_still_reported() {
    let dir = project(
        "connectivity-049-182-gate",
        &[("gate", "quest.gate.state != 'failed' && run.flag")],
        "",
        &[],
    );
    let (code, out) = run(&dir, &["check", "quests/q.lute", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("reads its own state (`quest.gate.state`)"),
        "{out}"
    );
}

// ---- #183 -------------------------------------------------------------

#[test]
fn constraints_on_an_invalid_manifest_fails_with_the_manifest_error() {
    let dir = lute_scratch::ScratchDir::named("connectivity-049-183");
    for (rel, text) in [
        (
            "lute.project.yaml",
            format!("{PROFILES}constraints:\n  - id: first-reachable\n    kind: reachable\n    node: quest:first\n"),
        ),
        ("schema/world.schema.yaml", SCHEMA.to_string()),
        (
            "quests/q.lute",
            format!("---\nkind: quest\nid: acme.quests\n---\n\n{}", quest("first", "true")),
        ),
    ] {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let line =
        "./lute.project.yaml:1:1: error [E-MANIFEST] lute.project.yaml needs `defaultProfile:`";
    let (_, check) = run(&dir, &["check", "--project", "."]);
    assert!(check.contains(line), "{check}");
    let (code, out) = run(&dir, &["constraints", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains(line), "{out}");

    let (code, report) = json(&dir, &["constraints", "--project", ".", "--json"]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["ok"], false, "{report}");
    assert_eq!(report["constraints"], serde_json::json!([]), "{report}");
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{report}");
    assert_eq!(diagnostics[0]["code"], "E-MANIFEST", "{report}");
    assert_eq!(diagnostics[0]["path"], "./lute.project.yaml", "{report}");
}

// ---- #184 / #185 ------------------------------------------------------

#[test]
fn constraint_verdicts_carry_subject_and_reason_and_agree_with_reach() {
    let constraints = "constraints:\n  - id: first-reachable\n    kind: reachable\n    node: quest:first\n  - id: first-completes\n    kind: completable\n    quest: first\n  - id: second-reachable\n    kind: reachable\n    node: quest:second\n";
    let dir = project(
        "connectivity-049-184",
        &[
            ("first", "true"),
            ("second", "quest.first.state == 'complete'"),
        ],
        constraints,
        &[],
    );
    // `first` alone (no dependent) is unanchored; with one, it is reachable.
    let lone = project(
        "connectivity-049-184-lone",
        &[("first", "true")],
        "constraints:\n  - id: first-reachable\n    kind: reachable\n    node: quest:first\n  - id: first-completes\n    kind: completable\n    quest: first\n",
        &[],
    );
    let (code, out) = run(&lone, &["constraints", "--project", "."]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "first-reachable: reachable holds [proven]\nfirst-completes: completable unknown \
         [unknown]\n  reason: no play or test script was run (--all-scripts / --script)\n"
    );
    let (_, report) = json(&lone, &["constraints", "--project", ".", "--json"]);
    assert_eq!(report["schemaVersion"], "0.49.0.constraints");
    assert_eq!(report["diagnostics"], serde_json::json!([]));
    let c = &report["constraints"];
    assert_eq!(c[0]["subject"], "quest:first");
    assert_eq!(c[0]["verdict"], "holds");
    assert_eq!(c[0]["evidence"], "proven");
    assert_eq!(
        c[0]["reason"],
        "unanchored: available from the start of play"
    );
    assert_eq!(c[1]["subject"], "quest:first");
    assert_eq!(c[1]["verdict"], "unknown");
    for r in c.as_array().unwrap() {
        assert_eq!(r["counterexamples"], serde_json::json!([]), "{r}");
        assert_eq!(r["related"], serde_json::json!([]), "{r}");
    }

    // #185: a holding `reachable` has no related diagnostic.
    let (_, report) = json(&dir, &["constraints", "--project", ".", "--json"]);
    let second = &report["constraints"][2];
    assert_eq!(second["id"], "second-reachable");
    assert_eq!(second["verdict"], "holds");
    assert_eq!(second["reason"], "connectivity proves reachable");
    assert_eq!(second["related"], serde_json::json!([]), "{report}");
}

#[test]
fn a_constraint_declaration_error_is_a_diagnostic() {
    let dir = project(
        "connectivity-049-decl",
        &[("first", "true")],
        "constraints:\n  - id: bad\n    kind: reachable\n  - id: worse\n    kind: nope\n    node: quest:first\n",
        &[],
    );
    let (code, report) = json(&dir, &["constraints", "--project", ".", "--json"]);
    assert_eq!(code, 1, "{report}");
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(!diagnostics.is_empty(), "{report}");
    for d in diagnostics {
        assert_eq!(d["code"], "E-CONSTRAINT-DECL", "{report}");
        assert_eq!(d["path"], "./lute.project.yaml", "{report}");
    }
    // No `constraints` entry is a diagnostic any more.
    assert!(
        report["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("code").is_none()),
        "{report}"
    );
}

// ---- F4 review --------------------------------------------------------

/// A project whose quest document body is `body`, plus `extra` files.
fn project_body(name: &str, body: &str, extra: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = project(name, &[], "", extra);
    std::fs::write(
        dir.join("quests/q.lute"),
        format!("---\nkind: quest\nid: acme.quests\n---\n\n{body}"),
    )
    .unwrap();
    dir
}

fn start_cycle_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.contains("E-QUEST-START-CYCLE]"))
        .collect()
}

#[test]
fn a_cycle_through_a_subquest_anchor_is_a_start_cycle() {
    // `a` waits on its own subquest `b`, which activates with `a`.
    let dir = project_body(
        "connectivity-049-subquest",
        "<quest id=\"a\" tier=\"run\" title=\"A\" start=\"quest.b.state == 'active'\">\n  <objective id=\"o\" title=\"Sub\" quest=\"b\"/>\n</quest>\n\n<quest id=\"b\" tier=\"run\" title=\"B\" start=\"true\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    let lines = start_cycle_lines(&out);
    assert_eq!(lines.len(), 1, "{out}");
    assert!(
        lines[0].contains("quest start cycle: quest(a) -> quest(b) -> quest(a)"),
        "{out}"
    );
}

#[test]
fn one_diagnostic_per_strongly_connected_set() {
    // A three-quest ring, and two rings sharing `y`: one diagnostic each.
    let dir = project(
        "connectivity-049-scc",
        &[
            ("a", "quest.c.state == 'active'"),
            ("b", "quest.a.state == 'active'"),
            ("c", "quest.b.state == 'active'"),
            ("x", "quest.y.state == 'active'"),
            (
                "y",
                "quest.x.state == 'active' && quest.z.state == 'active'",
            ),
            ("z", "quest.y.state == 'active'"),
        ],
        "",
        &[],
    );
    let (_, out) = run(&dir, &["check", "--project", "."]);
    let lines = start_cycle_lines(&out);
    assert_eq!(lines.len(), 2, "{out}");
    assert!(
        lines[0].contains(":6:")
            && lines[0].contains("quest(a) -> quest(b) -> quest(c) -> quest(a)"),
        "{out}"
    );
    assert!(
        lines[1].contains("quest(x) -> quest(y) -> quest(x)"),
        "{out}"
    );
}

#[test]
fn declaration_and_alternative_order_change_nothing() {
    let forward = [
        ("p", "quest.q.state == 'active' || visited('entry')"),
        ("q", "quest.p.state == 'active'"),
        ("r", "quest.s.state == 'active'"),
        ("s", "quest.r.state == 'active'"),
    ];
    let reversed = [
        ("s", "quest.r.state == 'active'"),
        ("r", "quest.s.state == 'active'"),
        ("q", "quest.p.state == 'active'"),
        ("p", "visited('entry') || quest.q.state == 'active'"),
    ];
    let views = |name: &str, quests: &[(&str, &str)]| {
        let dir = project(name, quests, "", &[ENTRY]);
        let (_, check) = run(&dir, &["check", "--project", "."]);
        let codes: Vec<String> = start_cycle_lines(&check)
            .iter()
            .map(|l| l.split("] ").nth(1).unwrap().to_string())
            .collect();
        let (_, graph) = run(&dir, &["scenario", "--project", "."]);
        let reach: Vec<String> = ["p", "q", "r", "s"]
            .iter()
            .map(|q| {
                run(
                    &dir,
                    &[
                        "scenario",
                        "--project",
                        ".",
                        "reach",
                        "--node",
                        &format!("quest:{q}"),
                    ],
                )
                .1
                .lines()
                .nth(2)
                .unwrap()
                .to_string()
            })
            .collect();
        (codes, graph, reach)
    };
    let a = views("connectivity-049-order-a", &forward);
    let b = views("connectivity-049-order-b", &reversed);
    assert_eq!(a.0, b.0);
    assert_eq!(a.0.len(), 1, "{:?}", a.0);
    assert_eq!(a.1, b.1);
    assert_eq!(a.2, b.2);
    assert!(
        a.2[0].contains("Reachable") && a.2[1].contains("Reachable"),
        "{:?}",
        a.2
    );
    assert!(a.2[2].contains("E-QUEST-START-CYCLE"), "{:?}", a.2);
}

#[test]
fn an_or_entry_does_not_help_a_separate_and_anchor() {
    let dir = project(
        "connectivity-049-and",
        &[
            (
                "a",
                "(quest.b.state == 'active' || visited('entry')) && quest.b.state == 'complete'",
            ),
            ("b", "quest.a.state == 'active'"),
        ],
        "",
        &[ENTRY],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(start_cycle_lines(&out).len(), 1, "{out}");
}

#[test]
fn a_subquest_parent_is_no_way_into_a_cycle() {
    // `b`'s parent `p` starts freely, but `b` also needs `a`, which needs `b`.
    let dir = project_body(
        "connectivity-049-parent",
        "<quest id=\"p\" tier=\"run\" title=\"P\" start=\"true\">\n  <objective id=\"o\" title=\"Sub\" quest=\"b\"/>\n</quest>\n\n<quest id=\"a\" tier=\"run\" title=\"A\" start=\"quest.b.state == 'active'\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n\n<quest id=\"b\" tier=\"run\" title=\"B\" start=\"quest.a.state == 'active'\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    let (_, out) = run(&dir, &["check", "--project", "."]);
    let lines = start_cycle_lines(&out);
    assert_eq!(lines.len(), 1, "{out}");
    assert!(
        lines[0].contains("quest(a) -> quest(b) -> quest(a)"),
        "{out}"
    );
}

#[test]
fn an_accept_anchor_never_makes_a_start_cycle() {
    // `x` is accepted from `y`'s body and `y` starts once `x` is active:
    // the engine may accept `x` anyway (dsl 0.49.0 §14).
    let dir = project_body(
        "connectivity-049-accept",
        "<quest id=\"x\" tier=\"run\" title=\"X\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n\n<quest id=\"y\" tier=\"run\" title=\"Y\" start=\"quest.x.state == 'active'\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\">\n    ::accept{quest=\"x\"}\n  </objective>\n</quest>\n",
        &[],
    );
    let (_, out) = run(&dir, &["check", "--project", "."]);
    assert!(start_cycle_lines(&out).is_empty(), "{out}");
}

#[test]
fn a_lifecycle_unreachable_scene_is_no_way_in() {
    let dir = project(
        "connectivity-049-dead-entry",
        &[
            ("cycA", "quest.cycB.state == 'active'"),
            ("cycB", "quest.cycA.state == 'active' || visited('entry')"),
            ("dead", "false"),
        ],
        "",
        &[(
            "scenes/entry.lute",
            "---\nkind: scene\nid: entry\nafter: \"completed('dead')\"\n---\n\n## E {#e}\n\n@mira: E.\n",
        )],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    let lines = start_cycle_lines(&out);
    assert_eq!(lines.len(), 1, "{out}");
    assert!(
        lines[0].contains("quest(cycA) -> quest(cycB) -> quest(cycA)"),
        "{out}"
    );
    let (_, graph) = run(&dir, &["scenario", "--project", "."]);
    assert!(
        graph.contains(
            "(unlayered -- part of a start cycle, E-QUEST-START-CYCLE): quest(cycA), quest(cycB)"
        ),
        "{graph}"
    );
}

#[test]
fn a_fact_edge_along_a_back_edge_is_not_layered() {
    let schema = "state:\n  run.flag: { type: bool, default: false }\ncast:\n  mira: { name: Mira }\nentities:\n  item: { members: [lens] }\nrelations:\n  hasItem: { args: [item], tier: run }\n";
    let dir = project(
        "connectivity-049-facts",
        &[(
            "q",
            "(visited('entry') || visited('late')) && holds('hasItem', ['lens'])",
        )],
        "",
        &[
            ENTRY,
            (
                "scenes/late.lute",
                "---\nkind: scene\nid: late\nafter: \"completed('q')\"\n---\n\n## L {#l}\n\n@mira: L.\n::assert{hasItem(lens)}\n",
            ),
        ],
    );
    std::fs::write(dir.join("schema/world.schema.yaml"), schema).unwrap();
    let (code, out) = run(&dir, &["scenario", "--project", ".", "--facts"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(
            "    scene(late) -> quest(q) [hasItem(lens)] (not layered: it closes a cycle)"
        ),
        "{out}"
    );
    let (_, report) = json(&dir, &["scenario", "--project", ".", "--facts", "--json"]);
    let edge = report["roots"][0]["factEdges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["from"] == "scene(late)")
        .unwrap_or_else(|| panic!("{report}"))
        .clone();
    assert_eq!(edge["layered"], false, "{report}");
}
