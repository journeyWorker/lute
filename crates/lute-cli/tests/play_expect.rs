//! dsl 0.22.0 §3–§5: assertions in play scripts (`lute play` judges them;
//! `lute test` runs every expect-carrying play and counts what it presented)
//! and the new scenario-test expectations and seeds (`transcriptLacks`,
//! `offered`, `entry:`/`entries:`, `quests:`, `entriesRead:`), driven
//! through the built `lute` binary over the `play-hub` fixture.

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
    let dir =
        std::env::temp_dir().join(format!("lute-play-expect-{tag}-{}-{n}", std::process::id()));
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

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Two hub visits: `hub.firstEver` (priority 20, once: user) then
/// `hub.welcome` (its `when: user.metHypnos` now holds); the second visit
/// completes `firstEscape` (`run.hubVisits >= 2`).
const PASSING: &str = "\
steps:
  - occasion: hubVisit
    label: first visit
    expect:
      winner: hub.firstEver
      offered: [hub.idle, hub.firstEver]
      notOffered: [hub.welcome, hub.trophy]
  - occasion: hubVisit
    expect: { winner: hub.welcome }
expect:
  end: complete
  quests: { firstEscape: complete }
  state: { run.hubVisits: 2, user.metHypnos: true }
  transcriptContains: ['Back already?']
  transcriptLacks: ['Zzz.']
";

/// The same walk, wrong about step 2.
const FAILING: &str = "\
steps:
  - occasion: hubVisit
  - occasion: hubVisit
    label: second visit
    expect: { winner: hub.idle }
";

fn play(tag: &str, script: &str) -> Output {
    let dir = temp_dir(tag);
    let script = write(&dir, "s.play.yaml", script);
    lute(&[
        "play",
        fixture().to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ])
}

#[test]
fn a_play_whose_expectations_hold_exits_zero() {
    let out = play("pass", PASSING);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn a_failed_play_expectation_exits_one_naming_the_step_label_and_actual() {
    let out = play("fail", FAILING);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    let line = t
        .lines()
        .find(|l| l.contains("step 2 (second visit)") && l.contains("expect winner"))
        .unwrap_or_else(|| panic!("no miss line for step 2:\n{t}"));
    assert!(line.contains("hub.idle"), "the expectation: {line}");
    assert!(
        line.contains("actual hub.welcome"),
        "the actual winner: {line}"
    );

    // End-of-play misses name the actual value too.
    let out = play(
        "fail-end",
        "steps:\n  - occasion: hubVisit\nexpect:\n  state: { run.hubVisits: 5 }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.lines()
            .any(|l| l.contains("state run.hubVisits") && l.contains("actual 1")),
        "{t}"
    );
}

#[test]
fn an_unknown_expect_key_is_a_usage_error_listing_the_legal_keys() {
    let out = play(
        "bad-key",
        "steps:\n  - occasion: hubVisit\n    expect: { winer: hub.firstEver }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("`winer`"), "{t}");
    assert!(
        t.contains(
            "facts, notFacts, notOffered, offered, options, presented, quests, state, winner"
        ),
        "{t}"
    );
}

#[test]
fn lute_test_runs_every_play_that_carries_an_expect() {
    let dir = temp_dir("test-plays");
    write(&dir, "plays/pass.play.yaml", PASSING);
    write(&dir, "plays/fail.play.yaml", FAILING);
    // No `expect:` anywhere: `lute play` material, not a test.
    write(
        &dir,
        "plays/tour.play.yaml",
        "steps:\n  - occasion: hubVisit\n",
    );
    let project = fixture();
    let args = [
        "test",
        dir.to_str().unwrap(),
        "--project",
        project.to_str().unwrap(),
    ];
    let out = lute(&args);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.lines()
            .any(|l| l.starts_with("PASS") && l.contains("pass.play.yaml")),
        "{t}"
    );
    assert!(
        t.lines()
            .any(|l| l.starts_with("FAIL") && l.contains("fail.play.yaml")),
        "{t}"
    );
    assert!(t.contains("step 2 (second visit)"), "{t}");
    assert!(
        !t.contains("tour.play.yaml"),
        "a play without expect is not run: {t}"
    );
    assert!(t.contains("1 passed, 1 failed"), "{t}");

    let mut json_args = args.to_vec();
    json_args.push("--json");
    let out = lute(&json_args);
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let fail = v["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["test"].as_str().unwrap().ends_with("fail.play.yaml"))
        .unwrap_or_else(|| panic!("{v:#}"));
    assert_eq!(fail["kind"], "play", "{v:#}");
    assert_eq!(fail["passed"], false, "{v:#}");
    assert_eq!(fail["misses"][0]["step"], 2, "{v:#}");
    assert_eq!(fail["misses"][0]["actual"], "hub.welcome", "{v:#}");
}

#[test]
fn a_halted_play_fails_in_lute_test_unless_its_end_is_declared() {
    let dir = temp_dir("halted");
    // `oracle.vision`'s `when` reads `now()`: the reference runner cannot
    // decide it, so the play halts incomplete there.
    let halting = "steps:\n  - occasion: hubVisit\n    expect: { winner: hub.firstEver }\n  \
                   - occasion: talk\n    target: npc.oracle\n";
    write(&dir, "h.play.yaml", halting);
    let project = fixture();
    let args = [
        "test",
        dir.to_str().unwrap(),
        "--project",
        project.to_str().unwrap(),
    ];
    let out = lute(&args);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("end: expected complete"), "{t}");
    assert!(t.contains("actual incomplete"), "{t}");

    write(
        &dir,
        "h.play.yaml",
        &format!("{halting}expect:\n  end: incomplete\n"),
    );
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn coverage_counts_the_documents_a_play_presented() {
    let dir = temp_dir("play-coverage");
    write(&dir, "plays/pass.play.yaml", PASSING);
    let project = fixture();
    let out = lute(&[
        "test",
        dir.to_str().unwrap(),
        "--project",
        project.to_str().unwrap(),
        "--coverage",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["coverage"]["plays"], 1, "{v:#}");
    // T3-20: each untested unit is `{file, id, kind}`.
    let untested: Vec<&str> = v["coverage"]["untested"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["file"].as_str().unwrap())
        .collect();
    let listed = |f: &str| untested.iter().any(|p| p.ends_with(f));
    assert!(
        !listed("scenes/hub/first-ever.lute"),
        "presented: {untested:?}"
    );
    assert!(
        !listed("scenes/hub/welcome.lute"),
        "presented: {untested:?}"
    );
    assert!(
        listed("scenes/hub/idle.lute"),
        "offered but never presented: {untested:?}"
    );
}

/// lighthouse N3: a scene reached only by the occasion an `advance:` step's
/// clock raises is presented exactly as an `occasion:` step's winner is —
/// `--coverage` listed it untested. The header says plays count toward
/// documents only (the branch/arm rows are the traced paths').
#[test]
fn coverage_counts_the_documents_an_advance_step_presented() {
    let dir = temp_dir("advance-coverage");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
         run.slot: { type: { enum: [morning, night] }, default: morning, owner: engine }\n\
         clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, night]\n  raise: dawn\n",
    );
    write(
        &dir,
        "scenes/dawn.lute",
        "---\nkind: scene\nid: day.dawn\nuses: ../world.schema.yaml\non: dawn\n---\n\n\
         ## Dawn\n\n@narrator: Morning again.\n",
    );
    write(
        &dir,
        "scenes/unseen.lute",
        "---\nkind: scene\nid: day.unseen\nuses: ../world.schema.yaml\non: visit\n---\n\n\
         ## Unseen\n\n@narrator: Nobody comes.\n",
    );
    write(
        &dir,
        "plays/day.play.yaml",
        "steps:\n  - advance: day\n    expect: { winner: day.dawn }\n",
    );
    let d = dir.to_str().unwrap();
    let out = lute(&["test", d, "--project", d, "--coverage"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(
        t.contains(
            "coverage over 0 traced path(s) and 1 play(s) (plays count toward what they \
             presented and the choices they picked, not match arms):"
        ),
        "{t}"
    );
    assert!(t.contains("1 untested unit(s)"), "{t}");
    assert!(t.contains("scenes/unseen.lute"), "{t}");
    assert!(
        !t.contains("scenes/dawn.lute"),
        "presented by the advance's raise: {t}"
    );
}

/// T3-20 (round-5 OT-F11): a play's branch picks fold into the same
/// chosen-vs-never-chosen row a traced path's do, and the coverage report
/// lists the beats no play presented — the ending-proof view — even when a
/// scene test traces them.
#[test]
fn coverage_counts_play_picks_and_lists_beats_no_play_presented() {
    let dir = temp_dir("play-picks");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "scenes/ask.lute",
        "---\nkind: scene\nid: ask\non: ask\nonce: false\n---\n\n## Ask\n\n@narrator: Well?\n\
         <branch id=\"fate\" prompt=\"?\">\n  <choice id=\"drowned\" label=\"D\">\n    \
         @narrator: d.\n  </choice>\n  <choice id=\"murdered\" label=\"M\">\n    \
         @narrator: m.\n  </choice>\n</branch>\n",
    );
    write(
        &dir,
        "scenes/ending.lute",
        "---\nkind: scene\nid: ending\non: finale\n---\n\n## End\n\n@narrator: fin.\n",
    );
    for (name, pick) in [("a", "drowned"), ("b", "murdered")] {
        write(
            &dir,
            &format!("plays/{name}.play.yaml"),
            &format!(
                "steps:\n  - occasion: ask\n    choose: {{ fate: {pick} }}\nexpect:\n  end: complete\n"
            ),
        );
    }
    // A scene test traces the ending; it is still no play's proof.
    write(
        &dir,
        "tests/ending.test.yaml",
        "file: ../scenes/ending.lute\nexpect:\n  end: complete\n",
    );
    let d = dir.to_str().unwrap();
    let out = lute(&["test", d, "--project", d, "--coverage"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("branch/hub fate ("), "{t}");
    assert!(t.contains(": 2/2 chosen [drowned, murdered]"), "{t}");
    assert!(t.contains("every testable document, beat and entry"), "{t}");
    let (_, unplayed) = t
        .split_once("1 beat(s) no play presents")
        .unwrap_or_else(|| panic!("{t}"));
    assert!(unplayed.contains("scenes/ending.lute"), "{t}");
    assert!(!unplayed.contains("scenes/ask.lute"), "{t}");

    let out = lute(&["test", d, "--project", d, "--coverage", "--json"]);
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let unplayed = v["coverage"]["notPresentedByPlay"].as_array().unwrap();
    assert_eq!(unplayed.len(), 1, "{v:#}");
    assert_eq!(unplayed[0]["id"], "ending", "{v:#}");
}

// ---------------------------------------------------------------------------
// `*.test.yaml` additions (dsl 0.22.0 §3, §5).
// ---------------------------------------------------------------------------

const ASK: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
     state:\n  run.bold: { type: bool, default: false }\n---\n\n## One\n\n\
     <branch id=\"ask\">\n\
     <choice id=\"demand\" label=\"Demand\" when=\"run.bold\">\n@narrator: demanded.\n</choice>\n\
     <choice id=\"plead\" label=\"Plead\">\n@narrator: pleaded.\n</choice>\n\
     <choice id=\"leave\" label=\"Leave\">\n@narrator: left.\n</choice>\n\
     </branch>\n";

#[test]
fn options_and_transcript_lacks_judge_what_the_walk_offered_and_said() {
    let dir = temp_dir("options");
    write(&dir, "s.lute", ASK);
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nchoose: { ask: plead }\nexpect:\n  options: { ask: [leave, plead] }\n  \
         transcriptLacks: ['demanded.', 'left.']\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // Seeded bold, `demand` is offered too — the set is exact, so the old
    // expectation now misses, naming what was offered.
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nstate: { run.bold: true }\nchoose: { ask: demand }\nexpect:\n  \
         options: { ask: [leave, plead] }\n  transcriptLacks: ['demanded.']\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("options ask: expected [leave, plead], got [demand, leave, plead]"),
        "{t}"
    );
    assert!(t.contains("transcriptLacks \"demanded.\": present"), "{t}");
}

#[test]
fn quests_seed_the_status_a_scene_reads() {
    let dir = temp_dir("quest-seed");
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n\
         <match on=\"quest.caseClosed.state\">\n<when is=\"complete\">\n@narrator: solved.\n\
         </when>\n<otherwise>\n@narrator: open.\n</otherwise>\n</match>\n",
    );
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nquests: { caseClosed: complete }\nexpect:\n  \
         transcriptContains: ['solved.']\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn a_lore_test_presents_the_named_entries_in_order() {
    let project = fixture();
    let lore = project.join("lore/inbox.lute");
    let dir = temp_dir("lore-entries");
    let test = |body: &str| {
        write(
            &dir,
            "t.test.yaml",
            &format!("file: {}\n{body}", lore.display()),
        );
        lute(&[
            "test",
            dir.to_str().unwrap(),
            "--project",
            project.to_str().unwrap(),
        ])
    };

    // Twice in a row: the second presentation is a re-read. The transcript
    // expectations match presented content only (dsl 0.24.0, T1-2), so the
    // first-read/re-read split is judged by its effect: the first read
    // asserts `heardOf(meg)`.
    let out = test(
        "entries: [megNote, megNote]\nexpect:\n  \
         transcriptContains: [\"@meg: Don't get yourself killed out there.\"]\n  \
         transcriptLacks: ['first read', 're-read']\n  facts: [heardOf(meg)]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // `entriesRead:` seeds the save: the one presentation is already a
    // re-read, so its `::assert` is skipped.
    let out = test(
        "entry: megNote\nentriesRead: { run: [megNote] }\nexpect:\n  \
         notFacts: [heardOf(meg)]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // An unknown entry id is refused, in the test's own spelling.
    let out = test("entry: nope\nexpect:\n  end: complete\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("E-TRACE-ENTRY"), "{t}");
    assert!(t.contains("`entry: nope`"), "{t}");
}

// ---------------------------------------------------------------------------
// 0.23.1: world expectations on a step, facts in tests, one test file.
// ---------------------------------------------------------------------------

#[test]
fn a_step_expect_judges_quests_and_state_right_after_that_step() {
    // lamplight N12: night one's lifecycle, judged at its own step.
    let out = play(
        "step-world",
        "steps:\n  - occasion: hubVisit\n    expect:\n      quests: { firstEscape: active }\n      \
         state: { run.hubVisits: 1 }\n  - occasion: hubVisit\n    expect:\n      \
         quests: { firstEscape: complete }\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = play(
        "step-world-miss",
        "steps:\n  - occasion: hubVisit\n    label: night one\n    expect:\n      \
         quests: { firstEscape: complete }\n      state: { run.hubVisits: 2 }\n  - occasion: hubVisit\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("step 1 (night one) at hubVisit: expect quests firstEscape: expected complete, actual active"),
        "{t}"
    );
    assert!(
        t.contains("expect state run.hubVisits: expected 2, actual 1"),
        "{t}"
    );
}

#[test]
fn a_selection_expect_on_a_non_occasion_step_is_a_usage_error() {
    let out = play(
        "engine-winner",
        "steps:\n  - engine: { state: { run.hubVisits: 1 } }\n    expect: { winner: hub.idle }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("`expect.winner` applies only to an `occasion` or `advance` step"),
        "{t}"
    );
}

#[test]
fn a_test_asserts_facts_and_runs_alone_and_a_missing_document_is_one_failure() {
    let dir = temp_dir("test-facts");
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         enums:\n  who: [meg, zag]\nrelations:\n  met: { args: [who], tier: run }\n---\n\n## One\n\n\
         @narrator: hello.\n::assert{met(meg)}\n",
    );
    let t_ok = write(
        &dir,
        "ok.test.yaml",
        "file: s.lute\nexpect:\n  facts: [met(meg)]\n  notFacts: [met(zag)]\n",
    );
    write(
        &dir,
        "gone.test.yaml",
        "file: nope.lute\nexpect:\n  end: complete\n",
    );
    // One file runs alone.
    let out = lute(&["test", t_ok.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    // The whole directory: the missing document fails its own test only.
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("E-TEST-FILE"), "{t}");
    assert!(t.contains("1 passed, 1 failed"), "{t}");
    write(
        &dir,
        "ok.test.yaml",
        "file: s.lute\nexpect:\n  notFacts: [met(meg)]\n",
    );
    let out = lute(&["test", dir.join("ok.test.yaml").to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("notFacts met(meg): expected does not hold, got holds"),
        "{t}"
    );
}

// ---------------------------------------------------------------------------
// 0.24.0 T1-2: transcript expectations match presented content lines, in one
// canonical form (`@speaker: text`) shared by `lute play` and `lute test`.
// ---------------------------------------------------------------------------

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn transcript_expectations_match_presented_lines_only_in_play_and_test_alike() {
    let project = temp_dir("said");
    copy_tree(&fixture(), &project);
    // A guarded line the first visit never presents (`run.hubVisits` is 0).
    write(
        &project,
        "scenes/hub/first-ever.lute",
        "---\nkind: scene\nid: hub.firstEver\nuses: ../../world.schema.yaml\non: hubVisit\n\
         priority: 20\nonce: user\n---\n\n## The House of Hades\n\n\
         @hypnos: Oh, a new face.\n\
         @hypnos{when=\"run.hubVisits >= 5\"}: Never said.\n\
         ::set{user.metHypnos = true}\n::set{run.hubVisits = run.hubVisits + 1}\n",
    );
    let run_play = |script: &str| {
        let s = write(&project, "plays/probe.play.yaml", script);
        let out = lute(&[
            "play",
            project.to_str().unwrap(),
            "--script",
            s.to_str().unwrap(),
        ]);
        std::fs::remove_file(s).unwrap();
        out
    };
    // The skipped line is in the human transcript (`skip @hypnos "…"`) but
    // was never presented: `transcriptContains` misses, `transcriptLacks`
    // holds.
    let out = run_play(
        "steps:\n  - occasion: hubVisit\nexpect:\n  transcriptContains: ['Never said.']\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("skip @hypnos"),
        "the human transcript still shows the skip: {t}"
    );
    assert!(
        t.contains("expected \"Never said.\" present, actual absent"),
        "{t}"
    );
    let out = run_play(
        "steps:\n  - occasion: hubVisit\nexpect:\n  transcriptLacks: ['Never said.', 'step 1', 'priority']\n  \
         transcriptContains: ['@hypnos: Oh, a new face.']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // A scene test over the same document matches the same canonical form.
    let tests = temp_dir("said-test");
    write(
        &tests,
        "t.test.yaml",
        &format!(
            "file: {}\nexpect:\n  transcriptContains: ['@hypnos: Oh, a new face.']\n  \
             transcriptLacks: ['Never said.', 'trace:']\n",
            project.join("scenes/hub/first-ever.lute").display()
        ),
    );
    let out = lute(&[
        "test",
        tests.to_str().unwrap(),
        "--project",
        project.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// 0.24.0 T1-13: a hub's `options:` is the choices eligible at each visit
/// (unioned), as a branch's is — it used to read `[]` for every hub.
#[test]
fn options_judges_a_hub_like_a_branch() {
    let dir = temp_dir("hub-options");
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## H\n\n\
         <hub id=\"chat\">\n  <choice id=\"a\" label=\"A\">\n    @narrator: a.\n  </choice>\n  \
         <choice id=\"never\" label=\"N\" when=\"scene.visited.chat.leave\">\n    @narrator: n.\n  </choice>\n  \
         <choice id=\"leave\" label=\"Leave\" exit>\n    @narrator: bye.\n  </choice>\n</hub>\n",
    );
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nchoose: { chat: [a, leave] }\nexpect:\n  options: { chat: [a, leave] }\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nchoose: { chat: [a, leave] }\nexpect:\n  options: { chat: [a, never, leave] }\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("got [a, leave]"), "{t}");
}

/// A test and a play name a branch/hub's menu choices `options:`; a play's
/// `offered` is the occasion's beat candidates. The old test spellings
/// (`expect.offered`, top-level `accept:`) name their replacements, and a
/// key that belongs to the other file says which file owns it.
#[test]
fn test_and_play_keys_name_the_spelling_and_the_file_they_belong_to() {
    let dir = temp_dir("key-renames");
    write(&dir, "s.lute", ASK);
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\naccept: [q]\npayload: { a: 1 }\nchoose: { ask: plead }\nexpect:\n  \
         offered: { ask: [leave, plead] }\n  winner: x\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    for want in [
        "error [E-TEST-KEY] `accept:` is now `accepts:` in a `*.test.yaml`",
        "unknown top-level key `payload` in a `*.test.yaml` (`payload:` belongs to a play step, \
         in a `*.play.yaml`; a test seeds the raise's payload as state, \
         `state: { occasion.payload.<field>: … }`)",
        "error [E-TEST-KEY] `expect.offered` is now `expect.options` — in a test and in a play, \
         `options` are the menu choices a branch/hub presents (a play's `offered` lists beat \
         candidates)",
        "unknown `expect:` key `winner` in a `*.test.yaml` (`winner:` belongs to a play step's \
         `expect:`, in a `*.play.yaml`)",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }

    // The reverse: a test's menu-choice map written as a play's `offered`,
    // and test keys in a play.
    let out = play(
        "offered-map",
        "steps:\n  - occasion: hubVisit\n    expect: { offered: { ask: [a] } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains(
            "`expect.offered` must be a list of beat ids — in a play, menu choices are \
             `options: { <branch or hub id>: [option ids] }`; `offered` lists beat candidates"
        ),
        "{t}"
    );
    let out = play(
        "test-keys",
        "steps:\n  - occasion: hubVisit\n    expect: { accepts: [q] }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("(`accepts` is a `*.test.yaml` expectation, not a play's)"),
        "{t}"
    );
    let out = play("test-top", "file: s.lute\nsteps:\n  - occasion: hubVisit\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("(`file:` is a `*.test.yaml` key, not a play's)"),
        "{t}"
    );
}

/// A document's spelling of a step key (`on`, `for`) names the step's.
#[test]
fn a_documents_spelling_of_a_step_key_names_the_step_key() {
    for (name, script, want) in [
        (
            "doc-on",
            "steps:\n  - on: hubVisit\n",
            "step 1: unknown key `on` — the step key is `occasion:`",
        ),
        (
            "doc-for",
            "steps:\n  - occasion: hubVisit\n    for: npc.mira\n",
            "step 1: unknown key `for` — the step key is `target:`",
        ),
    ] {
        let out = play(name, script);
        let t = text(&out);
        assert_eq!(out.status.code(), Some(2), "{t}");
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
}

/// 0.24.0 T3-5: `eligible:` suppresses the note it answers, and a map key
/// judges an entry of the file the test did not present — alone, under the
/// same mocks; a lore test may carry it without presenting anything.
#[test]
fn eligible_answers_its_note_and_judges_unpresented_entries() {
    let project = fixture();
    let lore = project.join("lore/inbox.lute");
    let dir = temp_dir("eligible-all");
    let test = |body: &str| {
        write(
            &dir,
            "t.test.yaml",
            &format!("file: {}\n{body}", lore.display()),
        );
        lute(&[
            "test",
            dir.to_str().unwrap(),
            "--project",
            project.to_str().unwrap(),
        ])
    };
    let out = test("entry: dusaNote\nexpect:\n  eligible: { dusaNote: false }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(
        !t.contains("is not eligible under these mocks"),
        "asserted, so no note: {t}"
    );

    let out = test("entry: megNote\nexpect:\n  eligible: { megNote: true, dusaNote: false }\n");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = test("expect:\n  eligible: { megNote: true, dusaNote: true }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("eligible dusaNote: expected true, got false"),
        "{t}"
    );
    let out = test("expect:\n  eligible: { nope: true }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("declares no such entry or beat"), "{t}");
}

/// 0.24.0 T3-10: a step `expect.options` judges a branch/hub's offered
/// options; `select: all` with nothing offered needs no `pick:`; a `newRun`
/// prints the `prev.run.*` values; an unquoted atom gets a quoting hint.
#[test]
fn play_harness_details() {
    let project = temp_dir("harness");
    copy_tree(&fixture(), &project);
    write(
        &project,
        "plugins/hub.occasions/occasions/hub.yaml",
        "occasions:\n  hubVisit: {}\n  talk: { target: true }\n  inbox: { select: all }\n  \
         quiet: { select: all }\n  ask: {}\n",
    );
    write(
        &project,
        "scenes/ask.lute",
        "---\nkind: scene\nid: ask\nuses: ../world.schema.yaml\non: ask\nonce: false\n---\n\n\
         ## Ask\n\n@hypnos: Well?\n\
         <branch id=\"fate\" prompt=\"?\">\n  <choice id=\"drowned\" label=\"D\">\n    @hypnos: d.\n  </choice>\n  \
         <choice id=\"murdered\" label=\"M\">\n    @hypnos: m.\n  </choice>\n  \
         <choice id=\"never\" label=\"N\" when=\"run.hubVisits > 9\">\n    @hypnos: n.\n  </choice>\n</branch>\n",
    );
    let run_play = |script: &str| {
        let s = write(&project, "plays/probe.play.yaml", script);
        let out = lute(&[
            "play",
            project.to_str().unwrap(),
            "--script",
            s.to_str().unwrap(),
        ]);
        std::fs::remove_file(s).unwrap();
        out
    };
    let out = run_play(
        "steps:\n  - occasion: quiet\n  - engine: { state: { run.hubVisits: 3 } }\n  - newRun: true\n  \
         - occasion: ask\n    choose: { fate: drowned }\n    \
         expect: { options: { fate: [drowned, murdered] } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("pick: none (nothing offered)"), "{t}");
    assert!(t.contains("prev.run.hubVisits = 3"), "{t}");
    let out = run_play(
        "steps:\n  - occasion: ask\n    choose: { fate: drowned }\n    \
         expect: { options: { fate: [drowned, murdered, never] } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("expect options fate"), "{t}");

    let out = run_play("steps:\n  - occasion: inbox\n");
    let t = text(&out);
    assert_ne!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("offers [megNote]"), "{t}");

    let out = run_play("facts: [heardOf(meg, meg)]\nsteps:\n  - occasion: quiet\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("quote the atom"), "{t}");
}

/// 0.24.0 T3-15: tracing against the nearest manifest settles the "existence
/// is unverified" note of a foreign quest read against the project's quests.
/// With `--project` the gate is the whole project's verdict, where a read no
/// quest defines is an error with the same did-you-mean.
#[test]
fn trace_with_project_verifies_quest_existence() {
    let project = temp_dir("trace-quests");
    copy_tree(&fixture(), &project);
    let scene = write(
        &project,
        "scenes/probe.lute",
        "---\nkind: scene\nid: probe\nuses: ../world.schema.yaml\non: hubVisit\n---\n\n## P\n\n\
         @hypnos{when=\"quest.firstEscape.state == 'active'\"}: Running.\n\
         @hypnos{when=\"quest.firstEscap.state == 'active'\"}: Typo.\n",
    );
    let scene = scene.to_str().unwrap();
    let out = lute(&["trace", scene]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(!t.contains("existence is unverified"), "{t}");
    assert!(
        t.contains("quest `firstEscap` is declared by no quest document of the project — did you mean `firstEscape`?"),
        "{t}"
    );
    let out = lute(&["trace", scene, "--project", project.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("[E-QUEST-REF-UNKNOWN] `quest.firstEscap.state` references quest `firstEscap`, which no project quest defines — did you mean `firstEscape`?"),
        "{t}"
    );
    assert!(t.contains("trace refused"), "{t}");
}

/// 0.24.0 T3-11: an `<on event>` handler of a quest an engine write already
/// completed does not run — and the play says so.
#[test]
fn play_prints_the_skipped_handler_of_a_settled_quest() {
    let project = temp_dir("handler-skip");
    copy_tree(&fixture(), &project);
    // `hubVisit` is also a world event here, so a quest may handle it.
    write(
        &project,
        "plugins/hub.occasions/plugin.yaml",
        "id: hub.occasions\nversion: 0.1.0\nkind: capability\n\
         depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n  events: events/\n",
    );
    write(
        &project,
        "plugins/hub.occasions/events/e.yaml",
        "events:\n  - name: hubVisit\n",
    );
    write(
        &project,
        "quests/boss.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Boss\n---\n\n\
         <quest id=\"boss\" title=\"Boss\" start=\"true\">\n\
         <objective id=\"slay\" title=\"Slay\" done=\"run.hubVisits >= 5\"/>\n\
         <on event=\"hubVisit\">\n@narrator: The hall remembers.\n</on>\n</quest>\n",
    );
    let s = write(
        &project,
        "plays/probe.play.yaml",
        "steps:\n  - engine: { state: { run.hubVisits: 5 } }\n  - occasion: hubVisit\n",
    );
    let out = lute(&[
        "play",
        project.to_str().unwrap(),
        "--script",
        s.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(
        t.contains("<on event=hubVisit> of quest boss skipped — quest complete"),
        "{t}"
    );
    assert!(!t.contains("The hall remembers."), "{t}");
}

/// A temp copy of the `bridge-check` fixture (a `::check` bridge directive,
/// the accept-driven quest `lampOut`, the lore document `lore.tomas`) plus
/// every `(path, text)` document.
fn bridge_project(tag: &str, docs: &[(&str, &str)]) -> PathBuf {
    let project = temp_dir(tag);
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-check"),
        &project,
    );
    for (rel, body) in docs {
        write(&project, rel, body);
    }
    project
}

/// `lute test` on one test file written into `project/tests/`.
fn scenario_test(project: &Path, body: &str) -> Output {
    let t = write(project, "tests/probe.test.yaml", body);
    lute(&[
        "test",
        t.to_str().unwrap(),
        "--project",
        project.to_str().unwrap(),
    ])
}

/// `lute play` of one script written into `project/plays/`.
fn play_in(project: &Path, script: &str) -> Output {
    let s = write(project, "plays/probe.play.yaml", script);
    lute(&[
        "play",
        project.to_str().unwrap(),
        "--script",
        s.to_str().unwrap(),
    ])
}

/// A scene beat whose `when` is false on day 1, gating a bridge call.
const LATE_SCENE: &str = "---\nkind: scene\nid: probe.late\ntitle: Late\non: townVisit\n\
     when: 'run.day > 3'\n---\n\n## Late\n\n\
     ::check{skill=\"stealth\" dc=\"10\" resultKey=\"late\" sync=\"true\"}\n\
     <match on=\"scene.check.late.passed\">\n  <when is=\"true\">\n    @narrator: Slipped in.\n  \
     </when>\n  <when is=\"false\">\n    @narrator: Caught.\n  </when>\n</match>\n";

/// A lore entry whose `when` is false on day 1.
const GATE_LORE: &str = "---\nkind: lore\nid: lore.gate\ntitle: Gate\n---\n\n\
     <entry id=\"gateNote\" title=\"Gate\" when=\"run.day > 3\">\n  @narrator: The gate is open.\n\
     </entry>\n";

/// dsl 0.26.0 §7 (T1-7): a test that walks an entry or scene whose `when`
/// is false under its mocks fails unless it asserts `eligible:`; asserting
/// `eligible: false` does not walk the body (no bridge answer needed), and
/// a scene test may assert `eligible:` too.
#[test]
fn walking_an_ineligible_unit_fails_unless_eligible_is_asserted() {
    let project = bridge_project(
        "eligible-gate",
        &[
            ("scenes/probe/late.lute", LATE_SCENE),
            ("lore/gate.lute", GATE_LORE),
        ],
    );
    // A scene: walked although never presented — a failure, naming why.
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/late.lute\nbridges:\n  check:\n    - { passed: false, margin: 0 }\n\
         expect:\n  transcriptContains: [\"Caught.\"]\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("eligible probe.late: not eligible under these mocks (its `when` (run.day > 3) is false (`run.day` is 1))"),
        "{t}"
    );
    // `eligible: false` holds, and the body — with its bridge call — is not
    // walked: no answer is needed, nothing of it is said.
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/late.lute\nexpect:\n  eligible: false\n  \
         transcriptLacks: [\"Caught.\", \"Slipped in.\"]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    // Eligible under its mocks: walked as before.
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/late.lute\nstate: { run.day: 5 }\nbridges:\n  check:\n    \
         - { passed: true, margin: 1 }\nexpect:\n  eligible: true\n  transcriptContains: [\"Slipped in.\"]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // A lore entry, the same.
    let out = scenario_test(
        &project,
        "file: ../lore/gate.lute\nentry: gateNote\nexpect:\n  transcriptContains: [\"The gate is open.\"]\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("eligible gateNote: not eligible"), "{t}");
    let out = scenario_test(
        &project,
        "file: ../lore/gate.lute\nentry: gateNote\nexpect:\n  eligible: { gateNote: false }\n  \
         transcriptLacks: [\"The gate is open.\"]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// dsl 0.26.0 §8 (T3-10): an entry may be named `<document id>.<entry id>`
/// in a test's `entry:` / `entries:` and `eligible:` keys, and in a play's
/// `pick:`, `entriesRead:` and step `winner` / `offered` / `presented`.
#[test]
fn an_entry_is_named_by_its_document_alias_in_test_and_play() {
    let project = bridge_project("entry-alias", &[("lore/gate.lute", GATE_LORE)]);
    let out = scenario_test(
        &project,
        "file: ../lore/gate.lute\nentry: lore.gate.gateNote\nexpect:\n  \
         eligible: { lore.gate.gateNote: false }\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = scenario_test(
        &project,
        "file: ../lore/gate.lute\nentries: [lore.gate.gateNote]\nstate: { run.day: 5 }\n\
         expect:\n  transcriptContains: [\"The gate is open.\"]\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    let out = play_in(
        &project,
        "entriesRead: { user: [lore.tomas.tomasBusy] }\nsteps:\n  \
         - engine: { accept: [lampOut] }\n  \
         - occasion: talk\n    target: npc.tomas\n    \
           expect: { winner: lore.tomas.tomasOil, presented: [lore.tomas.tomasOil], \
           offered: [lore.tomas.tomasBusy] }\n\
         expect:\n  state: { entry.tomasBusy.everRead: true, entry.tomasOil.read: true }\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// dsl 0.26.0 §7 (T2-9): `engine: { accept: [id] }` accepts an
/// accept-driven quest mid-play; the next settle activates it.
#[test]
fn the_engine_accepts_an_accept_driven_quest_mid_play() {
    let project = bridge_project("engine-accept", &[]);
    let out = play_in(
        &project,
        "steps:\n  - engine: { state: { user.bond.mara: 1 } }\n    expect: { quests: { lampOut: unset } }\n  \
         - engine: { accept: [lampOut] }\n    expect: { quests: { lampOut: active } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("quest lampOut accepted (engine)"), "{t}");
    let out = play_in(&project, "steps:\n  - engine: { accept: [dayEnd] }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("no accept-driven quest of this project"), "{t}");
}

/// dsl 0.26.0 §7 (T2-9): an engine accept of a quest that is already active
/// (or settled) is ignored with a note — it is not accepted a second time
/// and its status does not change.
#[test]
fn an_engine_accept_of_an_active_quest_is_ignored_with_a_note() {
    let project = bridge_project("engine-accept-twice", &[]);
    let out = play_in(
        &project,
        "steps:\n  - engine: { accept: [lampOut] }\n    expect: { quests: { lampOut: active } }\n  \
         - engine: { accept: [lampOut] }\n    expect: { quests: { lampOut: active } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert_eq!(
        t.matches("quest lampOut accepted (engine)").count(),
        1,
        "{t}"
    );
    assert!(
        t.contains("note: quest lampOut is already active — engine accept ignored"),
        "{t}"
    );
}

/// dsl 0.26.0 §7 (T3-6), 0.27 (T1-11): a `transcriptContains` needle copied
/// from a line with attributes (`@narrator{emotion="…"}: …`) matches the
/// presented line; a needle naming OTHER attributes matches nothing, so the
/// same needle holds under `transcriptLacks`; a miss quotes a real presented
/// line (attributes included), never the needle — in play and test alike.
#[test]
fn a_transcript_needle_matches_line_attributes_and_a_miss_quotes_a_real_line() {
    let project = bridge_project(
        "needle",
        &[(
            "scenes/probe/said.lute",
            "---\nkind: scene\nid: probe.said\ntitle: Said\non: townVisit\npriority: 99\n---\n\n\
             ## Said\n\n@narrator{emotion=\"delighted\"}: The lamp is lit.\n",
        )],
    );
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/said.lute\nexpect:\n  \
         transcriptContains: ['@narrator{emotion=\"delighted\"}: The lamp is lit.']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/said.lute\nexpect:\n  transcriptContains: ['The lamp is lot.']\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    let nearest = "nearest line: \"@narrator{emotion=\\\"delighted\\\"}: The lamp is lit.\"";
    assert!(t.contains(nearest), "{t}");
    for (key, code) in [("transcriptContains", 1), ("transcriptLacks", 0)] {
        let out = scenario_test(
            &project,
            &format!(
                "file: ../scenes/probe/said.lute\nexpect:\n  \
                 {key}: ['@narrator{{emotion=\"shy\"}}: The lamp is lit.']\n"
            ),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(code), "{key}: {t}");
        if code == 1 {
            assert!(t.contains(nearest), "{t}");
        }
    }
    let out = scenario_test(
        &project,
        "file: ../scenes/probe/said.lute\nexpect:\n  transcriptLacks: ['@narrator: The lamp']\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("present (line: \"@narrator{emotion=\\\"delighted\\\"}: The lamp is lit.\")"),
        "{t}"
    );

    let out = play_in(
        &project,
        "steps:\n  - occasion: townVisit\nexpect:\n  \
         transcriptContains: ['@narrator{emotion=\"delighted\"}: The lamp is lit.']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = play_in(
        &project,
        "steps:\n  - occasion: townVisit\nexpect:\n  transcriptContains: ['The lamp is lot.']\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains(nearest), "{t}");
    for (key, code) in [("transcriptContains", 1), ("transcriptLacks", 0)] {
        let out = play_in(
            &project,
            &format!(
                "steps:\n  - occasion: townVisit\nexpect:\n  \
                 {key}: ['@narrator{{emotion=\"shy\"}}: The lamp is lit.']\n"
            ),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(code), "{key}: {t}");
        if code == 1 {
            assert!(t.contains(nearest), "{t}");
        }
    }
}

/// 0.27 prerelease OT-F-2: a needle whose attribute block names what no
/// presented line can carry — a misspelt key, a value outside its domain —
/// is refused with a did-you-mean before anything plays, in play and test
/// alike. It used to make `transcriptLacks` hold although the line was said.
#[test]
fn a_needle_attribute_no_line_can_carry_is_refused_with_a_did_you_mean() {
    let project = bridge_project(
        "needle-vocab",
        &[(
            "scenes/probe/said.lute",
            "---\nkind: scene\nid: probe.said\ntitle: Said\non: townVisit\npriority: 99\n---\n\n\
             ## Said\n\n@narrator{emotion=\"delighted\"}: The lamp is lit.\n",
        )],
    );
    for (block, hint) in [
        ("emotoin=\"delighted\"", "did you mean `emotion`?"),
        ("emotion=\"delightd\"", "did you mean `delighted`?"),
    ] {
        let needle = format!("['@narrator{{{block}}}: The lamp is lit.']");
        let out = scenario_test(
            &project,
            &format!("file: ../scenes/probe/said.lute\nexpect:\n  transcriptLacks: {needle}\n"),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(1), "{t}");
        assert!(t.contains("E-TEST-NEEDLE") && t.contains(hint), "{t}");
        let out = play_in(
            &project,
            &format!("steps:\n  - occasion: townVisit\nexpect:\n  transcriptLacks: {needle}\n"),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(2), "{t}");
        assert!(t.contains(hint), "{t}");
    }
}

/// 0.27 prerelease OT N-2: a `transcriptLacks` needle whose speaker is no
/// speaker of the project, and a `facts` / `notFacts` atom naming an unknown
/// relation, the wrong arity or a non-member, used to hold vacuously (or
/// only miss). They are refused like seed facts, with a did-you-mean,
/// located at the entry: a usage error in play, an invalid test in test.
#[test]
fn a_misspelt_needle_speaker_or_expected_fact_is_refused_not_vacuous() {
    let project = bridge_project(
        "expect-vocab",
        &[(
            "scenes/probe/said.lute",
            "---\nkind: scene\nid: probe.said\ntitle: Said\non: townVisit\npriority: 99\n---\n\n\
             ## Said\n\n@mara: The lamp is lit.\n::assert{knows(lamp)}\n",
        )],
    );
    let schema = project.join("world.schema.yaml");
    let with_cast = std::fs::read_to_string(&schema).unwrap()
        + "cast:\n  mara: { name: Mara }\n  tomas: { name: Tomas }\n";
    std::fs::write(&schema, with_cast).unwrap();

    for (key, entry, hint) in [
        (
            "transcriptLacks",
            "['@mra: The lamp is lit.']",
            "`@mra` is not a speaker of this project — did you mean `mara`?",
        ),
        (
            "notFacts",
            "['knowz(lamp)']",
            "undeclared relation `knowz` — did you mean `knows`?",
        ),
        (
            "notFacts",
            "['knows(lamp, mara)']",
            "has 2 argument(s), and `knows` takes 1",
        ),
        (
            "facts",
            "['knows(lammp)']",
            "`lammp`, which is not a member of `item` — did you mean `lamp`?",
        ),
    ] {
        let out = play_in(
            &project,
            &format!("steps:\n  - occasion: townVisit\nexpect:\n  {key}: {entry}\n"),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(2), "{key}: {t}");
        assert!(t.contains("probe.play.yaml:4:") && t.contains(hint), "{t}");
    }
    // A step's expectation is located at its own atom.
    let out = play_in(
        &project,
        "steps:\n  - occasion: townVisit\n    expect:\n      notFacts: [knows(lamp), knowz(lamp)]\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("probe.play.yaml:4:31: step 1: `expect.notFacts` entry `knowz(lamp)`"),
        "{t}"
    );
    // The spelt-right forms play and judge as before.
    let out = play_in(
        &project,
        "steps:\n  - occasion: townVisit\nexpect:\n  facts: [knows(lamp)]\n  \
         transcriptContains: ['@mara: The lamp is lit.']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    for (key, entry, code, hint) in [
        (
            "transcriptLacks",
            "'@mra: The lamp is lit.'",
            "E-TEST-NEEDLE",
            "did you mean `mara`?",
        ),
        (
            "notFacts",
            "'knowz(lamp)'",
            "E-TRACE-MOCK-FACT",
            "did you mean `knows`?",
        ),
        (
            "notFacts",
            "'knows(lammp)'",
            "E-TRACE-MOCK-FACT",
            "did you mean `lamp`?",
        ),
    ] {
        let out = scenario_test(
            &project,
            &format!("file: ../scenes/probe/said.lute\nexpect:\n  {key}: [{entry}]\n"),
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(1), "{key}: {t}");
        assert!(
            t.contains("probe.test.yaml:3:") && t.contains(code) && t.contains(hint),
            "{t}"
        );
    }
}

/// A one-scene project: `run.route` over the enum `route`, and a `festival`
/// scene (`once: user`, answering `evening`) whose branch `pick` offers
/// `ren` / `mika`.
fn festival_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.route: { type: { domain: route }, default: none }\nenums:\n  route: [none, ren, mika]\n",
    );
    write(
        &dir,
        "scenes/festival.lute",
        "---\nkind: scene\nid: festival\ntitle: The festival\non: evening\nonce: user\n---\n\n\
         ## The jetty\n\n<branch id=\"pick\">\n  <choice id=\"ren\" label=\"Ren\" into=\"run.route\" value=\"ren\">\n    \
         @narrator: You float Ren's lantern.\n  </choice>\n  <choice id=\"mika\" label=\"Mika\" into=\"run.route\" value=\"mika\">\n    \
         @narrator: You float Mika's lantern.\n  </choice>\n</branch>\n",
    );
    dir
}

/// Every usage error of a play is reported in one run, one per line, in the
/// order they were written — a seed, a step's `choose:` and a step's
/// `expect:` — not one per run in the order of the checks; a list entry is
/// located at the entry.
#[test]
fn every_play_usage_error_is_reported_at_once_in_file_order() {
    let dir = festival_project("usage-order");
    let script = write(
        &dir,
        "plays/p.play.yaml",
        "state: { run.route: rne }\nfacts: [\"empty(ada\"]\nsteps:\n  - occasion: evening\n    \
         choose: { pik: ren }\n    expect: { state: { run.route: mkia }, bogus: 1 }\n",
    );
    let d = dir.to_str().unwrap();
    let out = lute(&["play", d, "--script", script.to_str().unwrap()]);
    let t = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(out.status.code(), Some(2), "{t}");
    let located: Vec<&str> = t
        .lines()
        .map(|l| l.split(": ").nth(1).unwrap_or(""))
        .collect();
    let at: Vec<&str> = located
        .iter()
        .map(|l| l.rsplit_once("p.play.yaml:").map_or("", |(_, a)| a))
        .collect();
    // The unknown `expect:` key is a parse error: the script does not reach
    // the project checks, so it is reported alone.
    assert_eq!(at, ["6:43"], "{t}");
    std::fs::write(
        &script,
        "state: { run.route: rne }\nfacts: [\"empty(ada\"]\nsteps:\n  - occasion: evening\n    \
         choose: { pik: ren }\n    expect: { state: { run.route: mkia } }\n",
    )
    .unwrap();
    let out = lute(&["play", d, "--script", script.to_str().unwrap()]);
    let t = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(out.status.code(), Some(2), "{t}");
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines.len(), 4, "{t}");
    assert!(
        lines[0].contains("p.play.yaml:1:10: `state.run.route`"),
        "{t}"
    );
    assert!(
        lines[1].contains("p.play.yaml:2:9: `facts:` entry `empty(ada`"),
        "{t}"
    );
    assert!(lines[1].contains("parentheses do not balance"), "{t}");
    assert!(!lines[1].contains("quote the atom"), "{t}");
    assert!(
        lines[2].contains("p.play.yaml:5:15: step 1: `choose.pik`"),
        "{t}"
    );
    // LF28-8: a non-member is located at the value, not the key.
    assert!(
        lines[3].contains("p.play.yaml:6:35: step 1: `expect.state.run.route: mkia`"),
        "{t}"
    );
    assert!(lines.iter().all(|l| l.starts_with("lute play: ")), "{t}");
}

/// A save's `visited:` scene was presented: its `once: user` is spent, so
/// the scene does not win again (it used to, unless also seeded under
/// `presented: { user: … }`).
#[test]
fn a_visited_seed_spends_the_scenes_once_user() {
    let dir = festival_project("visited-once");
    let script = write(
        &dir,
        "plays/p.play.yaml",
        "visited: [festival]\nsteps:\n  - occasion: evening\n    expect: { winner: none }\n",
    );
    let d = dir.to_str().unwrap();
    let out = lute(&["play", d, "--script", script.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn presented_advancing_beat_moves_clock_settles_quest_and_raises_slot() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-advance");
    let script = fixture.join("plays/advance.play.yaml");
    let out = lute(&[
        "play",
        fixture.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    let shown = text(&out);
    assert_eq!(out.status.code(), Some(0), "{shown}");
    assert!(shown.contains("expect: every expectation held"), "{shown}");
    assert!(
        shown.contains("both moves apply, so remove this step if it duplicates that movement"),
        "{shown}"
    );
}
