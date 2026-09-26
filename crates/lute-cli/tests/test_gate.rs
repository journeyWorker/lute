//! 0.21.1 "no silent wrong answers" — the `lute test` / `lute trace` gate
//! holes the dogfood runs found (T1-13, T1-14, T2-5, T3-11, T3-15). Every
//! test here reproduces a verdict the tool used to get wrong in silence.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-gate-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_at(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A shape-only project manifest (no plugins), so the nearest-manifest rules
/// have a project to find.
const MANIFEST: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

/// A scene whose `<match>` reads a DERIVED relation. Under `derive: false`
/// (dsl 0.22.0 §6) `lute trace` runs no rules, so an unmocked derived atom is
/// unknown and the walk halts there (exit 3) — the line after the match is
/// never reached. By default the rule derives it and the walk completes.
const DERIVED_MATCH: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
     entities:\n  npc: { members: [ana, bo] }\n\
     relations:\n  friend: { args: [npc, npc] }\n  allied: { args: [npc, npc], derive: true }\n\
     state:\n  run.met: { type: bool, default: true }\n\
     facts:\n  - \"friend(ana, bo)\"\n\
     rules:\n  - \"allied(A, B) :- friend(A, B)\"\n---\n\n## One\n\n@narrator: before.\n\
     <match on=\"run.met\">\n<when is=\"true\" test=\"holds(allied(ana, bo))\">\n\
     @narrator: allied.\n</when>\n\
     <otherwise>\n@narrator: apart.\n</otherwise>\n</match>\n@narrator: after.\n";

/// T1-13 + T3-11: an incomplete trace PASSED whenever the test did not
/// mention `exit:` (`{"exit":"incomplete","passed":true}`), and a failing
/// one never said which guard stopped the walk.
#[test]
fn an_incomplete_trace_fails_unless_the_test_declares_it_and_names_what_to_supply() {
    let dir = temp_dir("incomplete");
    write_at(&dir, "s.lute", DERIVED_MATCH);
    write_at(
        &dir,
        "t.test.yaml",
        "file: s.lute\nderive: false\nexpect:\n  transcriptContains: [\"before.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let text = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(1),
        "incomplete must not pass: {text}"
    );
    assert!(text.contains("FAIL"), "{text}");
    assert!(text.contains("exit: incomplete"), "{text}");
    assert!(
        text.contains("expect: { exit: incomplete }"),
        "the opt-in must be named: {text}"
    );
    // T3-11: the halting guard and the test key that would decide it.
    let unresolved = text
        .lines()
        .find(|l| l.contains("unresolved:"))
        .unwrap_or_else(|| panic!("no unresolved line:\n{text}"));
    assert!(unresolved.contains("allied(ana, bo)"), "{unresolved}");
    assert!(
        unresolved.contains("facts: [\"allied(ana, bo)\"]"),
        "hint in the spelling a test file can use: {unresolved}"
    );

    // --json carries the same, machine-readable.
    let out = lute(&["test", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let t = &v["tests"][0];
    assert_eq!(t["passed"], false, "{v:#}");
    assert_eq!(t["exit"], "incomplete", "{v:#}");
    let u = &t["unresolved"][0];
    assert_eq!(u["atoms"][0], "--fact \"allied(ana, bo)\"", "{v:#}");
    assert_eq!(u["supply"][0], "`facts: [\"allied(ana, bo)\"]`", "{v:#}");

    // Declaring the incomplete exit is the opt-in, and it passes.
    write_at(
        &dir,
        "t.test.yaml",
        "file: s.lute\nderive: false\nexpect:\n  exit: incomplete\n  transcriptContains: [\"before.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));

    // `--no-derive` overrides a test that derives.
    write_at(
        &dir,
        "t.test.yaml",
        "file: s.lute\nexpect:\n  exit: incomplete\n  transcriptContains: [\"before.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap(), "--no-derive"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
}

/// dsl 0.22.0 §6: `lute test` applies the rules by default — the derived
/// `allied(ana, bo)` decides the match without being mocked; the walk
/// completes past it.
#[test]
fn a_rule_derived_fact_decides_a_test_guard_without_mocking_it() {
    let dir = temp_dir("derived-guard");
    write_at(&dir, "s.lute", DERIVED_MATCH);
    write_at(
        &dir,
        "t.test.yaml",
        "file: s.lute\nexpect:\n  exit: complete\n  transcriptContains: [\"allied.\", \"after.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
}

/// T1-13: a lore document has nothing to walk without an entry. `lute trace`
/// refuses it, but a test naming it walked nothing and PASSED `exit:
/// complete`. Since dsl 0.22.0 §5 the test names the entries to present;
/// without one it is still refused, and the refusal says how to name them.
#[test]
fn a_lore_document_is_a_test_subject_only_with_its_entries_named() {
    let dir = temp_dir("lore-subject");
    write_at(
        &dir,
        "book.lute",
        "---\nkind: lore\nid: book\n---\n\n<entry id=\"page\" title=\"Page\">\n\
         @narrator: a page.\n</entry>\n",
    );
    write_at(
        &dir,
        "t.test.yaml",
        "file: book.lute\nexpect:\n  exit: complete\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("E-TEST-LORE"), "{text}");
    assert!(
        text.contains("`entry: <id>`"),
        "the way forward is named: {text}"
    );
    assert!(
        text.contains("page"),
        "the declared entries are listed: {text}"
    );

    write_at(
        &dir,
        "t.test.yaml",
        "file: book.lute\nentry: page\nexpect:\n  transcriptContains: [\"a page.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
}

/// T1-13: `lute test tests --coverage` measured the untested set against
/// `tests/`, which holds no `.lute` at all — so it always reported "every
/// testable document is named". The denominator is the project.
#[test]
fn coverage_is_measured_against_the_project_not_the_tests_directory() {
    let dir = temp_dir("coverage-root");
    write_at(&dir, "lute.project.yaml", MANIFEST);
    let scene = |ep: u32| {
        format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: {ep}\n---\n\n## One\n\n\
             @narrator: e{ep}.\n"
        )
    };
    write_at(&dir, "scenes/tested.lute", &scene(1));
    write_at(&dir, "scenes/untested.lute", &scene(2));
    // A lore document is testable since dsl 0.22.0 §5 (`entry:`), so an
    // untested one is listed like any other document.
    write_at(
        &dir,
        "lore/book.lute",
        "---\nkind: lore\nid: book\n---\n\n<entry id=\"page\" title=\"Page\">\n@narrator: a page.\n</entry>\n",
    );
    write_at(
        &dir,
        "tests/t.test.yaml",
        "file: ../scenes/tested.lute\nexpect:\n  exit: complete\n",
    );
    let out = lute(&["test", dir.join("tests").to_str().unwrap(), "--coverage"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("2 untested document"), "{text}");
    assert!(text.contains("untested.lute"), "{text}");
    assert!(!text.contains("every testable document"), "{text}");
    assert!(
        text.contains("book.lute"),
        "lore is testable, so untested: {text}"
    );

    // Naming its entry discharges it.
    write_at(
        &dir,
        "tests/book.test.yaml",
        "file: ../lore/book.lute\nentry: page\nexpect:\n  exit: complete\n",
    );
    let out = lute(&["test", dir.join("tests").to_str().unwrap(), "--coverage"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    let (_, listed) = text
        .split_once("1 untested document")
        .unwrap_or_else(|| panic!("{text}"));
    assert!(!listed.contains("book.lute"), "{text}");
}

/// T2-5: `expect.state` compared only `::set` writes, so a declared default
/// or the test's own seed read "the path was never written".
#[test]
fn expect_state_compares_the_effective_value_default_and_seed_included() {
    let dir = temp_dir("effective-state");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
         user.score: { type: number, default: 0 }\n  run.mood: { type: string, default: calm }\n\
         ---\n\n## One\n\n@narrator: a.\n",
    );
    write_at(
        &dir,
        "default.test.yaml",
        "file: s.lute\nexpect:\n  state:\n    user.score: 0\n",
    );
    write_at(
        &dir,
        "seed.test.yaml",
        "file: s.lute\nstate:\n  run.mood: tense\nexpect:\n  state:\n    run.mood: tense\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("2 passed, 0 failed"), "{text}");

    // The effective value is a real value: a wrong expectation names it.
    write_at(
        &dir,
        "default.test.yaml",
        "file: s.lute\nexpect:\n  state:\n    user.score: 5\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("expected \"5\", got \"0\""), "{text}");
}

/// A beat scene (dsl 0.21.0 §3.1) that answers `arrive` only on day 3.
const BEAT_DAY3: &str = "---\nkind: scene\nid: town.wed\non: arrive\nwhen: 'run.day == 3'\n\
     state:\n  run.day: { type: number, default: 1 }\n---\n\n## Gate\n\n@guard: Wednesday.\n";

/// T1-13: trace ignored the frontmatter `when` of a beat scene, so a
/// Wednesday scene traced (and tested) under Thursday's state read as a plain
/// `complete` with no hint that it would never be presented.
#[test]
fn a_false_beat_when_is_named_by_trace_and_by_test() {
    let dir = temp_dir("beat-when");
    write_at(&dir, "wed.lute", BEAT_DAY3);
    let file = dir.join("wed.lute");

    let out = lute(&["trace", file.to_str().unwrap(), "--state", "run.day=4"]);
    let text = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the walk verdict is unchanged: {text}"
    );
    assert!(
        text.contains("note: beat `when` (run.day == 3) is false under these mocks"),
        "{text}"
    );

    // Under a state where it holds: no note.
    let out = lute(&["trace", file.to_str().unwrap(), "--state", "run.day=3"]);
    assert!(!stdout(&out).contains("beat `when`"), "{}", stdout(&out));

    write_at(
        &dir,
        "t.test.yaml",
        "file: wed.lute\nstate:\n  run.day: 4\nexpect:\n  transcriptContains: [\"Wednesday.\"]\n",
    );
    let out = lute(&["test", dir.to_str().unwrap()]);
    let text = stdout(&out);
    // dsl 0.26.0 §7 (T1-7): the test fails on the ineligible scene, and
    // the failure names the false premise (prerelease N3) — the trace's "the
    // walk below shows it as if it had been presented" note would
    // contradict it and is dropped.
    assert!(
        text.contains("eligible town.wed: not eligible under these mocks (its `when` (run.day == 3) is false)"),
        "{text}"
    );
    assert!(!text.contains("as if it had been presented"), "{text}");
}

/// T1-13: a `choose:` forced past an unknown guard was reported only as
/// `(forced)` on the decision line, and the summary read `trace complete`.
#[test]
fn a_selection_forced_past_an_unknown_guard_is_counted_unresolved() {
    let dir = temp_dir("forced-unknown");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         entities:\n  npc: { members: [ana, bo] }\n\
         relations:\n  friend: { args: [npc, npc] }\n  allied: { args: [npc, npc], derive: true }\n\
         facts:\n  - \"friend(ana, bo)\"\n\
         rules:\n  - \"allied(A, B) :- friend(A, B)\"\n---\n\n## One\n\n\
         <branch id=\"ask\">\n<choice id=\"trust\" label=\"Trust\" when=\"holds(allied(ana, bo))\">\n\
         @narrator: trusted.\n</choice>\n<choice id=\"leave\" label=\"Leave\">\n\
         @narrator: left.\n</choice>\n</branch>\n",
    );
    let file = dir.join("s.lute");
    // `--no-derive` (dsl 0.22.0 §6): the rule is not applied, so the guard
    // over the derived relation stays unknown.
    let out = lute(&[
        "trace",
        file.to_str().unwrap(),
        "--choose",
        "ask=trust",
        "--no-derive",
    ]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "exit unchanged: {text}");
    assert!(
        text.contains("1 unresolved (forced past an unknown guard"),
        "counted in the summary: {text}"
    );
    assert!(text.contains("--fact \"allied(ana, bo)\""), "{text}");

    let out = lute(&[
        "trace",
        file.to_str().unwrap(),
        "--choose",
        "ask=trust",
        "--no-derive",
        "--json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["forcedUnknown"][0]["id"], "ask -> trust", "{v:#}");

    // By default the rule derives `allied(ana, bo)`: nothing is forced.
    let out = lute(&[
        "trace",
        file.to_str().unwrap(),
        "--choose",
        "ask=trust",
        "--json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["forcedUnknown"], serde_json::json!([]), "{v:#}");
}

/// Two scenes of one mystery: `a` asserts the clue, `b` is traced with it
/// mocked.
fn two_scene_case(dir: &Path) {
    let front = |id: &str| {
        format!(
            "---\nkind: scene\nid: {id}\nentities:\n  npc: {{ members: [ann, bo] }}\n\
             relations:\n  clue: {{ args: [npc], tier: run }}\n---\n\n"
        )
    };
    write_at(
        dir,
        "scenes/a.lute",
        &format!(
            "{}## One\n\n@narrator: found it.\n::assert{{clue(ann)}}\n",
            front("case.a")
        ),
    );
    write_at(
        dir,
        "scenes/b.lute",
        &format!("{}## Two\n\n@narrator: thinking.\n", front("case.b")),
    );
}

/// T1-14: `W-TRACE-MOCK-UNPRODUCIBLE` judged a mocked fact against the traced
/// document's own asserts, so every clue a sibling scene establishes was
/// "not producible".
#[test]
fn mock_unproducible_consults_the_project_producers() {
    let dir = temp_dir("unproducible-project");
    write_at(&dir, "lute.project.yaml", MANIFEST);
    two_scene_case(&dir);
    let b = dir.join("scenes/b.lute");

    // `--project`: the sibling's assert counts.
    let out = lute(&[
        "trace",
        b.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--fact",
        "clue(ann)",
    ]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("W-TRACE-MOCK-UNPRODUCIBLE"), "{text}");

    // No flag: the nearest manifest is the project.
    let out = lute(&["trace", b.to_str().unwrap(), "--fact", "clue(ann)"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("W-TRACE-MOCK-UNPRODUCIBLE"), "{text}");
}

/// T1-14's single-file fallback: with no project anywhere, the note still
/// fires but says what it judged.
#[test]
fn mock_unproducible_without_a_project_says_it_judged_one_document() {
    let dir = temp_dir("unproducible-single");
    two_scene_case(&dir);
    let b = dir.join("scenes/b.lute");
    let out = lute(&["trace", b.to_str().unwrap(), "--fact", "clue(ann)"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    let note = text
        .lines()
        .find(|l| l.contains("W-TRACE-MOCK-UNPRODUCIBLE"))
        .unwrap_or_else(|| panic!("no note:\n{text}"));
    assert!(note.contains("this document"), "{note}");
    assert!(note.contains("--project"), "{note}");
}

/// T3-15: `scenario envelope` printed the Possible table as the full set, so
/// every Guaranteed path appeared twice.
#[test]
fn envelope_possible_lists_only_what_is_not_guaranteed() {
    let dir = temp_dir("envelope-diff");
    write_at(
        &dir,
        "a.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n\
         state:\n  run.a: { type: number }\n---\n## Shot 1.\n::set{run.a = 1}\n",
    );
    write_at(
        &dir,
        "b.lute",
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n\
         after: 'visited(\"a.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );
    let out = lute(&["scenario", dir.to_str().unwrap(), "envelope", "b.s01ep01"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    let (guaranteed, rest) = text
        .split_once("Guaranteed (safe to read")
        .unwrap()
        .1
        .split_once("Possible (set on")
        .unwrap_or_else(|| panic!("no Possible heading:\n{text}"));
    assert!(guaranteed.contains("run.a"), "{text}");
    let possible = rest.split_once("Guaranteed facts").unwrap().0;
    assert!(
        !possible.contains("run.a"),
        "a guaranteed path is not repeated under Possible:\n{text}"
    );
}

/// T3-15: `lute scenario … | head` panicked in `println!` once the reader
/// went away. Close the read end before the report is written.
#[test]
fn scenario_and_test_survive_a_closed_stdout() {
    let dir = temp_dir("epipe");
    write_at(
        &dir,
        "a.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n",
    );
    write_at(
        &dir,
        "t.test.yaml",
        "file: a.lute\nexpect:\n  exit: complete\n",
    );
    for args in [
        vec!["scenario", dir.to_str().unwrap()],
        vec!["test", dir.to_str().unwrap()],
        vec!["lore", dir.to_str().unwrap()],
    ] {
        let mut child = Command::new(BIN)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!err.contains("panicked"), "{args:?}: {err}");
        assert_ne!(out.status.code(), Some(101), "{args:?}: {err}");
    }
}
