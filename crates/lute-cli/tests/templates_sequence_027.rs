//! dsl 0.27.0 §6 beat templates and §8 `sequence:`, end to end through the
//! built `lute` binary: `check-project` is clean, `lute beats` shows the
//! derived headers, and `lute play` presents template beats in order with
//! the template body around the use's body (`::body`), and chains sequence
//! scenes without any `on:`/`after:`/`priority:` of their own.

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
fn a_sequence_chains_scenes_that_say_nothing_about_order() {
    let dir = temp_dir("seq");
    project(
        &dir,
        "sequence:\n  occasion: chapter\n  scenes: [one, two, three]\n",
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

/// `lute test --coverage` sees the beats `sequence:` derives: a scene with no
/// `on:` of its own that no play reaches is listed as unplayed.
#[test]
fn coverage_counts_sequence_derived_beats() {
    let dir = temp_dir("seqcov");
    project(
        &dir,
        "sequence:\n  occasion: chapter\n  scenes: [one, two, three]\n",
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
        cov.contains("1 beat(s) no play presents (a test may trace them; only a play proves they are reached in play):\n    ./scenes/three.lute"),
        "{cov}"
    );
}

#[test]
fn an_unknown_sequence_id_is_e_sequence_at_the_manifest() {
    let dir = temp_dir("seqbad");
    project(
        &dir,
        "sequence:\n  occasion: chapter\n  scenes: [one, tow]\n",
    );
    for id in ["one", "two"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id));
    }
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:9:"),
        "anchored at the id: {out}"
    );
    assert!(
        out.contains("E-SEQUENCE") && out.contains("did you mean `two`"),
        "{out}"
    );
}

/// ML-F2: on a `select: sequence` occasion the chain is the priority order
/// alone — both listed scenes play in the one raise.
#[test]
fn a_sequence_on_a_select_sequence_occasion_plays_in_one_raise() {
    let dir = temp_dir("seqsel");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: { mini: true }\n\
         sequence:\n  occasion: newGame\n  scenes: [one, two]\n",
    );
    write(
        &dir,
        "plugins/mini/plugin.yaml",
        "id: mini\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/mini/occasions/o.yaml",
        "occasions:\n  newGame: { select: sequence, description: \"every eligible beat\" }\n",
    );
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

/// FS-F5: a malformed `sequence:` is located, suggests the key, and does
/// not stop the documents from being checked.
#[test]
fn a_malformed_sequence_is_located_and_not_fatal() {
    let dir = temp_dir("seqshape");
    project(&dir, "sequence:\n  occasion: chapter\n  scene: [one]\n");
    write(&dir, "scenes/one.lute", &scene("one"));
    let (code, out) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("lute.project.yaml:9:3: error [E-SEQUENCE] `sequence.scene` is not a sequence key — did you mean `scenes`?"),
        "{out}"
    );
    assert!(!out.contains("must be a non-empty list"), "{out}");
    assert!(
        out.contains("ok: ./scenes/one.lute"),
        "the documents are checked: {out}"
    );
}
