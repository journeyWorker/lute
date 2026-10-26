//! dsl 0.51.0 §3.4 "Envelope consumers": the guaranteed-write set of a
//! scene whose guarded `::jump` skips a `::set` no longer holds the skipped
//! path, so the envelopes the project analyses recompute from it change with
//! it — `lute scenario envelope` (`lute-model` `scenario.rs`) and the whole-
//! project `check` reconciliation (`reconcile.rs`), neither of which changed.
//! On 0.50.0 both report `run.a` guaranteed downstream (checked on the
//! 0.50.0 build).

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Scene `a` setting `run.a` (behind a guarded jump that skips it when
/// `skip`), and scene `b` after it, reading `run.a`.
fn project(tag: &str, skip: bool) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    let jump = if skip {
        "::jump{to=\"done\" when=\"run.f\"}\n"
    } else {
        ""
    };
    write(
        &dir,
        "a.lute",
        &format!(
            "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nstate:\n  run.a: {{ type: int \
             }}\n  run.f: {{ type: bool, default: false }}\n---\n## Shot 1.\n{jump}\
             ::set{{run.a = 1}}\n::label{{name=\"done\"}}\n@narrator: Done.\n"
        ),
    );
    write(
        &dir,
        "b.lute",
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\nafter: 'visited(\"a.s01ep01\")'\n\
         state:\n  run.a: { type: int }\n  run.out: { type: int }\n---\n## Shot 1.\n\
         ::set{run.out = run.a}\n",
    );
    dir
}

/// `lute scenario envelope --node b.s01ep01`.
fn envelope(dir: &Path) -> String {
    let out = run(
        dir,
        &[
            "scenario",
            "--project",
            ".",
            "envelope",
            "--node",
            "b.s01ep01",
        ],
    );
    let text = stdout(&out);
    assert!(out.status.success(), "{text}");
    text
}

/// The `Guaranteed` table of [`envelope`].
fn guaranteed(dir: &Path) -> String {
    let text = envelope(dir);
    let after = text
        .split_once("Guaranteed (safe to read")
        .unwrap_or_else(|| panic!("no Guaranteed heading: {text}"))
        .1;
    after
        .split_once("Possible (set on")
        .unwrap_or_else(|| panic!("no Possible heading: {text}"))
        .0
        .to_string()
}

#[test]
fn a_skipped_set_leaves_the_downstream_scenario_envelope() {
    assert!(guaranteed(&project("ej-scenario-set", false)).contains("run.a"));
    let skipped = guaranteed(&project("ej-scenario-skip", true));
    assert!(!skipped.contains("run.a"), "{skipped}");
}

/// The project reconciliation reclassifies `b`'s read of `run.a` against
/// the envelope: no longer guaranteed, it is a warning-grade read (possible,
/// not guaranteed), which whole-project `check` suppresses by default and
/// `lute scenario envelope` lists.
#[test]
fn a_skipped_set_makes_the_downstream_read_warning_grade() {
    let reads = |text: &str| -> Vec<String> {
        text.split_once("warning-grade reads")
            .unwrap_or_else(|| panic!("no warning-grade section: {text}"))
            .1
            .lines()
            .filter(|l| l.contains("b.lute"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        reads(&envelope(&project("ej-read-set", false))),
        Vec::<String>::new()
    );
    let skipped = reads(&envelope(&project("ej-read-skip", true)));
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert!(skipped[0].contains("`run.a`"), "{skipped:?}");
    // Default whole-project `check` reports neither.
    for (tag, skip) in [("ej-check-set", false), ("ej-check-skip", true)] {
        let out = run(&project(tag, skip), &["check", "--project", "."]);
        assert!(out.status.success(), "{tag}: {}", stdout(&out));
    }
}
