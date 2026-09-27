//! dsl 0.27.0 T1-5(b), through the built `lute` binary: a `<match>` arm
//! whose subject has no portable `expr` (a `@def` that expands to
//! `visited('…')`) compiles to a raw CEL `test`, so `lute play` — which
//! executes the IR — takes the arm the author meant instead of falling to
//! `<otherwise>` on an empty, unknown guard.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-matchdef-{tag}-{}-{n}", std::process::id()));
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

/// `found` presents `find`; `poke` presents `m`, whose `<match>` reads
/// `visited('find')` through a def — true once `found` was raised.
fn project(tag: &str, arms: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: g\nprofiles:\n  g: { plugins: {} }\ndefaults:\n  luteVersion: \"0.27.0\"\n",
    );
    write(
        &dir,
        "find.lute",
        "---\nkind: scene\nid: find\non: found\n---\n\n## Find\n\n@narrator: Found him.\n",
    );
    write(
        &dir,
        "m.lute",
        &format!(
            "---\nkind: scene\nid: m\non: poke\nstate:\n  run.mood: {{ type: {{ enum: [calm, tense] }}, default: tense }}\n\
             defs:\n  seen: \"visited('find')\"\n---\n\n## M\n\n{arms}\n"
        ),
    );
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: found\n  - occasion: poke\n",
    );
    dir
}

fn arms_of(dir: &Path) -> Vec<serde_json::Value> {
    // The artifact is stdout; stderr carries the nearest-project note.
    let o = Command::new(BIN)
        .args(["compile", "m.lute"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let art: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "match")
        .unwrap()["arms"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_def_subject_without_a_portable_expr_takes_its_true_arm_in_play() {
    let dir = project(
        "visited",
        "<match on=\"@seen\">\n  <when is=\"true\">\n    @narrator: (match arm) He is here.\n  </when>\n  \
         <otherwise>\n    @narrator: (match otherwise) Alone.\n  </otherwise>\n</match>",
    );
    let arms = arms_of(&dir);
    assert_eq!(arms[0]["test"], "(visited('find')) == true", "{arms:#?}");
    assert!(arms[0].get("expr").is_none(), "{arms:#?}");

    let (code, out) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("(match arm) He is here."), "{out}");
    assert!(!out.contains("Alone."), "{out}");
}

/// An `is` arm whose `test` has no portable `expr` must still compare the
/// subject: the arm is `is` AND `test`, never the `test` alone.
#[test]
fn an_is_arm_with_an_unportable_test_still_compares_its_subject() {
    let dir = project(
        "isandtest",
        "<match on=\"run.mood\">\n  <when is=\"calm\" test=\"@seen\">\n    @narrator: Calm and seen.\n  </when>\n  \
         <otherwise>\n    @narrator: Not calm.\n  </otherwise>\n</match>",
    );
    let arms = arms_of(&dir);
    assert_eq!(
        arms[0]["test"], "run.mood == 'calm' && (visited('find'))",
        "{arms:#?}"
    );

    let (code, out) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("Not calm."), "{out}");
    assert!(!out.contains("Calm and seen."), "{out}");
}
