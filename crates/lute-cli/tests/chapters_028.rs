//! The manifest's `chapters:` through the built `lute` binary: several
//! chains, a chain on a targeted occasion, where a derived `after:` is
//! credited, what a rejected chain tells the scenes it lists, which `when:`
//! can stall a chain, and what listing something that is not a scene says.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-ch028-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let o = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        o.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

/// A project with the `demo` occasions plugin (`occasions` is its YAML
/// body), `schema` as its one shared schema, and `tail` appended to the
/// manifest.
fn project(dir: &Path, occasions: &str, schema: &str, tail: &str) {
    write(
        dir,
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: {{ demo: true }}\n\
             defaults:\n  uses: [world.schema.yaml]\n{tail}"
        ),
    );
    write(
        dir,
        "plugins/demo/plugin.yaml",
        "id: demo\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(dir, "plugins/demo/occasions/o.yaml", occasions);
    write(dir, "world.schema.yaml", schema);
}

/// A scene `id` with extra frontmatter `front` (each line `\n`-terminated).
fn scene(id: &str, front: &str) -> String {
    format!("---\nkind: scene\nid: {id}\ntitle: {id}\n{front}---\n\n## {id}\n\n@narrator: This is {id}.\n")
}

const CHAPTER: &str = "occasions:\n  chapter: { select: first, description: the next chapter }\n  \
                       stage: { select: first, description: a stage is cleared }\n  \
                       talk: { select: first, target: { prefix: npc, entity: npc }, description: a talk }\n";

const PULSE: &str = "entities:\n  npc: { members: [mara, tomas] }\nstate:\n  run.checkedPulse: { type: bool, default: false }\n";

/// Two chains, each on its own occasion, both apply: each scene plays on
/// its chain's occasion, in its chain's order.
#[test]
fn two_chains_each_order_their_own_occasion() {
    let dir = temp_dir("two");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: chapter\n    scenes: [c1, c2]\n  - on: stage\n    scenes: [s1, s2]\n",
    );
    for id in ["c1", "c2", "s1", "s2"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id, ""));
    }
    write(
        &dir,
        "plays/all.play.yaml",
        "steps:\n  - occasion: stage\n    expect: { winner: s1 }\n  - occasion: chapter\n    expect: { winner: c1 }\n  \
         - occasion: stage\n    expect: { winner: s2 }\n  - occasion: chapter\n    expect: { winner: c2 }\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/all.play.yaml"]);
    assert_eq!(code, Some(0), "{play}");
}

/// The old `sequence:` key is refused naming `chapters:`, and is not
/// applied; the scenes it listed are not told to list themselves in it, and
/// each is told once, however many beat keys it writes.
#[test]
fn the_old_sequence_key_names_chapters_and_is_not_applied() {
    let dir = temp_dir("oldkey");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "sequence:\n  occasion: chapter\n  scenes: [c1, c2]\n",
    );
    write(&dir, "scenes/c1.lute", &scene("c1", ""));
    write(
        &dir,
        "scenes/c2.lute",
        &scene("c2", "once: user\nwhen: \"run.checkedPulse\"\n"),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:8:1: error [E-CHAPTERS] `sequence:` is now `chapters:`"),
        "{out}"
    );
    assert!(!out.contains("list the scene"), "{out}");
    // The rewrite is the author's own chain, not a template.
    assert!(
        out.contains("write `chapters: [{ on: chapter, scenes: [c1, c2] }]`"),
        "{out}"
    );
    assert!(
        out.contains("lists this scene under `sequence:`, which is now `chapters:`"),
        "{out}"
    );
    assert_eq!(out.matches("[E-BEAT-ATTR]").count(), 1, "{out}");
    assert!(
        out.contains("scenes/c2.lute:5:1: error [E-BEAT-ATTR] `once:` and `when:` without `on:`"),
        "{out}"
    );
}

/// A tie with a priority `chapters:` derived says where that priority
/// comes from, and asks to move the scene that wrote its own.
#[test]
fn a_tie_with_a_chain_priority_names_the_chain() {
    let dir = temp_dir("tie");
    project(
        &dir,
        CHAPTER,
        "state:\n  run.leg: { type: number, default: 0 }\n",
        "chapters:\n  - on: chapter\n    scenes: [c1, c2]\n",
    );
    write(&dir, "scenes/c1.lute", &scene("c1", "once: user\n"));
    write(
        &dir,
        "scenes/c2.lute",
        &scene("c2", "once: user\nwhen: \"run.leg >= 2\"\n"),
    );
    write(
        &dir,
        "scenes/side.lute",
        &scene(
            "side",
            "on: chapter\npriority: 10\nonce: user\nwhen: \"run.leg >= 3\"\n",
        ),
    );
    let (_, out) = run(&dir, &["check-project", "."]);
    let tie = out
        .lines()
        .find(|l| l.contains("[W-BEAT-PRIORITY-TIE]"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(
        tie.contains(
            "share priority 10 (scene `c2`'s is written by `chapters:` in lute.project.yaml"
        ),
        "{tie}"
    );
    assert!(
        tie.contains("give scene `side` a different `priority`"),
        "{tie}"
    );
}

/// A chain on an occasion raised for a target lists a scene that declares
/// no `target:`: it would play for every target, so it is an error at the
/// scene's `id:`. With `target:` the chain checks clean.
#[test]
fn a_chain_on_a_targeted_occasion_needs_every_scene_to_name_its_target() {
    let dir = temp_dir("target");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: talk\n    scenes: [m1, m2]\n",
    );
    write(&dir, "scenes/m1.lute", &scene("m1", "target: npc.mara\n"));
    write(&dir, "scenes/m2.lute", &scene("m2", ""));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("scenes/m2.lute:3:1: error [E-CHAPTERS]")
            && out.contains("`talk` is raised for a target and `m2` declares no `target:`"),
        "{out}"
    );
    assert_eq!(out.matches("[E-CHAPTERS]").count(), 1, "{out}");

    write(&dir, "scenes/m2.lute", &scene("m2", "target: npc.mara\n"));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
}

/// A hand-written `after:` is the scene's own, however it is quoted: `lute
/// play` credits `chapters:` only for an `after:` the chain derived.
#[test]
fn only_a_derived_after_is_credited_to_chapters() {
    let dir = temp_dir("credit");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: chapter\n    scenes: [prologue, pryceWakes, accusation]\n",
    );
    write(&dir, "scenes/prologue.lute", &scene("prologue", ""));
    write(
        &dir,
        "scenes/pryceWakes.lute",
        &scene("pryceWakes", "when: \"run.checkedPulse\"\n"),
    );
    write(
        &dir,
        "plays/p.play.yaml",
        "steps:\n  - occasion: chapter\n  - occasion: chapter\n    expect: { presented: [accusation] }\n",
    );
    let credit = "(written by `chapters:` in lute.project.yaml)";

    write(
        &dir,
        "scenes/accusation.lute",
        &scene("accusation", "after: 'visited(\"pryceWakes\")'\n"),
    );
    let accusation = |play: &str| {
        play.lines()
            .find(|l| l.contains("✗ accusation [scene"))
            .unwrap_or_else(|| panic!("no accusation line: {play}"))
            .to_string()
    };
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/p.play.yaml"]);
    assert_eq!(code, Some(1), "{play}");
    let line = accusation(&play);
    assert!(
        line.contains("after: visited(\"pryceWakes\") is not satisfied"),
        "{play}"
    );
    assert!(!line.contains(credit), "hand-written: {play}");

    write(&dir, "scenes/accusation.lute", &scene("accusation", ""));
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/p.play.yaml"]);
    assert_eq!(code, Some(1), "{play}");
    assert!(accusation(&play).contains(credit), "derived: {play}");
}

/// A chain rejected for a misspelt key is located with a did-you-mean, and
/// a scene it lists is not told to list itself in `chapters:` — it already
/// is; it is told the chain was not applied.
#[test]
fn a_rejected_chain_does_not_tell_its_scenes_to_join_it() {
    let dir = temp_dir("rejected");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: chapter\n    scene: [prologue, pryceWakes, accusation]\n",
    );
    write(&dir, "scenes/prologue.lute", &scene("prologue", ""));
    write(
        &dir,
        "scenes/pryceWakes.lute",
        &scene("pryceWakes", "when: \"run.checkedPulse\"\n"),
    );
    write(&dir, "scenes/accusation.lute", &scene("accusation", ""));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:10:5: error [E-CHAPTERS] `scene` is not a chain key — did you mean `scenes`?"),
        "{out}"
    );
    assert!(!out.contains("list the scene"), "{out}");
    assert!(
        out.contains("scenes/pryceWakes.lute:5:1: error [E-BEAT-ATTR]")
            && out.contains("lists this scene in `chapters:`, but that chain is not applied"),
        "{out}"
    );
    assert_eq!(out.matches("[E-BEAT-ATTR]").count(), 1, "{out}");
    assert!(
        !out.contains("W-CHAPTER-STALL"),
        "a rejected chain has no stall: {out}"
    );
}

/// A chain on an occasion nothing declares is refused at the manifest and,
/// like a malformed one, applied nowhere: `lute beats` does not list its
/// scenes under the misspelt occasion.
#[test]
fn a_chain_on_an_undeclared_occasion_is_not_applied() {
    let dir = temp_dir("undeclared");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: chaptre\n    scenes: [c1, c2]\n",
    );
    for id in ["c1", "c2"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id, ""));
    }
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("[E-CHAPTERS]") && out.contains("did you mean `chapter`?"),
        "{out}"
    );
    let (code, beats) = run(&dir, &["beats", "."]);
    assert_eq!(code, Some(0), "{beats}");
    assert!(!beats.contains("chaptre"), "{beats}");
    assert!(!beats.contains("c1"), "{beats}");
}

/// `W-CHAPTER-STALL` fires only on a `when:` that can stay false for good.
/// A clock window a later raise still meets only delays the chain; one over
/// a counter the content writes may never hold, while one over another
/// `owner: engine` path is the engine's to make true.
#[test]
fn a_clock_only_condition_does_not_stall_a_chain() {
    let dir = temp_dir("clock");
    project(
        &dir,
        "occasions:\n  dayStart: { select: first, description: a new day }\n",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n  run.leg: { type: number, default: 0 }\n  \
         run.stage: { type: number, default: 0, owner: engine }\n\
         clock:\n  day: run.day\n  days: 7\n",
        "chapters:\n  - on: dayStart\n    scenes: [mon, tue, wed]\n",
    );
    write(
        &dir,
        "scenes/mon.lute",
        &scene("mon", "when: \"run.day == 1\"\n"),
    );
    write(
        &dir,
        "scenes/tue.lute",
        &scene("tue", "when: \"run.day == 2\"\n"),
    );
    write(
        &dir,
        "scenes/wed.lute",
        &scene("wed", "when: \"run.day >= 3\"\n"),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("W-CHAPTER-STALL"), "{out}");

    write(
        &dir,
        "scenes/tue.lute",
        &scene("tue", "when: \"run.leg == 2\"\n"),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out.matches("[W-CHAPTER-STALL]").count(), 1, "{out}");
    assert!(out.contains("(it reads `run.leg`"), "{out}");

    // The engine advances `run.stage`: the story cannot make it true, and
    // the chain waits for the engine.
    write(
        &dir,
        "scenes/tue.lute",
        &scene("tue", "when: \"run.stage == 2\"\n"),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("W-CHAPTER-STALL"), "{out}");
}

/// A clock window that closes stalls a chain: on a clock that ends, a slot
/// the scene before may already have passed; on an occasion the clock
/// raises, a day it never raises it on. A window a later raise still meets
/// (`!= 'morning'`) does not.
#[test]
fn a_clock_window_that_closes_stalls_a_chain() {
    let dir = temp_dir("window");
    project(
        &dir,
        "occasions:\n  chapter: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
         run.slot: { type: { enum: [morning, afternoon, evening] }, default: morning, owner: engine }\n\
         clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, evening]\n  days: 1\n",
        "chapters:\n  - on: chapter\n    scenes: [parlour, kitchen, garden]\n",
    );
    write(&dir, "scenes/parlour.lute", &scene("parlour", ""));
    write(
        &dir,
        "scenes/kitchen.lute",
        &scene("kitchen", "when: \"run.slot == 'afternoon'\"\n"),
    );
    write(&dir, "scenes/garden.lute", &scene("garden", ""));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out.matches("[W-CHAPTER-STALL]").count(), 1, "{out}");
    assert!(
        out.contains(
            "if `parlour` plays at day 1 evening or later, no later raise of `chapter` meets it \
             before the clock ends at day 1 evening"
        ),
        "{out}"
    );

    write(
        &dir,
        "scenes/kitchen.lute",
        &scene("kitchen", "when: \"run.slot != 'morning'\"\n"),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("W-CHAPTER-STALL"), "{out}");

    // The clock raises `dayStart` on each day an advance enters — never on
    // the day the run starts.
    let dir = temp_dir("daystart");
    project(
        &dir,
        "occasions:\n  dayStart: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n\
         clock:\n  day: run.day\n  raise: { dayStart: dayStart }\n  days: 7\n",
        "chapters:\n  - on: dayStart\n    scenes: [mon, tue]\n",
    );
    write(
        &dir,
        "scenes/mon.lute",
        &scene("mon", "when: \"run.day == 1\"\n"),
    );
    write(&dir, "scenes/tue.lute", &scene("tue", ""));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out.matches("[W-CHAPTER-STALL]").count(), 1, "{out}");
    assert!(
        out.contains("its `when` holds at no raise of `dayStart`"),
        "{out}"
    );
    // The stall says it once: not also as a beat the clock never raises.
    assert!(!out.contains("W-BEAT-UNRAISED"), "{out}");
    assert!(
        out.contains("declare `raiseAtStart: true` on the clock if the engine raises `dayStart`"),
        "{out}"
    );

    // Declaring that the engine raises it where the run starts closes it.
    project(
        &dir,
        "occasions:\n  dayStart: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n\
         clock:\n  day: run.day\n  raise: { dayStart: dayStart }\n  days: 7\n  raiseAtStart: true\n",
        "chapters:\n  - on: dayStart\n    scenes: [mon, tue]\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("W-CHAPTER-STALL"), "{out}");
}

/// Listing a bundle beat or a lore entry says which it is and what to do
/// instead; a scene's bare last id segment gets a did-you-mean for its full
/// id.
#[test]
fn listing_what_is_not_a_scene_says_what_it_is() {
    let dir = temp_dir("bare");
    project(
        &dir,
        CHAPTER,
        PULSE,
        "chapters:\n  - on: stage\n    scenes: [c4s1, talks.maraOne, codex.page]\n",
    );
    write(&dir, "scenes/c4s1.lute", &scene("main.c4s1", ""));
    write(
        &dir,
        "lore/talks.lute",
        "---\nkind: lore\nid: talks\n---\n\n<beat id=\"maraOne\" on=\"talk\" target=\"npc.mara\">\n  @narrator: Hi.\n</beat>\n",
    );
    write(
        &dir,
        "lore/codex.lute",
        "---\nkind: lore\nid: codex\n---\n\n<entry id=\"page\" on=\"chapter\">\n  @narrator: A page.\n</entry>\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lists `c4s1`, but no scene in this project declares `id: c4s1` — did you mean `main.c4s1`?"),
        "{out}"
    );
    assert!(
        out.contains("lists `talks.maraOne`, which is a bundle beat"),
        "{out}"
    );
    assert!(
        out.contains("lists `codex.page`, which is a lore entry"),
        "{out}"
    );
}
