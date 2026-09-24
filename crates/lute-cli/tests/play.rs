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
        "steps:\n  - occasion: talk\n    target: npc.achilles\n  - occasion: talk\n    target: npc.meg\n",
        0,
    );
    assert_eq!(
        candidate_ids(&v, 1),
        ["achilles.proud", "achilles.greeting"],
        "only beats targeting npc.achilles"
    );
    assert_eq!(candidate_ids(&v, 2), ["meg.a", "meg.b"]);
    // (Raising `talk` for no target at all is a usage error — see
    // `a_malformed_script_is_a_usage_error_before_anything_plays`.)
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
    // `start="true"`: the quest is active before the first step.
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
            // `talk` is declared `target: true`: raising it for nothing used
            // to play `(no candidates)` at exit 0, hiding a forgotten target.
            "steps:\n  - occasion: talk\n",
            "occasion `talk` is declared `target: true` — name what it is raised for",
        ),
        (
            "state:\n  quest.nope.state: active\nsteps:\n  - occasion: hubVisit\n",
            "`state.quest.nope.state`: no quest `nope` is declared in this project (quests: firstEscape)",
        ),
        (
            "state:\n  quest.firstEscape.state: done\nsteps:\n  - occasion: hubVisit\n",
            "a quest state is one of unset, active, complete, failed",
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

// ── 0.21.0 §7a: quests meet scenes and occasions ───────────────────────

/// A shape-only project (no plugin declares occasions): scene `haven.shed`
/// (`on: hubVisit`) whose `take` choice accepts the start-less `sideJob`;
/// `seen` completes on `visited('haven.shed')` alone; `holdLine` also needs
/// `calm`, judged only at `runEnd` (true whenever judged: `run.pressure`
/// defaults to 0). No beat answers `runEnd`.
fn quest_occasion_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.pressure: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "scenes/shed.lute",
        "---\nkind: scene\nid: haven.shed\nuses: ../world.schema.yaml\non: hubVisit\n---\n\n\
         ## Shed\n\n@guard: The shed is quiet.\n\n<branch id=\"offer\">\n\
         <choice id=\"take\" label=\"Take the job\">\n@guard: Deal.\n\
         ::accept{quest=\"sideJob\"}\n</choice>\n\
         <choice id=\"pass\" label=\"Pass\">\n@guard: Suit yourself.\n</choice>\n</branch>\n",
    );
    write(
        &dir,
        "quests/hold.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Hold\n---\n\n\
         <quest id=\"holdLine\" title=\"Hold the line\" start=\"true\">\n\
         <objective id=\"sawShed\" title=\"See the shed\" done=\"visited('haven.shed')\"/>\n\
         <objective id=\"calm\" title=\"Keep calm\" on=\"runEnd\" done=\"run.pressure < 2\"/>\n\
         </quest>\n\n\
         <quest id=\"seen\" title=\"Seen\" start=\"true\">\n\
         <objective id=\"looked\" title=\"Look in\" done=\"visited('haven.shed')\"/>\n\
         </quest>\n\n\
         <quest id=\"sideJob\" title=\"Side job\">\n\
         <objective id=\"paid\" title=\"Get paid\" done=\"run.pressure > 5\"/>\n\
         </quest>\n",
    );
    dir
}

/// Play `script` over `project` with `--json`; asserts exit 0.
fn play_project_json(project: &Path, tag: &str, script: &str) -> Json {
    let out = play_in(project, tag, script, true);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Every quest record at `records` (a `start` or step `quests` array),
/// flattened as `"<quest> -> <state>"` / `"<quest>.<objective> done"`.
fn quest_records(records: &Json) -> Vec<String> {
    records
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|doc| doc["commands"].as_array().unwrap())
        .map(|r| match r["kind"].as_str().unwrap() {
            "quest" => format!("{} -> {}", r["quest"].as_str().unwrap(), r["state"].as_str().unwrap()),
            "objective" => format!(
                "{}.{} done",
                r["quest"].as_str().unwrap(),
                r["objective"].as_str().unwrap()
            ),
            other => other.to_string(),
        })
        .collect()
}

#[test]
fn a_visited_objective_is_done_once_its_scene_is_presented() {
    let dir = quest_occasion_project("visited");
    let v = play_project_json(
        &dir,
        "visited",
        "steps:\n  - occasion: hubVisit\nchoose:\n  offer: pass\n",
    );
    // Before the shed is presented, `visited('haven.shed')` is false.
    assert_eq!(quest_records(&v["start"]["quests"]), ["holdLine -> active", "seen -> active"]);
    assert_eq!(winner(&v, 1), Some("haven.shed"));
    let s1 = quest_records(&step(&v, 1)["quests"]);
    assert!(s1.contains(&"seen.looked done".to_string()), "{s1:?}");
    assert!(s1.contains(&"seen -> complete".to_string()), "{s1:?}");
    assert!(s1.contains(&"holdLine.sawShed done".to_string()), "{s1:?}");
}

#[test]
fn an_on_objective_is_judged_only_at_a_step_raising_its_occasion() {
    let dir = quest_occasion_project("occasion");

    // `calm`'s `done` holds throughout, yet two hub visits never judge it.
    let v = play_project_json(
        &dir,
        "occasion-never",
        "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\nchoose:\n  offer: pass\n",
    );
    for n in 1..=2 {
        let records = quest_records(&step(&v, n)["quests"]);
        assert!(
            !records.iter().any(|r| r.starts_with("holdLine.calm") || r == "holdLine -> complete"),
            "step {n}: {records:?}"
        );
    }

    // `runEnd`: no beat answers it (shape-only vocabulary), yet it is a legal
    // step because an objective is judged at it.
    let v = play_project_json(
        &dir,
        "occasion-raised",
        "steps:\n  - occasion: hubVisit\n  - occasion: runEnd\nchoose:\n  offer: pass\n",
    );
    assert_eq!(v["exit"], "complete");
    assert!(!quest_records(&step(&v, 1)["quests"]).contains(&"holdLine -> complete".to_string()));
    assert_eq!(step(&v, 2)["occasion"], "runEnd");
    assert!(candidate_ids(&v, 2).is_empty(), "{}", step(&v, 2));
    assert_eq!(winner(&v, 2), None);
    assert_eq!(
        quest_records(&step(&v, 2)["quests"]),
        ["holdLine.calm done", "holdLine -> complete"]
    );

    let out = play_in(
        &dir,
        "occasion-human",
        "steps:\n  - occasion: hubVisit\n  - occasion: runEnd\nchoose:\n  offer: pass\n",
        false,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    let s2 = text
        .split("── step 2 · runEnd ──────────────\n")
        .nth(1)
        .unwrap_or_else(|| panic!("{text}"));
    let s2: Vec<&str> = s2.lines().take(4).collect();
    assert_eq!(
        s2,
        [
            "  (no candidates)",
            "  → (no eligible beat — the occasion passes)",
            "  holdLine.calm done",
            "  quest holdLine -> complete",
        ],
        "{text}"
    );

    // An occasion neither a beat nor an objective names stays a usage error.
    let out = play_in(
        &dir,
        "occasion-typo",
        "steps:\n  - occasion: runEnnd\n",
        false,
    );
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(
        stderr(&out).contains("occasion `runEnnd` is answered by no beat and judges no objective"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_scene_accept_activates_an_accept_driven_quest_after_the_presentation() {
    let dir = quest_occasion_project("accept");
    let script = "steps:\n  - occasion: hubVisit\nchoose:\n  offer: take\n";
    let v = play_project_json(&dir, "accept", script);

    let start = quest_records(&v["start"]["quests"]);
    assert!(!start.iter().any(|r| r.starts_with("sideJob")), "{start:?}");
    let accept = presented(&v, 1)
        .iter()
        .find(|r| r["kind"] == "accept")
        .unwrap_or_else(|| panic!("{}", step(&v, 1)));
    assert_eq!(accept["quest"], "sideJob");
    assert!(
        quest_records(&step(&v, 1)["quests"]).contains(&"sideJob -> active".to_string()),
        "{}",
        step(&v, 1)
    );

    let out = play_in(&dir, "accept-human", script, false);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "  quest sideJob accepted")
        .unwrap_or_else(|| panic!("{text}"));
    assert_eq!(lines[at + 1], "  quest sideJob -> active", "{text}");
    assert!(
        lines[..at].iter().any(|l| *l == "@guard: Deal."),
        "the accept lands after the presentation: {text}"
    );
}

#[test]
fn a_branch_without_the_accept_leaves_the_quest_unset() {
    let dir = quest_occasion_project("no-accept");
    let script = "steps:\n  - occasion: hubVisit\n  - occasion: runEnd\nchoose:\n  offer: pass\n";
    let v = play_project_json(&dir, "no-accept", script);
    assert!(!presented(&v, 1).iter().any(|r| r["kind"] == "accept"));
    for records in [
        quest_records(&v["start"]["quests"]),
        quest_records(&step(&v, 1)["quests"]),
        quest_records(&step(&v, 2)["quests"]),
    ] {
        assert!(!records.iter().any(|r| r.starts_with("sideJob")), "{records:?}");
    }
    let out = play_in(&dir, "no-accept-human", script, false);
    assert!(!stdout(&out).contains("sideJob"), "{}", stdout(&out));
}

// ── 0.21.1: no silent wrong answers ────────────────────────────────────

#[test]
fn a_quest_state_seed_registers_the_quest_instead_of_being_overwritten() {
    // A save's quest progress: `firstEscape` already complete. The seed used
    // to land in state only, so the start settle re-registered the quest as
    // `unset`, activated it, and every quest-gated beat read the wrong status.
    let v = play_json(
        "quest-seed",
        "state:\n  quest.firstEscape.state: complete\nsteps:\n  \
         - occasion: talk\n    target: npc.achilles\n  - occasion: hubVisit\nchoose:\n  gift: decline\n",
        0,
    );
    assert!(
        quest_records(&v["start"]["quests"]).is_empty(),
        "a seeded quest does not start over: {}",
        v["start"]
    );
    assert_eq!(winner(&v, 1), Some("achilles.proud"));
    assert_eq!(winner(&v, 2), Some("hub.trophy"), "after: completed(…) reads the seed");
}

/// A shape-only project exercising the transcript and scripted decisions:
/// `parlor` (`on: visit`) with a monologue, an `as=` line, two guarded lines
/// and a hub (`piano` gated off, `table` once, `leave` exit); `offer` (`on:
/// talk`, repeatable) with branch `ask` (`secret` gated off); `finale` (`on:
/// close`) sets `run.flag` then `::end`s. Quest `acc` completes on its
/// `on="close"` objective, `plain` on its ordinary one — both need
/// `run.flag`, which only the ending scene sets.
fn stage_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.flag: { type: bool, default: false }\n  run.lamp: { type: bool, default: false }\n",
    );
    write(
        &dir,
        "scenes/parlor.lute",
        "---\nkind: scene\nid: parlor\nuses: ../world.schema.yaml\non: visit\nonce: false\n\
         enums:\n  emotion: [calm, cross]\n  anchor: { members: [left, right], default: left }\n---\n\n\
         ## Parlor\n\n::bg{location=\"parlor\"}\n@wren{mono}: Quiet in here.\n\
         ::auto{character=\"maud\" anchor=\"left\"}\n\
         @maud{as=\"The Smith\" emotion=\"cross\"}: You again.\n\
         @maud{when=\"run.flag\"}: The lamp is lit.\n@maud{when=\"!run.flag\"}: Dark in here.\n\n\
         <hub id=\"look\">\n\
         <choice id=\"piano\" label=\"Piano\" when=\"run.lamp\" once>\n@narrator: Keys.\n</choice>\n\
         <choice id=\"table\" label=\"Table\" once>\n@narrator: A cup.\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n@narrator: Out.\n</choice>\n</hub>\n",
    );
    write(
        &dir,
        "scenes/offer.lute",
        "---\nkind: scene\nid: offer\nuses: ../world.schema.yaml\non: talk\nonce: false\n---\n\n\
         ## Offer\n\n@oskar: Well?\n\n<branch id=\"ask\">\n\
         <choice id=\"notYet\" label=\"Not yet\">\n@oskar: Later, then.\n</choice>\n\
         <choice id=\"accept\" label=\"Yes\">\n@oskar: Good.\n</choice>\n\
         <choice id=\"secret\" label=\"The secret\" when=\"run.lamp\">\n@oskar: Hush.\n</choice>\n\
         </branch>\n",
    );
    write(
        &dir,
        "scenes/finale.lute",
        "---\nkind: scene\nid: finale\nuses: ../world.schema.yaml\non: close\n---\n\n\
         ## Finale\n\n@narrator: The curtain falls.\n::set{run.flag = true}\n::end{reason=\"curtain\"}\n",
    );
    write(
        &dir,
        "quests/acc.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Acc\n---\n\n\
         <quest id=\"acc\" title=\"Accuse\" start=\"true\">\n\
         <objective id=\"named\" title=\"Name them\" on=\"close\" done=\"run.flag\"/>\n</quest>\n\n\
         <quest id=\"plain\" title=\"Plain\" start=\"true\">\n\
         <objective id=\"lit\" title=\"Light it\" done=\"run.flag\"/>\n</quest>\n",
    );
    dir
}

#[test]
fn an_end_stops_the_walk_only_after_the_step_settles_its_quests_and_occasion() {
    let dir = stage_project("end-order");
    let v = play_project_json(&dir, "end-order", "steps:\n  - occasion: close\n  - occasion: visit\n");
    // The ending scene's own progress lands: the ordinary advance AND the
    // `on="close"` objective the occasion judges. Before, `::end` skipped
    // both and still exited 0.
    assert_eq!(
        quest_records(&step(&v, 1)["quests"]),
        ["plain.lit done", "plain -> complete", "acc.named done", "acc -> complete"]
    );
    assert_eq!(v["exit"], "complete");
    assert!(
        v["endReason"].as_str().unwrap().contains("::end in scene `finale`"),
        "{}",
        v["endReason"]
    );
    assert!(step(&v, 2).is_null(), "nothing plays after `::end`: {v}");
}

#[test]
fn a_branch_choose_list_is_consumed_one_decision_per_presentation() {
    let dir = stage_project("branch-list");
    let script = "steps:\n  - occasion: talk\n  - occasion: talk\nchoose:\n  ask: [notYet, accept]\n";
    let v = play_project_json(&dir, "branch-list", script);
    let chose = |n| {
        presented(&v, n)
            .iter()
            .find(|r| r["kind"] == "choice")
            .map(|r| r["chose"].clone())
    };
    assert_eq!(chose(1), Some(Json::from("notYet")));
    assert_eq!(chose(2), Some(Json::from("accept")), "never truncated to its head");

    // A third presentation finds the list used up: incomplete, and says so.
    let out = play_in(
        &dir,
        "branch-list-out",
        "steps:\n  - occasion: talk\n  - occasion: talk\n  - occasion: talk\nchoose:\n  ask: [notYet, accept]\n",
        true,
    );
    assert_eq!(out.status.code(), Some(3), "{}", stdout(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("choice `ask`") && msg.contains("all 2 decisions of its `choose:` list"),
        "{msg}"
    );

    // A single decision still answers every presentation.
    let v = play_project_json(
        &dir,
        "branch-single",
        "steps:\n  - occasion: talk\n  - occasion: talk\nchoose:\n  ask: accept\n",
    );
    assert_eq!(v["exit"], "complete");
}

#[test]
fn forcing_a_spent_once_hub_option_halts_instead_of_being_skipped() {
    let dir = stage_project("spent-once");
    let out = play_in(
        &dir,
        "spent-once",
        "steps:\n  - occasion: visit\nchoose:\n  look: [table, table, leave]\n",
        true,
    );
    // Before: the second `table` was silently dropped, `leave` played, exit 0.
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["exit"], "error");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("E-TRACE-CHOICE") && msg.contains("`choose: look: table`") && msg.contains("once"),
        "{msg}"
    );
    assert!(
        !presented(&v, 1).iter().any(|r| r["chose"] == "leave"),
        "nothing after the refusal plays: {}",
        step(&v, 1)
    );
}

#[test]
fn an_ineligible_choose_is_an_error_like_an_ineligible_pick() {
    let dir = stage_project("bad-choose");
    let out = play_in(&dir, "bad-choose", "steps:\n  - occasion: talk\nchoose:\n  ask: secret\n", true);
    // Exit 1 like an ineligible `pick:` — it used to be 2, the usage-error code.
    assert_eq!(out.status.code(), Some(1), "{}{}", stdout(&out), stderr(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["exit"], "error");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("E-TRACE-CHOICE") && msg.contains("secret"), "{msg}");
}

#[test]
fn the_transcript_shows_the_source_not_the_lowered_ir() {
    let dir = stage_project("source-level");
    let script = "steps:\n  - occasion: visit\nchoose:\n  look: [table, leave]\n";
    let out = play_in(&dir, "source-level", script, false);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    for line in [
        "@wren{mono}: Quiet in here.",
        "@maud{as=\"The Smith\" emotion=\"cross\"}: You again.",
        "  skip @maud \"The lamp is lit.\" — when: false",
        "@maud: Dark in here.",
        // The menu marks what was not really offered.
        "▷ hub look: piano✗ [table] leave        ← chosen: table",
        "▷ hub look: piano✗ table(spent) [leave]        ← chosen: leave",
    ] {
        assert!(text.lines().any(|l| l == line), "missing `{line}` in:\n{text}");
    }
    // No line-guard plumbing, no compiler-injected staging (the preload and
    // the pose resets before maud's plain lines).
    assert!(!text.contains("match ->"), "{text}");
    assert!(!text.contains("preload") && !text.contains("posReset"), "{text}");

    // `--json` line records keep the line's identity and delivery.
    let v = play_project_json(&dir, "source-level-json", script);
    let lines: Vec<&Json> = presented(&v, 1).iter().filter(|r| r["kind"] == "line").collect();
    assert_eq!(lines[0]["role"], "monologue");
    assert_eq!(lines[0]["lineId"], "parlor.wren_0010");
    let smith = lines[1];
    assert_eq!(smith["role"], "dialogue");
    assert_eq!(smith["as"], "The Smith");
    assert_eq!(smith["emotion"], "cross");
    assert_eq!(smith["lineId"], "parlor.maud_0010");
    assert_eq!(smith["voiceKey"], "maud-0010");
}
