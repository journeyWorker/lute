//! dsl 0.27.0 §4 (T2-3, T2-4) through the built `lute` binary: an occasion's
//! `raisedWhen` gate and the schema's `terminal:` as `lute play` honours them
//! (refused steps, skipped clock raises, `end: terminal`) and as `lute beats`
//! marks them (a target whose gate can never hold).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-seam027-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A one-night ward: `enter` is raised only for a room the engine lets the
/// player into (`canEnter`, engine-written), the clock strikes the hour only
/// while the player has strength left, and the game is over once the
/// stalker has taken them. `relation` is `canEnter`'s declaration.
fn ward(tag: &str, relation: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { ward: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/ward/plugin.yaml",
        "id: ward\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/ward/occasions/o.yaml",
        "occasions:\n  \
         enter: { select: first, target: { prefix: room, entity: room }, raisedWhen: \"holds(canEnter(occasion.target))\" }\n  \
         hourStrikes: { select: first, raisedWhen: \"run.hp > 1\" }\n  \
         knock: { select: first }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        &format!(
            "state:\n  run.night: {{ type: number, default: 1, owner: engine }}\n  \
             run.hour: {{ type: {{ enum: [h23, h00, h01] }}, default: h23, owner: engine }}\n  \
             run.hp: {{ type: number, default: 3, owner: engine }}\n  \
             run.fate: {{ type: {{ enum: [alive, taken] }}, default: alive }}\n\
             clock:\n  day: run.night\n  slot: run.hour\n  slots: [h23, h00, h01]\n  raise: {{ slot: hourStrikes }}\n\
             entities:\n  room: {{ members: [lobby, office] }}\n\
             relations:\n  canEnter: {relation}\n\
             facts:\n  - canEnter(lobby)\n\
             terminal: \"run.fate == 'taken'\"\n"
        ),
    );
    for (id, on, target, body) in [
        ("lobby", "enter", "room.lobby", "The lobby is cold."),
        ("office", "enter", "room.office", "The office door gives."),
        (
            "strike",
            "hourStrikes",
            "",
            "The clock strikes at {{run.hour}}.",
        ),
    ] {
        let target = if target.is_empty() {
            String::new()
        } else {
            format!("target: {target}\n")
        };
        write(
            &dir,
            &format!("scenes/{id}.lute"),
            &format!(
                "---\nkind: scene\nid: {id}\non: {on}\n{target}once: false\n---\n\n## {id}\n\n@narrator: {body}\n"
            ),
        );
    }
    write(
        &dir,
        "scenes/taken.lute",
        "---\nkind: scene\nid: taken\non: knock\nonce: false\n---\n\n## Taken\n\n\
         ::set{run.fate = \"taken\"}\n@narrator: Something takes you.\n",
    );
    dir
}

/// The ward whose office the engine may open (`canEnter` reserved).
fn open_ward(tag: &str) -> PathBuf {
    ward(tag, "{ args: [room], reserved: true }")
}

fn play(dir: &Path, script: &str, json: bool) -> Output {
    write(dir, "s.play.yaml", script);
    let mut cmd = Command::new(BIN);
    cmd.args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"));
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

/// An `occasion:` step raising a gated occasion while its gate is false is
/// refused (exit 1); an `engine:` write on the same step that opens the
/// gate lets it play.
#[test]
fn an_occasion_step_while_its_gate_is_false_is_refused() {
    let dir = open_ward("gate");
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.office\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "step 1: E-OCCASION-GATE: the engine raises `enter` for `room.office` only when \
             `holds(canEnter(occasion.target))` (its `raisedWhen`), which is false here"
        ),
        "{t}"
    );
    assert!(!t.contains("The office door gives."), "{t}");

    // The lobby's gate holds (a seed fact); the office opens by an engine write.
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.lobby\n  \
         - occasion: enter\n    target: room.office\n    engine: { facts: [canEnter(office)] }\n",
        false,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("The lobby is cold.") && t.contains("The office door gives."),
        "{t}"
    );
}

/// HW27-10: the refusal is located at the step as written and names the
/// read the gate is false over.
#[test]
fn a_gate_refusal_is_located_and_names_its_false_read() {
    let dir = open_ward("gate-located");
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.office\n",
        false,
    );
    let t = text(&out);
    assert!(
        t.contains("s.play.yaml:2:5: step 1: E-OCCASION-GATE"),
        "{t}"
    );
    assert!(
        t.contains("which is false here since `canEnter(office)` does not hold"),
        "{t}"
    );
}

/// `lute test` in `dir` (JSON when `json`).
fn test_in(dir: &Path, json: bool) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("test").arg(dir.join("tests"));
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

/// HW27-04: `lute test` judges a beat's eligibility by the same seam `lute
/// play` refuses a raise by — `eligible: true` on a beat whose occasion's
/// gate is false under the mocks, or after the game is over, misses naming
/// that premise (`--json`: `notRaised`); a mock that opens the gate passes.
#[test]
fn a_test_judges_the_gate_and_terminal_like_play() {
    let dir = open_ward("test-gate");
    write(
        &dir,
        "tests/gate.test.yaml",
        "file: ../scenes/office.lute\nexpect:\n  eligible: true\n",
    );
    write(
        &dir,
        "tests/over.test.yaml",
        "file: ../scenes/lobby.lute\nstate: { run.fate: taken }\nexpect:\n  eligible: true\n",
    );
    write(
        &dir,
        "tests/open.test.yaml",
        "file: ../scenes/office.lute\nfacts: [canEnter(office)]\nexpect:\n  eligible: true\n",
    );
    let out = test_in(&dir, false);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "eligible: expected true, got false — the engine does not raise `enter`: its \
             `raisedWhen: holds(canEnter(occasion.target))` is false since `canEnter(office)` \
             does not hold"
        ),
        "{t}"
    );
    assert!(
        t.contains(
            "eligible: expected true, got false — the game is over (`terminal: run.fate == \
             'taken'` holds), so the engine raises no occasion"
        ),
        "{t}"
    );
    assert!(t.contains("PASS  ") && t.contains("open.test.yaml"), "{t}");

    let v: serde_json::Value = serde_json::from_slice(&test_in(&dir, true).stdout).unwrap();
    let not_raised = |test: &str| {
        v["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["test"].as_str().is_some_and(|p| p.ends_with(test)))
            .map(|r| r["expectations"][0]["notRaised"].clone())
            .unwrap()
    };
    assert_eq!(
        not_raised("gate.test.yaml"),
        serde_json::json!({
            "occasion": "enter",
            "reason": "gate",
            "condition": "holds(canEnter(occasion.target))",
            "falseReads": ["`canEnter(office)` does not hold"],
        }),
        "{v}"
    );
    assert_eq!(not_raised("over.test.yaml")["reason"], "terminal", "{v}");
    assert!(not_raised("open.test.yaml").is_null(), "{v}");
}

/// HW27-01: an `occasion:` step's `engine:` write is part of the step, not
/// a step of its own — the step's `expect:` judges the raise (and the world
/// after it), a `repeat:` counts its repetitions only, and the end counts
/// the script's steps.
#[test]
fn an_occasion_steps_engine_write_is_judged_with_its_raise() {
    let dir = open_ward("engine-expect");
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.office\n    \
         engine: { facts: [canEnter(office)] }\n    \
         expect: { winner: office, facts: [canEnter(office)] }\n  \
         - occasion: enter\n    target: room.lobby\n    engine: { state: { run.hp: 2 } }\n    \
         repeat: 2\n    expect: { winner: lobby, state: { run.hp: 2 } }\n",
        false,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("── expect: every expectation held"), "{t}");
    assert!(t.contains("── end: complete (3 steps)"), "{t}");

    // A miss names the raise, with no made-up repetition.
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.office\n    \
         engine: { facts: [canEnter(office)] }\n    expect: { winner: lobby }\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("✗ step 1 at enter room.office: expect winner: expected lobby, actual office"),
        "{t}"
    );

    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.office\n    \
         engine: { facts: [canEnter(office)] }\n",
        true,
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["steps"][0]["beforeRaise"], true, "{}", text(&out));
    assert_eq!(v["steps"][1]["winner"], "office", "{}", text(&out));
    assert!(v["steps"][1].get("beforeRaise").is_none(), "{}", text(&out));
}

/// OT-F-1 / FS-F9: a play's `choose:` naming no branch or hub is a located
/// usage error before anything plays — top level and on a step.
#[test]
fn a_play_choose_naming_no_decision_is_a_located_usage_error() {
    let dir = open_ward("choose");
    let out = play(
        &dir,
        "choose: { clsh: [sideRen] }\nsteps:\n  - occasion: knock\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("s.play.yaml:1:11: `choose.clsh` names no branch or hub of the project"),
        "{t}"
    );
    assert!(!t.contains("Something takes you."), "{t}");
    let out = play(
        &dir,
        "steps:\n  - occasion: knock\n    choose: { clsh: [sideRen] }\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("s.play.yaml:3:15: step 1: `choose.clsh` names no branch or hub"),
        "{t}"
    );
}

/// A clock raise whose gate is false is not made — no error — and the step
/// says so; the clock still moves.
#[test]
fn a_gated_clock_raise_is_skipped_with_a_note() {
    let dir = open_ward("slot");
    let out = play(
        &dir,
        "steps:\n  - advance: slot\n  - engine: { state: { run.hp: 1 } }\n  - advance: slot\n",
        true,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["steps"][0]["winner"], "strike", "{t}");
    assert!(v["steps"][0].get("notes").is_none(), "{t}");
    let step3 = &v["steps"][2];
    assert_eq!(step3["advance"]["to"], "day 1 h01", "{t}");
    assert!(step3.get("winner").is_none(), "no raise: {t}");
    assert_eq!(
        step3["notes"][0],
        "`hourStrikes` was not raised at day 1 h01 — its `raisedWhen: run.hp > 1` is false \
         there since `run.hp` is 1; the clock moved on without it",
        "{t}"
    );
}

/// Once `terminal:` holds the playthrough ends `end: terminal`; the step
/// that ended the game says so.
#[test]
fn the_game_over_ends_the_play_in_the_terminal_state() {
    let dir = open_ward("terminal");
    let out = play(&dir, "steps:\n  - occasion: knock\n", false);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains(
            "note: the game is over — `terminal: run.fate == 'taken'` holds, so the engine \
             raises no occasion from here"
        ),
        "{t}"
    );
    assert!(
        t.contains("── end: terminal — `terminal: run.fate == 'taken'` holds"),
        "{t}"
    );

    let out = play(&dir, "steps:\n  - occasion: knock\n", true);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["end"], "terminal", "{}", text(&out));
    assert_eq!(v["exit"], "complete", "{}", text(&out));
    // A play that never reaches it has no `end`.
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.lobby\n",
        true,
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("end").is_none(), "{}", text(&out));
}

/// After the game is over an `occasion:` or `advance:` step is refused
/// (exit 1); a `newRun` starts a new run and play goes on.
#[test]
fn raising_after_the_game_is_over_is_refused_and_a_new_run_continues() {
    let dir = open_ward("after");
    let out = play(
        &dir,
        "steps:\n  - occasion: knock\n  - advance: slot\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "step 2: E-OCCASION-GATE: `advance:` after the game is over — `terminal: run.fate == \
             'taken'` holds"
        ),
        "{t}"
    );
    assert!(!t.contains("The clock strikes"), "{t}");
    // HW27-08: the refused advance's header says where the clock stands.
    assert!(
        t.contains("── step 2 · advance slot: day 1 h23 → day 1 h23 ──"),
        "{t}"
    );

    let out = play(
        &dir,
        "steps:\n  - occasion: knock\n  - occasion: enter\n    target: room.lobby\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "step 2: E-OCCASION-GATE: the game is over — `terminal: run.fate == 'taken'` holds, \
             so the engine raises no occasion (`enter` for `room.lobby` included)"
        ),
        "{t}"
    );
    assert!(!t.contains("The lobby is cold."), "{t}");

    let out = play(
        &dir,
        "steps:\n  - occasion: knock\n  - newRun: true\n  - advance: slot\n",
        false,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("The clock strikes at h00."), "{t}");
    assert!(t.contains("── end: complete (3 steps)"), "{t}");
}

/// `lute beats` marks the ladder of a target whose gate can never hold —
/// here nothing ever lets the player into the office — judged under the
/// project's fact envelope, in text and JSON.
#[test]
fn lute_beats_marks_a_target_whose_gate_never_holds() {
    let dir = ward("beats", "{ args: [room] }");
    let out = Command::new(BIN).arg("beats").arg(&dir).output().unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains(
            "enter @ room.office — select: first · gate never holds: \
             `raisedWhen: holds(canEnter(occasion.target))`"
        ),
        "{t}"
    );
    let lobby = t
        .lines()
        .find(|l| l.contains("enter @ room.lobby"))
        .unwrap_or_default();
    assert!(!lobby.contains("gate never holds"), "{t}");

    let out = Command::new(BIN)
        .arg("beats")
        .arg(&dir)
        .arg("--json")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ladders = v["roots"][0]["ladders"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let ladder = |target: &str| {
        ladders
            .iter()
            .find(|l| l["target"] == target)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(ladder("room.office")["gateNeverHolds"], true, "{ladders:?}");
    assert_eq!(
        ladder("room.lobby")["raisedWhen"],
        "holds(canEnter(occasion.target))",
        "{ladders:?}"
    );
    assert!(
        ladder("room.lobby").get("gateNeverHolds").is_none(),
        "{ladders:?}"
    );

    // An engine-written (`reserved`) relation may hold for any room.
    let dir = open_ward("beats-open");
    let out = Command::new(BIN).arg("beats").arg(&dir).output().unwrap();
    assert!(!text(&out).contains("gate never holds"), "{}", text(&out));
}

/// HW27-11: the other side — a negated gate over a fact that holds at
/// every point of every run (a seed nothing retracts) never holds either:
/// `check-project` reports the beat unreachable and `lute beats` marks it.
/// The office door, whose `canEnter` nothing produces, stays live.
#[test]
fn a_negated_gate_over_a_fact_that_always_holds_never_holds() {
    let dir = ward("locked", "{ args: [room] }");
    write(
        &dir,
        "plugins/ward/occasions/locked.yaml",
        "occasions:\n  lockedDoor: { select: first, target: { prefix: room, entity: room }, \
         raisedWhen: \"!holds(canEnter(occasion.target))\" }\n",
    );
    for room in ["lobby", "office"] {
        write(
            &dir,
            &format!("scenes/door-{room}.lute"),
            &format!(
                "---\nkind: scene\nid: door.{room}\non: lockedDoor\ntarget: room.{room}\nonce: false\n---\n\n\
                 ## Door\n\n@narrator: The {room} door is shut.\n"
            ),
        );
    }
    let out = Command::new(BIN)
        .arg("check-project")
        .arg(&dir)
        .output()
        .unwrap();
    let t = text(&out);
    assert!(
        t.contains("door-lobby.lute:")
            && t.contains("[E-BEAT-UNREACHABLE] beat `door.lobby` is never eligible")
            && t.contains("`canEnter(lobby)`"),
        "{t}"
    );
    assert!(!t.contains("door-office.lute:"), "{t}");
    let t = text(&Command::new(BIN).arg("beats").arg(&dir).output().unwrap());
    assert!(
        t.contains(
            "lockedDoor @ room.lobby — select: first · gate never holds: \
             `raisedWhen: !holds(canEnter(occasion.target))`"
        ),
        "{t}"
    );
    let office = t
        .lines()
        .find(|l| l.contains("lockedDoor @ room.office"))
        .unwrap_or_default();
    assert!(
        !office.is_empty() && !office.contains("gate never holds"),
        "{t}"
    );
}
