//! 0.42.0 §3.3 (#77): `lute fix` renames `down` → `quiet` and `up` → `loud`
//! only in a project whose folded `volume` domain is exactly the old
//! documented scale `[silent, down, normal, up, full]` — its declaration,
//! `volume=` values and `volumes:` binding keys — preserving layout; a
//! custom or conflicting `volume` vocabulary is left alone.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

fn fix(path: &Path) -> String {
    let out = Command::new(BIN).arg("fix").arg(path).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const MANIFEST: &str =
    "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [vocabulary.schema.yaml]\n";
const SCENE: &str = "---\nkind: scene\nid: start\non: start\n---\n## Start\n\
    ::music{playback=\"start\" mood=\"calm\" volume=\"down\"}\n@narrator: Quiet, then up.\n\
    ::music{playback=\"change\" volume=\"up\"}\n";
const BINDINGS: &str = "bindingsVersion: \"0.41\"\ncanvas: {width: 1920, height: 1080}\n\
    volumes:\n  silent: 0\n  down: 0.25   # was relative\n  normal: 0.5\n  \"up\": 0.75\n  full: 1\n";

fn project(tag: &str, vocabulary: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", MANIFEST);
    write(&dir, "vocabulary.schema.yaml", vocabulary);
    write(&dir, "scenes/start.lute", SCENE);
    write(&dir, "player.bindings.yaml", BINDINGS);
    dir
}

/// The exact old scale: declaration, both `volume=` values (any quoting) and
/// both bindings keys are renamed in place — comments and quotes kept —, the
/// prose `up` and the `mood` are not, and the second run is a no-op.
#[test]
fn the_exact_old_scale_is_migrated_in_place() {
    let vocabulary = "enums:\n  mood: [calm]\n  musicPlayback: [start, change]\n  # absolute levels\n  volume: [silent, down, normal, up, full]   # the old scale\n";
    let dir = project("fix-volume-042", vocabulary);
    let out = fix(&dir);
    assert!(out.contains("lute: applied 6 fix(es) in 3 of 4 file(s)"), "{out}");
    assert_eq!(
        read(&dir, "vocabulary.schema.yaml"),
        vocabulary.replace("silent, down, normal, up, full", "silent, quiet, normal, loud, full")
    );
    assert_eq!(
        read(&dir, "scenes/start.lute"),
        SCENE.replace("volume=\"down\"", "volume=\"quiet\"").replace("volume=\"up\"", "volume=\"loud\"")
    );
    assert_eq!(
        read(&dir, "player.bindings.yaml"),
        BINDINGS.replace("  down: 0.25", "  quiet: 0.25").replace("\"up\": 0.75", "\"loud\": 0.75")
    );
    let again = fix(&dir);
    assert!(again.contains("lute: applied 0 fix(es) in 0 of 4 file(s)"), "{again}");

    let check = Command::new(BIN).arg("check-project").arg(&*dir).output().unwrap();
    let text = String::from_utf8_lossy(&check.stdout);
    assert!(check.status.success(), "{text}");
    assert!(!text.contains("volume member") && !text.contains("volume binding"), "{text}");
}

/// The long form and a block list are the same declaration; a prose value
/// (`description: "up"`) is not a member.
#[test]
fn long_form_and_block_declarations_are_migrated() {
    let long = "enums:\n  volume: { members: [silent, down, normal, up, full], default: up, labels: { down: Soft }, description: \"up\" }\n";
    let dir = project("fix-volume-042-long", long);
    fix(&dir);
    assert_eq!(
        read(&dir, "vocabulary.schema.yaml"),
        "enums:\n  volume: { members: [silent, quiet, normal, loud, full], default: loud, labels: { quiet: Soft }, description: \"up\" }\n"
    );

    let block = "enums:\n  volume:\n    - silent\n    - down\n    - normal\n    - up\n    - full\n  mood: [up]\n";
    let dir = project("fix-volume-042-block", block);
    fix(&dir);
    assert_eq!(
        read(&dir, "vocabulary.schema.yaml"),
        block.replace("- down", "- quiet").replace("- up\n", "- loud\n")
    );
}

/// A project that chose its own levels — a subset, other names, the old
/// members in another order — keeps every file byte for byte.
#[test]
fn custom_volume_vocabularies_are_untouched() {
    for (i, members) in [
        "[silent, down, normal, up]",
        "[low, normal, up, alarm]",
        "[silent, up, normal, down, full]",
    ]
    .into_iter()
    .enumerate()
    {
        let vocabulary = format!("enums:\n  mood: [calm]\n  volume: {members}\n");
        let dir = project(&format!("fix-volume-042-custom-{i}"), &vocabulary);
        let out = fix(&dir);
        assert!(out.contains("lute: applied 0 fix(es)"), "{members}: {out}");
        assert_eq!(read(&dir, "vocabulary.schema.yaml"), vocabulary);
        assert_eq!(read(&dir, "scenes/start.lute"), SCENE);
        assert_eq!(read(&dir, "player.bindings.yaml"), BINDINGS);
    }
}

/// Two documents folding different `volume` domains: the project's
/// declarations are not the old scale alone, so nothing is rewritten.
#[test]
fn conflicting_declarations_are_untouched() {
    let dir = project(
        "fix-volume-042-conflict",
        "enums:\n  volume: [silent, down, normal, up, full]\n",
    );
    let other = "---\nkind: scene\nid: other\non: start\nenums:\n  volume: [low, high]\n---\n## Other\n\
        ::music{playback=\"start\" volume=\"low\"}\n";
    write(&dir, "scenes/other.lute", other);
    let out = fix(&dir);
    assert!(out.contains("lute: applied 0 fix(es)"), "{out}");
    assert_eq!(read(&dir, "scenes/start.lute"), SCENE);
}

/// Outside any project, a document's own frontmatter declaration decides.
#[test]
fn a_standalone_document_with_the_old_scale_is_migrated() {
    let dir = lute_scratch::ScratchDir::named("fix-volume-042-standalone");
    let doc = "---\nkind: scene\nid: s\nenums:\n  volume: [silent, down, normal, up, full]\n---\n## S\n\
        ::music{playback=\"start\" volume=\"up\"}\n";
    write(&dir, "s.lute", doc);
    let out = fix(&dir.join("s.lute"));
    assert_eq!(out, "lute: applied 3 fix(es)\n");
    assert_eq!(
        read(&dir, "s.lute"),
        doc.replace("silent, down, normal, up, full", "silent, quiet, normal, loud, full")
            .replace("volume=\"up\"", "volume=\"loud\"")
    );
}
