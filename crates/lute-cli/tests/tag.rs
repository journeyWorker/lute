//! End-to-end CLI test for `lute tag`: spawn the built binary on a temp scene
//! and assert the §12 localization pass back-fills a stable `code` into each
//! untagged `:line` AND that a second run is a byte-identical no-op (idempotent,
//! never partial-writes). Pins the Task L2 acceptance contract.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn tag_backfills_code_and_is_idempotent() {
    let dir = temp_dir("tag");
    let f = dir.join("scene.lute");
    std::fs::write(
        &f,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n",
    )
    .unwrap();
    let out = Command::new(BIN)
        .args(["tag", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&f).unwrap();
    // Pin the exact 0.1.0 rewrite shape (Task 9a): `code` is inserted as
    // `{code="…"}` BETWEEN the speaker ident and the second colon — not merely
    // present somewhere. A tagger that placed the code in the wrong slot (or
    // reordered/dropped other attrs) would fail this, unlike a bare substring.
    assert!(
        after.contains("@narrator{code=\"0010\"}: hi"),
        "expected `@narrator{{code=\"0010\"}}: hi`, got:\n{after}"
    );
    assert_eq!(
        after,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator{code=\"0010\"}: hi\n",
        "full file must match the @speaker{{code}} rewrite exactly"
    );
    // idempotent: second run changes nothing
    let out2 = Command::new(BIN)
        .args(["tag", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out2.status.success());
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        after,
        "second tag run must be a no-op"
    );
}

/// dsl 0.22.0 §13: `lute tag <dir>` tags every `.lute` file under the
/// directory, recursively and in sorted order, naming each file it changed
/// and leaving already-tagged files untouched; non-`.lute` files are ignored.
#[test]
fn tag_walks_a_directory_recursively_in_sorted_order() {
    let dir = temp_dir("tag-dir");
    let scene = |line: &str| {
        format!("---\nkind: scene\nid: s\n---\n## Shot 1.\n{line}\n")
    };
    std::fs::create_dir_all(dir.join("scenes/talk")).unwrap();
    std::fs::write(dir.join("scenes/talk/b.lute"), scene("@ann: b")).unwrap();
    std::fs::write(dir.join("scenes/a.lute"), scene("@ann: a")).unwrap();
    let tagged = scene("@ann{code=\"0010\"}: done");
    std::fs::write(dir.join("scenes/done.lute"), &tagged).unwrap();
    std::fs::write(dir.join("scenes/notes.txt"), "@ann: not lute\n").unwrap();

    let out = Command::new(BIN)
        .args(["tag", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let a = dir.join("scenes/a.lute").display().to_string();
    let b = dir.join("scenes/talk/b.lute").display().to_string();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            format!("lute: {a}: tagged 1 line(s)").as_str(),
            format!("lute: {b}: tagged 1 line(s)").as_str(),
            "lute: tagged 2 line(s) in 2 of 3 file(s)",
        ],
        "{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("scenes/talk/b.lute")).unwrap(),
        scene("@ann{code=\"0010\"}: b")
    );
    assert_eq!(std::fs::read_to_string(dir.join("scenes/done.lute")).unwrap(), tagged);
    assert_eq!(
        std::fs::read_to_string(dir.join("scenes/notes.txt")).unwrap(),
        "@ann: not lute\n"
    );
}

/// A refused file in a tree does not stop the walk: `--force` renumbers the
/// draft and refuses the `codesLocked:` one, and the exit code reports the
/// refusal.
#[test]
fn tag_force_over_a_directory_continues_past_a_refusal() {
    let dir = temp_dir("tag-dir-force");
    let draft = "---\nkind: scene\nid: d\n---\n## Shot 1.\n@ann{code=\"0050\"}: one\n";
    let locked =
        "---\nkind: scene\nid: l\ncodesLocked: true\n---\n## Shot 1.\n@ann{code=\"0050\"}: one\n";
    std::fs::write(dir.join("a-locked.lute"), locked).unwrap();
    std::fs::write(dir.join("b-draft.lute"), draft).unwrap();

    let out = Command::new(BIN)
        .args(["tag", "--force", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("codesLocked"),
        "{out:?}"
    );
    assert_eq!(std::fs::read_to_string(dir.join("a-locked.lute")).unwrap(), locked);
    assert!(
        std::fs::read_to_string(dir.join("b-draft.lute"))
            .unwrap()
            .contains("@ann{code=\"0010\"}: one"),
        "the draft after the refused file is still renumbered"
    );
}

/// `lute fix <dir>` migrates every `.lute` file under the directory.
#[test]
fn fix_walks_a_directory_recursively() {
    let dir = temp_dir("fix-dir");
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    let legacy = "---\nkind: scene\nid: s\n---\n## Shot 1.\n:line[ann]: hi\n";
    std::fs::write(dir.join("nested/old.lute"), legacy).unwrap();
    std::fs::write(dir.join("new.lute"), "---\nkind: scene\nid: t\n---\n## Shot 1.\n@ann: hi\n")
        .unwrap();

    let out = Command::new(BIN)
        .args(["fix", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join("nested/old.lute")).unwrap(),
        "---\nkind: scene\nid: s\n---\n## Shot 1.\n@ann: hi\n"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).ends_with("in 1 of 2 file(s)\n"),
        "{out:?}"
    );
}
