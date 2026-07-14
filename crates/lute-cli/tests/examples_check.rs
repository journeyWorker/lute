// crates/lute-cli/tests/examples_check.rs
// Mirrors the harness in crates/lute-cli/tests/cli.rs (assert_cmd style).
use std::path::{Path, PathBuf};
use std::process::Command;

fn check(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("check")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

#[test]
fn extends_demo_scene_checks_clean_under_project() {
    // renamed extends-scene.lute -> extends-demo.lute, uses child.schema.yaml
    // (0.3.0 B4: declaration files migrated .schema.lute -> .schema.yaml, the
    // Lute `---` envelope stripped, per foundation B2/B4).
    let out = check(&[
        "../../docs/examples/extends-demo.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn extends_demo_chain_uses_declaration_yaml_not_schema_lute() {
    // B4: `child.schema.yaml`/`base.schema.yaml` are the declaration files
    // under docs/examples/ now — the old `.schema.lute` envelope form is gone.
    assert!(
        examples_dir().join("child.schema.yaml").exists(),
        "child.schema.yaml must exist after the B4 .schema.lute -> .schema.yaml migration"
    );
    assert!(
        examples_dir().join("base.schema.yaml").exists(),
        "base.schema.yaml must exist after the B4 .schema.lute -> .schema.yaml migration"
    );
    assert!(
        !examples_dir().join("child.schema.lute").exists()
            && !examples_dir().join("base.schema.lute").exists()
            && !examples_dir().join("state.schema.lute").exists(),
        "the old .schema.lute envelope files must be gone after migration"
    );
}

#[test]
fn showcase_episode_checks_clean_with_yaml_schema_chain() {
    // B4: showcase/schema/{base,game}.schema.lute -> .schema.yaml; episode01's
    // `uses:` (and game's `extends:`) target the new names.
    let showcase = examples_dir().join("showcase");
    assert!(
        showcase.join("schema/game.schema.yaml").exists()
            && showcase.join("schema/base.schema.yaml").exists(),
        "showcase/schema/{{base,game}}.schema.yaml must exist after migration"
    );
    let out = check(&[
        "../../docs/examples/showcase/episode01.lute",
        "--project",
        "../../docs/examples/showcase",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn rescue_halsin_quest_checks_clean_under_project() {
    // 0.3.0 T15: spec Appendix B worked example (act1.schema.yaml +
    // quest-rescue-halsin.lute) — derived recursion (canReach), epistemic
    // derivation (believesLocation), seeds, key-relations, quest gating on
    // `holds`, exercised end-to-end under the core-only project.
    let out = check(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn affinity_reaction_pair_checks_clean_under_project() {
    // 0.4.0 T8 (dsl §6.4/§6.5): the deduplicated affinity-reaction worked
    // example — the component (`reaction.component.lute`, a param-scoped
    // `<match>` admitted by 0.4.0 §6.2/§6.3) AND its caller
    // (`affinity-reaction.lute`) must BOTH check clean, standalone, under
    // the shared `docs/examples` project — the corpus gate this task's own
    // acceptance criteria hold new example files to.
    let out = check(&[
        "../../docs/examples/affinity-reaction.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    let out = check(&[
        "../../docs/examples/components/reaction.component.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn gated_line_checks_clean_under_project() {
    // 0.4.0 T11 (dsl §7.2/§7.4): the `when=` gated-line sugar worked
    // example — a sugared content line and its hand-written explicit-match
    // twin — must check clean under the shared `docs/examples` project.
    let out = check(&[
        "../../docs/examples/gated-line.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

// ---------------------------------------------------------------------
// Connectivity T15 (dsl connectivity spec §7): corpus grounding + the
// envelope soundness invariant, exercised against the REAL shipped
// `docs/examples/` corpus via the actual `lute` binary -- not a synthetic
// fixture -- so a diagnostic's project-root scoping and its interaction
// with the whole corpus's pre-existing content is genuinely proven, not
// merely asserted.
// ---------------------------------------------------------------------

fn check_project(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("check-project")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

/// Every diagnostic code appearing ANYWHERE in a `check-project --json`
/// result -- both per-file (`files[].diagnostics`) and project-wide
/// (`project_diagnostics`) -- pooled flat since this task only ever needs
/// membership, never per-file attribution.
fn all_codes(v: &serde_json::Value) -> Vec<String> {
    let mut codes = Vec::new();
    for f in v["files"].as_array().expect("files array") {
        for d in f["diagnostics"].as_array().expect("diagnostics array") {
            codes.push(d["code"].as_str().unwrap_or_default().to_string());
        }
    }
    for d in v["project_diagnostics"].as_array().expect("project_diagnostics array") {
        codes.push(d["code"].as_str().unwrap_or_default().to_string());
    }
    codes
}

#[test]
fn corpus_check_project_is_clean_end_to_end() {
    // dsl §7 corpus grounding: `check-project` over the WHOLE `docs/examples/`
    // tree (every resolved subproject root the walk discovers --
    // docs/examples itself, showcase/, plugindef-project/, idola-project/,
    // each scoped independently by `main.rs`'s `by_root` grouping) must
    // exit 0 clean -- the corpus this repo ships as worked examples must
    // actually check clean under the tool it demonstrates.
    let out = check_project(&[examples_dir().to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "the whole shipped corpus must check-project clean: {v}"
    );
    assert_eq!(v["ok"], true, "{v}");
}

#[test]
fn corpus_no_false_positive_episode_dup_across_or_within_project_roots() {
    // `E-CONN-EPISODE-ID-DUP` (§4.1/T3): canonical scene-identity
    // (`character`/`season`/`episode`) collisions ACROSS separate resolved
    // project roots must NOT false-positive -- `character: demo` recurs in
    // BOTH the docs/examples root AND plugindef-project/; `character:
    // bianca` recurs in docs/examples/, idola-project/, AND showcase/ --
    // each subproject's own `lute.project.yaml` scopes the uniqueness
    // check independently (`main.rs`'s `by_root` grouping), so none of
    // that cross-root reuse is a real collision.
    //
    // The corpus ALSO had real WITHIN-root collisions predating this
    // diagnostic entirely -- three `demo.s01ep01` scenes co-located
    // directly in the docs/examples root (affinity-reaction.lute,
    // components/scene.lute, param-def.lute), two `sofia.s01ep01`
    // (choice-persist.lute, gated-line.lute), and two `bianca.s01ep05`
    // inside the idola-project root (date-minigame.lute,
    // idola-portrait.lute) -- genuine TRUE positives this grounding test
    // caught on first run (nothing had ever run `check-project` over the
    // WHOLE corpus before this diagnostic existed to catch them). Fixed by
    // disambiguating each LATER-declared file's `episode:` frontmatter
    // only (components/scene.lute 1->2, param-def.lute 1->3,
    // gated-line.lute 1->5, idola-portrait.lute 5->6) -- content/behavior
    // unchanged (only scene identity), verified by every other corpus test
    // in this file/`examples_compile.rs` staying green and by the e2e
    // compile goldens (`e2e__components_scene.snap`/`e2e__gated_line.snap`)
    // drifting ONLY on the identity-derived `episode`/`episodeId`/`lineId`
    // fields, never on emitted commands.
    let out = check_project(&[examples_dir().to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes = all_codes(&v);
    assert!(
        !codes.iter().any(|c| c == "E-CONN-EPISODE-ID-DUP"),
        "corpus must be free of scene-identity collisions: {v}"
    );
}

#[test]
fn corpus_halsin_relational_objective_not_dead() {
    // `E-OBJECTIVE-UNSATISFIABLE` (§4.2/T7): quest-rescue-halsin's
    // `<objective done="holds(canReach(player, grove))">` must stay live --
    // `canReach` is `derive: true` (act1.schema.yaml), recursively derived
    // from `atLocation`/`connected`, BOTH unconditionally `facts:`-seeded,
    // so it is producible from load regardless of any episode's own
    // reachability (spec §4.2's own worked counterexample against a naive
    // assert-site-only search).
    let out = check_project(&[examples_dir().to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes = all_codes(&v);
    assert!(
        !codes.iter().any(|c| c == "E-OBJECTIVE-UNSATISFIABLE"),
        "quest-rescue-halsin's relational objective must stay live: {v}"
    );
}

/// Every shipped `kind: scene` `.lute` file under `docs/examples/`, paired
/// with the relative suffix `check-project`'s JSON reports it under and its
/// resolved project root (mirrors `main.rs::project_root_for`'s
/// nearest-ancestor-with-`lute.project.yaml` rule; hand-enumerated since
/// every subproject boundary in this fixed corpus is already known from its
/// directory layout -- `showcase/`, `idola-project/`, `plugindef-project/`
/// each carry their own `lute.project.yaml`, everything else resolves to
/// the docs/examples root).
fn shipped_scene_files() -> Vec<(&'static str, PathBuf)> {
    let root = examples_dir();
    let showcase = root.join("showcase");
    let idola = root.join("idola-project");
    let plugindef = root.join("plugindef-project");
    vec![
        ("affinity-reaction.lute", root.clone()),
        ("components/scene.lute", root.clone()),
        ("param-def.lute", root.clone()),
        ("bianca-s01ep02.lute", root.clone()),
        ("carry-ep.lute", root.clone()),
        ("choice-persist.lute", root.clone()),
        ("extends-demo.lute", root.clone()),
        ("gated-line.lute", root.clone()),
        ("property-tracks.lute", root.clone()),
        ("showcase/episode01.lute", showcase.clone()),
        ("showcase/hub-demo.lute", showcase.clone()),
        ("showcase/when-is-demo.lute", showcase),
        ("idola-project/date-minigame.lute", idola.clone()),
        ("idola-project/idola-portrait.lute", idola),
        ("plugindef-project/plugin-def.lute", plugindef),
    ]
}

#[test]
fn envelope_never_newly_errors_a_clean_standalone_scene() {
    // dsl §7:606-610 soundness invariant: `E-STATE-MAYBE-UNAVAILABLE` (the
    // envelope diagnostic, §4.3) must NEVER newly error a file single-file
    // `check()` already reports clean standalone. Grounded against the REAL
    // shipped corpus, not a synthetic fixture: every shipped scene whose
    // single-file `check` proves clean is cross-checked against its OWN
    // project's `check-project` run. This is exactly the class of test
    // that caught a REAL bug fixed in this same commit --
    // `showcase/episode01.lute`'s `run.sofaOutcome` read (consumed by a
    // domain-exhaustive `<match on="run.sofaOutcome"><otherwise>`, written
    // only conditionally by one `<choice persist="run" into="run.sofaOutcome">`
    // arm) is standalone-clean but was newly errored by `check-project`
    // before the `defassign::exhaustive_match_subject_spans` fix (main.rs's
    // T11 wiring recomputed `check_definite_assignment` raw, unaware of
    // `check.rs`'s own exhaustive-match-subject suppression).
    let mut clean_scenes_checked = 0usize;
    for (rel, project) in shipped_scene_files() {
        let file = examples_dir().join(rel);
        let single = check(&[
            file.to_str().unwrap(),
            "--project",
            project.to_str().unwrap(),
            "--json",
        ]);
        let sv: serde_json::Value = serde_json::from_slice(&single.stdout)
            .unwrap_or_else(|e| panic!("{rel}: invalid JSON from `check`: {e}"));
        let single_clean = sv["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .all(|d| d["severity"] != "error");
        if !single_clean {
            continue;
        }
        clean_scenes_checked += 1;
        let proj_out = check_project(&[project.to_str().unwrap(), "--json"]);
        let pv: serde_json::Value = serde_json::from_slice(&proj_out.stdout)
            .unwrap_or_else(|e| panic!("{rel}: invalid JSON from `check-project`: {e}"));
        let path_ends_with_rel = |v: &serde_json::Value| {
            v["path"].as_str().is_some_and(|p| p.ends_with(rel))
        };
        // The envelope diagnostic (`E-STATE-MAYBE-UNAVAILABLE`, error grade)
        // lands in `project_diagnostics` (`main.rs`'s T11 wiring pushes it
        // there via `check_envelope`, NOT into a per-file `diagnostics`
        // array) -- `files[].diagnostics` is checked too, defensively, but
        // `project_diagnostics` is where the real bug this test guards
        // against (T15) actually manifested.
        let newly_errored = pv["project_diagnostics"]
            .as_array()
            .expect("project_diagnostics array")
            .iter()
            .any(|d| path_ends_with_rel(d) && d["code"] == "E-STATE-MAYBE-UNAVAILABLE")
            || pv["files"]
                .as_array()
                .expect("files array")
                .iter()
                .filter(|f| path_ends_with_rel(f))
                .any(|f| {
                    f["diagnostics"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE")
                });
        assert!(
            !newly_errored,
            "{rel} is clean standalone but check-project newly errored it with \
             E-STATE-MAYBE-UNAVAILABLE: {pv}"
        );
    }
    assert!(
        clean_scenes_checked > 0,
        "expected at least one shipped scene to be standalone-clean -- the corpus list is stale"
    );
}
