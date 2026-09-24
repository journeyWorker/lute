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
    assert!(t.contains("notOffered, offered, presented, winner"), "{t}");
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

    // Twice in a row: the second presentation is a re-read.
    let out = test(
        "entries: [megNote, megNote]\nexpect:\n  \
         transcriptContains: ['first read', 're-read: effects skipped']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // `entriesRead:` seeds the save: the one presentation is already a re-read.
    let out = test(
        "entry: megNote\nentriesRead: { run: [megNote] }\nexpect:\n  \
         transcriptLacks: ['first read']\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // An unknown entry id is refused, in the test's own spelling.
    let out = test("entry: nope\nexpect:\n  exit: complete\n");
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("E-TRACE-ENTRY"), "{t}");
    assert!(t.contains("`entry: nope`"), "{t}");
}
