//! Every command that falls back to the nearest `lute.project.yaml` says so
//! on stderr, and names the project relative to the working directory (`.`
//! when it is the working directory) — the same text `lute test` and `lute
//! check` print. 0.36.2 shipped `trace`, `context` and `compile-stream` with
//! the canonical absolute path instead, because their imports resolved to a
//! second, absolute-path `discover_project` in `lute-load`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// `<tmp>/<unique>/proj/` holding a manifest and one scene; returns the
/// parent `<tmp>/<unique>/`, the working directory the commands run from.
fn fixture(tag: &str) -> PathBuf {
    let parent = std::env::temp_dir().join(format!("lute-project-note-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);
    let proj = parent.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("lute.project.yaml"), "defaultProfile: default\n").unwrap();
    std::fs::write(
        proj.join("scene.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a: hi\n",
    )
    .unwrap();
    parent
}

fn note_of(cwd: &Path, args: &[&str], stdin: Option<&str>) -> String {
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run lute");
    if let Some(text) = stdin {
        child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    stderr
        .lines()
        .find(|l| l.contains("(nearest lute.project.yaml)"))
        .unwrap_or_else(|| panic!("`lute {}` printed no project note:\n{stderr}", args.join(" ")))
        .to_string()
}

const RELATIVE: &str =
    "lute: note: using project proj (nearest lute.project.yaml); pass --project to choose another";
const HERE: &str =
    "lute: note: using project . (nearest lute.project.yaml); pass --project to choose another";

#[test]
fn trace_context_and_compile_stream_name_the_project_relative_to_cwd() {
    let parent = fixture("relative");
    let cases: [(&[&str], Option<&str>); 3] = [
        (&["trace", "proj/scene.lute"], None),
        (&["context", "proj/scene.lute"], None),
        (&["compile-stream", "proj/scene.lute"], Some("")),
    ];
    for (args, stdin) in cases {
        assert_eq!(note_of(&parent, args, stdin), RELATIVE, "lute {}", args.join(" "));
    }
    // Run from inside the project, the note names it `.`.
    assert_eq!(note_of(&parent.join("proj"), &["trace", "scene.lute"], None), HERE);
    let _ = std::fs::remove_dir_all(&parent);
}
