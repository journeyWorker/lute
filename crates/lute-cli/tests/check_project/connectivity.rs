//! Cross-scene connectivity and envelope reconciliation contracts.

use super::*;

// ---------------------------------------------------------------------
// Connectivity T11: `E-STATE-MAYBE-UNAVAILABLE` envelope diagnostic MUST
// RECONCILE with the per-file `E-MAYBE-UNSET` `check()` already emits for
// the SAME entry-dependent read -- never coexist alongside it (mirrors the
// `E-QUEST-ID-DUP` retain-pass precedent). A read that falls back to entry
// state earns `E-MAYBE-UNSET` from every per-file `check()` call; at
// project scope that diagnostic must be REPLACED by the envelope's own
// verdict: dropped silently when `Guaranteed`, dropped-and-suppressed when
// `Possible\Guaranteed` (warning grade, never surfaced by default), or
// dropped-and-replaced by `E-STATE-MAYBE-UNAVAILABLE` when `∉ Possible`.
// ---------------------------------------------------------------------

/// A scene doc declaring `after: visited(<after_key>)` (or no `after` at
/// all when `after_key` is empty) that reads `run.z` (declared, no
/// default) via a plain `::set` RHS -- the entry-dependent read shape
/// every reconciliation test below needs.
fn scene_reading_run_z(character: &str, after_expr: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n{after_expr}\
         state:\n  run.z: {{ type: int }}\n  run.out: {{ type: int }}\n---\n\
         ## Shot 1.\n::set{{run.out = run.z}}\n"
    )
}

#[test]
fn envelope_guaranteed_read_drops_the_reconciled_maybe_unset_and_exits_zero() {
    // `y` is the ONLY predecessor route and unconditionally sets `run.z` --
    // `run.z ∈ Guaranteed(x)`. Per-file `check()` on `x` alone flags
    // `E-MAYBE-UNSET` (it can't see the project); at project scope that
    // diagnostic MUST be reconciled away with no replacement.
    let dir = temp_dir("envelope-guaranteed");
    let y = "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\nstate:\n  run.z: { type: int }\n---\n## Shot 1.\n::set{run.z = 1}\n";
    write(&dir, "y.lute", y);
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"),
    );

    let out_x = run(&["check", dir.join("x.lute").to_str().unwrap()]);
    assert!(
        !out_x.status.success(),
        "x.lute alone must flag E-MAYBE-UNSET standalone (can't see the project): {}",
        String::from_utf8_lossy(&out_x.stdout)
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
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            !diags.iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
            "the reconciled read must not keep its per-file E-MAYBE-UNSET: {v}"
        );
        assert!(diags
            .iter()
            .all(|d| d["code"] != "E-STATE-MAYBE-UNAVAILABLE"));
    }
}

#[test]
fn envelope_possible_not_guaranteed_read_is_fully_suppressed_by_default() {
    // `after: visited(a) || visited(b)`; `a` unconditionally sets `run.z`,
    // `b` never does -- `run.z ∈ Possible(x) \ Guaranteed(x)`, warning
    // grade, default-suppressed. Project scope MUST exit 0 with NEITHER
    // the per-file E-MAYBE-UNSET NOR any E-STATE-MAYBE-UNAVAILABLE
    // (error or otherwise) anywhere in the default (human or --json)
    // output -- and no `envelope_warnings` key at all (T14 territory).
    let dir = temp_dir("envelope-possible-not-guaranteed");
    let a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nstate:\n  run.z: { type: int }\n---\n## Shot 1.\n::set{run.z = 1}\n";
    let b =
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "a.lute", a);
    write(&dir, "b.lute", b);
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z(
            "x",
            "after: 'visited(\"a.s01ep01\") || visited(\"b.s01ep01\")'\n",
        ),
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
    assert!(
        v.get("envelope_warnings").is_none(),
        "the warning grade must not be surfaced anywhere in default check-project output: {v}"
    );
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            diags
                .iter()
                .all(|d| d["code"] != "E-MAYBE-UNSET" && d["code"] != "E-STATE-MAYBE-UNAVAILABLE"),
            "no diagnostic at all for a Possible-but-not-Guaranteed read by default: {v}"
        );
    }

    let out_human = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(out_human.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out_human.stdout);
    assert!(!stdout.contains("E-MAYBE-UNSET"), "{stdout}");
    assert!(!stdout.contains("E-STATE-MAYBE-UNAVAILABLE"), "{stdout}");
}

#[test]
fn envelope_never_possible_read_replaces_maybe_unset_with_state_unavailable_error() {
    // `y` is the ONLY predecessor route and NEVER sets `run.z` -- `run.z ∉
    // Possible(x)`. Project scope MUST replace the per-file `E-MAYBE-UNSET`
    // with a project-wide error-grade `E-STATE-MAYBE-UNAVAILABLE` (never
    // both at once), and the wording must carry the declared-routes
    // qualifier verbatim.
    let dir = temp_dir("envelope-never-possible");
    let y =
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "y.lute", y);
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"),
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "E-STATE-MAYBE-UNAVAILABLE");
    assert_eq!(project_diags[0]["severity"], "error");
    assert!(
        project_diags[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("under your declared routes")),
        "{v}"
    );
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            !diags.iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
            "the reconciled read must not keep its per-file E-MAYBE-UNSET: {v}"
        );
    }
}

/// Ledger LG28-8: no scene sets an `owner: engine` path, so its
/// unavailable read is advised a guard or a schema `default:`, never an
/// `after:` naming a scene that sets it.
#[test]
fn envelope_unavailable_engine_path_is_not_told_to_add_an_after() {
    let dir = temp_dir("envelope-engine-owned");
    let y =
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "y.lute", y);
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n").replace(
            "run.z: { type: int }",
            "run.z: { type: int, owner: engine }",
        ),
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let line = stdout
        .lines()
        .find(|l| l.contains("E-STATE-MAYBE-UNAVAILABLE"))
        .unwrap_or_else(|| panic!("{stdout}"));
    assert!(line.contains("`owner: engine`"), "{line}");
    assert!(!line.contains("add an `after:`"), "{line}");
}

/// P28S-03/HW28-06: a quest id with a `.` is refused where it is declared
/// (`E-PATH-IDENT`); across the project that is the one report, not also
/// an `E-UNDECLARED` at every read of it.
#[test]
fn a_dotted_quest_id_is_reported_once_at_its_declaration() {
    let dir = temp_dir("dotted-quest-reads");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "q.lute",
        "---\nkind: quest\nid: wing\nstate:\n  run.n: { type: int, default: 0 }\n---\n\
         <quest id=\"wing.hush\" title=\"Hush\" start=\"run.n >= 1\" tier=\"run\">\n  \
         <objective id=\"bed\" title=\"Bed\" done=\"run.n >= 2\"/>\n</quest>\n",
    );
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: s\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         @narrator{when=\"quest.wing.hush.state == 'complete'\"}: Asleep.\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-PATH-IDENT"), "{stdout}");
    assert!(!stdout.contains("E-UNDECLARED"), "{stdout}");
    // Checked alone, the read still says what is wrong.
    let alone = run(&["check", dir.join("s.lute").to_str().unwrap()]);
    assert!(
        String::from_utf8_lossy(&alone.stdout).contains("E-UNDECLARED"),
        "{}",
        String::from_utf8_lossy(&alone.stdout)
    );
}

/// A quest id with a `-` is a name (dsl 0.30.0): its declaration is clean.
/// A read that writes it after a `.` (`quest.lamp-duty.state`) parses as a
/// subtraction, so it is one `E-PATH-IDENT` at the read naming the quoted
/// spelling — across the project and checked alone — and the quoted read
/// checks clean.
#[test]
fn a_hyphenated_quest_id_read_after_a_dot_names_the_quoted_spelling() {
    let dir = temp_dir("hyphen-quest-reads");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "q.lute",
        "---\nkind: quest\nid: wing\nstate:\n  run.n: { type: int, default: 0 }\n---\n\
         <quest id=\"lamp-duty\" title=\"Lamps\" start=\"run.n >= 1\" tier=\"run\">\n  \
         <objective id=\"lit\" title=\"Lit\" done=\"run.n >= 2\"/>\n</quest>\n",
    );
    let scene = |when: &str| {
        format!(
            "---\nkind: scene\ncharacter: s\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             @narrator{{when=\"{when}\"}}: Lit.\n"
        )
    };
    let errors = |out: &std::process::Output| -> Vec<String> {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| l.contains(": error ["))
            .map(String::from)
            .collect()
    };
    let bracket = "write `quest[\"lamp-duty\"].state`";
    for when in ["quest.lamp-duty.state == 'active'", "quest.lamp-duty.state"] {
        write(&dir, "a.lute", &scene(when));
        for args in [
            vec!["check-project", dir.to_str().unwrap()],
            vec!["check", dir.join("a.lute").to_str().unwrap()],
        ] {
            let errs = errors(&run(&args));
            assert_eq!(errs.len(), 1, "{when} {args:?}: {errs:?}");
            assert!(
                errs[0].contains("a.lute")
                    && errs[0].contains("[E-PATH-IDENT]")
                    && errs[0].contains(bracket),
                "{when} {args:?}: {errs:?}"
            );
        }
    }
    write(
        &dir,
        "a.lute",
        &scene("quest['lamp-duty'].state == 'active'"),
    );
    let errs = errors(&run(&["check-project", dir.to_str().unwrap()]));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn envelope_tainted_node_leaves_maybe_unset_untouched() {
    // `after` references an UNRESOLVABLE `visited()` target -- the node is
    // tainted (`propagate`'s own unreliable D/D placeholder). The
    // reconciliation pass must NOT touch this node's reads at all: the
    // per-file `E-MAYBE-UNSET` stays exactly as `check()` reported it, and
    // no `E-STATE-MAYBE-UNAVAILABLE` is ever added for it.
    let dir = temp_dir("envelope-tainted");
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"ghost.s01ep01\")'\n"),
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "a tainted node's Env is untrustworthy -- must never seed E-STATE-MAYBE-UNAVAILABLE: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("x.lute"))
        .unwrap();
    assert!(
        x["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-MAYBE-UNSET"),
        "a tainted node's per-file E-MAYBE-UNSET must be left untouched: {v}"
    );
}

#[test]
fn envelope_reconciliation_is_per_node_across_a_cycle() {
    // Per-node cycle recovery (spec §4.1): a p <-> q prerequisite cycle
    // excludes ONLY its own members (and anything downstream) from
    // `topo_order`, so `propagate` still builds a REAL `envs` entry for every
    // cycle-INDEPENDENT node. Two contrasting non-cyclic scenes both read
    // `run.z` (declared, never set on any route -> E-MAYBE-UNSET standalone):
    //   * `x` is a cycle-INDEPENDENT entry -> it HAS a real `envs` entry, so
    //     reconciliation runs: its per-file E-MAYBE-UNSET is REPLACED by the
    //     project-wide error-grade E-STATE-MAYBE-UNAVAILABLE (never both).
    //   * `d` is DOWNSTREAM of the cycle (`after: visited(p)`), so it is
    //     excluded from `topo_order`/`envs` -> `check_envelope` skips it and
    //     reconciliation leaves its genuine E-MAYBE-UNSET untouched (nothing
    //     trustworthy to reclassify against). This keeps coverage of the
    //     still-excluded/untrusted path the old whole-root wipeout exercised.
    // E-CONN-CYCLE still fires. Before per-node recovery the WHOLE root's
    // envs was emptied, so BOTH x and d kept their raw E-MAYBE-UNSET and x
    // never earned its project-wide envelope verdict -- that was the bug this
    // test used to encode.
    let dir = temp_dir("envelope-cycle-per-node");
    let p = "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\nafter: 'visited(\"q.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    let q = "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\nafter: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "p.lute", p);
    write(&dir, "q.lute", q);
    write(&dir, "x.lute", &scene_reading_run_z("x", ""));
    write(
        &dir,
        "d.lute",
        &scene_reading_run_z("d", "after: 'visited(\"p.s01ep01\")'\n"),
    );

    // Red proof: both cycle-independent `x` and downstream `d` flag
    // E-MAYBE-UNSET standalone (neither can see the project).
    for f in ["x.lute", "d.lute"] {
        let out_f = run(&["check", dir.join(f).to_str().unwrap()]);
        assert!(
            !out_f.status.success(),
            "{f} alone must flag E-MAYBE-UNSET standalone: {}",
            String::from_utf8_lossy(&out_f.stdout)
        );
    }

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
        project_diags.iter().any(|d| d["code"] == "E-CONN-CYCLE"),
        "the p<->q cycle must still be reported: {v}"
    );

    // Cycle-INDEPENDENT `x`: reconciled -> per-file E-MAYBE-UNSET GONE, and an
    // error-grade E-STATE-MAYBE-UNAVAILABLE now stands in for it project-wide.
    assert!(
        project_diags
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"
                && d["severity"] == "error"
                && d["path"].as_str().is_some_and(|p| p.ends_with("x.lute"))),
        "cycle-independent x must earn its project-wide E-STATE-MAYBE-UNAVAILABLE error: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("x.lute"))
        .unwrap();
    assert!(
        !x["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-MAYBE-UNSET"),
        "cycle-independent x's per-file E-MAYBE-UNSET must be reconciled away, not kept alongside \
         its project envelope verdict: {v}"
    );

    // DOWNSTREAM-of-cycle `d`: genuinely absent from envs -> its E-MAYBE-UNSET
    // survives untouched, and it earns NO E-STATE-MAYBE-UNAVAILABLE.
    let d = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("d.lute"))
        .unwrap();
    assert!(
        d["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|dg| dg["code"] == "E-MAYBE-UNSET"),
        "downstream-of-cycle d's genuine E-MAYBE-UNSET must survive (no trustworthy envelope to \
         reclassify against): {v}"
    );
    assert!(
        !project_diags
            .iter()
            .any(|dg| dg["code"] == "E-STATE-MAYBE-UNAVAILABLE"
                && dg["path"].as_str().is_some_and(|p| p.ends_with("d.lute"))),
        "downstream-of-cycle d must NOT earn a project envelope verdict: {v}"
    );
}

#[test]
fn envelope_out_of_scope_scene_maybe_unset_survives_check_project() {
    // `scene.local` is entry-dependent (declared, no default, never
    // locally proven) but SCENE-tier -- out of the envelope's `run.*`/
    // `user.*` scope (dsl §4.3 §386-393). Reconciliation must NEVER touch
    // it: the per-file `E-MAYBE-UNSET` survives check-project untouched,
    // no `E-STATE-MAYBE-UNAVAILABLE` is ever produced for it.
    let dir = temp_dir("envelope-out-of-scope-scene");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x6\nseason: 1\nepisode: 1\nstate:\n  scene.local: { type: int }\n---\n## Shot 1.\n@narrator: value {{scene.local}}\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "scene.* is never envelope-classified: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("x.lute"))
        .unwrap();
    assert!(
        x["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-MAYBE-UNSET"),
        "an out-of-scope scene.* read's E-MAYBE-UNSET must survive reconciliation: {v}"
    );
}

#[test]
fn envelope_out_of_scope_quest_state_read_is_clean_in_check_project() {
    // `quest.foo.state` is QUEST-tier -- out of the envelope's
    // `run.*`/`user.*` scope entirely (dsl §4.3 §386-393), so it is never
    // envelope-classified. 0.21.1 T1-1: it is also an always-assigned
    // lifecycle enum (`unset` before activation), so the per-file read no
    // longer draws `E-MAYBE-UNSET` either (this test used to pin that error).
    let dir = temp_dir("envelope-out-of-scope-quest");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x7\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: value {{quest.foo.state}}\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "quest.* is never envelope-classified: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("x.lute"))
        .unwrap();
    assert!(
        !x["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-MAYBE-UNSET"),
        "a quest.<id>.state read is definite: {v}"
    );
}

#[test]
fn envelope_mixed_slot_span_collision_only_reconciles_the_in_scope_path() {
    // `run.out = run.upstream && scene.local` -- BOTH reads sit in the
    // SAME CEL slot, so `check_read` fires each with the IDENTICAL `Span`
    // (defassign.rs has no per-path span within one slot). `run.upstream`
    // is Guaranteed at `x` (its only predecessor route, `y8`, sets it
    // unconditionally) -- reconciled away. `scene.local` is scene-tier,
    // out of scope, and genuinely never set -- its E-MAYBE-UNSET at that
    // SAME span must survive. A span-only match would wrongly drop BOTH.
    let dir = temp_dir("envelope-mixed-slot-collision");
    let y = "---\nkind: scene\ncharacter: y8\nseason: 1\nepisode: 1\nstate:\n  run.upstream: { type: bool }\n---\n## Shot 1.\n::set{run.upstream = true}\n";
    write(&dir, "y.lute", y);
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x8\nseason: 1\nepisode: 1\nafter: 'visited(\"y8.s01ep01\")'\nstate:\n  run.upstream: { type: bool }\n  scene.local: { type: bool }\n  run.out: { type: bool }\n---\n## Shot 1.\n::set{run.out = run.upstream && scene.local}\n",
    );

    // Standalone red proof: BOTH reads flag E-MAYBE-UNSET at the same span.
    let out_x = run(&["check", dir.join("x.lute").to_str().unwrap(), "--json"]);
    let vx: serde_json::Value = serde_json::from_slice(&out_x.stdout).unwrap();
    let unset: Vec<&serde_json::Value> = vx["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-MAYBE-UNSET")
        .collect();
    assert_eq!(
        unset.len(),
        2,
        "expected both reads to flag standalone: {vx}"
    );
    assert_eq!(
        unset[0]["span"], unset[1]["span"],
        "both reads must share the same slot span: {vx}"
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "run.upstream is Guaranteed -> no envelope diagnostic at all: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("x.lute"))
        .unwrap();
    let remaining: Vec<&serde_json::Value> = x["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-MAYBE-UNSET")
        .collect();
    assert_eq!(
        remaining.len(),
        1,
        "exactly the scene.local site must survive reconciliation, run.upstream's must not: {v}"
    );
    assert!(
        remaining[0]["message"]
            .as_str()
            .unwrap()
            .contains("scene.local"),
        "the surviving E-MAYBE-UNSET must be scene.local's, not run.upstream's: {v}"
    );
}
