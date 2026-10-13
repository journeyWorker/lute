//! 0.41.1 §3.5 (#81): a `layer: bridge` + `bridgeCall` directive with no
//! `effects.writes` reading a bridge result is `E-BRIDGE-NO-RESULT-WRITE` at
//! its declaration, from `check` and `check-project`, and nothing compiles;
//! a result write (or no `layer: bridge`) is clean.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const MESSAGE: &str = "error [E-BRIDGE-NO-RESULT-WRITE] bridge directive `odds` declares \
    `layer: bridge` but no `effects.writes` reads a bridge result; it will not block — add a \
    `fromBridgeResult` write or remove `layer: bridge`\n";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-declaration-041")
}

/// A scratch copy of the fixture with `d.yaml` replaced by `directives`.
fn project(tag: &str, directives: Option<&str>) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    copy(&fixture(), &dir);
    if let Some(text) = directives {
        std::fs::write(dir.join("plugins/arena/directives/d.yaml"), text).unwrap();
    }
    dir
}

fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn run(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN).current_dir(dir).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn check_project_reports_the_declaration() {
    let dir = project("bridge-decl-041-project", None);
    let (code, stdout, stderr) = run(&dir, &["check-project", "."]);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(stderr, format!("./plugins/arena/directives/d.yaml:2:11: {MESSAGE}"));
    assert_eq!(
        stdout,
        "ok: ./odds.lute (0 warning(s))\n\
         failed: . (1 file(s), 1 project-wide error(s), 0 project-wide warning(s))\n"
    );
}

#[test]
fn check_reports_the_declaration() {
    let dir = project("bridge-decl-041-check", None);
    let (code, stdout, stderr) = run(&dir, &["check", "odds.lute"]);
    assert_eq!(code, 1, "{stdout}{stderr}");
    let decl = dir.join("plugins/arena/directives/d.yaml");
    assert!(
        stderr.contains(&format!("{}:2:11: {MESSAGE}", decl.display())),
        "{stderr}"
    );
}

#[test]
fn compile_is_refused() {
    let dir = project("bridge-decl-041-compile", None);
    let (code, _, stderr) = run(&dir, &["compile", "--all", ".", "-o", "out"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("[E-BRIDGE-NO-RESULT-WRITE]"), "{stderr}");
    assert!(stderr.contains("no output written"), "{stderr}");
    assert!(!dir.join("out").exists());

    let (code, _, stderr) = run(&dir, &["compile", "odds.lute", "-o", "odds.json"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(!dir.join("odds.json").exists());
}

/// A result write makes it a blocking bridge; without `layer: bridge` the
/// same call is a plain non-blocking record. Neither is reported.
#[test]
fn a_result_write_or_no_bridge_layer_is_clean() {
    let with_write = "directives:\n  - name: odds\n    layer: bridge\n    attrs:\n      \
        - { name: sync, type: bool, default: true }\n    semantics: [ \"writes.sceneState\", \"bridgeCall\" ]\n    \
        effects:\n      writes:\n        - { scope: run, path: [chance], value: { fromBridgeResult: chance } }\n    \
        bridge: { service: arena, operation: odds }\n";
    let dir = project("bridge-decl-041-write", Some(with_write));
    std::fs::write(
        dir.join("odds.lute"),
        "---\nkind: scene\nid: odds\nstate:\n  run.chance: { type: int, default: 0 }\nplugins:\n  \
         arena: {}\n---\n## Odds\n::odds\n@narrator: The odds are {{run.chance}}.\n",
    )
    .unwrap();
    let (code, stdout, _) = run(&dir, &["check-project", "."]);
    assert_eq!(code, 0, "{stdout}");
    assert!(!stdout.contains("E-BRIDGE-NO-RESULT-WRITE"), "{stdout}");

    let plain = std::fs::read_to_string(fixture().join("plugins/arena/directives/d.yaml"))
        .unwrap()
        .replace("    layer: bridge\n", "");
    let dir = project("bridge-decl-041-plain", Some(&plain));
    let (code, stdout, _) = run(&dir, &["check-project", "."]);
    assert_eq!(code, 0, "{stdout}");
    let (code, _, stderr) = run(&dir, &["compile", "--all", ".", "-o", "out"]);
    assert_eq!(code, 0, "{stderr}");
}
