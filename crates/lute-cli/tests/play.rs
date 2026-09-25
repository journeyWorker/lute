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
fn an_end_ends_only_its_presentation_and_the_play_goes_on() {
    let dir = stage_project("end-order");
    let v = play_project_json(
        &dir,
        "end-order",
        "steps:\n  - occasion: close\n  - occasion: talk\nchoose:\n  ask: notYet\n",
    );
    // The ending scene's own progress lands: the ordinary advance AND the
    // `on="close"` objective the occasion judges.
    assert_eq!(
        quest_records(&step(&v, 1)["quests"]),
        ["plain.lit done", "plain -> complete", "acc.named done", "acc -> complete"]
    );
    // 0.23.1: `::end` ended the finale, not the playthrough.
    assert_eq!(winner(&v, 2), Some("offer"), "the next step plays after `::end`: {v}");
    assert_eq!(v["exit"], "complete");
    assert_eq!(v["endReason"], "complete (2 steps)");
    assert!(v.get("skipped").is_none(), "{v}");
}

#[test]
fn an_end_step_ends_the_playthrough_and_lists_the_steps_it_skips() {
    let dir = stage_project("end-step");
    let script = "steps:\n  - occasion: talk\n  - end: true\n  - label: never\n    occasion: close\n\
                  choose:\n  ask: notYet\n";
    let v = play_project_json(&dir, "end-step", script);
    assert_eq!(v["exit"], "complete");
    assert_eq!(v["endReason"], "`end: true` at step 2 (1 later step skipped)");
    assert_eq!(v["skipped"], serde_json::json!([{ "step": 3, "label": "never" }]));
    assert_eq!(step(&v, 2)["end"], true);
    assert!(step(&v, 3).is_null(), "{v}");
    let out = play_in(&dir, "end-step-text", script, false);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("── step 3 (never) · skipped (the playthrough ended)"), "{text}");

    for (bad, why) in [
        ("steps:\n  - end: false\n", "`end` must be `true`"),
        ("steps:\n  - end: true\n    repeat: 2\n", "`repeat` does not apply to an `end` step"),
        ("steps:\n  - end: true\n    occasion: talk\n", "not both"),
    ] {
        let out = play_in(&dir, "end-bad", bad, false);
        assert_eq!(out.status.code(), Some(2), "{bad}");
        assert!(stderr(&out).contains(why), "{bad}: {}", stderr(&out));
    }
}

#[test]
fn staging_prints_as_authored_and_ir_prints_the_lowered_records() {
    let dir = stage_project("staging-source");
    let script = "steps:\n  - occasion: visit\nchoose:\n  look: [table, leave]\n";
    let text = stdout(&play_in(&dir, "staging-source", script, false));
    assert!(text.lines().any(|l| l == "::bg{location=\"parlor\"}"), "{text}");
    assert!(text.lines().any(|l| l == "::auto{character=\"maud\" anchor=\"left\"}"), "{text}");
    assert!(!text.contains("::background") && !text.contains("::sprite"), "{text}");

    let s = write(&temp_dir("staging-ir"), "s.play.yaml", script);
    let out = Command::new(BIN)
        .args(["play", dir.to_str().unwrap(), "--script", s.to_str().unwrap(), "--ir"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    let ir = stdout(&out);
    assert!(ir.lines().any(|l| l == "::background{location=\"parlor\" wait=true}"), "{ir}");
    assert!(ir.contains("(injected: "), "the lowered view shows injected staging: {ir}");
}

/// 0.23.1: an occasion and a world event of the same name — the engine
/// raises both at once.
fn boss_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.boss: true }\n",
    );
    write(
        &dir,
        "plugins/g.boss/plugin.yaml",
        "id: g.boss\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  events: events/\n",
    );
    write(
        &dir,
        "plugins/g.boss/occasions/o.yaml",
        "occasions:\n  bossDefeated: { target: { prefix: boss, entity: foe } }\n",
    );
    write(&dir, "plugins/g.boss/events/e.yaml", "events:\n  - name: bossDefeated\n");
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.hits: { type: number, default: 0 }\nentities:\n  foe: { members: [warden, hound] }\n",
    );
    write(
        &dir,
        "scenes/fall.lute",
        "---\nkind: scene\nid: warden.fall\nuses: ../world.schema.yaml\non: bossDefeated\n\
         target: boss.warden\nonce: false\n---\n\n## Fall\n\n@narrator: Down it goes.\n",
    );
    write(
        &dir,
        "quests/forge.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Forge\n---\n\n\
         <quest id=\"forge\" title=\"Forge\" start=\"true\">\n\
         <objective id=\"maul\" title=\"Maul\" on=\"bossDefeated\" target=\"boss.warden\" done=\"run.hits >= 1\"/>\n\
         <on event=\"bossDefeated\">\n@narrator: The stair shakes.\n::set{run.hits += 1}\n</on>\n</quest>\n",
    );
    dir
}

#[test]
fn raising_an_occasion_fires_the_same_named_world_event_before_judging() {
    let dir = boss_project("boss-play");
    let v = play_project_json(
        &dir,
        "boss-play",
        "steps:\n  - occasion: bossDefeated\n    target: boss.warden\n",
    );
    // The handler runs first (its write is what `done` reads), then the
    // occasion judges the targeted objective in the same raise.
    assert_eq!(
        quest_records(&step(&v, 1)["quests"]),
        ["line", "set", "forge.maul done", "forge -> complete"],
        "{v}"
    );

    // `lute trace` raises it the same way.
    let quest = dir.join("quests/forge.lute");
    let out = Command::new(BIN)
        .args([
            "trace",
            quest.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--occasion",
            "bossDefeated@boss.warden",
        ])
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(text.contains("The stair shakes."), "{text}{}", stderr(&out));
    assert!(text.contains("-> complete"), "{text}");
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
    // dsl 0.22.0 §11: the default voiceKey is project-unique.
    assert_eq!(smith["voiceKey"], "parlor.maud-0010");
}

// ── 0.22.0: a reference player that can stand in for the engine ────────

/// A project whose plugin declares `hubVisit`, `talk` (targets drawn from
/// entity kind `person` under `npc.`), `board` (`select: all`) and the world
/// event `storm`. `slew` is a RESERVED relation (the engine asserts kills)
/// and `feared` is derived from it. Quest `climb` is `tier="run"` and
/// completes at `run.floor >= 5`; `ever` (user tier) at `user.runs >= 3`;
/// `notes` only at `board` (`on=` objective). Both `climb` and `ever`
/// handle `storm`. Lore: `notice` (`once="user"`), `memo` (`once="run"`),
/// `old` (eligible once `notice` was ever read).
fn harness_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.occ: true }\n",
    );
    write(
        &dir,
        "plugins/g.occ/plugin.yaml",
        "id: g.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  events: events/\n",
    );
    write(
        &dir,
        "plugins/g.occ/occasions/o.yaml",
        "occasions:\n  hubVisit: {}\n  talk: { target: { prefix: npc, entity: person } }\n  \
         board: { select: all }\n",
    );
    write(&dir, "plugins/g.occ/events/e.yaml", "events:\n  - name: storm\n");
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.floor: { type: number, default: 0 }\n  \
         run.outcome: { type: { enum: [climbing, fell, escaped] }, default: climbing }\n  \
         user.runs: { type: number, default: 0 }\n  user.brave: { type: bool, default: false }\n\
         entities:\n  person: { members: [maud, oskar] }\n  foe: { members: [warden, hound] }\n\
         relations:\n  slew: { args: [foe], tier: run, reserved: true }\n  \
         heard: { args: [person], tier: user }\n  feared: { args: [foe], derive: true }\n\
         rules:\n  - 'feared(F) :- slew(F)'\n",
    );
    write(
        &dir,
        "scenes/hub.lute",
        "---\nkind: scene\nid: hub.idle\nuses: ../world.schema.yaml\non: hubVisit\nonce: false\n---\n\n\
         ## Hub\n\n@maud: Quiet night.\n",
    );
    write(
        &dir,
        "scenes/victory.lute",
        "---\nkind: scene\nid: hub.victory\nuses: ../world.schema.yaml\non: hubVisit\n\
         when: \"holds(slew(warden))\"\npriority: 10\nonce: false\n---\n\n## Victory\n\n\
         @maud: The warden is dead.\n",
    );
    write(
        &dir,
        "scenes/maud.lute",
        "---\nkind: scene\nid: maud.talk\nuses: ../world.schema.yaml\non: talk\ntarget: npc.maud\n\
         once: false\n---\n\n## Maud\n\n@maud: Hello.\n",
    );
    write(
        &dir,
        "quests/climb.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Climb\n---\n\n\
         <quest id=\"climb\" title=\"Climb\" start=\"true\" tier=\"run\">\n\
         <objective id=\"high\" title=\"Get high\" done=\"run.floor >= 5\"/>\n\
         <on event=\"storm\">\n@narrator: Thunder over the stair.\n</on>\n</quest>\n\n\
         <quest id=\"ever\" title=\"Ever\" start=\"true\">\n\
         <objective id=\"three\" title=\"Three runs\" done=\"user.runs >= 3\"/>\n\
         <on event=\"storm\">\n@narrator: The long quest hears the storm.\n</on>\n</quest>\n\n\
         <quest id=\"notes\" title=\"Notes\" start=\"true\">\n\
         <objective id=\"looked\" title=\"Look at the board\" on=\"board\" done=\"true\"/>\n\
         </quest>\n",
    );
    write(
        &dir,
        "lore/board.lute",
        "---\nkind: lore\nid: board\ntitle: Board\nuses: ../world.schema.yaml\n---\n\n\
         <entry id=\"notice\" on=\"board\" once=\"user\" category=\"note\" title=\"Notice\">\n  \
         @maud: A notice.\n  ::assert{heard(maud)}\n</entry>\n\n\
         <entry id=\"memo\" on=\"board\" once=\"run\" category=\"note\" title=\"Memo\">\n  \
         @maud: A memo.\n</entry>\n\n\
         <entry id=\"old\" on=\"board\" category=\"note\" title=\"Old\" when=\"entry.notice.everRead\">\n  \
         @maud: You have read the notice before.\n</entry>\n",
    );
    dir
}

/// `harness_project` played with `--json`, asserting the exit code.
fn harness_json(tag: &str, script: &str, exit: i32) -> Json {
    let out = play_in(&harness_project(tag), tag, script, true);
    assert_eq!(out.status.code(), Some(exit), "{}{}", stdout(&out), stderr(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// `harness_project` played for its human transcript, asserting the exit.
fn harness_text(tag: &str, script: &str, exit: i32) -> String {
    let out = play_in(&harness_project(tag), tag, script, false);
    assert_eq!(out.status.code(), Some(exit), "{}{}", stdout(&out), stderr(&out));
    stdout(&out)
}

#[test]
fn an_engine_step_writes_state_and_reserved_facts_and_the_lifecycle_settles_after_it() {
    let v = harness_json(
        "engine",
        "steps:\n  - occasion: hubVisit\n  \
         - engine: { state: { run.floor: 6, run.outcome: fell }, facts: [slew(warden)] }\n  \
         - occasion: hubVisit\n  - engine: { retract: [slew(warden)] }\n  - occasion: hubVisit\n",
        0,
    );
    assert_eq!(winner(&v, 1), Some("hub.idle"));
    // The write is recorded, presents nothing, and the objective it
    // satisfies completes in the same step — not at the next presentation.
    let engine = step(&v, 2);
    assert!(engine.get("candidates").is_none(), "{engine}");
    let writes: Vec<&str> = engine["engine"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(writes, ["set", "set", "assert"], "{engine}");
    assert_eq!(
        quest_records(&engine["quests"]),
        ["climb.high done", "climb -> complete"]
    );
    // The reserved kill fact the engine asserted gates content …
    assert_eq!(winner(&v, 3), Some("hub.victory"));
    // … until the engine retracts it.
    assert_eq!(candidate(&v, 5, "hub.victory")["reason"], "when: false");
    assert_eq!(winner(&v, 5), Some("hub.idle"));
}

#[test]
fn a_repeated_engine_delta_accumulates_and_each_repetition_settles() {
    let v = harness_json(
        "engine-add",
        "steps:\n  - engine: { state: { user.runs: { add: 1 } } }\n    repeat: 3\n    label: a run ends\n",
        0,
    );
    let steps = v["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3, "one record per repetition: {v}");
    for (k, s) in steps.iter().enumerate() {
        assert_eq!(s["step"], 1);
        assert_eq!(s["label"], "a run ends");
        assert_eq!(s["iteration"], k + 1);
        assert_eq!(s["engine"][0]["value"], k + 1, "{s}");
    }
    // `ever` completes exactly when the third run is counted.
    assert!(quest_records(&steps[1]["quests"]).is_empty());
    assert_eq!(
        quest_records(&steps[2]["quests"]),
        ["ever.three done", "ever -> complete"]
    );
}

#[test]
fn engine_writes_are_validated_before_anything_plays() {
    let dir = harness_project("engine-usage");
    for (script, needle) in [
        (
            "steps:\n  - engine: { state: { run.flor: 1 } }\n",
            "`engine.state.run.flor` is not a declared state path",
        ),
        (
            "steps:\n  - engine: { state: { run.floor: high } }\n",
            "does not take `high`",
        ),
        (
            "steps:\n  - engine: { state: { run.outcome: won } }\n",
            "climbing, fell, escaped",
        ),
        (
            "steps:\n  - engine: { state: { user.brave: { add: 1 } } }\n",
            "is not a `number` path, so it takes no `{ add: … }`",
        ),
        (
            "steps:\n  - engine: { state: { quest.climb.state: complete } }\n",
            "seed a save's quest status with top-level `quests:`",
        ),
        (
            "steps:\n  - engine: { facts: [slain(warden)] }\n",
            "names an undeclared relation `slain`",
        ),
        (
            "steps:\n  - engine: { facts: [feared(warden)] }\n",
            "`feared` is derived by rules",
        ),
        (
            "steps:\n  - engine: { facts: [slew(dragon)] }\n",
            "`dragon` is not a member of `foe` (warden, hound)",
        ),
        (
            "steps:\n  - engine: { facts: [\"slew(warden, hound)\"] }\n",
            "`slew` takes 1 argument(s)",
        ),
        (
            "steps:\n  - engine: { facts: [slew(_)] }\n",
            "is not a ground fact",
        ),
        ("steps:\n  - engine: {}\n", "`engine:` writes nothing"),
        (
            "steps:\n  - engine: { state: { run.floor: 1 }, emit: [x] }\n",
            "unknown `engine:` key `emit`",
        ),
        (
            "steps:\n  - newRun: { retract: [slew(warden)] }\n",
            "unknown `newRun:` key `retract`",
        ),
        ("steps:\n  - newRun: false\n", "`newRun` must be `true` or"),
        (
            "steps:\n  - engine: { state: { run.floor: 1 } }\n    occasion: hubVisit\n",
            "not both",
        ),
        (
            "steps:\n  - engine: { state: { run.floor: 1 } }\n    pick: memo\n",
            "`pick` applies only to an `occasion` step, not `engine`",
        ),
        (
            "steps:\n  - occasion: hubVisit\n    repeat: 0\n",
            "`repeat` must be a whole number ≥ 1",
        ),
    ] {
        let out = play_in(&dir, "engine-usage", script, false);
        assert_eq!(out.status.code(), Some(2), "{script}\n{}", stderr(&out));
        assert!(stderr(&out).contains(needle), "{script}\n{}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{script}\n{}", stdout(&out));
    }
}

#[test]
fn a_new_run_resets_run_tier_quests_then_applies_its_seed() {
    let v = harness_json(
        "new-run-seed",
        "steps:\n  - engine: { state: { run.floor: 6, user.runs: 1 } }\n  \
         - newRun: { state: { run.floor: 2 }, facts: [slew(hound)] }\n  \
         - engine: { state: { run.floor: 5 } }\n",
        0,
    );
    assert_eq!(
        quest_records(&step(&v, 1)["quests"]),
        ["climb.high done", "climb -> complete"]
    );
    // `climb` is `tier="run"`: unset and its objectives undone at the new
    // run, so its `start` activates it again; the seed lands after the
    // reset (run.floor 2, not the default 0) and before the settle.
    let new_run = step(&v, 2);
    assert_eq!(new_run["newRun"], true);
    let seed: Vec<&str> = new_run["seed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().or(r["fact"].as_str()).unwrap())
        .collect();
    assert_eq!(seed, ["run.floor", "slew(hound)"]);
    assert_eq!(quest_records(&new_run["quests"]), ["climb -> active"]);
    // The objective completes a second time — it really was reset.
    assert_eq!(
        quest_records(&step(&v, 3)["quests"]),
        ["climb.high done", "climb -> complete"]
    );
}

#[test]
fn an_event_step_runs_the_handlers_of_active_quests_only() {
    let text = harness_text(
        "event",
        "steps:\n  - event: storm\n  - engine: { state: { run.floor: 9 } }\n  - event: storm\n",
        0,
    );
    let first = text.split("── step 2").next().unwrap();
    assert!(first.contains("── step 1 · event storm"), "{text}");
    assert!(first.contains("@narrator: Thunder over the stair."), "{text}");
    assert!(first.contains("@narrator: The long quest hears the storm."), "{text}");
    // `climb` completed at step 2: its handler no longer answers.
    let third = text.split("── step 3 · event storm").nth(1).unwrap();
    assert!(!third.contains("Thunder"), "{text}");
    assert!(third.contains("The long quest hears the storm."), "{text}");
}

#[test]
fn events_and_occasions_are_distinct_vocabularies_with_a_pointer_between_them() {
    let dir = harness_project("event-usage");
    for (script, needle) in [
        (
            "steps:\n  - event: board\n",
            "`event: board` names no declared world event — `board` is an occasion; raise it with `occasion: board`",
        ),
        (
            "steps:\n  - occasion: storm\n",
            "`storm` is a world event; fire it with `event: storm`",
        ),
        ("steps:\n  - event: gale\n", "(declared: storm)"),
        ("steps:\n  - event: questFailed\n", "is a quest lifecycle event"),
    ] {
        let out = play_in(&dir, "event-usage", script, false);
        assert_eq!(out.status.code(), Some(2), "{script}\n{}", stderr(&out));
        assert!(stderr(&out).contains(needle), "{script}\n{}", stderr(&out));
    }
}

#[test]
fn pick_none_closes_the_list_without_spending_and_still_judges_on_objectives() {
    let v = harness_json(
        "pick-none",
        "steps:\n  - occasion: board\n    pick: none\n  - occasion: board\n    pick: notice\n",
        0,
    );
    assert_eq!(step(&v, 1)["pick"], "none");
    assert_eq!(winner(&v, 1), None);
    assert!(step(&v, 1).get("presented").is_none(), "{}", step(&v, 1));
    // The occasion still judged `notes.looked` (`on="board"`).
    assert_eq!(
        quest_records(&step(&v, 1)["quests"]),
        ["notes.looked done", "notes -> complete"]
    );
    // Nothing was spent: `notice` is still a first read.
    assert_eq!(presented(&v, 2)[0]["firstRead"], true);

    // `pick: none` on a `select: first` occasion is a usage error.
    let out = play_in(
        &harness_project("pick-none-first"),
        "pick-none-first",
        "steps:\n  - occasion: hubVisit\n    pick: none\n",
        false,
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("`pick: none` applies only to a `select: all` occasion"));
}

#[test]
fn entry_once_is_spent_by_its_read_flags_and_ever_read_survives_the_run() {
    let v = harness_json(
        "entry-once",
        "steps:\n  - { occasion: board, pick: notice }\n  - { occasion: board, pick: memo }\n  \
         - { occasion: board, pick: old }\n  - newRun: true\n  - { occasion: board, pick: memo }\n",
        0,
    );
    // Before any read, `old` (`when: entry.notice.everRead`) is closed.
    assert_eq!(candidate(&v, 1, "old")["reason"], "when: false");
    assert_eq!(
        candidate(&v, 2, "notice")["reason"],
        "once: user — already read"
    );
    assert_eq!(candidate(&v, 3, "old")["eligible"], true);
    assert_eq!(
        candidate(&v, 3, "memo")["reason"],
        "once: run — already read this run"
    );
    // A new run re-opens the run-tier entry, not the user-tier one, and
    // `everRead` is never reset.
    assert_eq!(candidate(&v, 5, "memo")["eligible"], true);
    assert_eq!(
        candidate(&v, 5, "notice")["reason"],
        "once: user — already read"
    );
    assert_eq!(candidate(&v, 5, "old")["eligible"], true);
}

#[test]
fn a_save_seeds_history_quests_and_reads_before_step_one() {
    // `presented.user` spends a `once: user` beat; `quests:` resumes a
    // completed quest (the trophy's `after: completed(…)` reads it).
    let v = play_json(
        "save",
        "presented: { user: [hub.firstEver] }\nquests: { firstEscape: complete }\n\
         entriesRead: { run: [megNote] }\nsteps:\n  - occasion: hubVisit\n  \
         - occasion: inbox\n    pick: megNote\n",
        0,
    );
    assert!(quest_records(&v["start"]["quests"]).is_empty(), "{}", v["start"]);
    assert_eq!(
        candidate(&v, 1, "hub.firstEver")["reason"],
        "once: user — already presented"
    );
    assert_eq!(winner(&v, 1), Some("hub.trophy"));
    assert_eq!(presented(&v, 2)[0]["firstRead"], false, "read in this run already");

    // `visited:` feeds `visited('<id>')`: the objective is done at start.
    let dir = quest_occasion_project("save-visited");
    let v = play_project_json(
        &dir,
        "save-visited",
        "visited: [haven.shed]\nsteps:\n  - occasion: runEnd\n",
    );
    let start = quest_records(&v["start"]["quests"]);
    assert!(start.contains(&"seen.looked done".to_string()), "{start:?}");

    // `entriesRead.user` sets `everRead` only.
    let v = harness_json(
        "save-ever",
        "entriesRead: { user: [notice] }\nsteps:\n  - { occasion: board, pick: memo }\n",
        0,
    );
    assert_eq!(candidate(&v, 1, "old")["eligible"], true);
    assert_eq!(candidate(&v, 1, "notice")["reason"], "once: user — already read");
}

#[test]
fn a_save_naming_an_unknown_id_is_a_usage_error() {
    for (script, needle) in [
        (
            "visited: [hub.idel]\nsteps:\n  - occasion: hubVisit\n",
            "`visited:` names `hub.idel`, which is no scene in this project — did you mean `hub.idle`?",
        ),
        (
            "presented: { run: [hub.nope] }\nsteps:\n  - occasion: hubVisit\n",
            "`presented.run` names `hub.nope`, which is no scene beat",
        ),
        (
            "presented: { user: [megNote] }\nsteps:\n  - occasion: hubVisit\n",
            "an entry's read history is `entriesRead:`",
        ),
        (
            "quests: { secondEscape: active }\nsteps:\n  - occasion: hubVisit\n",
            "`quests.secondEscape`: no quest `secondEscape` is declared",
        ),
        (
            "quests: { firstEscape: done }\nsteps:\n  - occasion: hubVisit\n",
            "a quest state is one of unset, active, complete, failed",
        ),
        (
            "entriesRead: { run: [megnote] }\nsteps:\n  - occasion: hubVisit\n",
            "`entriesRead.run` names `megnote`, which is no entry",
        ),
        (
            "entriesRead: { ever: [megNote] }\nsteps:\n  - occasion: hubVisit\n",
            "takes only `run:` and `user:`",
        ),
    ] {
        let out = play_in(&fixture(), "save-usage", script, false);
        assert_eq!(out.status.code(), Some(2), "{script}\n{}", stderr(&out));
        assert!(stderr(&out).contains(needle), "{script}\n{}", stderr(&out));
    }
}

#[test]
fn a_step_choose_replaces_the_script_choose_for_that_step_only() {
    // The script-wide list would answer `gift` with `accept` first. The
    // step's own `decline` is used at step 1 and consumes nothing of the
    // script-wide list, which still starts at `accept` in the next run.
    let v = play_json(
        "step-choose",
        "quests: { firstEscape: complete }\nsteps:\n  \
         - { occasion: talk, target: npc.achilles, choose: { gift: decline } }\n  \
         - newRun: true\n  - { occasion: talk, target: npc.achilles }\n\
         choose:\n  gift: [accept, decline]\n",
        0,
    );
    let chose = |n| {
        presented(&v, n)
            .iter()
            .find(|r| r["kind"] == "choice")
            .map(|r| r["chose"].clone())
            .unwrap()
    };
    assert_eq!(winner(&v, 1), Some("achilles.proud"));
    assert_eq!(chose(1), "decline");
    assert_eq!(winner(&v, 3), Some("achilles.proud"));
    assert_eq!(chose(3), "accept");
}

#[test]
fn a_target_outside_the_occasion_domain_is_a_usage_error() {
    let dir = harness_project("target-domain");
    let out = play_in(
        &dir,
        "target-domain",
        "steps:\n  - { occasion: talk, target: npc.mawd }\n",
        false,
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("did you mean `npc.maud`?"),
        "{}",
        stderr(&out)
    );
    // A member of the kind with no beat is legal — the occasion passes.
    let v = harness_json(
        "target-domain-ok",
        "steps:\n  - { occasion: talk, target: npc.oskar }\n",
        0,
    );
    assert_eq!(winner(&v, 1), None);
}

#[test]
fn labels_and_repetitions_are_printed_in_the_transcript() {
    let text = harness_text(
        "label",
        "steps:\n  - { occasion: talk, target: npc.maud, label: greet, repeat: 2 }\n",
        0,
    );
    for line in [
        "── step 1 (greet) [1/2] · talk → npc.maud ──────────────",
        "── step 1 (greet) [2/2] · talk → npc.maud ──────────────",
        "── end: complete (2 steps) ──────────────",
    ] {
        assert!(text.lines().any(|l| l == line), "missing `{line}` in:\n{text}");
    }
}

#[test]
fn a_missed_expectation_fails_the_play_naming_the_step_and_the_actual_value() {
    let script = "steps:\n  - occasion: hubVisit\n    label: first visit\n    \
                  expect: { winner: hub.victory }\n  \
                  - engine: { facts: [slew(warden)] }\n  - occasion: hubVisit\n    \
                  expect: { winner: hub.victory, offered: [hub.victory, hub.idle] }\n\
                  expect:\n  facts: [feared(warden)]\n  quests: { climb: active }\n";
    let text = harness_text("expect-miss", script, 1);
    assert!(text.contains("── end: complete (3 steps)"), "the walk itself completed:\n{text}");
    assert!(text.contains("── expect: 1 missed"), "{text}");
    let miss = text.lines().find(|l| l.starts_with("  ✗ step 1")).unwrap_or_else(|| panic!("{text}"));
    assert!(miss.contains("(first visit)") && miss.contains("hub.idle"), "{miss}");

    let v = harness_json("expect-miss-json", script, 1);
    let misses = v["expect"]["misses"].as_array().unwrap();
    assert_eq!(misses.len(), 1, "{v}");
    assert_eq!(misses[0]["step"], 1);
    assert_eq!(misses[0]["actual"], "hub.idle");

    // Every expectation holding leaves the exit to the walk.
    let text = harness_text(
        "expect-held",
        "steps:\n  - occasion: hubVisit\n    expect: { winner: hub.idle, notOffered: [hub.victory] }\n",
        0,
    );
    assert!(text.contains("── expect: every expectation held"), "{text}");
}

#[test]
fn derive_false_leaves_derived_facts_out_of_the_end_of_play() {
    let steps = "steps:\n  - engine: { facts: [slew(warden)] }\n";
    harness_text(
        "derive-on",
        &format!("{steps}expect:\n  facts: [feared(warden), slew(warden)]\n"),
        0,
    );
    harness_text(
        "derive-off",
        &format!("derive: false\n{steps}expect:\n  facts: [slew(warden)]\n  notFacts: [feared(warden)]\n"),
        0,
    );
}

// ── 0.23.0 §2/§3: deadlines, objective targets, composing occasions ────

/// A plugin declaring `hubVisit` (`select: first`), `evening` (`select:
/// sequence`) and `talk` (targets `npc.<person>`). `hubVisit`: `hub.main`
/// (repeatable) and the `also` beat `hub.aside` (priority 9, `once: run`).
/// `evening`: `eve.routine` (priority 5, repeatable, advances `run.day`) and
/// `eve.letter` (`once: run`, eligible from day 2). Quest `deadline` needs
/// `letter` (`by` day 3) and `sealed` (never); `errand` completes when `talk`
/// is raised for `npc.maud`.
fn compose_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.occ: true }\n",
    );
    write(
        &dir,
        "plugins/g.occ/plugin.yaml",
        "id: g.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.occ/occasions/o.yaml",
        "occasions:\n  hubVisit: {}\n  evening: { select: sequence }\n  \
         talk: { target: { prefix: npc, entity: person } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1 }\n  \
         run.answered: { type: bool, default: false }\n  \
         run.sealed: { type: bool, default: false }\n\
         entities:\n  person: { members: [maud, oskar] }\n",
    );
    let scene = |rel: &str, id: &str, fm: &str, body: &str| {
        write(
            &dir,
            rel,
            &format!("---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\n{fm}---\n\n## {id}\n\n{body}"),
        );
    };
    scene("scenes/main.lute", "hub.main", "on: hubVisit\nonce: false\n", "@maud: Welcome.\n");
    scene(
        "scenes/aside.lute",
        "hub.aside",
        "on: hubVisit\npriority: 9\nalso: true\n",
        "@oskar: Psst.\n",
    );
    scene(
        "scenes/routine.lute",
        "eve.routine",
        "on: evening\npriority: 5\nonce: false\n",
        "@maud: Supper.\n::set{ run.day = run.day + 1 }\n",
    );
    scene(
        "scenes/letter.lute",
        "eve.letter",
        "on: evening\nwhen: 'run.day >= 2'\n",
        "@oskar: A letter came.\n",
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Q\n---\n\n\
         <quest id=\"deadline\" title=\"Deadline\" start=\"true\">\n\
         <objective id=\"letter\" title=\"Answer\" done=\"run.answered\" by=\"run.day >= 3\"/>\n\
         <objective id=\"sealed\" title=\"Seal\" done=\"run.sealed\"/>\n\
         <on event=\"questFailed\">\n@narrator: Too late.\n</on>\n</quest>\n\n\
         <quest id=\"errand\" title=\"Errand\" start=\"true\">\n\
         <objective id=\"maud\" title=\"Talk to Maud\" on=\"talk\" target=\"npc.maud\" done=\"true\"/>\n\
         </quest>\n",
    );
    dir
}

/// Step `n`'s presented beat ids, in order: `presented`, then `then`.
fn presented_ids(v: &Json, n: usize) -> Vec<String> {
    let s = step(v, n);
    s.get("presented")
        .into_iter()
        .chain(s["then"].as_array().into_iter().flatten())
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

/// Step `n`'s quest records, flattened like [`quest_records`] but telling a
/// `by` failure (`"<quest>.<objective> failed"`) from a completion.
fn quest_log(v: &Json, n: usize) -> Vec<String> {
    step(v, n)["quests"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|doc| doc["commands"].as_array().unwrap())
        .filter_map(|r| match r["kind"].as_str().unwrap() {
            "quest" => Some(format!("{} -> {}", r["quest"].as_str()?, r["state"].as_str()?)),
            "objective" => Some(format!(
                "{}.{} {}",
                r["quest"].as_str()?,
                r["objective"].as_str()?,
                if r["failed"] == true { "failed" } else { "done" }
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn an_also_beat_is_presented_after_the_winner_and_spends_its_once() {
    let dir = compose_project("also");
    let v = play_project_json(&dir, "also", "steps:\n  - occasion: hubVisit\n  - occasion: hubVisit\n");
    // `hub.aside` outranks `hub.main`, yet never wins: it rides along after.
    assert_eq!(candidate_ids(&v, 1), ["hub.aside", "hub.main"]);
    assert_eq!(candidate(&v, 1, "hub.aside")["also"], true);
    assert!(candidate(&v, 1, "hub.main").get("also").is_none());
    assert_eq!(winner(&v, 1), Some("hub.main"));
    assert_eq!(presented_ids(&v, 1), ["hub.main", "hub.aside"]);
    assert_eq!(step(&v, 1)["then"][0]["also"], true);
    // Presenting it spent its `once: run`; the winner repeats alone.
    assert_eq!(candidate(&v, 2, "hub.aside")["eligible"], false);
    assert_eq!(winner(&v, 2), Some("hub.main"));
    assert_eq!(presented_ids(&v, 2), ["hub.main"]);
    assert!(step(&v, 2).get("then").is_none(), "{}", step(&v, 2));

    let out = play_in(&dir, "also-human", "steps:\n  - occasion: hubVisit\n", false);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    let main = text.find("  → hub.main\n").unwrap_or_else(|| panic!("{text}"));
    let aside = text.find("  + hub.aside (also)\n").unwrap_or_else(|| panic!("{text}"));
    assert!(main < aside, "{text}");
    assert!(text.find("Welcome.").unwrap() < text.find("Psst.").unwrap(), "{text}");
}

#[test]
fn select_sequence_presents_every_eligible_beat_in_order_and_spends_each_once() {
    let dir = compose_project("sequence");
    let v = play_project_json(
        &dir,
        "sequence",
        "steps:\n  - occasion: evening\n  - occasion: evening\n  - occasion: evening\n",
    );
    assert_eq!(step(&v, 1)["select"], "sequence");
    // Eligibility is decided at the raise: the routine moves the day to 2,
    // but `eve.letter` was ineligible when `evening` was raised on day 1.
    assert_eq!(presented_ids(&v, 1), ["eve.routine"]);
    assert_eq!(candidate(&v, 1, "eve.letter")["reason"], "when: false");
    // Day 2: both, in selection order (priority first).
    assert_eq!(presented_ids(&v, 2), ["eve.routine", "eve.letter"]);
    assert_eq!(winner(&v, 2), Some("eve.routine"));
    // `eve.letter` spent its `once: run`; the routine repeats.
    assert_eq!(presented_ids(&v, 3), ["eve.routine"]);
    assert_eq!(candidate(&v, 3, "eve.letter")["reason"], "once: run — already presented this run");

    let script = "steps:\n  - occasion: evening\n  - occasion: evening\n    \
                  expect: { presented: [eve.routine, eve.letter] }\n";
    let out = play_in(&dir, "sequence-expect", script, false);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("· evening (select: sequence)"), "{text}");
    assert!(text.contains("  → eve.routine\n  → eve.letter\n"), "{text}");

    let wrong = "steps:\n  - occasion: evening\n  - occasion: evening\n    \
                 expect: { presented: [eve.letter, eve.routine] }\n";
    let out = play_in(&dir, "sequence-miss", wrong, false);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));

    let out = play_in(&dir, "sequence-pick", "steps:\n  - occasion: evening\n    pick: eve.routine\n", false);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(stderr(&out).contains("applies only to a `select: all` occasion"), "{}", stderr(&out));
}

#[test]
fn a_by_deadline_fails_its_objective_and_the_quest_after_the_presentation_that_passes_it() {
    let dir = compose_project("by-miss");
    // Each evening's routine advances the day; the second reaches day 3.
    let v = play_project_json(&dir, "by-miss", "steps:\n  - occasion: evening\n  - occasion: evening\n");
    assert!(quest_log(&v, 1).is_empty(), "{:?}", quest_log(&v, 1));
    assert_eq!(quest_log(&v, 2), ["deadline.letter failed", "deadline -> failed"]);
    let out = play_in(&dir, "by-miss-human", "steps:\n  - occasion: evening\n  - occasion: evening\n", false);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("  deadline.letter failed (by)\n"), "{text}");
    assert!(text.contains("  quest deadline -> failed\n"), "{text}");
    assert!(text.contains("Too late."), "questFailed ran: {text}");

    // Done first: the deadline passes without failing it; the quest waits
    // on `sealed`.
    let v = play_project_json(
        &dir,
        "by-done",
        "steps:\n  - engine: { state: { run.answered: true } }\n  - occasion: evening\n  \
         - occasion: evening\n  - occasion: evening\n",
    );
    assert_eq!(quest_log(&v, 1), ["deadline.letter done"]);
    for n in 2..=4 {
        assert!(quest_log(&v, n).iter().all(|r| !r.starts_with("deadline")), "step {n}: {:?}", quest_log(&v, n));
    }
    assert_eq!(v["exit"], "complete");
}

#[test]
fn a_targeted_objective_is_judged_only_at_a_step_for_its_target() {
    let dir = compose_project("target");
    let v = play_project_json(
        &dir,
        "target",
        "steps:\n  - { occasion: talk, target: npc.oskar }\n  - { occasion: talk, target: npc.maud }\n",
    );
    assert!(quest_log(&v, 1).is_empty(), "{:?}", quest_log(&v, 1));
    assert_eq!(quest_log(&v, 2), ["errand.maud done", "errand -> complete"]);
}
