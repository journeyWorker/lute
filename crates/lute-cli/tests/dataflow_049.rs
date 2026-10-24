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

// ---- review F3 ----------------------------------------------------------

/// A two-scene project whose `ask` directive writes two `run.res.*` results
/// and a `fromAttr` slot; `b` runs `after: visited('a')`.
fn bridge_project(tag: &str, a_body: &str, b_body: Option<&str>) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    let mut files = vec![
        ("lute.project.yaml", MANIFEST_158.to_string()),
        (
            "plugins/acme.stage/plugin.yaml",
            "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  bridge: bridge/\n  directives: directives/\n".to_string(),
        ),
        (
            "plugins/acme.stage/bridge/b.yaml",
            "bridgeCapabilities:\n  - service: s\n    operation: ask\n    replay: recorded\n    result:\n      - { name: a, type: string }\n      - { name: b, type: string }\n".to_string(),
        ),
        (
            "plugins/acme.stage/directives/d.yaml",
            "directives:\n  - name: ask\n    layer: bridge\n    semantics: [ \"bridgeCall\" ]\n    effects:\n      writes:\n        - { scope: run, path: [res, a], value: { fromBridgeResult: a } }\n        - { scope: run, path: [res, b], value: { fromBridgeResult: b } }\n    bridge: { service: s, operation: ask }\n  - name: askSlot\n    layer: bridge\n    attrs:\n      - { name: key, required: true, type: string }\n    semantics: [ \"bridgeCall\" ]\n    effects:\n      writes:\n        - { scope: run, path: [slot, { fromAttr: { name: key } }], value: { fromBridgeResult: a } }\n    bridge: { service: s, operation: ask }\n".to_string(),
        ),
        (
            "schema/world.schema.yaml",
            "state:\n  run.res.a: { type: string }\n  run.res.b: { type: string }\n  run.slot.k1: { type: string }\n  run.e: { type: { enum: [x, y] }, default: x }\ncast:\n  mira: { name: Mira }\ndefs:\n  off: { type: bool, cel: \"false\", host: true }\n  on: { type: bool, cel: \"true\", host: true }\n".to_string(),
        ),
        ("scenes/a.lute", format!("---\nkind: scene\nid: a\n---\n\n## H {{#h}}\n\n{a_body}\n")),
    ];
    if let Some(b) = b_body {
        files.push((
            "scenes/b.lute",
            format!("---\nkind: scene\nid: b\nafter: visited('a')\n---\n\n## H {{#h}}\n\n{b}\n"),
        ));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    write(&dir, &refs);
    dir
}

#[test]
fn bridge_result_reaches_a_later_scene_and_names_its_writer() {
    let dir = bridge_project(
        "dataflow-049-f31",
        "@mira: hi\n::ask{}",
        Some("@mira: {{run.res.a}}"),
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &dir,
        &["scenario", "envelope", "--node", "b", "--project", "."],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("run.res.a   written by: scene(a)"), "{out}");
    for (i, call) in ["::ask{when=\"run.e == 'y'\"}", "::ask{wait=\"false\"}"]
        .iter()
        .enumerate()
    {
        let dir = bridge_project(
            &format!("dataflow-049-f31-none-{i}"),
            &format!("@mira: hi\n{call}"),
            Some("@mira: {{run.res.a}}"),
        );
        let (code, out) = run(&dir, &["check", "--project", "."]);
        assert_eq!(code, 1, "{call}\n{out}");
        assert!(
            out.contains("[E-STATE-MAYBE-UNAVAILABLE] state path `run.res.a`"),
            "{call}\n{out}"
        );
    }
}

#[test]
fn bridge_wait_and_placement_matrix() {
    let read = "@mira: {{run.res.a}}";
    let cases = [
        (format!("::ask{{wait=\"true\"}}\n{read}"), true),
        (format!("::ask{{wait=@on}}\n{read}"), true),
        (format!("::ask{{wait=@off}}\n{read}"), false),
        ("::ask{}\n@mira: {{run.res.b}}".to_string(), true),
        ("::askSlot{key=\"k1\"}\n@mira: {{run.slot.k1}}".to_string(), true),
        (format!("<match subject=\"run.e\">\n<when is=\"x\">\n::ask{{}}\n</when>\n<when is=\"y\">\n::ask{{}}\n</when>\n</match>\n{read}"), true),
        (format!("<match subject=\"run.e\">\n<when is=\"x\">\n::ask{{}}\n</when>\n<otherwise>\n@mira: no\n</otherwise>\n</match>\n{read}"), false),
        (format!("<hub id=\"hb\">\n<choice id=\"c1\" text=\"A\">\n::ask{{}}\n{read}\n</choice>\n<choice id=\"c2\" text=\"B\" exit>\n@mira: bye\n</choice>\n</hub>"), true),
        (format!("<hub id=\"hb\">\n<choice id=\"c1\" text=\"A\">\n::ask{{}}\n</choice>\n<choice id=\"c2\" text=\"B\" exit>\n@mira: bye\n</choice>\n</hub>\n{read}"), false),
    ];
    for (i, (body, clean)) in cases.iter().enumerate() {
        let dir = bridge_project(&format!("dataflow-049-matrix-{i}"), body, None);
        let (code, out) = run(&dir, &["check", "scenes/a.lute", "--project", "."]);
        assert_eq!(code == 0, *clean, "{body}\n{out}");
        assert_eq!(out.contains("E-MAYBE-UNSET"), !clean, "{body}\n{out}");
    }
}

#[test]
fn quoted_member_unset_arm_compiles_runs_and_traces() {
    let body = "<match subject=\"user.stepRank['look-around']\">\n  <when is=\"unset\">\n    @mira: Not yet.\n  </when>\n  <otherwise>\n    @mira: Played.\n  </otherwise>\n</match>";
    let dir = project_180("dataflow-049-unset-arm", body);
    write(
        &dir,
        &[
            ("unset.yaml", "state:\n  user.stepRank.intro: gold\n"),
            ("set.yaml", "state:\n  user.stepRank.look-around: gold\n"),
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
            "ir.json",
        ],
    );
    assert_eq!(code, 0, "{out}");
    let ir = std::fs::read_to_string(dir.join("ir.json")).unwrap();
    assert!(
        ir.contains(r#""cel": "(!(\"look-around\" in user.stepRank))""#),
        "{ir}"
    );
    for (mock, want, not) in [
        ("unset.yaml", "Not yet.", "Played."),
        ("set.yaml", "Played.", "Not yet."),
    ] {
        let (_, ran) = run(&dir, &["run", "ir.json", "--mock", mock]);
        assert!(ran.contains(want) && !ran.contains(not), "{mock}\n{ran}");
        let (_, traced) = run(
            &dir,
            &[
                "trace",
                "scenes/intro.lute",
                "--project",
                ".",
                "--mock",
                mock,
            ],
        );
        assert!(
            traced.contains(want) && !traced.contains(not),
            "{mock}\n{traced}"
        );
    }
}

/// The text between the backticks after `marker` in `out`.
fn backticked_after<'o>(out: &'o str, marker: &str) -> &'o str {
    let rest = &out[out
        .find(marker)
        .unwrap_or_else(|| panic!("no `{marker}` in {out}"))
        + marker.len()..];
    let rest = rest.strip_prefix('`').expect("a backticked spelling");
    &rest[..rest.find('`').expect("a closing backtick")]
}

#[test]
fn emitted_guards_round_trip_through_check() {
    let cases = [
        (
            "user.stepRank['look-around']",
            "user.stepRank['look-around'] == 'gold'",
        ),
        ("user.stepRank.intro", "user.stepRank.intro == 'gold'"),
        (
            "user['big-fam']['look-around']",
            "user['big-fam']['look-around'] == 'gold'",
        ),
    ];
    let schema_extra = "  user.big-fam: { type: { domain: rank }, per: step }\n";
    for (i, (path, read)) in cases.iter().enumerate() {
        let with = |tag: &str, when: &str| {
            let dir = project_180(tag, &format!("@mira{{when=\"{when}\"}}: x"));
            let schema = dir.join("schema/world.schema.yaml");
            let text = std::fs::read_to_string(&schema)
                .unwrap()
                .replace("cast:", &format!("{schema_extra}cast:"));
            std::fs::write(&schema, text).unwrap();
            run(&dir, &["check", "--project", "."])
        };
        let (_, out) = with(&format!("dataflow-049-rt-read-{i}"), read);
        let guard = backticked_after(&out, "guard the read with ").to_string();
        let (code, back) = with(
            &format!("dataflow-049-rt-guard-{i}"),
            &format!("{guard} && {read}"),
        );
        assert_eq!(code, 0, "{path}: `{guard}`\n{back}");
        let (_, out) = with(
            &format!("dataflow-049-rt-unset-{i}"),
            &format!("{path} == 'unset'"),
        );
        let negated = backticked_after(&out, "Test for unset with ").to_string();
        let (code, back) = with(&format!("dataflow-049-rt-neg-{i}"), &negated);
        assert_eq!(code, 0, "{path}: `{negated}`\n{back}");
    }
}
