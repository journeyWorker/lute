//! dsl 0.24.0 §5 (T1-14, T2-10): bridge answers in `lute play`, `lute
//! trace` and `lute test`, over `tests/fixtures/bridge-check` — a `beats`
//! project whose plugin declares a `::check` skill check writing
//! `scene.check.<key>.{passed,margin}` from its bridge result. `probe.c`
//! makes two calls (`guards`, then `sneak`), each followed by a `<match>`
//! over its `passed` slot.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-check")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-bridges-{tag}-{}", std::process::id()));
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
        .args(["play", fixture().to_str().unwrap(), "--script", script.to_str().unwrap()])
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
    assert!(text.contains("(bridge unanswered: passed, margin)"), "{text}");
    assert!(text.contains("plugin call `check`"), "{text}");
    assert!(text.contains("bridges: { check: [ { passed: …, margin: … } ] }"), "{text}");
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
    assert!(text.contains("(bridge answered: passed=true, margin=3)"), "{text}");
    assert!(text.contains("(bridge answered: passed=false, margin=-2)"), "{text}");
    let passed = text.find("Passed.").expect(&text);
    let spotted = text.find("Spotted.").expect(&text);
    assert!(passed < spotted, "{text}");
    assert!(!text.contains("Failed.") && !text.contains("Sneaked."), "{text}");
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
    assert!(text.contains("Passed.") && text.contains("Spotted."), "{text}");
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
    assert!(text.contains("step 1: its `bridges:` answers were not all consumed"), "{text}");
    assert!(text.contains("`check` {passed: false, margin: 9}"), "{text}");
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
            "lacks `margin`",
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

#[test]
fn trace_reads_an_unmocked_bridge_result_as_unknown() {
    let o = trace(None);
    let text = out(&o);
    assert_eq!(o.status.code(), Some(3), "{text}");
    assert!(!text.contains("-> arm 2"), "the shape default must not decide: {text}");
    assert!(
        text.contains("bridges: { check: [ { passed: <value> } ] }"),
        "the hint names the tag and field: {text}"
    );
}

#[test]
fn trace_takes_the_mocked_answer_in_call_order() {
    let o = trace(Some(
        "bridges:\n  check:\n    - { passed: true, margin: 3 }\n    - { passed: false, margin: 0 }\n",
    ));
    let text = out(&o);
    assert_eq!(o.status.code(), Some(0), "{text}");
    assert!(text.contains("(bridge answered: passed=true, margin=3)"), "{text}");
    assert!(text.contains("Passed.") && text.contains("Spotted."), "{text}");
    assert!(!text.contains("Failed.") && !text.contains("Sneaked."), "{text}");

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
            .args(["test", t.to_str().unwrap(), "--project", dir.to_str().unwrap()])
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
    assert!(out(&without).contains("bridges: { check:"), "{}", out(&without));
}
