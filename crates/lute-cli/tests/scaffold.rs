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
    assert!(
        !text(&check).contains("warning ["),
        "no advisory at all: {}",
        text(&check)
    );
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
        assert!(
            text(&play).contains("every expectation held"),
            "{}",
            text(&play)
        );
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
    for occ in [
        "examine:",
        "interview:",
        "target: { prefix: item",
        "target: { prefix: npc",
    ] {
        assert!(occasions.contains(occ), "{occ}: {occasions}");
    }
    assert!(
        read("world.schema.yaml").contains(", not "),
        "a negated rule body"
    );
    assert!(read("lore/evidence.lute").contains("::assert{"));
    assert!(read("scenes/accusation.lute").contains("when=\"holds(culprit("));
    for rel in [
        "scenes/case/arrival.lute",
        "scenes/accusation.lute",
        "quests/case.lute",
    ] {
        let doc = read(rel);
        for legacy in ["character:", "season:", "episode:", "start=\"true\""] {
            assert!(
                !doc.contains(legacy),
                "{rel} carries legacy `{legacy}`: {doc}"
            );
        }
    }
    assert!(!proj.join("mocks").exists(), "no dsl-0.4 trace mock");
    assert_checks_tests_and_plays_clean(&proj);
}

/// Round-5 D-2/D-4: the `minimal` starter scene uses `id:` identity (as `lute
/// new scene` does, so a fresh project never mixes the two conventions), and
/// the template ships a `tests/` test that passes — the first thing a writer
/// runs after `check-project`. The test names its scene `../scenes/…`, the
/// path a test in `tests/` needs.
#[test]
fn minimal_template_uses_id_identity_and_ships_a_passing_test() {
    let proj = init_template("minimal", "minimal");
    let p = proj.to_str().unwrap();
    let opening = std::fs::read_to_string(proj.join("scenes/opening.lute")).unwrap();
    assert!(opening.contains("\nid: opening\n"), "{opening}");
    for legacy in ["character:", "season:", "episode:"] {
        assert!(!opening.contains(legacy), "legacy `{legacy}`: {opening}");
    }
    let test_file = std::fs::read_to_string(proj.join("tests/opening.test.yaml")).unwrap();
    assert!(
        test_file.contains("file: ../scenes/opening.lute"),
        "{test_file}"
    );

    let check = lute(&["check-project", p]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(!text(&check).contains("warning ["), "{}", text(&check));

    let test = lute(&["test", p, "--project", p]);
    assert_eq!(test.status.code(), Some(0), "{}", text(&test));
    assert!(
        text(&test).contains("1 passed, 0 failed"),
        "{}",
        text(&test)
    );

    // A test that asserts something false fails, so the passing one is real.
    std::fs::write(
        proj.join("tests/opening.test.yaml"),
        test_file.replace("run.greeted: true", "run.greeted: false"),
    )
    .unwrap();
    let test = lute(&["test", p, "--project", p]);
    assert_eq!(test.status.code(), Some(1), "{}", text(&test));
}

fn scene(proj: &Path, rel: &str) -> String {
    std::fs::read_to_string(proj.join("scenes").join(rel)).unwrap()
}

/// `lute new scene --occasion` writes a beat with an `id:`, omits what the
/// manifest's `defaults:` supplies, nests under `scenes/` by the name's `/`,
/// names the file after the id, and still checks clean.
#[test]
fn new_scene_on_writes_a_beat_that_respects_defaults() {
    let proj = init_beats("new-on");
    let out = lute(&[
        "new",
        "scene",
        "talk/tomas-first",
        "--occasion",
        "talk",
        "--target",
        "npc.tomas",
        "--dir",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!proj.join("scenes/talk/tomas-first.lute").exists());
    let beat = scene(&proj, "talk/tomasFirst.lute");
    assert!(beat.contains("\nid: talk.tomasFirst\n"), "{beat}");
    assert!(beat.contains("\non: talk\ntarget: npc.tomas\n"), "{beat}");
    assert!(
        !beat.contains("luteVersion") && !beat.contains("uses:"),
        "{beat}"
    );
    assert!(
        !beat.contains("character:"),
        "no legacy identity triple: {beat}"
    );

    let check = lute(&["check-project", proj.to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
}

/// `lute new scene --occasion` writes a `priority:` below every beat already
/// on the occasion, so stubs scaffolded one after another rank in creation
/// order and never tie.
#[test]
fn new_scene_on_ranks_below_every_existing_beat_of_the_occasion() {
    let proj = init_beats("new-on-priority");
    let d = proj.to_str().unwrap();
    let priority = |rel: &str| -> i64 {
        let s = scene(&proj, rel);
        let line = s
            .lines()
            .find_map(|l| l.strip_prefix("priority: "))
            .unwrap_or_else(|| panic!("no priority: {s}"));
        line.split_whitespace().next().unwrap().parse().unwrap()
    };
    for name in ["stub-a", "stub-b", "stub-c"] {
        let out = lute(&[
            "new",
            "scene",
            name,
            "--occasion",
            "talk",
            "--target",
            "npc.mara",
            "--dir",
            d,
        ]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    }
    let (a, b, c) = (
        priority("stubA.lute"),
        priority("stubB.lute"),
        priority("stubC.lute"),
    );
    assert!(a > b && b > c, "{a} {b} {c}");
    let check = lute(&["check-project", d]);
    let t = text(&check);
    assert!(!t.contains("W-BEAT-PRIORITY-TIE"), "{t}");
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
        "new",
        "scene",
        "tavi-shell",
        "--occasion",
        "talk",
        "--target",
        "npc.mara",
        "--dir",
        sub.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let msg = text(&out);
    assert!(
        msg.contains("`--dir` names the project, not the destination folder"),
        "{msg}"
    );
    assert!(msg.contains("resolves to `"), "{msg}");
    assert!(
        msg.contains("did you mean `lute new scene talk/tavi-shell --dir "),
        "{msg}"
    );
    assert!(
        !proj.join("scenes/tavi-shell.lute").exists(),
        "nothing lands at the root"
    );
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

/// Round-3 CR N7: run from a project subdirectory with no `--dir`, the
/// refusal names the current directory it resolved and how, and never
/// blames a `--dir` the author did not pass.
#[test]
fn new_from_a_subdirectory_without_dir_names_the_current_directory() {
    let proj = init_beats("new-subdir-cwd");
    let sub = proj.join("scenes/talk");
    let out = Command::new(BIN)
        .args([
            "new",
            "scene",
            "tavi-shell",
            "--occasion",
            "talk",
            "--target",
            "npc.mara",
        ])
        .current_dir(&sub)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let msg = text(&out);
    assert!(
        msg.contains("no `--dir` was given, so `lute new` started from the current directory `"),
        "{msg}"
    );
    assert!(
        msg.contains("run from the project root, or pass `--dir <root>`"),
        "{msg}"
    );
    assert!(!msg.contains("`--dir` names the project"), "{msg}");
    assert!(
        msg.contains("did you mean `lute new scene talk/tavi-shell --dir "),
        "{msg}"
    );
    assert!(!sub.join("tavi-shell.lute").exists());
}

/// A dotted name keeps its dots as the id (`isolde.night` → `id:
/// isolde.night`, not the camelCase `isoldeNight`), the file is named after
/// the id, a quest's document id gets no `quest.` prefix, and the document
/// checks clean.
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
    let quest = std::fs::read_to_string(proj.join("quests/lamp.extraOil.lute")).unwrap();
    assert!(quest.contains("\nid: lamp.extraOil\n"), "{quest}");

    // The case typed is kept: `harborNight` stays, a phrase joins in camel case.
    let out = lute(&["new", "scene", "isles.harborNight", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(scene(&proj, "isles.harborNight.lute").contains("\nid: isles.harborNight\n"));
    let out = lute(&["new", "scene", "The Epilogue", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(scene(&proj, "theEpilogue.lute").contains("\nid: theEpilogue\n"));

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
    let stub = std::fs::read_to_string(proj.join("quests/oilRun.lute")).unwrap();
    assert!(
        stub.contains("<quest id=\"oilRun\" title=\"Oil Run\">"),
        "{stub}"
    );
    assert!(!stub.contains("start="), "{stub}");
    assert!(
        stub.contains("::accept{quest=\"oilRun\"}"),
        "the stub says how it starts: {stub}"
    );
    let check = lute(&["check-project", d]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));

    let out = lute(&["new", "quest", "always-on", "--start", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let auto = std::fs::read_to_string(proj.join("quests/alwaysOn.lute")).unwrap();
    assert!(
        auto.contains("<quest id=\"alwaysOn\" title=\"Always On\" start=\"true\">"),
        "{auto}"
    );

    // Accept the stub where the player takes it on: the project is clean.
    let welcome = proj.join("scenes/town/welcome.lute");
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

/// Round-5 FS-F11: in a `lute init` project (no manifest `defaults:`),
/// `lute new quest` and `lute new lore` head their documents exactly like
/// `lute new scene` — the same `uses:` of the project schemas — invent no
/// document-local state, and title the document in Title Case. The quest's
/// objective reads the project (a scene it can see), and the project checks
/// without error. Document ids carry no `quest.`/`lore.` prefix, so they
/// share one namespace with scenes: a name another document's id already
/// takes is refused, and nothing is written.
#[test]
fn new_quest_and_lore_share_the_scene_head_and_invent_no_state() {
    let proj = temp_dir("new-agree").join("proj");
    let out = lute(&["init", proj.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    let d = proj.to_str().unwrap();
    for (kind, name) in [
        ("scene", "the-cellar"),
        ("quest", "cellar-key"),
        ("lore", "old-map"),
    ] {
        let out = lute(&["new", kind, name, "--dir", d]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    }
    let read = |rel: &str| std::fs::read_to_string(proj.join(rel)).unwrap();
    let uses = |doc: &str| {
        doc.lines()
            .skip_while(|l| !l.starts_with("uses:"))
            .take_while(|l| *l != "---")
            .filter(|l| !l.starts_with('#'))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let scene = read("scenes/theCellar.lute");
    let quest = read("quests/cellarKey.lute");
    let lore = read("lore/oldMap.lute");
    assert!(!uses(&scene).is_empty(), "{scene}");
    assert_eq!(uses(&quest), uses(&scene), "{quest}");
    assert_eq!(uses(&lore), uses(&scene), "{lore}");
    assert!(!quest.contains("state:"), "no invented counter: {quest}");
    assert!(scene.contains("\ntitle: The Cellar\n"), "{scene}");
    assert!(quest.contains("\ntitle: Cellar Key\n"), "{quest}");
    assert!(quest.contains("title=\"Cellar Key\""), "{quest}");
    assert!(lore.contains("\ntitle: Old Map\n"), "{lore}");
    assert!(
        quest.contains("\nid: cellarKey\n"),
        "no `quest.` prefix: {quest}"
    );
    assert!(lore.contains("\nid: oldMap\n"), "no `lore.` prefix: {lore}");

    for (kind, name) in [("quest", "the-cellar"), ("lore", "cellar-key")] {
        let out = lute(&["new", kind, name, "--dir", d]);
        assert_eq!(out.status.code(), Some(2), "{}", text(&out));
        assert!(
            text(&out).contains("already declares `id: "),
            "{}",
            text(&out)
        );
    }
    assert!(!proj.join("quests/theCellar.lute").exists());
    assert!(!proj.join("lore/cellarKey.lute").exists());
    assert!(lore.contains("\nid: oldMap\n"), "no `lore.` prefix: {lore}");

    let check = lute(&["check-project", d]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(!text(&check).contains("error ["), "{}", text(&check));
}

/// An undeclared occasion, an out-of-domain target, a target on an occasion
/// not raised for one, and a targeted occasion without `--target` are all
/// refused (exit 2) and leave no file behind.
#[test]
fn new_scene_on_refuses_what_the_project_does_not_declare() {
    let proj = init_beats("new-refuse");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "scene", "x", "--occasion", "tallk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("did you mean `talk`?"),
        "{}",
        text(&out)
    );
    assert!(!proj.join("scenes/x.lute").exists());

    let out = lute(&[
        "new",
        "scene",
        "y",
        "--occasion",
        "talk",
        "--target",
        "npc.oskar",
        "--dir",
        d,
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("npc.oskar"), "{}", text(&out));
    assert!(!proj.join("scenes/y.lute").exists());

    let out = lute(&[
        "new",
        "scene",
        "z",
        "--occasion",
        "townVisit",
        "--target",
        "npc.mara",
        "--dir",
        d,
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(!proj.join("scenes/z.lute").exists());

    // `talk` is raised for a target: a scene without one would play for all.
    let out = lute(&["new", "scene", "w", "--occasion", "talk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("occasion `talk` is raised for a target")
            && text(&out).contains("pass `--target npc."),
        "{}",
        text(&out)
    );
    assert!(!proj.join("scenes/w.lute").exists());

    // `--target` alone names no occasion.
    let out = lute(&["new", "scene", "v", "--target", "npc.mara", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("--occasion <OCCASION> --target npc.mara"),
        "{}",
        text(&out)
    );
    assert!(!proj.join("scenes/v.lute").exists());
}

/// The old `--on` spelling is refused, naming `--occasion`, and writes
/// nothing.
#[test]
fn new_scene_refuses_the_old_on_flag() {
    let proj = init_beats("new-old-on");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "scene", "x", "--on", "townVisit", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("`--on` is now `--occasion`")
            && text(&out).contains("lute new scene x --occasion townVisit"),
        "{}",
        text(&out)
    );
    assert!(!proj.join("scenes/x.lute").exists());
}

/// Outside a project `lute new` says so; `--occasion` has nothing to answer
/// there and is refused.
#[test]
fn new_outside_a_project_says_so() {
    let dir = temp_dir("new-outside");
    let d = dir.to_str().unwrap();
    let out = lute(&["new", "scene", "intro", "--occasion", "talk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("not inside a Lute project"),
        "{}",
        text(&out)
    );
    assert!(!dir.join("scenes/intro.lute").exists());

    let out = lute(&["new", "scene", "intro", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not inside a Lute project"),
        "{}",
        text(&out)
    );
    let intro = scene(&dir, "intro.lute");
    assert!(
        intro.contains("\nid: intro\n") && intro.contains("luteVersion:"),
        "{intro}"
    );
}
