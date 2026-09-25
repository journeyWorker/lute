//! dsl 0.24.0 §5 (T1-14, T2-10): bridge answers in `lute play`, `lute
//! trace` and `lute test`, over `tests/fixtures/bridge-check` — a `beats`
//! project whose plugin declares a `::check` skill check writing
//! `scene.check.<key>.{passed,margin}` from its bridge result. `probe.c`
//! makes two calls (`guards`, then `sneak`), each followed by a `<match>`
//! over its `passed` slot; a gated line reads `guards`'s `margin`, so both
//! fields are read (dsl 0.25.0 §7 — `margin_unread` drops that line).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-check")
}

/// One fresh directory per call: tests run in parallel inside one process,
/// so a `tag` + pid name alone let two `trace` mocks overwrite each other.
fn temp_dir(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-bridges-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

fn out(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn play(tag: &str, script: &str) -> Output {
    let script = write(&temp_dir(tag), "s.play.yaml", script);
    Command::new(BIN)
        .args([
            "play",
            fixture().to_str().unwrap(),
            "--script",
            script.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn trace(mock: Option<&str>) -> Output {
    let mut args = vec![
        "trace".to_string(),
        fixture().join("scenes/probe/c.lute").display().to_string(),
        "--project".to_string(),
        fixture().display().to_string(),
    ];
    if let Some(m) = mock {
        let m = write(&temp_dir("trace-mock"), "m.yaml", m);
        args.push("--mock".to_string());
        args.push(m.display().to_string());
    }
    Command::new(BIN).args(&args).output().unwrap()
}

#[test]
fn play_halts_at_an_unanswered_call_before_the_default_arm() {
    let o = play("unanswered", "steps:\n  - occasion: hubVisit\n");
    let text = out(&o);
    assert_eq!(o.status.code(), Some(3), "{text}");
    assert!(
        text.contains("(bridge unanswered: passed, margin)"),
        "{text}"
    );
    assert!(text.contains("plugin call `check`"), "{text}");
    assert!(
        text.contains("bridges: { check: [ { passed: <bool>, margin: <number> } ] }"),
        "{text}"
    );
    // Nothing after the call was walked: no arm, no line.
    assert!(!text.contains("match ->"), "{text}");
    assert!(!text.contains("Failed."), "{text}");
    assert!(!text.contains("Passed."), "{text}");
}

#[test]
fn play_consumes_top_level_answers_in_call_order() {
    let o = play(
        "top",
        "bridges:\n  check:\n    - { passed: true, margin: 3 }\n    - { passed: false, margin: -2 }\n\
         steps:\n  - occasion: hubVisit\n",
    );
    let text = out(&o);
    assert_eq!(o.status.code(), Some(0), "{text}");
    assert!(
        text.contains("(bridge answered: passed=true, margin=3)"),
        "{text}"
    );
    assert!(
        text.contains("(bridge answered: passed=false, margin=-2)"),
        "{text}"
    );
    let passed = text.find("Passed.").expect(&text);
    let spotted = text.find("Spotted.").expect(&text);
    assert!(passed < spotted, "{text}");
    assert!(
        !text.contains("Failed.") && !text.contains("Sneaked."),
        "{text}"
    );
}

#[test]
fn play_step_answers_come_before_the_top_level_ones() {
    let o = play(
        "step",
        "bridges:\n  check:\n    - { passed: false, margin: 0 }\n\
         steps:\n  - occasion: hubVisit\n    bridges:\n      check:\n        - { passed: true, margin: 1 }\n",
    );
    let text = out(&o);
    assert_eq!(o.status.code(), Some(0), "{text}");
    // The step's answer decides the first call, the top level's the second.
    assert!(
        text.contains("Passed.") && text.contains("Spotted."),
        "{text}"
    );
}

#[test]
fn play_fails_a_step_that_leaves_its_own_answers_unconsumed() {
    let o = play(
        "leftover",
        "steps:\n  - occasion: hubVisit\n    bridges:\n      check:\n        - { passed: true, margin: 1 }\n\
         \x20       - { passed: true, margin: 1 }\n        - { passed: false, margin: 9 }\n",
    );
    let text = out(&o);
    assert_eq!(o.status.code(), Some(1), "{text}");
    assert!(
        text.contains("step 1: its `bridges:` answers were not all consumed"),
        "{text}"
    );
    assert!(
        text.contains("`check` {passed: false, margin: 9}"),
        "{text}"
    );
}

#[test]
fn play_refuses_a_bad_field_or_a_misfit_value_at_load() {
    for (script, want) in [
        (
            "bridges:\n  check:\n    - { passed: true, margn: 3 }\nsteps:\n  - occasion: hubVisit\n",
            "gives `margn`, which no effect of a `check` call reads",
        ),
        (
            "bridges:\n  check:\n    - { passed: maybe, margin: 3 }\nsteps:\n  - occasion: hubVisit\n",
            "`passed: maybe` does not fit `scene.check.",
        ),
        (
            "bridges:\n  check:\n    - { passed: true }\nsteps:\n  - occasion: hubVisit\n",
            "lacks `margin`, which content reads — an answer gives every bridge result `::check` content reads: `{ passed: <bool>, margin: <number> }`",
        ),
        (
            "bridges:\n  chek:\n    - { passed: true, margin: 1 }\nsteps:\n  - occasion: hubVisit\n",
            "did you mean `check`?",
        ),
    ] {
        let o = play("bad", script);
        let text = out(&o);
        assert_eq!(o.status.code(), Some(2), "{script}\n{text}");
        assert!(text.contains(want), "{script}\n{text}");
    }
}

/// dsl 0.25.0 §7: a missing read field reads the same in `lute play` and
/// `lute trace` (one wording, the trace one).
#[test]
fn a_missing_bridge_field_reads_the_same_in_play_and_trace() {
    let want =
        "lacks `margin`, which content reads — an answer gives every bridge result `::check` \
                content reads: `{ passed: <bool>, margin: <number> }`";
    let played = out(&play(
        "lacks-same",
        "bridges:\n  check:\n    - { passed: true }\nsteps:\n  - occasion: hubVisit\n",
    ));
    let traced = out(&trace(Some("bridges:\n  check:\n    - { passed: true }\n")));
    assert!(played.contains(want), "{played}");
    assert!(traced.contains(want), "{traced}");
}

#[test]
fn trace_reads_an_unmocked_bridge_result_as_unknown() {
    let o = trace(None);
    let text = out(&o);
    assert_eq!(o.status.code(), Some(3), "{text}");
    assert!(
        !text.contains("-> arm 2"),
        "the shape default must not decide: {text}"
    );
    assert!(
        text.contains("bridges: { check: [ { passed: <bool>, margin: <number> } ] }"),
        "the hint names the tag and every field the loader demands, typed: {text}"
    );
}

#[test]
fn trace_takes_the_mocked_answer_in_call_order() {
    let o = trace(Some(
        "bridges:\n  check:\n    - { passed: true, margin: 3 }\n    - { passed: false, margin: 0 }\n",
    ));
    let text = out(&o);
    assert_eq!(o.status.code(), Some(0), "{text}");
    assert!(
        text.contains("(bridge answered: passed=true, margin=3)"),
        "{text}"
    );
    assert!(
        text.contains("Passed.") && text.contains("Spotted."),
        "{text}"
    );
    assert!(
        !text.contains("Failed.") && !text.contains("Sneaked."),
        "{text}"
    );

    let bad = trace(Some("bridges:\n  check:\n    - { passed: 7, margin: 3 }\n"));
    assert_eq!(bad.status.code(), Some(1), "{}", out(&bad));
    assert!(out(&bad).contains("E-TRACE-MOCK-TYPE"), "{}", out(&bad));
}

#[test]
fn a_scenario_test_needs_its_bridge_answers() {
    let dir = fixture();
    let run = |tag: &str, body: &str| {
        let t = write(&dir, &format!("tests/{tag}.test.yaml"), body);
        let o = Command::new(BIN)
            .args([
                "test",
                t.to_str().unwrap(),
                "--project",
                dir.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&t);
        o
    };
    let with = run(
        "bridge-with",
        "file: ../scenes/probe/c.lute\nbridges:\n  check:\n    - { passed: true, margin: 3 }\n\
         \x20   - { passed: true, margin: 1 }\nexpect:\n  transcriptContains: [\"Passed.\", \"Sneaked.\"]\n",
    );
    assert_eq!(with.status.code(), Some(0), "{}", out(&with));
    let without = run(
        "bridge-without",
        "file: ../scenes/probe/c.lute\nexpect:\n  transcriptContains: [\"Passed.\"]\n",
    );
    assert_eq!(without.status.code(), Some(1), "{}", out(&without));
    assert!(
        out(&without).contains("bridges: { check:"),
        "{}",
        out(&without)
    );
}

/// `lute test <file> --project <fixture>` on a scenario test written into
/// the fixture's `tests/` for the run.
fn scenario(tag: &str, body: &str) -> (PathBuf, Output) {
    let dir = fixture();
    let t = write(&dir, &format!("tests/{tag}.test.yaml"), body);
    let o = Command::new(BIN)
        .args([
            "test",
            t.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&t);
    (t, o)
}

/// ER N7: the unanswered-bridge hint gave `{ passed: <value> }`, an answer
/// the loader then refused for lacking `margin`. The hint is now the whole
/// typed shape, and filling in its placeholders is an accepted answer.
#[test]
fn the_unanswered_bridge_hint_is_an_answer_the_loader_accepts() {
    let (_, without) = scenario(
        "bridge-hint",
        "file: ../scenes/probe/c.lute\nexpect:\n  transcriptContains: [\"Passed.\"]\n",
    );
    let text = out(&without);
    let hint = "{ passed: <bool>, margin: <number> }";
    assert!(
        text.contains(&format!("supply bridges: {{ check: [ {hint} ] }}")),
        "{text}"
    );

    let answer = hint.replace("<bool>", "true").replace("<number>", "2");
    let (_, with) = scenario(
        "bridge-hinted",
        &format!(
            "file: ../scenes/probe/c.lute\nbridges: {{ check: [ {answer}, {answer} ] }}\n\
             expect:\n  transcriptContains: [\"Passed.\", \"Sneaked.\"]\n"
        ),
    );
    assert_eq!(with.status.code(), Some(0), "{}", out(&with));
}

/// Four bad `bridges:` entries, each on a known line of a mock whose first
/// line is its `file:` key: a missing field (answer at 4:7), a misfit value
/// (`passed` at 5:7), a stray field (`margn` at 7:7), an unknown tag
/// (`chek` at 8:3).
const BAD_BRIDGES: &str = "bridges:\n  check:\n    - { passed: true }\n    - passed: maybe\n      \
                           margin: 1\n      margn: 2\n  chek:\n    - { passed: true, margin: 1 }\n";

fn assert_anchored(text: &str, file: &str) {
    for want in [
        format!("{file}:4:7: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1 lacks `margin`"),
        format!("{file}:5:7: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 2: `passed: maybe`"),
        format!(
            "{file}:7:7: error [E-TRACE-MOCK-UNDECLARED] `bridges.check` answer 2 gives `margn`"
        ),
        format!(
            "{file}:8:3: error [E-TRACE-MOCK-UNDECLARED] `bridges.chek` answers no plugin call"
        ),
    ] {
        assert!(text.contains(&want), "missing `{want}` in:\n{text}");
    }
    assert!(!text.contains(":0:0:"), "{text}");
}

/// ER N7: a `bridges:` error rendered at `<traced document>:0:0`. It is the
/// mock's own entry at fault, so it renders at that entry's line:column in
/// the mock — a scenario test, a `trace --mock` file, a `mocks/*.yaml`.
#[test]
fn a_bridges_mock_error_is_anchored_at_its_entry_in_the_mock() {
    let (t, o) = scenario(
        "bridge-anchored",
        &format!("file: ../scenes/probe/c.lute\n{BAD_BRIDGES}expect:\n  exit: complete\n"),
    );
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert_anchored(&out(&o), &t.display().to_string());

    let doc = fixture().join("scenes/probe/c.lute");
    let mock = write(
        &temp_dir("trace-anchored"),
        "m.yaml",
        &format!("file: {}\n{BAD_BRIDGES}", doc.display()),
    );
    let o = Command::new(BIN)
        .args([
            "trace",
            doc.to_str().unwrap(),
            "--project",
            fixture().to_str().unwrap(),
        ])
        .args(["--mock", mock.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert_anchored(&out(&o), &mock.display().to_string());

    let project = temp_dir("check-project-anchored");
    copy_dir(&fixture(), &project);
    let mock = write(
        &project,
        "mocks/bad.yaml",
        &format!("file: ../scenes/probe/c.lute\n{BAD_BRIDGES}"),
    );
    let o = Command::new(BIN)
        .args(["check-project", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert_anchored(&out(&o), &mock.display().to_string());
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// The fixture copied to a temp project with the gated line over `margin`
/// removed: no content reads `margin` any more.
fn margin_unread(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    copy_dir(&fixture(), &dir);
    let c = dir.join("scenes/probe/c.lute");
    let text = std::fs::read_to_string(&c).unwrap();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.contains("guards.margin"))
        .collect();
    assert_eq!(
        kept.len() + 1,
        text.lines().count(),
        "the fixture reads margin once"
    );
    std::fs::write(&c, kept.join("\n") + "\n").unwrap();
    dir
}

/// dsl 0.25.0 §7: a bridge result field no content reads may be left out
/// of an answer — in `lute play`, `lute trace`, a scenario test and a
/// `mocks/*.yaml` alike — and every hint lists only the fields content
/// reads. A field content reads stays required (the tests above).
#[test]
fn an_unread_bridge_result_field_may_be_left_out() {
    let dir = margin_unread("unread");
    let answers = "bridges:\n  check:\n    - { passed: true }\n    - { passed: false }\n";

    // play: answered without `margin`, and the halt hint omits it.
    let script = write(
        &dir,
        "s.play.yaml",
        &format!("{answers}steps:\n  - occasion: hubVisit\n"),
    );
    let run_play = |script: &Path| {
        Command::new(BIN)
            .args([
                "play",
                dir.to_str().unwrap(),
                "--script",
                script.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let o = run_play(&script);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    assert!(
        out(&o).contains("(bridge answered: passed=true)"),
        "{}",
        out(&o)
    );
    assert!(
        out(&o).contains("Passed.") && out(&o).contains("Spotted."),
        "{}",
        out(&o)
    );
    let bare = write(&dir, "bare.play.yaml", "steps:\n  - occasion: hubVisit\n");
    let o = run_play(&bare);
    assert_eq!(o.status.code(), Some(3), "{}", out(&o));
    assert!(
        out(&o).contains("(bridge unanswered: passed)"),
        "{}",
        out(&o)
    );
    assert!(
        out(&o).contains("bridges: { check: [ { passed: <bool> } ] }"),
        "{}",
        out(&o)
    );

    // trace: the same answer is accepted, and the unmocked hint omits it.
    let doc = dir.join("scenes/probe/c.lute");
    let trace = |mock: Option<&Path>| {
        let mut c = Command::new(BIN);
        c.args([
            "trace",
            doc.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
        ]);
        if let Some(m) = mock {
            c.args(["--mock", m.to_str().unwrap()]);
        }
        c.output().unwrap()
    };
    let mock = write(&temp_dir("unread-mock"), "m.yaml", answers);
    let o = trace(Some(&mock));
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    assert!(
        out(&o).contains("Passed.") && out(&o).contains("Spotted."),
        "{}",
        out(&o)
    );
    let o = trace(None);
    assert_eq!(o.status.code(), Some(3), "{}", out(&o));
    assert!(
        out(&o).contains("bridges: { check: [ { passed: <bool> } ] }"),
        "{}",
        out(&o)
    );

    // A scenario test and a checked `mocks/*.yaml` take it too.
    write(
        &dir,
        "tests/unread.test.yaml",
        &format!(
            "file: ../scenes/probe/c.lute\n{answers}expect:\n  transcriptContains: [\"Passed.\"]\n"
        ),
    );
    let o = Command::new(BIN)
        .args(["test", dir.join("tests/unread.test.yaml").to_str().unwrap()])
        .args(["--project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    write(
        &dir,
        "mocks/unread.yaml",
        &format!("file: ../scenes/probe/c.lute\n{answers}"),
    );
    let o = Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out(&o).contains("E-TRACE-MOCK"), "{}", out(&o));
}
