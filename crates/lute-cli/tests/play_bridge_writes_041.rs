//! 0.41.1 §4.1 (#82): a play script's `bridges:` answer may take the long
//! form `{ fields, writes }`, `writes` being the runtime's `WritesInput`.
//! `lute play` applies the writes at the call, `--events` sends them with the
//! `bridgeResult`, and the flat form stays exactly as it was.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-bridge-041")
}

/// `lute play <fixture> --script <script> [extra]`: (exit code, stdout, stderr).
fn play(script: &str, extra: &[&str]) -> (i32, String, String) {
    let dir = lute_scratch::ScratchDir::named("play-bridge-writes-041");
    let path = dir.join("s.play.yaml");
    std::fs::write(&path, script).unwrap();
    let out = Command::new(BIN)
        .arg("play")
        .arg(fixture())
        .arg("--script")
        .arg(&path)
        .args(extra)
        .output()
        .unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace(&*path.to_string_lossy(), "<script>");
    (out.status.code().unwrap(), text(&out.stdout), text(&out.stderr))
}

const STEPS: &str = "choose:\n  after: [rest]\nsteps:\n  - occasion: duel\n    target: foe.kato\n";

const LONG: &str = "bridges:\n  battle:\n    - fields: { outcome: win, hpFloor: 3 }\n      writes:\n        \
    state:\n          - { path: run.score, value: 1 }\n        facts: [pulled(rope)]\n        retract: []\n        \
    accept: []\n";

const EFFECTS: &str = "::battle{foe=occasion.target mode=\"ambush\" resultKey=\"fight\"}        (bridge answered: outcome=\"win\", hpFloor=3)\n  \
    set scene.battle.fight.outcome = \"win\"  (effect of ::battle)\n  \
    set scene.battle.fight.hpFloor = 3  (effect of ::battle)\n  \
    set run.score = 1  (effect of ::battle)\n  \
    assert pulled(rope)  (effect of ::battle)\n";

/// The spec's long form at the top level: the writes land at the call —
/// before the lines after it read them — and the play's end state has them.
#[test]
fn top_level_long_form_writes_land_at_the_call() {
    let script = format!("{LONG}{STEPS}expect:\n  state: {{ run.score: 1 }}\n  facts: [pulled(rope)]\n");
    let (code, stdout, stderr) = play(&script, &[]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains(EFFECTS), "{stdout}");
    assert!(stdout.contains("@narrator: Floor 3, score 1.\n"), "{stdout}");
}

/// The same long form on a step, with an `add`, beside the flat form.
#[test]
fn step_long_form_and_flat_answers_share_a_queue() {
    let script = "choose:\n  after: [rest, rest]\nsteps:\n  - occasion: duel\n    target: foe.kato\n    bridges:\n      \
        battle:\n        - fields: { outcome: lose, hpFloor: 0 }\n          writes: { state: [ { path: run.score, add: 2 } ] }\n  \
        - occasion: duel\n    target: foe.mira\n    bridges:\n      battle:\n        - { outcome: win, hpFloor: 1 }\n\
        expect:\n  state: { run.score: 2 }\n";
    let (code, stdout, stderr) = play(script, &[]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("@narrator: Floor 0, score 2.\n"), "{stdout}");
    assert!(stdout.contains("@narrator: Floor 1, score 2.\n"), "{stdout}");
}

/// A flat answer writes nothing beyond its result fields.
#[test]
fn flat_answers_are_unchanged() {
    let script = format!("bridges:\n  battle:\n    - {{ outcome: win, hpFloor: 3 }}\n{STEPS}");
    let (code, stdout, stderr) = play(&script, &[]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(!stdout.contains("set run.score"), "{stdout}");
    assert!(stdout.contains("@narrator: Floor 3, score 0.\n"), "{stdout}");
}

/// `--events` answers the runtime's `awaitBridge` with the writes as
/// written, and the runtime's replay agrees with the play.
#[test]
fn events_send_the_writes_with_the_answer() {
    let (code, stdout, stderr) = play(&format!("{LONG}{STEPS}"), &["--events"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let answer = stdout
        .lines()
        .find(|l| l.contains("\"type\":\"bridgeResult\""))
        .expect("a bridgeResult input");
    assert!(
        answer.starts_with(
            "{\"input\":{\"fields\":{\"hpFloor\":3,\"outcome\":\"win\"},\"request\":1,\"type\":\"bridgeResult\",\
             \"writes\":{\"facts\":[\"pulled(rope)\"],\"state\":[{\"path\":\"run.score\",\"value\":1}]}},"
        ),
        "{answer}"
    );
    assert!(answer.contains("Floor 3, score 1."), "{answer}");
}

#[test]
fn malformed_long_form_answers_are_usage_errors() {
    let cases = [
        (
            "{ fields: { outcome: win, hpFloor: 3 }, writes: { state: [] }, extra: 1 }",
            "<script>:2:3: `bridges.battle` answer 1 has unknown key `extra` — a long-form answer is \
             `{ fields: { … }, writes: { … } }`",
        ),
        (
            "{ outcome: win, hpFloor: 3, writes: { state: [ { path: run.score, value: 1 } ] } }",
            "<script>:2:3: `bridges.battle` answer 1 has unknown key `outcome` — a long-form answer is \
             `{ fields: { … }, writes: { … } }`",
        ),
        (
            "{ fields: { outcome: [win], hpFloor: 3 } }",
            "<script>:2:3: `bridges.battle` answer 1 `fields` must map field names to scalar literals \
             (bool/number/string)",
        ),
        (
            "{ fields: { outcome: win, hpFloor: 3 }, writes: { bogus: [] } }",
            "<script>:2:3: `bridges.battle` answer 1 `writes` is malformed: unknown field `bogus`, \
             expected one of `state`, `facts`, `retract`, `accept`",
        ),
        (
            "{ fields: { outcome: win, hpFloor: 3 }, writes: { state: [ { path: run.nope, value: 1 } ] } }",
            "top level: `bridges.battle` answer 1 `writes`: state `run.nope` is not a declared state \
             path in this project",
        ),
        (
            "{ fields: { outcome: win, hpFloor: 3 }, writes: { state: [ { path: run.day, value: 2 } ] } }",
            "top level: `bridges.battle` answer 1 `writes`: bridgeResult writes `run.day`, the clock's \
             path — the clock cannot move inside a walk; move it with `advanceClock` after the answer",
        ),
    ];
    for (answer, message) in cases {
        let script = format!("bridges:\n  battle:\n    - {answer}\n{STEPS}");
        let (code, stdout, stderr) = play(&script, &[]);
        assert_eq!(code, 2, "{answer}: {stdout}{stderr}");
        assert_eq!(stderr, format!("lute play: {message}\n"), "{answer}");
    }
}
