//! 0.41.0 §3.3: summaries count `info` separately, appended only when
//! non-zero, so a run without info prints the summary it always printed.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn run(args: &[&str], dir: &PathBuf) -> String {
    let out = Command::new(BIN).args(args).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn summaries_append_the_info_count_only_when_non_zero() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depends-invalid");
    let project = run(&["check-project", "."], &dir);
    assert!(project.contains("ok: ./scenes/probe/c.lute (0 warning(s), 2 info)\n"), "{project}");
    assert!(project.contains("ok: ./scenes/talk/mara-idle.lute (0 warning(s))\n"), "{project}");
    assert!(
        project.ends_with(
            "failed: . (8 file(s), 8 project-wide error(s), 0 project-wide warning(s), 1 project-wide info)\n"
        ),
        "{project}"
    );
    let clean = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bridge-check");
    let clean_out = run(&["check-project", "."], &clean);
    let last = clean_out.lines().last().unwrap_or_default();
    assert!(last.starts_with("ok: . (") && !last.contains("info"), "{clean_out}");
}
