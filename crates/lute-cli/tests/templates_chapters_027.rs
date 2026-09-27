//! Beat templates and the manifest's `chapters:`, end to end through the
//! built `lute` binary: `check-project` is clean, `lute beats` shows the
//! derived headers, and `lute play` presents template beats in order with
//! the template body around the use's body (`::body`), and chains the
//! listed scenes without any `on:`/`after:`/`priority:` of their own.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-tpl027-{tag}-{}", std::process::id()));
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

fn project(dir: &Path, scenes: &str) {
    write(
        dir,
        "lute.project.yaml",
        &format!(
            "defaultProfile: core\nprofiles:\n  core:\n    plugins: {{}}\n\
             defaults:\n  uses: [world.schema.yaml]\n{scenes}"
        ),
    );
    write(
        dir,
        "world.schema.yaml",
        "state:\n  user.bond: { type: number, default: 0 }\n",
    );
}

const BOND: &str = "---\ncomponent: bondStory\n\
params:\n  who: { type: string }\n  need: { type: number, default: 0 }\n  prev: { type: string, default: \"\" }\n\
beat:\n  on: bond\n  once: user\n  when: \"user.bond >= @need\"\n  after: \"@prev\"\n---\n\
## Bond\n@narrator: A bond story begins.\n::body\n@narrator: The bond deepens.\n";

const BONDS: &str = "---\nkind: lore\nid: bonds\ncomponents: [../components/bond.lute]\n---\n\n\
<beat use=\"bondStory\" id=\"first\" who=\"aria\" priority=\"2\">\n@narrator: Hello, aria.\n::set{ user.bond += 1 }\n</beat>\n\n\
<beat use=\"bondStory\" id=\"second\" who=\"aria\" need=\"1\" prev=\"bonds.first\" priority=\"1\">\n@narrator: Again, aria.\n</beat>\n";

#[test]
fn template_beats_check_list_and_play_in_order() {
    let dir = temp_dir("bond");
    project(&dir, "");
    write(&dir, "components/bond.lute", BOND);
    write(&dir, "lore/bonds.lute", BONDS);
    write(
        &dir,
        "plays/bond.play.yaml",
        "steps:\n  - occasion: bond\n    expect: { winner: bonds.first }\n  - occasion: bond\n    expect: { winner: bonds.second }\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    let (_, beats) = run(&dir, &["beats", "."]);
    assert!(beats.contains("bonds.second"), "{beats}");
    assert!(beats.contains("visited(\"bonds.first\")"), "{beats}");
    assert!(beats.contains("user.bond >= 1"), "{beats}");
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/bond.play.yaml"]);
    assert_eq!(code, Some(0), "{play}");
    let at = |s: &str| {
        play.find(s)
            .unwrap_or_else(|| panic!("missing {s:?}:\n{play}"))
    };
    // The template's head, the use's body at `::body`, then the tail.
    assert!(at("A bond story begins.") < at("Hello, aria."), "{play}");
    assert!(at("Hello, aria.") < at("The bond deepens."), "{play}");
    assert!(at("Again, aria.") > at("The bond deepens."), "{play}");
    let (code, test) = run(&dir, &["test", "."]);
    assert_eq!(code, Some(0), "{test}");
}

#[test]
fn a_template_misuse_fails_check_project() {
    let dir = temp_dir("misuse");
    project(&dir, "");
    write(&dir, "components/bond.lute", BOND);
    write(
        &dir,
        "lore/bonds.lute",
        "---\nkind: lore\nid: bonds\ncomponents: [../components/bond.lute]\n---\n\n\
         <beat use=\"bondStroy\" id=\"a\" who=\"x\">\n@narrator: Hi.\n</beat>\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("E-TEMPLATE") && out.contains("did you mean `bondStory`"),
        "{out}"
    );
}

fn scene(id: &str) -> String {
    format!("---\nkind: scene\nid: {id}\ntitle: {id}\n---\n\n## {id}\n\n@narrator: This is {id}.\n")
}

#[test]
fn a_chain_orders_scenes_that_say_nothing_about_order() {
    let dir = temp_dir("seq");
    project(
        &dir,
        "chapters:\n  - on: chapter\n    scenes: [one, two, three]\n",
    );
    for id in ["one", "two", "three"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id));
    }
    write(
        &dir,
        "plays/all.play.yaml",
        "steps:\n  - occasion: chapter\n    expect: { winner: one }\n  - occasion: chapter\n    expect: { winner: two }\n  - occasion: chapter\n    expect: { winner: three }\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    let (_, beats) = run(&dir, &["beats", "."]);
    assert!(beats.contains("visited(\"two\")"), "{beats}");
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/all.play.yaml"]);
    assert_eq!(code, Some(0), "{play}");
    let (code, test) = run(&dir, &["test", "."]);
    assert_eq!(code, Some(0), "{test}");
}

/// `lute test --coverage` sees the beats `chapters:` derives: a scene with no
/// `on:` of its own that no play reaches is listed as unplayed.
#[test]
fn coverage_counts_chapter_derived_beats() {
    let dir = temp_dir("seqcov");
    project(
        &dir,
        "chapters:\n  - on: chapter\n    scenes: [one, two, three]\n",
    );
    for id in ["one", "two", "three"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id));
    }
    write(
        &dir,
        "plays/two.play.yaml",
        "steps:\n  - occasion: chapter\n    expect: { winner: one }\n  - occasion: chapter\n    expect: { winner: two }\n",
    );
    let (code, cov) = run(&dir, &["test", ".", "--coverage"]);
    assert_eq!(code, Some(0), "{cov}");
    assert!(
        cov.contains("1 untested unit(s) under . — no *.test.yaml presents them and no play presents them:\n    scenes/three.lute"),
        "{cov}"
    );
    // OT-F-14: an untested beat is unplayed too, listed once.
    assert_eq!(cov.matches("scenes/three.lute").count(), 1, "{cov}");
    assert!(
        cov.contains("every other beat under . is presented by a play"),
        "{cov}"
    );
}

#[test]
fn an_unknown_chapter_id_is_e_chapters_at_the_manifest() {
    let dir = temp_dir("seqbad");
    project(&dir, "chapters:\n  - on: chapter\n    scenes: [one, tow]\n");
    for id in ["one", "two"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id));
    }
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:9:19:"),
        "anchored at the id: {out}"
    );
    assert!(
        out.contains("E-CHAPTERS") && out.contains("did you mean `two`"),
        "{out}"
    );
}

/// ML-F2: on a `select: sequence` occasion the chain is the priority order
/// alone — both listed scenes play in the one raise.
#[test]
fn a_chain_on_a_select_sequence_occasion_plays_in_one_raise() {
    let dir = temp_dir("seqsel");
    select_sequence_project(&dir, "[one, two]");
    for id in ["one", "two"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id));
    }
    write(
        &dir,
        "plays/new.play.yaml",
        "steps:\n  - occasion: newGame\n    expect: { presented: [one, two] }\n",
    );
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/new.play.yaml"]);
    assert_eq!(code, Some(0), "{play}");
}

/// A project whose `chapters:` chains `scenes` on `newGame`, a `select:
/// sequence` occasion.
fn select_sequence_project(dir: &Path, scenes: &str) {
    write(
        dir,
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: {{ mini: true }}\n\
             chapters:\n  - on: newGame\n    scenes: {scenes}\n"
        ),
    );
    write(
        dir,
        "plugins/mini/plugin.yaml",
        "id: mini\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        dir,
        "plugins/mini/occasions/o.yaml",
        "occasions:\n  newGame: { select: sequence, description: \"every eligible beat\" }\n",
    );
}

/// On a `select: sequence` occasion the list's order IS the derived
/// priority, so a listed scene's own `priority:` that breaks it is
/// `W-CHAPTER-ORDER` at that key; one that keeps the order is silent.
#[test]
fn an_own_priority_that_reorders_a_select_sequence_warns() {
    let dir = temp_dir("seqorder");
    select_sequence_project(&dir, "[one, two, three]");
    write(&dir, "scenes/one.lute", &scene("one"));
    write(
        &dir,
        "scenes/two.lute",
        &scene("two").replacen("title: two\n", "title: two\npriority: 50\n", 1),
    );
    write(
        &dir,
        "scenes/three.lute",
        &scene("three").replacen("title: three\n", "title: three\npriority: 5\n", 1),
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains(
            "scenes/two.lute:5:1: warning [W-CHAPTER-ORDER] scene `two` sets its own `priority: \
             50`, so it plays out of the order the chain on `newGame` lists"
        ),
        "{out}"
    );
    assert_eq!(out.matches("W-CHAPTER-ORDER").count(), 1, "{out}");
}

/// A malformed chain is located, suggests the key, and does not stop the
/// documents from being checked.
#[test]
fn a_malformed_chain_is_located_and_not_fatal() {
    let dir = temp_dir("seqshape");
    project(&dir, "chapters:\n  - on: chapter\n    scene: [one]\n");
    write(&dir, "scenes/one.lute", &scene("one"));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:9:5: error [E-CHAPTERS]")
            && out.contains("did you mean `scenes`?"),
        "{out}"
    );
    assert!(!out.contains("must be a non-empty list"), "{out}");
    assert!(
        out.contains("ok: ./scenes/one.lute"),
        "the documents are checked: {out}"
    );
}

/// T3-16: `lute trace --beat` frames a template use as ONE component frame,
/// named, with the use's own body marked inside it; a beat ending in a
/// `::use` closes that frame; a plugin call is no bare `<tag>`; a line
/// guard reads as a guard, not the `<match>` it lowers to (T3-22).
#[test]
fn trace_frames_a_template_use_once_and_names_guards() {
    let dir = temp_dir("trace-frames");
    project(&dir, "");
    write(
        &dir,
        "components/bond.lute",
        "---\ncomponent: bondStory\nparams:\n  who: { type: string }\n\
         beat:\n  on: bond\n---\n## Bond\n::body\n@narrator: The bond deepens.\n",
    );
    write(
        &dir,
        "components/greet.lute",
        "---\ncomponent: greet\n---\n## G\n@narrator: Hi there.\n",
    );
    write(
        &dir,
        "lore/bonds.lute",
        "---\nkind: lore\nid: bonds\ncomponents: [../components/bond.lute, ../components/greet.lute]\n---\n\n\
         <beat use=\"bondStory\" id=\"first\" who=\"aria\">\n@narrator: Hello.\n@narrator{when=\"user.bond > 5\"}: Deep bond.\n</beat>\n\n\
         <beat id=\"wave\" on=\"bond\">\n@narrator: Wave.\n::use{component=\"greet\"}\n</beat>\n",
    );
    let (code, out) = run(&dir, &["trace", "lore/bonds.lute", "--beat", "first"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains(
            "    -- component bondStory begin --\n    -- body --\n    @narrator  Hello.\n  \
             guard `user.bond > 5`: skipped\n    -- body end --\n    @narrator  The bond deepens.\n"
        ),
        "{out}"
    );
    assert_eq!(
        out.matches("-- component bondStory begin --").count(),
        1,
        "{out}"
    );
    assert_eq!(
        out.matches("-- component bondStory end --").count(),
        1,
        "{out}"
    );
    assert!(out.contains("  guard `user.bond > 5`: skipped\n"), "{out}");
    assert!(!out.contains("<match"), "{out}");

    let (code, out) = run(&dir, &["trace", "lore/bonds.lute", "--beat", "wave"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("    -- component greet begin --\n    @narrator  Hi there.\n    -- component greet end --\n"),
        "a beat ending in a `::use` closes its frame: {out}"
    );
}

/// T3-22/T3-23: coverage names a line guard a guard, attributes a match a
/// component `::use` expanded to the component's file and use (never merged
/// into a host construct), prints paths relative to where it runs, and
/// every JSON unit row carries `doc`/`local`, with no beat both `untested`
/// and `notPresentedByPlay`.
#[test]
fn coverage_names_guards_attributes_component_matches_and_relative_paths() {
    let dir = temp_dir("coverage-sites");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1 }\ndefs:\n  today: { type: number, cel: \"run.day\" }\n",
    );
    write(
        &dir,
        "components/card.lute",
        "---\ncomponent: card\nparams:\n  n: number\n---\n## Card\n\
         <match on=\"@n\">\n<when is=\"1\">\n@narrator: Calm.\n</when>\n<otherwise>\n@narrator: Wild.\n</otherwise>\n</match>\n",
    );
    write(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\nid: s\ncomponents: [../components/card.lute]\n---\n## S\n\
         <match on=\"run.day\">\n<when is=\"1\">\n::use{component=\"card\" n=@today}\n</when>\n\
         <when is=\"2\">\n@narrator: Two.\n</when>\n<otherwise>\n@narrator: Later.\n</otherwise>\n</match>\n\
         @narrator{when=\"run.day == 2\"}: Guarded.\n",
    );
    write(
        &dir,
        "lore/barks.lute",
        "---\nkind: lore\nid: barks\n---\n\n<entry id=\"renPass\" on=\"talk\">\n@narrator: Ren nods.\n</entry>\n\n\
         <beat id=\"ember\" on=\"talk\">\n@narrator: Ember.\n</beat>\n",
    );
    write(
        &dir,
        "tests/s.test.yaml",
        "file: ../scenes/s.lute\nexpect: { transcriptContains: [\"Calm.\"] }\n",
    );
    let (code, out) = run(&dir, &["test", "tests", "--coverage"]);
    assert_eq!(code, Some(0), "{out}");
    let abs = std::fs::canonicalize(&dir).unwrap();
    assert!(
        !out.contains(abs.to_str().unwrap()),
        "no absolute path: {out}"
    );
    assert!(
        out.contains("  guard `run.day == 2` (scenes/s.lute:"),
        "a line guard is a guard: {out}"
    );
    assert!(out.contains(": skipped; never taken\n"), "{out}");
    assert!(
        out.contains("match `run.day` (scenes/s.lute:7:1): 1/3 arm(s) executed [arm 1]"),
        "the host match keeps its own arms: {out}"
    );
    assert!(
        out.contains("(components/card.lute:7:1 (card#1 in scenes/s.lute)): 1/2 arm(s) executed"),
        "the component's match is its own site: {out}"
    );
    assert!(out.contains("untested unit(s) under . —"), "{out}");
    assert!(
        out.contains("    lore/barks.lute: renPass, ember\n"),
        "{out}"
    );

    let o = Command::new(BIN)
        .args(["test", "tests", "--coverage", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let untested = v["coverage"]["untested"].as_array().unwrap();
    let ember = untested.iter().find(|u| u["kind"] == "beat").unwrap();
    assert_eq!(ember["id"], "barks.ember", "{v:#}");
    assert_eq!(ember["doc"], "barks", "{v:#}");
    assert_eq!(ember["local"], "ember", "{v:#}");
    let entry = untested.iter().find(|u| u["kind"] == "entry").unwrap();
    assert_eq!(entry["id"], "renPass", "{v:#}");
    assert_eq!(entry["doc"], "barks", "{v:#}");
    assert_eq!(entry["file"], "lore/barks.lute", "{v:#}");
    assert_eq!(v["coverage"]["root"], ".", "{v:#}");
    for u in v["coverage"]["notPresentedByPlay"].as_array().unwrap() {
        assert!(!untested.contains(u), "listed twice: {v:#}");
    }
    let guard = v["coverage"]["arms"]
        .as_object()
        .unwrap()
        .values()
        .find(|a| a["kind"] == "guard")
        .unwrap_or_else(|| panic!("{v:#}"));
    assert_eq!(guard["executed"], serde_json::json!(["skipped"]), "{v:#}");
}

#[test]
fn a_headless_template_plays_and_beats_notes_the_when_a_use_replaces() {
    let dir = temp_dir("headless");
    project(&dir, "");
    // A beat template's body needs no `## ` heading.
    write(&dir, "components/bond.lute", &BOND.replace("## Bond\n", ""));
    write(
        &dir,
        "lore/bonds.lute",
        "---\nkind: lore\nid: bonds\ncomponents: [../components/bond.lute]\n---\n\n\
<beat use=\"bondStory\" id=\"first\" who=\"aria\" need=\"2\" when=\"user.bond >= 0\">\n@narrator: Hello, aria.\n</beat>\n",
    );
    write(
        &dir,
        "plays/bond.play.yaml",
        "steps:\n  - occasion: bond\n    expect: { winner: bonds.first }\n",
    );
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("E-CONTENT-OUTSIDE-SHOT"), "{out}");
    let (_, beats) = run(&dir, &["beats", "."]);
    assert!(
        beats.contains(
            "user.bond >= 0 (replaces template `bondStory`'s `when: user.bond >= @need`)"
        ),
        "{beats}"
    );
    let (_, json) = run(&dir, &["beats", ".", "--json"]);
    assert!(json.contains("\"replacesTemplateWhen\""), "{json}");
    let (code, play) = run(&dir, &["play", ".", "--script", "plays/bond.play.yaml"]);
    assert_eq!(code, Some(0), "{play}");
    let at = |s: &str| {
        play.find(s)
            .unwrap_or_else(|| panic!("missing {s:?}:\n{play}"))
    };
    assert!(at("A bond story begins.") < at("Hello, aria."), "{play}");
    assert!(at("Hello, aria.") < at("The bond deepens."), "{play}");
}
