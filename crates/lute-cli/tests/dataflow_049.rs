//! 0.49.0 §5: data flow. #158 (`into=` writes before the arm body), #179
//! (blocking bridge results are definite assignments), #180 (`'m' in
//! family` is the presence guard of every `per:` member, and every hint
//! names a spelling `check` accepts). Each project is the issue's repro.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).unwrap();
        std::fs::write(path, text).unwrap();
    }
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

// ---- #158 -------------------------------------------------------------

const MANIFEST_158: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n";

#[test]
fn into_record_survives_an_arm_ending_in_jump() {
    let dir = lute_scratch::ScratchDir::named("dataflow-049-158");
    write(
        &dir,
        &[
            ("lute.project.yaml", MANIFEST_158),
            (
                "plugins/acme.stage/plugin.yaml",
                "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  {}\n",
            ),
            (
                "schema/world.schema.yaml",
                "enums:\n  pick: [none, left, right]\nstate:\n  run.pick: { type: { domain: pick }, default: none }\ncast:\n  mira: { name: Mira }\n  theo: { name: Theo }\n",
            ),
            (
                "scenes/intro.lute",
                "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n<branch id=\"route\" prompt=\"Which way?\">\n  <choice id=\"goLeft\" text=\"Left\" into=\"run.pick\" value=\"left\">\n    @mira: Left it is.\n    ::jump{to=\"after\"}\n  </choice>\n  <choice id=\"goRight\" text=\"Right\" into=\"run.pick\" value=\"right\">\n    @mira: Right it is.\n  </choice>\n</branch>\n::label{name=\"after\"}\n@theo: Done.\n::end\n",
            ),
            ("mock.yaml", "choose: { route: goLeft }\n"),
        ],
    );
    let (code, out) = run(
        &dir,
        &[
            "compile",
            "scenes/intro.lute",
            "--project",
            ".",
            "-o",
            "intro.ir.json",
        ],
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&dir, &["run", "intro.ir.json", "--mock", "mock.yaml"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("run.pick = left"), "{out}");
}

// ---- #179 -------------------------------------------------------------

const SCENE_179: &str = "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n@mira: Tell me about your day.\n::askFree{prompt=\"Say something to Mira\" resultKey=\"day1\"}\n<match subject=\"scene.ask.day1.tone\">\n  <when is=\"warm\">\n    @mira: That is lovely.\n  </when>\n  <when is=\"cold\">\n    @mira: Oh.\n  </when>\n  <when is=\"unclear\">\n    @mira: Hm?\n  </when>\n</match>\n";

fn project_179(tag: &str, scene: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        &[
            ("lute.project.yaml", MANIFEST_158),
            (
                "plugins/acme.stage/plugin.yaml",
                "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  bridge: bridge/\n  directives: directives/\n  state: state/\n",
            ),
            (
                "plugins/acme.stage/bridge/b.yaml",
                "bridgeCapabilities:\n  - service: dialogue\n    operation: askFree\n    replay: recorded\n    result:\n      - { name: reply, type: string }\n      - { name: tone, type: { enum: [warm, cold, unclear] } }\n",
            ),
            (
                "plugins/acme.stage/directives/d.yaml",
                "directives:\n  - name: askFree\n    layer: bridge\n    attrs:\n      - { name: prompt, required: true, type: string }\n      - { name: resultKey, required: true, type: { slotId: { namespace: scene.ask } } }\n      - { name: sync, type: bool, default: true }\n    semantics: [ \"writes.sceneState\", \"bridgeCall\" ]\n    state:\n      declares:\n        - { scope: scene, path: [ask, { fromAttr: { name: resultKey, slotType: localId } }], shape: askResult }\n    effects:\n      writes:\n        - { scope: scene, path: [ask, { fromAttr: { name: resultKey } }, reply], value: { fromBridgeResult: reply } }\n        - { scope: scene, path: [ask, { fromAttr: { name: resultKey } }, tone],  value: { fromBridgeResult: tone } }\n    bridge: { service: dialogue, operation: askFree }\n",
            ),
            (
                "plugins/acme.stage/state/s.yaml",
                "stateShapes:\n  - name: askResult\n    fields:\n      - { name: reply, type: string }\n      - { name: tone, type: { enum: [warm, cold, unclear] } }\n",
            ),
            ("schema/world.schema.yaml", "cast:\n  mira: { name: Mira }\n"),
            ("scenes/intro.lute", scene),
        ],
    );
    dir
}

#[test]
fn blocking_bridge_result_is_assigned() {
    let dir = project_179("dataflow-049-179-clean", SCENE_179);
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn blocking_bridge_result_keeps_exhaustiveness() {
    let scene = SCENE_179.replace("  <when is=\"unclear\">\n    @mira: Hm?\n  </when>\n", "");
    let dir = project_179("dataflow-049-179-arm", &scene);
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("[E-NONEXHAUSTIVE]"), "{out}");
    assert!(!out.contains("E-MAYBE-UNSET"), "{out}");
}

#[test]
fn non_blocking_bridge_result_stays_maybe_unset() {
    for (tag, call) in [
        (
            "dataflow-049-179-nowait",
            "resultKey=\"day1\" wait=\"false\"}",
        ),
        (
            "dataflow-049-179-guarded",
            "resultKey=\"day1\" when=\"false\"}",
        ),
    ] {
        let scene = SCENE_179.replace("resultKey=\"day1\"}", call);
        let dir = project_179(tag, &scene);
        let (code, out) = run(&dir, &["check", "--project", "."]);
        assert_eq!(code, 1, "{out}");
        assert!(
            out.contains("[E-MAYBE-UNSET] state path `scene.ask.day1.tone`"),
            "{out}"
        );
    }
}

// ---- #180 -------------------------------------------------------------

fn project_180(tag: &str, body: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        &[
            (
                "lute.project.yaml",
                "defaultProfile: game\nprofiles:\n  game:\n    plugins: {}\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n",
            ),
            (
                "schema/world.schema.yaml",
                "enums:\n  rank: [gold, silver]\nentities:\n  step: { members: [intro, look-around] }\nstate:\n  user.stepRank: { type: { domain: rank }, per: step }\ncast:\n  mira: { name: Mira }\n",
            ),
            (
                "scenes/intro.lute",
                &format!("---\nkind: scene\nid: intro\n---\n\n## Hall {{#hall}}\n\n{body}\n"),
            ),
        ],
    );
    dir
}

fn check_180(tag: &str, body: &str) -> (i32, String) {
    let dir = project_180(tag, body);
    run(&dir, &["check", "--project", "."])
}

#[test]
fn member_guard_forms_are_accepted() {
    for (i, body) in [
        "@mira{when=\"'look-around' in user.stepRank && user.stepRank['look-around'] == 'gold'\"}: Gold.",
        "@mira{when=\"!('look-around' in user.stepRank)\"}: Not yet.",
        "@mira{when=\"!('look-around' in user.stepRank) || user.stepRank['look-around'] == 'gold'\"}: Gold or not yet.",
        "@mira{when=\"has(user.stepRank.intro) && user.stepRank.intro == 'gold'\"}: Gold.",
        "@mira{when=\"'intro' in user.stepRank && user.stepRank.intro == 'gold'\"}: Gold.",
        "@mira{when=\"!has(user.stepRank.intro)\"}: Not yet.",
        "<match subject=\"user.stepRank['look-around']\">\n  <when is=\"unset\">\n    @mira: Not yet.\n  </when>\n  <otherwise>\n    @mira: Played.\n  </otherwise>\n</match>",
    ]
    .iter()
    .enumerate()
    {
        let (code, out) = check_180(&format!("dataflow-049-180-ok-{i}"), body);
        assert_eq!(code, 0, "{body}\n{out}");
    }
}

#[test]
fn every_hint_names_an_accepted_spelling() {
    let cases = [
        (
            "@mira{when=\"user.stepRank['look-around'] == 'gold'\"}: Gold.",
            "E-STATE-MAYBE-UNAVAILABLE",
            "guard the read with `'look-around' in user.stepRank`",
        ),
        (
            "@mira{when=\"user.stepRank.intro == 'gold'\"}: Gold.",
            "E-STATE-MAYBE-UNAVAILABLE",
            "guard the read with `has(user.stepRank.intro)`",
        ),
        (
            "@mira{when=\"isSet(user.stepRank['look-around']) && user.stepRank['look-around'] == 'gold'\"}: Gold.",
            "E-CEL-PROFILE",
            "use `'look-around' in user.stepRank`",
        ),
        (
            "@mira{when=\"has(user.stepRank['look-around']) && user.stepRank['look-around'] == 'gold'\"}: Gold.",
            "E-CEL-PARSE",
            "for a member that is not an identifier write `'look-around' in user.stepRank`",
        ),
        (
            "@mira{when=\"user.stepRank['look-around'] == 'unset'\"}: Not yet.",
            "E-UNSET-LITERAL",
            "Test for unset with `!('look-around' in user.stepRank)`, or in a `<match subject=\"user.stepRank['look-around']\">` use `<when is=\"unset\">`",
        ),
        (
            "@mira{when=\"user.stepRank == 'gold'\"}: Gold.",
            "E-UNDECLARED",
            "or `'<member>' in user.stepRank` to test whether a member is set",
        ),
    ];
    for (i, (body, code, hint)) in cases.iter().enumerate() {
        let (status, out) = check_180(&format!("dataflow-049-180-hint-{i}"), body);
        assert_eq!(status, 1, "{body}\n{out}");
        assert!(out.contains(&format!("[{code}]")), "{body}\n{out}");
        assert!(out.contains(hint), "{body}: wants {hint}\n{out}");
    }
    // A member that is not one gets the read's diagnostic.
    let (_, quoted) = check_180(
        "dataflow-049-180-bad-in",
        "@mira{when=\"'nope' in user.stepRank\"}: x",
    );
    assert!(
        quoted.contains("[E-UNDECLARED] state path `user.stepRank.nope` is not declared"),
        "{quoted}"
    );
}

#[test]
fn trace_and_run_evaluate_member_presence() {
    let body = "@mira{when=\"!('look-around' in user.stepRank)\"}: Not yet.\n@mira{when=\"'look-around' in user.stepRank && user.stepRank['look-around'] == 'gold'\"}: Gold.";
    let dir = project_180("dataflow-049-180-trace", body);
    write(
        &dir,
        &[
            ("unset.yaml", "state:\n  user.stepRank.intro: gold\n"),
            ("set.yaml", "state:\n  user.stepRank.look-around: gold\n"),
        ],
    );
    let (_, unset) = run(
        &dir,
        &[
            "trace",
            "scenes/intro.lute",
            "--project",
            ".",
            "--mock",
            "unset.yaml",
        ],
    );
    assert!(
        unset.contains("Not yet.") && !unset.contains("Gold."),
        "{unset}"
    );
    let (_, set) = run(
        &dir,
        &[
            "trace",
            "scenes/intro.lute",
            "--project",
            ".",
            "--mock",
            "set.yaml",
        ],
    );
    assert!(set.contains("Gold.") && !set.contains("Not yet."), "{set}");
    let (code, out) = run(
        &dir,
        &[
            "compile",
            "scenes/intro.lute",
            "--project",
            ".",
            "-o",
            "ir.json",
        ],
    );
    assert_eq!(code, 0, "{out}");
    let (_, unset) = run(&dir, &["run", "ir.json", "--mock", "unset.yaml"]);
    assert!(
        unset.contains("Not yet.") && !unset.contains("Gold."),
        "{unset}"
    );
    let (_, set) = run(&dir, &["run", "ir.json", "--mock", "set.yaml"]);
    assert!(set.contains("Gold.") && !set.contains("Not yet."), "{set}");
}
