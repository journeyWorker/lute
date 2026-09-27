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
    ward(tag, "{ args: [room], tier: run, reserved: true }")
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

/// HW27-15: `lute trace` heads a beat the engine would not raise with that
/// premise — its occasion's false `raisedWhen`, a holding `terminal:` — not
/// with a `when` that holds; a false `when` names itself.
#[test]
fn a_trace_names_the_gate_or_terminal_a_beat_is_not_raised_by() {
    let dir = open_ward("trace-gate");
    write(
        &dir,
        "lore/notes.lute",
        "---\nkind: lore\nid: notes\n---\n\n\
         <beat id=\"door\" on=\"enter\" target=\"room.office\" when=\"run.hp > 0\">\n  \
         @narrator: The door.\n</beat>\n",
    );
    let trace = |extra: &[&str]| {
        let out = Command::new(BIN)
            .current_dir(&dir)
            .args([
                "trace",
                "lore/notes.lute",
                "--project",
                ".",
                "--beat",
                "door",
            ])
            .args(extra)
            .output()
            .unwrap();
        text(&out)
    };
    let t = trace(&[]);
    assert!(
        t.contains(
            "<beat notes.door>   (not eligible: the engine does not raise `enter`: its \
             `raisedWhen: holds(canEnter(occasion.target))` is false since `canEnter(office)` \
             does not hold)"
        ),
        "{t}"
    );
    let t = trace(&["--fact", "canEnter(office)", "--state", "run.fate=taken"]);
    assert!(
        t.contains("(not eligible: the game is over (`terminal: run.fate == 'taken'` holds)"),
        "{t}"
    );
    let t = trace(&["--fact", "canEnter(office)", "--state", "run.hp=0"]);
    assert!(
        t.contains("(not eligible: its `when` (run.hp > 0) is false (`run.hp` is 0))"),
        "{t}"
    );
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
    // A play that never reaches it ended `complete`, not `terminal`.
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.lobby\n",
        true,
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["end"], "complete", "{}", text(&out));
}

/// `expect.end` tells the two apart: a play that reached `terminal:` fails
/// `end: complete` and passes `end: terminal`, and one that did not fails
/// `end: terminal` — `exit:` used to pass both as `complete`.
#[test]
fn expect_end_tells_terminal_from_complete() {
    let dir = open_ward("expect-end");
    let over = "steps:\n  - occasion: knock\n";
    let out = play(&dir, &format!("{over}expect: {{ end: terminal }}\n"), false);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = play(&dir, &format!("{over}expect: {{ end: complete }}\n"), false);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("end of play: expect end: expected complete, actual terminal"),
        "{}",
        text(&out)
    );
    let open = "steps:\n  - occasion: enter\n    target: room.lobby\nexpect: { end: terminal }\n";
    let out = play(&dir, open, false);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("expect end: expected terminal, actual complete"),
        "{}",
        text(&out)
    );
    // The old key is refused, located, naming the new one.
    let out = play(
        &dir,
        &format!("{over}expect: {{ exit: complete }}\n"),
        false,
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains(":3:11: `expect.exit` is now `expect.end`"),
        "{}",
        text(&out)
    );
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

/// The game-over note follows the settle that ended the game (a quest the
/// step completed), and a terminal over state a new run keeps says so — on
/// the step that ended it and on the `newRun` that did not reopen it.
#[test]
fn the_game_over_note_follows_its_settle_and_names_a_kept_terminal() {
    let dir = open_ward("note-order");
    let schema = std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap();
    write(
        &dir,
        "world.schema.yaml",
        &schema.replace(
            "terminal: \"run.fate == 'taken'\"",
            "terminal: \"quest.doom.state == 'complete'\"",
        ),
    );
    write(
        &dir,
        "quests/doom.lute",
        "---\nkind: quest\nid: qs\n---\n\n<quest id=\"doom\" title=\"Doom\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"fall\" title=\"Fall\" on=\"knock\" done=\"run.fate == 'taken'\"/>\n</quest>\n",
    );
    let out = play(&dir, "steps:\n  - occasion: knock\n", false);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let settle = t
        .find("quest doom -> complete")
        .unwrap_or_else(|| panic!("{t}"));
    let note = t
        .find("note: the game is over")
        .unwrap_or_else(|| panic!("{t}"));
    assert!(settle < note, "the note follows the settle: {t}");
    assert!(t.contains("`newRun: true` starts a new run)"), "{t}");

    // `user.*` outlives a new run: the note does not offer one.
    let dir = open_ward("kept-terminal");
    for rel in ["world.schema.yaml", "scenes/taken.lute"] {
        let body = std::fs::read_to_string(dir.join(rel)).unwrap();
        write(&dir, rel, &body.replace("run.fate", "user.fate"));
    }
    let out = play(
        &dir,
        "steps:\n  - occasion: knock\n  - newRun: true\n",
        false,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("starts a new run"), "{t}");
    assert!(
        t.contains("it still holds after a new run: it reads `user.fate`, which a new run keeps)"),
        "{t}"
    );
    let new_run = t
        .find("── step 2 · new run")
        .unwrap_or_else(|| panic!("{t}"));
    assert!(
        t[new_run..].contains(
            "note: the game is still over after the new run — `terminal: user.fate == 'taken'` \
             holds"
        ),
        "{t}"
    );
}

/// `lute beats` marks the ladder of a target whose gate can never hold —
/// here nothing ever lets the player into the office — judged under the
/// project's fact envelope, in text and JSON.
#[test]
fn lute_beats_marks_a_target_whose_gate_never_holds() {
    let dir = ward("beats", "{ args: [room], tier: run }");
    let out = Command::new(BIN).arg("beats").arg(&dir).output().unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains(
            "enter @ room.office — select: first · raisedWhen: \
             holds(canEnter(occasion.target)) · gate never holds"
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
    let dir = ward("locked", "{ args: [room], tier: run }");
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
            "lockedDoor @ room.lobby — select: first · raisedWhen: \
             !holds(canEnter(occasion.target)) · gate never holds"
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

/// `lute beats` always shows a ladder's gate and a `for` beat's kind (text
/// and JSON, as the compiled index has them), spells an unspent `once` one
/// way; `lute calendar` names the member each `for` presentation is for;
/// `reach --endings` counts a beat whose writes can make `terminal:` hold,
/// carries the occasion's gate on a `gate:` line, and its JSON writers are
/// records.
#[test]
fn tools_show_gates_for_beats_terminal_endings_and_structured_writers() {
    let dir = open_ward("tools-views");
    write(
        &dir,
        "plugins/ward/occasions/extra.yaml",
        "occasions:\n  evening: { select: sequence }\n",
    );
    write(
        &dir,
        "lore/end.lute",
        "---\nkind: lore\nid: end\ntitle: End\n---\n\n\
         <beat id=\"each\" on=\"evening\" for=\"kind:room\" once=\"false\">\n  @narrator: Each.\n</beat>\n\n\
         <beat id=\"alive\" on=\"knock\" when=\"run.fate == 'alive'\">\n  @narrator: Still here.\n</beat>\n",
    );
    let d = dir.to_str().unwrap();
    let lute = |args: &[&str]| text(&Command::new(BIN).args(args).output().unwrap());

    let t = lute(&["beats", d, "--occasion", "hourStrikes"]);
    assert!(
        t.contains("  hourStrikes — select: first · raisedWhen: run.hp > 1\n"),
        "{t}"
    );
    assert!(t.contains(" strike ") && t.contains(" none "), "{t}");
    assert!(!t.contains(" no "), "{t}");
    let t = lute(&["beats", d, "--occasion", "evening"]);
    assert!(t.contains("end.each (for kind:room)"), "{t}");
    let v: serde_json::Value =
        serde_json::from_str(&lute(&["beats", d, "--occasion", "evening", "--json"])).unwrap();
    let row = &v["roots"][0]["ladders"][0]["beats"][0];
    assert_eq!(row["for"], "kind:room", "{v}");
    assert_eq!(row["forKind"]["kind"], "room", "{v}");

    let t = lute(&["calendar", d, "--occasion", "evening"]);
    assert!(t.contains("end.each for "), "{t}");
    assert!(!t.contains("end.each, end.each"), "{t}");

    let t = lute(&["scenario", d, "reach", "--endings"]);
    assert!(
        t.contains(
            "  taken (scene, scenes/taken.lute): reachable\n    ends: writes `run.fate`, which \
             `terminal: run.fate == 'taken'` reads\n"
        ),
        "{t}"
    );
    let t = lute(&["scenario", d, "reach", "--endings=hourStrikes"]);
    assert!(
        t.contains(
            "    gate: `raisedWhen: run.hp > 1` — it needs:\n      run.hp — the engine (`owner: engine`)\n    \
             when: no `when` — holds whenever its gate lets the occasion be raised\n"
        ),
        "{t}"
    );
    let v: serde_json::Value = serde_json::from_str(&lute(&[
        "scenario",
        d,
        "--format",
        "json",
        "reach",
        "--endings=knock",
    ]))
    .unwrap();
    let alive = v["roots"][0]["endings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "end.alive")
        .unwrap_or_else(|| panic!("{v}"));
    assert_eq!(
        alive["needs"][0]["writers"][0],
        serde_json::json!({ "kind": "scene", "id": "taken", "file": "scenes/taken.lute",
            "how": "set", "via": null, "text": "scene `taken`" }),
        "{v}"
    );
}

/// `reach --endings`: a beat is no writer of its own `when` (it writes it
/// only once the `when` held), and a `terminal:` over a quest's state
/// counts the beats whose raise judges that quest as endings.
#[test]
fn endings_skip_a_beats_own_writes_and_trace_a_quest_terminal() {
    let dir = open_ward("endings-own");
    write(
        &dir,
        "scenes/relapse.lute",
        "---\nkind: scene\nid: relapse\non: knock\npriority: 5\nwhen: \"run.fate == 'alive'\"\n---\n\n\
         ## Relapse\n\n::set{run.fate = \"alive\"}\n@narrator: You hold on.\n",
    );
    let d = dir.to_str().unwrap();
    let lute = |args: &[&str]| text(&Command::new(BIN).args(args).output().unwrap());
    let t = lute(&["scenario", d, "reach", "--endings=knock"]);
    let relapse = &t[t.find("  relapse (scene").unwrap_or_else(|| panic!("{t}"))..];
    let need = relapse
        .lines()
        .find(|l| l.trim_start().starts_with("run.fate —"))
        .unwrap_or_else(|| panic!("{t}"));
    assert_eq!(need, "      run.fate — written by scene `taken`", "{t}");

    // Only the beat itself writes what its `when` needs.
    std::fs::remove_file(dir.join("scenes/taken.lute")).unwrap();
    let t = lute(&["scenario", d, "reach", "--endings=knock"]);
    assert!(
        t.contains(
            "      run.fate — only this beat writes it, after its conditions held — nothing else \
             does, so it keeps its declared default"
        ),
        "{t}"
    );

    let dir = open_ward("endings-quest");
    let schema = std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap();
    write(
        &dir,
        "world.schema.yaml",
        &schema.replace(
            "terminal: \"run.fate == 'taken'\"",
            "terminal: \"quest.doom.state == 'complete'\"",
        ),
    );
    write(
        &dir,
        "quests/doom.lute",
        "---\nkind: quest\nid: qs\n---\n\n<quest id=\"doom\" title=\"Doom\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"fall\" title=\"Fall\" on=\"knock\" done=\"run.fate == 'taken'\"/>\n</quest>\n",
    );
    let d = dir.to_str().unwrap();
    let t = lute(&["scenario", d, "reach", "--endings"]);
    assert!(
        t.contains(
            "  taken (scene, scenes/taken.lute): reachable\n    ends: its `knock` raise judges \
             quest `doom`, whose state `terminal: quest.doom.state == 'complete'` reads\n"
        ),
        "{t}"
    );
    assert!(!t.contains("lobby (scene"), "{t}");
}

/// A calendar cell whose occasion's `raisedWhen` is false reads `gate false`
/// (not `-`, which is "raised, nothing eligible"); `--json` marks it
/// `gated: true`, a cell whose gate holds carries no such mark.
#[test]
fn a_calendar_cell_whose_gate_is_false_says_so() {
    let dir = open_ward("calendar-gate");
    let calendar = |extra: &[&str]| {
        let out = Command::new(BIN)
            .args(["calendar", &dir.display().to_string()])
            .args([
                "--occasion",
                "enter",
                "--target",
                "room.office",
                "--target",
                "room.lobby",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let t = calendar(&[]);
    let row = t
        .lines()
        .find(|l| l.contains("lobby") && !l.contains("room."))
        .unwrap_or_else(|| panic!("{t}"));
    assert!(row.starts_with("gate false"), "{t}");
    let csv = calendar(&["--csv"]);
    assert!(
        csv.contains("enter,room.office,first,,,,,gate false"),
        "{csv}"
    );
    let v: serde_json::Value = serde_json::from_str(&calendar(&["--json"])).unwrap();
    let results = v["cells"][0]["results"].as_array().unwrap();
    let at = |t: &str| results.iter().find(|r| r["target"] == t).unwrap();
    assert_eq!(at("room.office")["gated"], true, "{v}");
    assert!(at("room.lobby").get("gated").is_none(), "{v}");
    assert_eq!(at("room.lobby")["winner"], "lobby", "{v}");
}

/// A play usage error is plain text: no spec section, no ticket id.
#[test]
fn a_play_usage_error_cites_no_spec_section() {
    let dir = open_ward("usage-plain");
    let out = play(
        &dir,
        "steps:\n  - occasion: enter\n    target: room.offce\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("target `room.offce` is outside occasion `enter`'s domain")
            && t.contains("did you mean `room.office`?"),
        "{t}"
    );
    assert!(!t.contains('§') && !t.contains("dsl "), "{t}");
}

/// Two villagers and one targeted `talk` occasion whose declaration (after
/// the occasion name) is `talk`; `schema_extra` is appended to the schema.
fn villagers(tag: &str, talk: &str, schema_extra: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles: { g: { plugins: { p: true } } }\n\
         defaults: { uses: [w.schema.yaml] }\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports: { occasions: occasions/ }\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        &format!("occasions:\n  talk: {{ select: first, target: {{ prefix: npc, entity: villager }}, {talk} }}\n"),
    );
    write(
        &dir,
        "w.schema.yaml",
        &format!(
            "state:\n  user.bond: {{ type: number, default: 0, per: villager }}\n\
             entities:\n  villager: {{ members: [mara, ines] }}\n{schema_extra}"
        ),
    );
    write(
        &dir,
        "lore/a.lute",
        "---\nkind: lore\nid: a\n---\n\n<beat id=\"hi\" on=\"talk\" target=\"kind:villager\" once=\"false\">\n  \
         @narrator: Hi.\n</beat>\n",
    );
    dir
}

/// A family read by the raised member names the member path and its value
/// (it used to say the family was unset), and the refused raise's step says
/// it was not raised — not that nothing answered it (`--json`: `notRaised`).
#[test]
fn a_gate_over_a_member_family_names_the_member_path_and_is_not_raised() {
    let dir = villagers(
        "gate-member",
        "raisedWhen: \"user.bond[occasion.target] >= 1\"",
        "",
    );
    let script = "steps:\n  - occasion: talk\n    target: npc.ines\n";
    let t = text(&play(&dir, script, false));
    assert!(
        t.contains("which is false here since `user.bond.ines` is 0 —"),
        "{t}"
    );
    assert!(!t.contains("`user.bond` is unset"), "{t}");
    assert!(t.contains("  (not raised: gate false)"), "{t}");
    assert!(!t.contains("(no candidates)"), "{t}");
    let v: serde_json::Value = serde_json::from_slice(&play(&dir, script, true).stdout).unwrap();
    assert_eq!(v["steps"][0]["notRaised"], "gate false", "{v}");
}

/// A gate over the raise's payload is fixed by the step's own `payload:`,
/// not by an earlier write.
#[test]
fn a_payload_gate_refusal_points_at_the_steps_payload() {
    let dir = villagers(
        "gate-payload",
        "payload: { weight: number }, raisedWhen: \"occasion.payload.weight > 0\"",
        "",
    );
    let t = text(&play(
        &dir,
        "steps:\n  - occasion: talk\n    target: npc.ines\n    payload: { weight: 0 }\n",
        false,
    ));
    assert!(
        t.contains(
            "since `occasion.payload.weight` is 0 — raise it with a payload that satisfies it \
             (`payload:` on this step), or drop the step"
        ),
        "{t}"
    );
    assert!(!t.contains("an `engine:` write"), "{t}");
}

/// A gate written as a `@def` over a negated fact is quoted as written and
/// names the fact that holds; the compiled gate keeps the author's text
/// beside the expansion an engine evaluates.
#[test]
fn a_def_gate_over_a_negated_fact_is_quoted_as_written_and_names_the_fact() {
    let dir = villagers(
        "gate-def",
        "raisedWhen: \"@standing\"",
        "relations:\n  fell: { args: [villager], tier: run }\ndefs:\n  standing: \"!holds(fell(occasion.target))\"\n",
    );
    let t = text(&play(
        &dir,
        "facts: [fell(ines)]\nsteps:\n  - occasion: talk\n    target: npc.ines\n",
        false,
    ));
    assert!(
        t.contains("only when `@standing` (its `raisedWhen`), which is false here since `fell(ines)` holds"),
        "{t}"
    );
    let out = Command::new(BIN)
        .args(["compile", "--project", &dir.display().to_string()])
        .arg(dir.join("lore/a.lute"))
        .arg("-o")
        .arg(dir.join("a.json"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("a.json")).unwrap()).unwrap();
    let gate = &v["gates"][0]["raisedWhen"];
    assert_eq!(gate["authored"], "@standing", "{v}");
    assert!(
        gate["raw"]
            .as_str()
            .unwrap()
            .contains("holds(fell(occasion.target))"),
        "{v}"
    );
}
