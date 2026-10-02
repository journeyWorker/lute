//! dsl 0.27.0 §2, through the built `lute` binary: a directive's
//! `effects.writes` value shape is validated at plugin load, and a
//! `{ fromAttr }` value / `by:` is applied.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-strict027-{tag}-{}-{n}", std::process::id()));
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

/// The round-5 hollow-ward shape: `::fright{amount=N}` moves an
/// engine-owned `run.sanity` by the call's `amount`.
fn project(tag: &str, value: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { p: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.sanity: { type: int, default: 10, owner: engine }\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  directives: directives/\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions:\n  poke: { select: first }\n",
    );
    write(
        &dir,
        "plugins/p/directives/d.yaml",
        &format!(
"directives:\n  - name: fright\n    attrs:\n      - {{ name: amount, type: int }}\n      \
             - {{ name: mood, type: string }}\n    effects:\n      writes:\n        \
             - {{ scope: run, path: [sanity], value: {value} }}\n"
        ),
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\non: poke\nonce: false\n---\n\n## A\n\n::fright{amount=2}\n\
         @narrator: Sanity {{run.sanity}}.\n",
    );
    dir
}

#[test]
fn an_op_by_from_attr_is_applied_by_play() {
    let dir = project("by-attr", "{ op: decrement, by: { fromAttr: amount } }");
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: poke\n  - occasion: poke\nexpect:\n  state: { run.sanity: 6 }\n",
    );
    let (code, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("Sanity 8.") && t.contains("Sanity 6."), "{t}");
}

#[test]
fn a_value_from_attr_is_applied_by_play() {
    let dir = project("value-attr", "{ fromAttr: amount }");
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: poke\nexpect:\n  state: { run.sanity: 2 }\n",
    );
    let (code, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{t}");
}

#[test]
fn an_unknown_write_value_shape_fails_the_plugin_load() {
    for (value, says) in [
        ("{ fromAtr: amount }", "did you mean `fromAttr`?"),
        (
            "{ op: increment, by: { fromAttr: amount }, extra: 1 }",
            "is not a write source",
        ),
        ("{ op: add, by: 1 }", "`op: add` is not an op"),
        (
            "{ op: increment, by: two }",
            "`by:` must be a number or `{ fromAttr: <attr> }`",
        ),
        (
            "{ op: increment, by: { name: amount } }",
            "`by` must be `{ fromAttr: <attr name> }`",
        ),
        ("[1, 2]", "is a list"),
    ] {
        let dir = project("bad-shape", value);
        let (code, t) = run(&dir, &["check-project", "."]);
        assert_ne!(code, Some(0), "{value}: {t}");
        assert!(t.contains("E-PLUGIN-PARSE"), "{value}: {t}");
        assert!(t.contains(says), "{value}: {t}");
    }
}

#[test]
fn a_from_attr_must_name_a_declared_attr_and_a_by_one_a_number() {
    for (value, says) in [
        (
            "{ fromAttr: amont }",
            "declares no such attr (declared: amount, mood); did you mean `amount`?",
        ),
        (
            "{ op: increment, by: { fromAttr: mood } }",
            "needs an `int` attr",
        ),
    ] {
        let dir = project("bad-attr", value);
        let (code, t) = run(&dir, &["check-project", "."]);
        assert_ne!(code, Some(0), "{value}: {t}");
        assert!(t.contains("E-PLUGIN-PARSE"), "{value}: {t}");
        assert!(t.contains(says), "{value}: {t}");
    }
}

// -- T1-10: a param `default: "@def"` is judged at each `::use` ---------------

#[test]
fn an_omitted_param_default_def_is_judged_for_definite_assignment_at_the_use() {
    let dir = temp_dir("param-default");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: base\nprofiles:\n  base: {}\ndefaults:\n  uses: [world.schema.yaml]\n  \
         components: [gauge.component.lute]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.depth: { type: int, default: 0 }\ndefs:\n  lastFathoms: \"prev.run.depth * 10\"\n",
    );
    write(
        &dir,
        "gauge.component.lute",
        "---\ncomponent: gauge\nparams:\n  fathoms: { type: int, default: \"@lastFathoms\" }\n---\n\n\
         ## Gauge\n\n@narrator: The gauge shows {{@fathoms}} fathoms.\n",
    );
    write(
        &dir,
        "defaulted.lute",
        "---\nkind: scene\nid: defaulted\n---\n\n## A\n\n::use{component=\"gauge\"}\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_ne!(code, Some(0), "{t}");
    let line = t
        .lines()
        .find(|l| l.contains("E-MAYBE-UNSET"))
        .unwrap_or_else(|| panic!("{t}"));
    assert!(
        line.starts_with("./defaulted.lute:8:") && line.contains("prev.run.depth"),
        "{t}"
    );
    // An explicit argument that is assigned is clean: the default is not read.
    write(
        &dir,
        "defaulted.lute",
        "---\nkind: scene\nid: defaulted\n---\n\n## A\n\n::use{component=\"gauge\" fathoms=\"3\"}\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
}
