//! 0.41.1 §3.2 (#76): `lute fix` raises an existing, older language stamp —
//! `defaults.luteVersion` in `lute.project.yaml`, a document's frontmatter
//! `luteVersion` — to the toolchain's, in place. A missing stamp is never
//! added, a newer or non-numeric one is never touched, each changed scalar is
//! one fix, and a second run changes nothing.

use std::path::Path;
use std::process::Command;

use lute_check::LUTE_LANG_VERSION as CURRENT;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

fn run(args: &[&str], path: &Path) -> (i32, String) {
    let out = Command::new(BIN).args(args).arg(path).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

const MANIFEST: &str = "# project\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  pov: narrator\n  luteVersion: \"0.40.0\"   # stamp\n  uses: [world.schema.yaml]\n";
const SCHEMA: &str = "state:\n  run.music: { type: { domain: musicAction }, default: start }\nenums:\n  musicAction: [start, stop]\n";
const OLD_DOC: &str = "---\nkind: scene\nid: old\nluteVersion: '0.39.2'\n---\n## Old\n@narrator: luteVersion: \"0.1.0\" stays prose.\n";
const BARE_DOC: &str = "---\nkind: scene\nid: bare\n---\n## Bare\n@narrator: No stamp.\n";

/// The issue's project: the manifest stamp, a document's own stamp and the
/// domain rename (key and reference) are all fixed in one run — four fixes
/// over three of four files; a document without a stamp gains none; the
/// rerun is a no-op and whole-project `check` no longer reports a stale stamp.
#[test]
fn fix_raises_project_and_document_stamps() {
    let dir = lute_scratch::ScratchDir::named("fix-stamp-041");
    write(&dir, "lute.project.yaml", MANIFEST);
    write(&dir, "world.schema.yaml", SCHEMA);
    write(&dir, "old.lute", OLD_DOC);
    write(&dir, "bare.lute", BARE_DOC);

    let (code, before) = run(&["check", "--project"], &dir);
    assert_eq!(code, 0, "{before}");
    assert!(before.contains("W-LUTE-VERSION-STALE"), "{before}");

    let (code, out) = run(&["fix"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lute: applied 4 fix(es) in 3 of 4 file(s)"),
        "{out}"
    );
    assert_eq!(
        read(&dir, "lute.project.yaml"),
        MANIFEST.replace("\"0.40.0\"", &format!("\"{CURRENT}\""))
    );
    assert_eq!(
        read(&dir, "old.lute"),
        OLD_DOC.replace("'0.39.2'", &format!("'{CURRENT}'"))
    );
    assert_eq!(read(&dir, "bare.lute"), BARE_DOC);
    assert_eq!(
        read(&dir, "world.schema.yaml"),
        SCHEMA.replace("musicAction", "musicPlayback")
    );

    let (code, again) = run(&["fix"], &dir);
    assert_eq!(code, 0, "{again}");
    assert!(
        again.contains("lute: applied 0 fix(es) in 0 of 4 file(s)"),
        "{again}"
    );

    let (code, after) = run(&["check", "--project"], &dir);
    assert_eq!(code, 0, "{after}");
    assert!(!after.contains("W-LUTE-VERSION-STALE"), "{after}");
}

/// A flow-mapping `defaults:` and an unquoted scalar are spliced the same way.
#[test]
fn fix_raises_a_flow_mapping_and_a_plain_scalar() {
    let dir = lute_scratch::ScratchDir::named("fix-stamp-041-flow");
    let manifest = "defaults: { pov: narrator, luteVersion: 0.40.0 }\n";
    write(&dir, "lute.project.yaml", manifest);
    let doc = "---\nkind: scene\nid: a\nluteVersion: 0.40.0\n---\n## A\n@narrator: Hi.\n";
    write(&dir, "a.lute", doc);
    let (code, out) = run(&["fix"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lute: applied 2 fix(es) in 2 of 2 file(s)"),
        "{out}"
    );
    assert_eq!(
        read(&dir, "lute.project.yaml"),
        manifest.replace("0.40.0", CURRENT)
    );
    assert_eq!(read(&dir, "a.lute"), doc.replace("0.40.0", CURRENT));
}

/// Quoted keys name the same stamps.
#[test]
fn fix_raises_stamps_under_quoted_keys() {
    let dir = lute_scratch::ScratchDir::named("fix-stamp-041-quoted");
    let manifest = "\"defaults\":\n  'luteVersion': \"0.40.0\"\n";
    write(&dir, "lute.project.yaml", manifest);
    let doc = "---\nkind: scene\nid: a\n\"luteVersion\": \"0.40.0\"\n---\n## A\n@narrator: Hi.\n";
    write(&dir, "a.lute", doc);
    let (code, out) = run(&["fix"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lute: applied 2 fix(es) in 2 of 2 file(s)"),
        "{out}"
    );
    assert_eq!(
        read(&dir, "lute.project.yaml"),
        manifest.replace("0.40.0", CURRENT)
    );
    assert_eq!(read(&dir, "a.lute"), doc.replace("0.40.0", CURRENT));
}

/// A future stamp, a current stamp, a non-numeric stamp, a nested
/// `luteVersion` key and an unrelated plugin `version:` are never rewritten.
#[test]
fn fix_leaves_future_current_and_unrelated_values_alone() {
    let dir = lute_scratch::ScratchDir::named("fix-stamp-041-keep");
    let manifest = "defaults:\n  luteVersion: \"999.0.0\"\n";
    let current =
        format!("---\nkind: scene\nid: c\nluteVersion: \"{CURRENT}\"\n---\n## C\n@narrator: Hi.\n");
    let odd = "---\nkind: scene\nid: d\nluteVersion: \"0.40\"\n---\n## D\n@narrator: Hi.\n";
    let nested =
        "---\nkind: scene\nid: e\nmeta:\n  luteVersion: \"0.40.0\"\n---\n## E\n@narrator: Hi.\n";
    let plugin = "id: game.x\nversion: 0.1.0\nkind: capability\nluteVersion: \"0.40.0\"\n";
    write(&dir, "lute.project.yaml", manifest);
    write(&dir, "c.lute", &current);
    write(&dir, "d.lute", odd);
    write(&dir, "e.lute", nested);
    write(&dir, "plugins/game.x/plugin.yaml", plugin);
    let (code, out) = run(&["fix"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lute: applied 0 fix(es) in 0 of 5 file(s)"),
        "{out}"
    );
    assert_eq!(read(&dir, "lute.project.yaml"), manifest);
    assert_eq!(read(&dir, "c.lute"), current);
    assert_eq!(read(&dir, "d.lute"), odd);
    assert_eq!(read(&dir, "e.lute"), nested);
    assert_eq!(read(&dir, "plugins/game.x/plugin.yaml"), plugin);
}

/// One file named on the command line: its stamp is one fix.
#[test]
fn fix_raises_a_single_document_stamp() {
    let dir = lute_scratch::ScratchDir::named("fix-stamp-041-one");
    write(&dir, "old.lute", OLD_DOC);
    let (code, out) = run(&["fix"], &dir.join("old.lute"));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out, "lute: applied 1 fix(es)\n");
    assert_eq!(
        read(&dir, "old.lute"),
        OLD_DOC.replace("'0.39.2'", &format!("'{CURRENT}'"))
    );
}
