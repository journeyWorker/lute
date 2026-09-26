//! 0.27 prerelease diagnostics: what a message names, how it is labelled and
//! where it points — each test pins one observable wording or count a writer
//! acts on.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-diag027-{tag}-{}-{n}", std::process::id()));
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

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// ML-F9: a refused test trace lists the check's diagnostics with their own
/// severity (a stale-version warning is not an `error`), under the document's
/// folded path (`lore/a.lute`, not `tests/sub/../../lore/a.lute`).
#[test]
fn a_refused_test_trace_keeps_each_diagnostics_severity() {
    let dir = temp_dir("refused");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: base\nprofiles:\n  base: { plugins: {} }\n\
         defaults:\n  luteVersion: \"0.1.0\"\n",
    );
    write(
        &dir,
        "lore/a.lute",
        "---\nkind: lore\nid: a\n---\n\n<entry id=\"e\">\n@narrator{when=\"@nope\"}: Hi.\n</entry>\n",
    );
    write(
        &dir,
        "tests/sub/a.test.yaml",
        "file: ../../lore/a.lute\nentry: e\nexpect:\n  exit: complete\n",
    );
    let out = run(&dir, &["test", "tests/sub/a.test.yaml", "--project", "."]);
    let t = text(&out);
    assert!(t.contains("trace refused:"), "{t}");
    assert!(
        t.contains("lore/a.lute:1:1: warning [W-LUTE-VERSION-STALE]"),
        "{t}"
    );
    assert!(t.contains("error [E-UNDECLARED-REF]"), "{t}");
    assert!(!t.contains(".."), "the document path is folded: {t}");
}

const MANIFEST: &str = "defaultProfile: base\nprofiles:\n  base: { plugins: {} }\n\
                        defaults:\n  uses: [world.schema.yaml]\n";

fn check_project(dir: &Path) -> String {
    text(&run(dir, &["check-project", "."]))
}

/// ML-F4: a `<match on>` over a `count(…)` def names the fix — a `<match>`
/// with no `on` whose arms compare the count.
#[test]
fn a_fact_query_match_subject_shows_the_subjectless_form() {
    let dir = temp_dir("relsubject");
    write(&dir, "lute.project.yaml", MANIFEST);
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  badge: { members: [stone, tide] }\nrelations:\n  hasBadge: { args: [badge] }\n\
         defs:\n  badgeCount: \"count(hasBadge(_))\"\n",
    );
    write(
        &dir,
        "scenes/gate.lute",
        "---\nkind: scene\nid: gate\ntitle: Gate\n---\n\n## Gate\n\n<match on=\"@badgeCount\">\n\
         \x20 <when is=\"2..\">\n    @narrator: Two.\n  </when>\n  <otherwise>\n    @narrator: Fewer.\n  </otherwise>\n</match>\n",
    );
    let t = check_project(&dir);
    assert!(
        t.contains("[E-MATCH-RELATION-SUBJECT] `@badgeCount` expands to a `count(…)` fact query"),
        "{t}"
    );
    assert!(
        t.contains("drop `on`") && t.contains("<when test=\"@badgeCount >= 1\">"),
        "{t}"
    );
}

/// A small project with an `effects: true` component and a `run.route` enum.
fn component_project(tag: &str, use_line: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        &format!("{MANIFEST}  components: [components/ending.component.lute]\n"),
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.route: { type: { enum: [none, ren, kai] }, default: none }\n  \
         run.ending: { type: { enum: [none, good, bad] }, default: none }\n\
         defs:\n  isNight: \"run.route == 'kai'\"\n",
    );
    write(
        &dir,
        "components/ending.component.lute",
        "---\ncomponent: ending\neffects: true\nparams:\n  kind: { type: { enum: [none, good, bad] } }\n  \
         intro: string\n---\n\n## Record\n\n::set{run.ending = @kind}\n@narrator: {{@intro}}\n",
    );
    write(
        &dir,
        "scenes/end.lute",
        &format!(
            "---\nkind: scene\nid: end\ntitle: End\n---\n\n## End\n\n<branch id=\"pick\">\n  \
             <choice id=\"a\" label=\"A\" into=\"run.route\" value=\"kia\">\n    @narrator: A.\n  </choice>\n\
             \x20 <choice id=\"b\" label=\"B\">\n    @narrator: B.\n  </choice>\n</branch>\n\n{use_line}\n"
        ),
    );
    dir
}

/// OT-F-11 / ML-F6: argument faults name the value and the nearest fix, once.
#[test]
fn argument_faults_name_the_value_and_the_nearest_fix_once() {
    let dir = component_project(
        "args",
        "::use{component=\"ending\" kind=\"goood\" intor=\"Hi\" when=\"!@isNigth\"}",
    );
    let t = check_project(&dir);
    assert!(
        t.contains("[E-INTO-VALUE] `value=\"kia\"` cannot be recorded into `run.route`")
            && t.contains("did you mean `kai`?"),
        "{t}"
    );
    assert!(
        t.contains("has no parameter `intor` — did you mean `intro`?"),
        "{t}"
    );
    assert!(
        !t.contains("requires argument `intro`"),
        "the typo explains it: {t}"
    );
    assert!(
        t.contains("`@isNigth` is not a declared def — did you mean `@isNight`?"),
        "{t}"
    );
    assert!(
        t.contains("argument `kind=\"goood\"`") && t.contains("did you mean `good`?"),
        "{t}"
    );
    assert!(
        !t.contains("[E-SET-TYPE]"),
        "the spliced write repeats the argument fault: {t}"
    );
}

/// OT-F-11: a `<when is>` literal outside the subject's enum names the
/// nearest member.
#[test]
fn a_when_is_literal_gets_a_did_you_mean() {
    let dir = component_project("whenis", "");
    write(
        &dir,
        "scenes/end.lute",
        "---\nkind: scene\nid: end\ntitle: End\n---\n\n## End\n\n<match on=\"run.ending\">\n  \
         <when is=\"god\">\n    @narrator: G.\n  </when>\n  <otherwise>\n    @narrator: O.\n  </otherwise>\n</match>\n",
    );
    let t = check_project(&dir);
    assert!(
        t.contains("[E-WHEN-LITERAL-DOMAIN]") && t.contains("did you mean `good`?"),
        "{t}"
    );
}

/// ML-F3: a parent whose objectives are run-looking subquests is flagged
/// with them, and each warning names the tree that must change together.
#[test]
fn an_implicit_quest_tree_is_flagged_together() {
    let dir = temp_dir("tier");
    write(&dir, "lute.project.yaml", MANIFEST);
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.fish: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q\ntitle: Q\n---\n\n\
         <quest id=\"one\" title=\"One\" start=\"true\">\n  <objective id=\"a\" title=\"A\" done=\"run.fish >= 1\"/>\n</quest>\n\n\
         <quest id=\"lessons\" title=\"Lessons\" start=\"true\">\n  <objective id=\"b\" title=\"B\" quest=\"one\"/>\n</quest>\n",
    );
    let t = check_project(&dir);
    let warned: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-QUEST-TIER-IMPLICIT]"))
        .collect();
    assert_eq!(warned.len(), 2, "{t}");
    assert!(
        warned
            .iter()
            .any(|l| l.contains("quest `lessons`") && l.contains("on `one` together")),
        "{t}"
    );
}

/// League R1: a quest that reads nothing (`done="true"`) takes no side —
/// its run-looking sibling and their parent are still flagged, and it is
/// named in the tree that must change together.
#[test]
fn a_quest_reading_nothing_does_not_hide_its_run_looking_tree() {
    let dir = temp_dir("tier-neutral");
    write(&dir, "lute.project.yaml", MANIFEST);
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.fish: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q\ntitle: Q\n---\n\n\
         <quest id=\"gardens\" title=\"Gardens\" start=\"true\">\n  \
         <objective id=\"seeds\" title=\"Seeds\" quest=\"seeds\"/>\n  \
         <objective id=\"bees\" title=\"Bees\" quest=\"bees\"/>\n</quest>\n\n\
         <quest id=\"seeds\" title=\"Seeds\" start=\"true\">\n  \
         <objective id=\"deliver\" title=\"Deliver\" done=\"true\"/>\n</quest>\n\n\
         <quest id=\"bees\" title=\"Bees\" start=\"true\">\n  \
         <objective id=\"three\" title=\"Three\" done=\"run.fish >= 3\"/>\n</quest>\n\n\
         <quest id=\"idle\" title=\"Idle\" start=\"true\">\n  \
         <objective id=\"x\" title=\"X\" done=\"true\"/>\n</quest>\n",
    );
    let t = check_project(&dir);
    let warned: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-QUEST-TIER-IMPLICIT]"))
        .collect();
    assert_eq!(warned.len(), 3, "gardens, seeds, bees — not idle: {t}");
    assert!(
        warned
            .iter()
            .any(|l| l.contains("quest `bees`") && l.contains("on `gardens`, `seeds` together")),
        "{t}"
    );
    assert!(
        warned
            .iter()
            .any(|l| l.contains("quest `seeds`") && l.contains("it reads no state itself")),
        "{t}"
    );
}
