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
    let dir = std::env::temp_dir().join(format!(
        "lute-play-expect-{tag}-{}-{n}",
        std::process::id()
    ));
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
  exit: complete
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
    assert!(line.contains("actual hub.welcome"), "the actual winner: {line}");

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
    assert!(t.contains("facts, notFacts, notOffered, offered, options, presented, quests, state, winner"), "{t}");
}

#[test]
fn lute_test_runs_every_play_that_carries_an_expect() {
    let dir = temp_dir("test-plays");
    write(&dir, "plays/pass.play.yaml", PASSING);
    write(&dir, "plays/fail.play.yaml", FAILING);
    // No `expect:` anywhere: `lute play` material, not a test.
    write(&dir, "plays/tour.play.yaml", "steps:\n  - occasion: hubVisit\n");
    let project = fixture();
    let args = ["test", dir.to_str().unwrap(), "--project", project.to_str().unwrap()];
    let out = lute(&args);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.lines().any(|l| l.starts_with("PASS") && l.contains("pass.play.yaml")),
        "{t}"
    );
    assert!(
        t.lines().any(|l| l.starts_with("FAIL") && l.contains("fail.play.yaml")),
        "{t}"
    );
    assert!(t.contains("step 2 (second visit)"), "{t}");
    assert!(!t.contains("tour.play.yaml"), "a play without expect is not run: {t}");
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
fn a_halted_play_fails_in_lute_test_unless_its_exit_is_declared() {
    let dir = temp_dir("halted");
    // `oracle.vision`'s `when` reads `now()`: the reference runner cannot
    // decide it, so the play halts incomplete there.
    let halting = "steps:\n  - occasion: hubVisit\n    expect: { winner: hub.firstEver }\n  \
                   - occasion: talk\n    target: npc.oracle\n";
    write(&dir, "h.play.yaml", halting);
    let project = fixture();
    let args = ["test", dir.to_str().unwrap(), "--project", project.to_str().unwrap()];
    let out = lute(&args);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("exit: expected complete"), "{t}");
    assert!(t.contains("actual incomplete"), "{t}");

    write(
        &dir,
        "h.play.yaml",
        &format!("{halting}expect:\n  exit: incomplete\n"),
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
    let untested: Vec<&str> = v["coverage"]["untested"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    let listed = |f: &str| untested.iter().any(|p| p.ends_with(f));
    assert!(!listed("scenes/hub/first-ever.lute"), "presented: {untested:?}");
    assert!(!listed("scenes/hub/welcome.lute"), "presented: {untested:?}");
    assert!(listed("scenes/hub/idle.lute"), "offered but never presented: {untested:?}");
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
fn offered_and_transcript_lacks_judge_what_the_walk_offered_and_said() {
    let dir = temp_dir("offered");
    write(&dir, "s.lute", ASK);
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nchoose: { ask: plead }\nexpect:\n  offered: { ask: [leave, plead] }\n  \
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
         offered: { ask: [leave, plead] }\n  transcriptLacks: ['demanded.']\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("offered ask: expected [leave, plead], got [demand, leave, plead]"),
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
        lute(&["test", dir.to_str().unwrap(), "--project", project.to_str().unwrap()])
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
    let out = test("entry: nope\nexpect:\n  exit: complete\n");
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
    assert!(t.contains("expect state run.hubVisits: expected 2, actual 1"), "{t}");
}

#[test]
fn a_selection_expect_on_a_non_occasion_step_is_a_usage_error() {
    let out = play(
        "engine-winner",
        "steps:\n  - engine: { state: { run.hubVisits: 1 } }\n    expect: { winner: hub.idle }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("`expect.winner` applies only to an `occasion` or `advance` step"), "{t}");
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
    write(&dir, "gone.test.yaml", "file: nope.lute\nexpect:\n  exit: complete\n");
    // One file runs alone.
    let out = lute(&["test", t_ok.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    // The whole directory: the missing document fails its own test only.
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("E-TEST-FILE"), "{t}");
    assert!(t.contains("1 passed, 1 failed"), "{t}");
    write(&dir, "ok.test.yaml", "file: s.lute\nexpect:\n  notFacts: [met(meg)]\n");
    let out = lute(&["test", dir.join("ok.test.yaml").to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("notFacts met(meg): expected does not hold, got holds"), "{t}");
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
        let out = lute(&["play", project.to_str().unwrap(), "--script", s.to_str().unwrap()]);
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
    assert!(t.contains("skip @hypnos"), "the human transcript still shows the skip: {t}");
    assert!(t.contains("\"Never said.\" absent"), "{t}");
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
    let out = lute(&["test", tests.to_str().unwrap(), "--project", project.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// 0.24.0 T1-13: a hub's `offered:` is the choices eligible at each visit
/// (unioned), as a branch's is — it used to read `[]` for every hub.
#[test]
fn offered_judges_a_hub_like_a_branch() {
    let dir = temp_dir("hub-offered");
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
        "file: s.lute\nchoose: { chat: [a, leave] }\nexpect:\n  offered: { chat: [a, leave] }\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    write(
        &dir,
        "t.test.yaml",
        "file: s.lute\nchoose: { chat: [a, leave] }\nexpect:\n  offered: { chat: [a, never, leave] }\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("got [a, leave]"), "{t}");
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
        write(&dir, "t.test.yaml", &format!("file: {}\n{body}", lore.display()));
        lute(&["test", dir.to_str().unwrap(), "--project", project.to_str().unwrap()])
    };
    let out = test("entry: dusaNote\nexpect:\n  eligible: { dusaNote: false }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(!t.contains("is not eligible under these mocks"), "asserted, so no note: {t}");

    let out = test("entry: megNote\nexpect:\n  eligible: { megNote: true, dusaNote: false }\n");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = test("expect:\n  eligible: { megNote: true, dusaNote: true }\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("eligible dusaNote: expected true, got false"), "{t}");
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
        let out = lute(&["play", project.to_str().unwrap(), "--script", s.to_str().unwrap()]);
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

/// 0.24.0 T3-15: `lute trace --project` settles the "existence is
/// unverified" note of a foreign quest read against the project's quests.
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
    let out = lute(&["trace", scene.to_str().unwrap()]);
    let t = text(&out);
    assert!(t.contains("quest `firstEscape`'s existence is unverified"), "{t}");
    let out = lute(&["trace", scene.to_str().unwrap(), "--project", project.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(!t.contains("existence is unverified"), "{t}");
    assert!(
        t.contains("quest `firstEscap` is declared by no quest document of the project — did you mean `firstEscape`?"),
        "{t}"
    );
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
    write(&project, "plugins/hub.occasions/events/e.yaml", "events:\n  - name: hubVisit\n");
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
    let out = lute(&["play", project.to_str().unwrap(), "--script", s.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("<on event=hubVisit> of quest boss skipped — quest complete"), "{t}");
    assert!(!t.contains("The hall remembers."), "{t}");
}
