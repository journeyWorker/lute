//! `lute play` acceptance (dsl 0.21.0 §6): the `tests/fixtures/play-hub`
//! project — a tiny Hades-like hub whose project-local plugin declares the
//! `hubVisit` / `talk` (targeted) / `inbox` (`select: all`) occasions — played
//! end to end through the built `lute` binary: selection order, verdicts,
//! `once` spending, `newRun`, quest-gated beats, entry beats, and the exit-code
//! contract (0 complete, 1 error, 2 usage, 3 incomplete).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-hub")
}

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-play-{tag}-{}-{n}", std::process::id()));
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

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// Play `script` over `project`, human transcript.
fn play_in(project: &Path, tag: &str, script: &str, json: bool) -> Output {
    let dir = temp_dir(tag);
    let script = write(&dir, "s.play.yaml", script);
    let mut args = vec![
        "play".to_string(),
        project.display().to_string(),
        "--script".to_string(),
        script.display().to_string(),
    ];
    if json {
        args.push("--json".to_string());
    }
    Command::new(BIN).args(&args).output().unwrap()
}

/// Play `script` over the hub fixture with `--json`; asserts the exit code.
fn play_json(tag: &str, script: &str, exit: i32) -> Json {
    let out = play_in(&fixture(), tag, script, true);
    assert_eq!(
        out.status.code(),
        Some(exit),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    serde_json::from_slice(&out.stdout).expect("--json emits one JSON object")
}

fn step(v: &Json, n: usize) -> &Json {
    &v["steps"][n - 1]
}

fn winner(v: &Json, n: usize) -> Option<&str> {
    step(v, n)["winner"].as_str()
}

fn candidate<'a>(v: &'a Json, n: usize, id: &str) -> &'a Json {
    step(v, n)["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("step {n} has no candidate `{id}`: {}", step(v, n)))
}

fn candidate_ids(v: &Json, n: usize) -> Vec<&str> {
    step(v, n)["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect()
}

/// Every runner record of step `n`'s presented beat.
fn presented(v: &Json, n: usize) -> &Vec<Json> {
    step(v, n)["presented"]["commands"]
        .as_array()
        .unwrap_or_else(|| panic!("step {n} presented nothing: {}", step(v, n)))
}

#[test]
fn priority_outranks_index_order() {
    // `hub.idle` (scenes/hub/idle.lute) precedes `hub.welcome`
    // (scenes/hub/welcome.lute) in index order, but welcome's priority 10
    // beats idle's 0 once both are eligible.
    let v = play_json(
        "priority",
        "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n",
        0,
    );
    assert_eq!(winner(&v, 1), Some("hub.firstEver"));
    assert_eq!(candidate(&v, 2, "hub.idle")["eligible"], true);
    assert_eq!(candidate(&v, 2, "hub.welcome")["eligible"], true);
    assert_eq!(winner(&v, 2), Some("hub.welcome"));
}

#[test]
fn a_priority_tie_falls_back_to_index_order() {
    let v = play_json(
        "tie",
        "steps:\n  - occasion: talk\n    target: npc.meg\n",
        0,
    );
    assert_eq!(candidate(&v, 1, "meg.a")["eligible"], true);
    assert_eq!(candidate(&v, 1, "meg.b")["eligible"], true);
    assert_eq!(winner(&v, 1), Some("meg.a"));
}

#[test]
fn when_over_run_state_decides_eligibility() {
    let unseeded = play_json("when-off", "steps:\n  - occasion: hubVisit\n", 0);
    assert_eq!(candidate(&unseeded, 1, "hub.restless")["reason"], "when: false");
    assert_eq!(winner(&unseeded, 1), Some("hub.firstEver"));

    // Seeded past the gate. The seed also satisfies firstEscape's objective,
    // so the quest completes in the start settle and the trophy (50) is up
    // first; once it is spent, restless (30) outranks firstEver (20).
    let seeded = play_json(
        "when-on",
        "state:\n  run.hubVisits: 3\nsteps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n",
        0,
    );
    assert_eq!(candidate(&seeded, 1, "hub.restless")["eligible"], true);
    assert_eq!(winner(&seeded, 1), Some("hub.trophy"));
    assert_eq!(winner(&seeded, 2), Some("hub.restless"));
}

#[test]
fn once_run_is_spent_by_a_presentation_and_the_next_beat_wins() {
    let v = play_json(
        "once-run",
        "steps:\n  - occasion: talk\n    target: npc.meg\n  - occasion: talk\n    target: npc.meg\n  \
         - occasion: talk\n    target: npc.meg\n",
        0,
    );
    assert_eq!(winner(&v, 1), Some("meg.a"));
    assert_eq!(
        candidate(&v, 2, "meg.a")["reason"],
        "once: run — already presented this run"
    );
    assert_eq!(winner(&v, 2), Some("meg.b"));
    // Both spent: the occasion passes with no story.
    assert_eq!(step(&v, 3)["winner"], Json::Null);
    assert!(step(&v, 3).get("presented").is_none(), "{}", step(&v, 3));
}

#[test]
fn once_false_repeats() {
    let v = play_json(
        "once-false",
        "steps:\n  - occasion: talk\n    target: npc.achilles\n  \
         - occasion: talk\n    target: npc.achilles\n",
        0,
    );
    assert_eq!(winner(&v, 1), Some("achilles.greeting"));
    assert_eq!(winner(&v, 2), Some("achilles.greeting"));
}

#[test]
fn select_all_presents_the_pick() {
    let v = play_json(
        "pick",
        "steps:\n  - occasion: inbox\n    pick: megNote\n",
        0,
    );
    assert_eq!(step(&v, 1)["select"], "all");
    assert_eq!(winner(&v, 1), Some("megNote"));
    assert_eq!(step(&v, 1)["presented"]["kind"], "entry");
}

#[test]
fn select_all_without_a_pick_is_a_usage_error() {
    let out = play_in(
        &fixture(),
        "no-pick",
        "steps:\n  - occasion: inbox\n",
        false,
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("`select: all`"), "{}", stderr(&out));
    assert!(stdout(&out).is_empty(), "nothing plays: {}", stdout(&out));
}

#[test]
fn an_ineligible_pick_is_an_error() {
    // `dusaNote` needs `befriended(achilles)`, which nothing asserted yet.
    let v = play_json(
        "bad-pick",
        "steps:\n  - occasion: inbox\n    pick: dusaNote\n",
        1,
    );
    assert_eq!(v["exit"], "error");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("dusaNote") && msg.contains("when: false"), "{msg}");
    assert!(step(&v, 1).get("presented").is_none(), "{}", step(&v, 1));
}

#[test]
fn a_target_restricts_the_candidates() {
    let v = play_json(
        "target",
        "steps:\n  - occasion: talk\n    target: npc.achilles\n  - occasion: talk\n    target: npc.meg\n  \
         - occasion: talk\n",
        0,
    );
    assert_eq!(
        candidate_ids(&v, 1),
        ["achilles.proud", "achilles.greeting"],
        "only beats targeting npc.achilles"
    );
    assert_eq!(candidate_ids(&v, 2), ["meg.a", "meg.b"]);
    // Raised for no target: every `talk` beat restricts itself to one.
    assert!(candidate_ids(&v, 3).is_empty(), "{}", step(&v, 3));
}

#[test]
fn quest_gated_beats_become_eligible_once_the_quest_completes_during_play() {
    let v = play_json(
        "quest-gate",
        "steps:\n  - occasion: talk\n    target: npc.achilles\n  - occasion: hubVisit\n  \
         - occasion: hubVisit\n  - occasion: talk\n    target: npc.achilles\n  \
         - occasion: hubVisit\nchoose:\n  gift: accept\n",
        0,
    );
    // No `start`: the quest is active before the first step.
    let start = &v["start"]["quests"][0]["commands"];
    assert_eq!(start[0]["quest"], "firstEscape");
    assert_eq!(start[0]["state"], "active");

    // `when: quest.firstEscape.state == 'complete'` and `after:
    // completed("firstEscape")` both read the real, still-active quest.
    assert_eq!(candidate(&v, 1, "achilles.proud")["reason"], "when: false");
    assert_eq!(winner(&v, 1), Some("achilles.greeting"));
    assert_eq!(
        candidate(&v, 2, "hub.trophy")["reason"],
        "after: prerequisite not satisfied"
    );

    // The second hub visit completes the quest: objective, transition,
    // reward grant, and `questComplete` handler, in that order.
    let quest: Vec<&str> = step(&v, 3)["quests"][0]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(quest, ["objective", "quest", "grant", "line"], "{}", step(&v, 3));

    assert_eq!(winner(&v, 4), Some("achilles.proud"));
    assert_eq!(winner(&v, 5), Some("hub.trophy"));
}

#[test]
fn new_run_resets_run_state_and_run_once_but_not_user_once() {
    let v = play_json(
        "new-run",
        "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n  - newRun: true\n  \
         - occasion: hubVisit\n  - occasion: hubVisit\n",
        0,
    );
    assert_eq!(winner(&v, 1), Some("hub.firstEver"));
    assert_eq!(winner(&v, 2), Some("hub.welcome"));
    assert_eq!(step(&v, 3)["newRun"], true);
    // The quest completed in run 1 stays complete (`quest.*` persists), so
    // the trophy is up; `once: run` spending was reset for it and welcome.
    assert_eq!(winner(&v, 4), Some("hub.trophy"));
    assert_eq!(winner(&v, 5), Some("hub.welcome"));
    // `once: user` persists across the run boundary …
    assert_eq!(
        candidate(&v, 5, "hub.firstEver")["reason"],
        "once: user — already presented"
    );
    // … `user.metHypnos` (welcome's `when`) persists, and `run.hubVisits`
    // restarted from its declared default: welcome counts 0 -> 1 again.
    assert_eq!(
        step(&v, 5)["presented"]["stateDelta"]["run.hubVisits"],
        1,
        "{}",
        step(&v, 5)
    );
}

#[test]
fn new_run_resets_run_tier_facts_and_entry_reads_but_keeps_user_tier_facts() {
    let v = play_json(
        "new-run-facts",
        "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n  \
         - occasion: talk\n    target: npc.achilles\n  - occasion: inbox\n    pick: megNote\n  \
         - newRun: true\n  - occasion: inbox\n    pick: megNote\n  - occasion: inbox\n    pick: dusaNote\n\
         choose:\n  gift: accept\n",
        0,
    );
    // `entry.megNote.read` is run-tier (dsl 0.19.0 §5): a first read again.
    assert_eq!(presented(&v, 6)[0]["kind"], "entry");
    assert_eq!(presented(&v, 6)[0]["firstRead"], true);
    // `befriended(achilles)` is a `tier: user` fact: it survives the run.
    assert_eq!(candidate(&v, 7, "dusaNote")["eligible"], true);
    assert_eq!(winner(&v, 7), Some("dusaNote"));
}

#[test]
fn an_entry_beat_applies_its_effects_on_the_first_read_only() {
    let v = play_json(
        "entry",
        "steps:\n  - occasion: inbox\n    pick: megNote\n  - occasion: inbox\n    pick: megNote\n",
        0,
    );
    let first = presented(&v, 1);
    assert_eq!(first[0]["kind"], "entry");
    assert_eq!(first[0]["firstRead"], true);
    assert!(
        first
            .iter()
            .any(|r| r["kind"] == "assert" && r["fact"] == "heardOf(meg)"),
        "{first:?}"
    );
    let second = presented(&v, 2);
    assert_eq!(second[0]["firstRead"], false);
    assert!(
        second
            .iter()
            .any(|r| r["kind"] == "skipped" && r["fact"] == "heardOf(meg)"),
        "{second:?}"
    );
    assert!(
        !second.iter().any(|r| r["kind"] == "assert"),
        "a re-read applies no effect: {second:?}"
    );
}

#[test]
fn an_unknown_when_halts_incomplete_naming_the_beat() {
    let v = play_json(
        "unknown-when",
        "steps:\n  - occasion: talk\n    target: npc.oracle\n",
        3,
    );
    assert_eq!(v["exit"], "incomplete");
    assert_eq!(candidate(&v, 1, "oracle.vision")["eligible"], Json::Null);
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("oracle.vision") && msg.contains("now()/validAt"),
        "{msg}"
    );
    assert_eq!(winner(&v, 1), None, "an undecided beat is never presented");
}

#[test]
fn an_unscripted_branch_halts_incomplete() {
    let v = play_json(
        "unscripted",
        "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n  \
         - occasion: talk\n    target: npc.achilles\n",
        3,
    );
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("achilles.proud") && msg.contains("`gift`") && msg.contains("accept, decline"),
        "{msg}"
    );
}

#[test]
fn a_malformed_script_is_a_usage_error_before_anything_plays() {
    for (script, needle) in [
        ("steps: []\n", "`steps:` is empty"),
        ("state:\n  run.hubVisits: 1\n", "`steps:` is required"),
        (
            "steps:\n  - occasion: hubVisit\nschedule: x\n",
            "unknown top-level key `schedule`",
        ),
        ("steps:\n  - {}\n", "step 1 is empty"),
        (
            "steps:\n  - occasion: hubVisit\n    newRun: true\n",
            "not both",
        ),
        (
            "steps:\n  - occasion: dayStart\n",
            "occasion `dayStart` is declared by no resolved plugin",
        ),
        (
            "steps:\n  - occasion: hubVisit\n    target: npc.meg\n",
            "not declared `target: true`",
        ),
        (
            "steps:\n  - occasion: hubVisit\n    pick: hub.idle\n",
            "applies only to a `select: all` occasion",
        ),
        (
            "steps:\n  - occasion: inbox\n    pick: hub.idle\n",
            "names no beat answering `inbox`",
        ),
        (
            "state:\n  run.hubVisitz: 1\nsteps:\n  - occasion: hubVisit\n",
            "`state.run.hubVisitz` is not a declared state path",
        ),
    ] {
        let out = play_in(&fixture(), "usage", script, false);
        assert_eq!(out.status.code(), Some(2), "{script}\n{}", stderr(&out));
        assert!(stderr(&out).contains(needle), "{script}\n{}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{script}\n{}", stdout(&out));
    }
}

#[test]
fn without_declared_occasions_the_beats_on_values_are_the_vocabulary() {
    // No plugin declares occasions: `on:` is shape-only, every occasion is
    // `select: first`, and an occasion no beat answers is a usage error.
    let dir = temp_dir("shape-only");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "scenes/arrive.lute",
        "---\nkind: scene\nid: town.arrive\non: arrive\n---\n\n## Gate\n\n@guard: Welcome to town.\n",
    );
    let out = play_in(&dir, "shape-only", "steps:\n  - occasion: arrive\n", false);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    assert!(stdout(&out).contains("Welcome to town."), "{}", stdout(&out));

    let out = play_in(&dir, "shape-only-typo", "steps:\n  - occasion: arrival\n", false);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("occasion `arrival` is answered by no beat"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_human_transcript_names_each_step_its_verdicts_and_the_winner() {
    let script = fixture().join("plays/tour.play.yaml");
    let out = Command::new(BIN)
        .args([
            "play",
            fixture().to_str().unwrap(),
            "--script",
            script.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    for line in [
        "── start ──────────────",
        "  quest firstEscape -> active",
        "── step 1 · hubVisit ──────────────",
        "  ✓ hub.firstEver [scene, priority 20]",
        "  ✗ hub.trophy [scene, priority 50] — after: prerequisite not satisfied",
        "  → hub.firstEver",
        "── step 4 · talk → npc.achilles ──────────────",
        "▷ choice gift: [accept] decline        ← chosen: accept",
        "  grant firstEscape darkness 10",
        "── step 5 · inbox (select: all, pick: megNote) ──────────────",
        "  entry megNote (first read)",
        "── step 9 · new run ──────────────",
        "── end: complete (11 steps) ──────────────",
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "missing `{line}` in:\n{text}"
        );
    }
    // Eligible candidates list before ineligible ones, each in selection
    // order.
    let s3 = text
        .split("── step 3 · hubVisit")
        .nth(1)
        .and_then(|rest| rest.split("  →").next())
        .unwrap();
    let ids: Vec<&str> = s3
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix(['✓', '✗']))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["hub.welcome", "hub.idle", "hub.trophy", "hub.restless", "hub.firstEver"],
        "{text}"
    );
}
