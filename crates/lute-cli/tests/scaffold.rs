//! `lute init --template beats|investigation` / `lute new` (dsl 0.22.0 §13).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-scaffold-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn init_template(tag: &str, template: &str) -> PathBuf {
    let proj = temp_dir(tag).join("proj");
    let out = lute(&["init", proj.to_str().unwrap(), "--template", template]);
    assert!(out.status.success(), "{}", text(&out));
    proj
}

fn init_beats(tag: &str) -> PathBuf {
    init_template(tag, "beats")
}

/// `check-project`, `test` and every `plays/*.play.yaml` pass as scaffolded.
fn assert_checks_tests_and_plays_clean(proj: &Path) {
    let p = proj.to_str().unwrap();
    let check = lute(&["check-project", p]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(!text(&check).contains("warning ["), "no advisory at all: {}", text(&check));
    let test = lute(&["test", p, "--project", p]);
    assert_eq!(test.status.code(), Some(0), "{}", text(&test));
    let plays: Vec<PathBuf> = std::fs::read_dir(proj.join("plays"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!plays.is_empty(), "a play script is scaffolded");
    for script in plays {
        let play = lute(&["play", p, "--script", script.to_str().unwrap()]);
        assert_eq!(play.status.code(), Some(0), "{}", text(&play));
        assert!(text(&play).contains("every expectation held"), "{}", text(&play));
    }
}

/// The beats scaffold passes `check-project`, `test` and `play` out of the
/// box — the three commands its README and play script promise.
#[test]
fn beats_template_checks_tests_and_plays_clean() {
    let proj = init_beats("beats");
    for rel in [
        "plugins/game.occasions/plugin.yaml",
        "plays/first-day.play.yaml",
        "tests/mara-first.test.yaml",
    ] {
        assert!(proj.join(rel).is_file(), "missing {rel}");
    }
    assert_checks_tests_and_plays_clean(&proj);
}

/// T3-14: `investigation` is rebuilt on the beats skeleton — manifest
/// `defaults:`, an occasions plugin with the targeted `examine`/`interview`,
/// evidence lore that `::assert`s, a stratified `not` rule, an accusation
/// guarded by derived facts — with no dsl-0.3 identity frontmatter, and it
/// checks, tests and plays clean.
#[test]
fn investigation_template_is_current_and_checks_tests_and_plays_clean() {
    let proj = init_template("investigation", "investigation");
    let read = |rel: &str| std::fs::read_to_string(proj.join(rel)).unwrap();
    assert!(read("lute.project.yaml").contains("defaults:"));
    let occasions = read("plugins/case.occasions/occasions/case.yaml");
    for occ in ["examine:", "interview:", "target: { prefix: item", "target: { prefix: npc"] {
        assert!(occasions.contains(occ), "{occ}: {occasions}");
    }
    assert!(read("world.schema.yaml").contains(", not "), "a negated rule body");
    assert!(read("lore/evidence.lute").contains("::assert{"));
    assert!(read("scenes/accusation.lute").contains("when=\"holds(culprit("));
    for rel in ["scenes/case/arrival.lute", "scenes/accusation.lute", "quests/case.lute"] {
        let doc = read(rel);
        for legacy in ["character:", "season:", "episode:", "start=\"true\""] {
            assert!(!doc.contains(legacy), "{rel} carries legacy `{legacy}`: {doc}");
        }
    }
    assert!(!proj.join("mocks").exists(), "no dsl-0.4 trace mock");
    assert_checks_tests_and_plays_clean(&proj);
}

fn scene(proj: &Path, rel: &str) -> String {
    std::fs::read_to_string(proj.join("scenes").join(rel)).unwrap()
}

/// `lute new scene --on` writes a beat with an `id:`, omits what the
/// manifest's `defaults:` supplies, nests under `scenes/` by the name's `/`,
/// and still checks clean.
#[test]
fn new_scene_on_writes_a_beat_that_respects_defaults() {
    let proj = init_beats("new-on");
    let out = lute(&[
        "new",
        "scene",
        "talk/tomas-first",
        "--on",
        "talk",
        "--target",
        "npc.tomas",
        "--dir",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let beat = scene(&proj, "talk/tomas-first.lute");
    assert!(beat.contains("\nid: talk.tomasFirst\n"), "{beat}");
    assert!(beat.contains("\non: talk\ntarget: npc.tomas\n"), "{beat}");
    assert!(!beat.contains("luteVersion") && !beat.contains("uses:"), "{beat}");
    assert!(!beat.contains("character:"), "no legacy identity triple: {beat}");

    let check = lute(&["check-project", proj.to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
}

/// T3-14: `--dir` names the PROJECT. A directory inside a project that is not
/// its root is refused (exit 2, nothing written) with the `<sub>/<name>`
/// spelling that would put the document where the author pointed — instead
/// of silently writing `<root>/scenes/<name>.lute`.
#[test]
fn new_with_a_non_root_dir_inside_a_project_is_refused_with_the_nested_name() {
    let proj = init_beats("new-nested-dir");
    let sub = proj.join("scenes/talk");
    let out = lute(&[
        "new", "scene", "tavi-shell", "--on", "talk", "--dir", sub.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let msg = text(&out);
    assert!(
        msg.contains("`--dir` names the project; did you mean `lute new scene talk/tavi-shell --dir "),
        "{msg}"
    );
    assert!(!proj.join("scenes/tavi-shell.lute").exists(), "nothing lands at the root");
    assert!(!sub.join("tavi-shell.lute").exists());

    // Run from the project root with a relative `--dir`: no `--dir` needed.
    let out = Command::new(BIN)
        .args(["new", "quest", "side", "--dir", "quests/extra"])
        .current_dir(&proj)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("did you mean `lute new quest extra/side`?"),
        "{}",
        text(&out)
    );
    assert!(!proj.join("quests/side.lute").exists());
    assert!(!proj.join("quests/extra").exists());
}

/// T3-14: a dotted name keeps its dots as the id (`isolde.night` →
/// `id: isolde.night`, not the camelCase `isoldeNight`), the file keeps the
/// typed name, and the document checks clean.
#[test]
fn new_scene_with_a_dotted_name_keeps_the_dotted_id() {
    let proj = init_beats("new-dotted");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "scene", "mara.night", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let beat = scene(&proj, "mara.night.lute");
    assert!(beat.contains("\nid: mara.night\n"), "{beat}");

    let out = lute(&["new", "quest", "lamp.extra-oil", "--dir", d, "--start"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let quest = std::fs::read_to_string(proj.join("quests/lamp.extra-oil.lute")).unwrap();
    assert!(quest.contains("\nid: quest.lamp.extraOil\n"), "{quest}");

    let check = lute(&["check-project", d]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(!text(&check).contains("warning ["), "{}", text(&check));
}

/// T3-14: `lute new quest` scaffolds an accept-driven stub (no `start`) whose
/// comment tells the author to `::accept` it; `--start` restores the
/// auto-starting form. Both check without error; once content accepts the
/// stub, the project checks with no advisory at all.
#[test]
fn new_quest_is_accept_driven_unless_start() {
    let proj = init_beats("new-quest-accept");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "quest", "oil-run", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let stub = std::fs::read_to_string(proj.join("quests/oil-run.lute")).unwrap();
    assert!(stub.contains("<quest id=\"oilRun\" title=\"oil-run\">"), "{stub}");
    assert!(!stub.contains("start="), "{stub}");
    assert!(stub.contains("::accept{quest=\"oilRun\"}"), "the stub says how it starts: {stub}");
    let check = lute(&["check-project", d]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));

    let out = lute(&["new", "quest", "always-on", "--start", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let auto = std::fs::read_to_string(proj.join("quests/always-on.lute")).unwrap();
    assert!(auto.contains("<quest id=\"alwaysOn\" title=\"always-on\" start=\"true\">"), "{auto}");

    // Accept the stub where the player takes it on: the project is clean.
    let welcome = proj.join("scenes/hub/welcome.lute");
    let mut text_ = std::fs::read_to_string(&welcome).unwrap();
    text_.push_str("::accept{quest=\"oilRun\"}\n");
    std::fs::write(&welcome, text_).unwrap();
    let check = lute(&["check-project", d]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(!text(&check).contains("warning ["), "{}", text(&check));

    // `--start` belongs to quests only.
    let out = lute(&["new", "scene", "x", "--start", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
}

/// An undeclared occasion or an out-of-domain target is refused (exit 2)
/// and leaves no file behind.
#[test]
fn new_scene_on_refuses_what_the_project_does_not_declare() {
    let proj = init_beats("new-refuse");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "scene", "x", "--on", "tallk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("did you mean `talk`?"), "{}", text(&out));
    assert!(!proj.join("scenes/x.lute").exists());

    let out = lute(&["new", "scene", "y", "--on", "talk", "--target", "npc.oskar", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("npc.oskar"), "{}", text(&out));
    assert!(!proj.join("scenes/y.lute").exists());

    let out = lute(&["new", "scene", "z", "--on", "hubVisit", "--target", "npc.mara", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(!proj.join("scenes/z.lute").exists());
}

/// Outside a project `lute new` says so; `--on` has nothing to answer there
/// and is refused.
#[test]
fn new_outside_a_project_says_so() {
    let dir = temp_dir("new-outside");
    let d = dir.to_str().unwrap();
    let out = lute(&["new", "scene", "intro", "--on", "talk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("not inside a Lute project"), "{}", text(&out));
    assert!(!dir.join("scenes/intro.lute").exists());

    let out = lute(&["new", "scene", "intro", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not inside a Lute project"),
        "{}",
        text(&out)
    );
    let intro = scene(&dir, "intro.lute");
    assert!(intro.contains("\nid: intro\n") && intro.contains("luteVersion:"), "{intro}");
}
