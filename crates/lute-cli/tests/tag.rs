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
        "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:narrator: hi\n",
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
        after.contains(":narrator{code=\"0010\"}: hi"),
        "expected `:narrator{{code=\"0010\"}}: hi`, got:\n{after}"
    );
    assert_eq!(
        after,
        "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:narrator{code=\"0010\"}: hi\n",
        "full file must match the 0.1.0 :speaker{{code}} rewrite exactly"
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
