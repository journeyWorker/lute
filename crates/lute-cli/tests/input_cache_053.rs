//! Spec 0.53.0 §3.3, §3.4, §3.8 and §3.9 item 2: the commands that read
//! two or more documents assemble them through ONE input cache for the run,
//! and the commands that read one manifest field build no project model.
//!
//! The counter is the `LUTE_STATS=input-cache` line (§3.7): the bases,
//! vocabularies and assemblies of every cache the run used, a model's
//! included. "Once" is per cache key: a base per distinct (root, profile,
//! `plugins:`), a vocabulary per distinct (base, directory, `uses:`,
//! `extends:`). On 0.52 every document built its own (`build_input` on a
//! fresh cache), and `doctor`, `catalog refresh` and `scenario reach
//! --endings` built a whole model besides.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const MANIFEST: &str =
    "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n  other:\n    plugins: {}\n";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// A project root: the manifest, a `bump` component under `components/` and
/// two scenes under `scenes/` that use it. Two directories, so two
/// vocabulary keys under one base.
fn project(root: &Path) {
    write(&root.join("lute.project.yaml"), MANIFEST);
    write(
        &root.join("components/bump.component.lute"),
        "---\ncomponent: bump\n---\n\n## Bump\n\n@narrator: Before.\n",
    );
    scene(root, "a", None);
    scene(root, "b", None);
}

/// A scene of `root` named `name` that uses `bump`, selecting `profile`
/// when given.
fn scene(root: &Path, name: &str, profile: Option<&str>) {
    let profile = profile.map_or_else(String::new, |p| format!("profile: {p}\n"));
    write(
        &root.join(format!("scenes/{name}.lute")),
        &format!(
            "---\nkind: scene\nid: scene.{name}\n{profile}components: [../components/bump.component.lute]\n\
             ---\n## Shot 1.\n::bg{{location=\"a\"}}\n@alice: hi\n::use{{component=\"bump\"}}\n"
        ),
    );
}

/// Run `lute args…` in `cwd` with the counter on; the output and the stats
/// line's counts (`bases/vocabs/assemblies`).
fn counted(cwd: &Path, args: &[&str]) -> (Output, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("LUTE_STATS", "input-cache")
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find_map(|l| l.strip_prefix("lute: stats: input-cache "))
        .unwrap_or_else(|| panic!("no stats line for {args:?}: {stderr}"));
    let counts = line
        .split(' ')
        .map(|kv| kv.split_once('=').unwrap().1)
        .collect::<Vec<_>>()
        .join("/");
    (out, counts)
}

/// [`counted`], asserting the exit code.
fn counts(cwd: &Path, args: &[&str], exit: i32) -> String {
    let (out, counts) = counted(cwd, args);
    assert_eq!(
        out.status.code(),
        Some(exit),
        "{args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    counts
}

/// §3.3 `check <file>…`: two documents of one root, one base and one
/// vocabulary (0.52: one per document).
#[test]
fn check_files_two_documents_build_one_base() {
    let dir = lute_scratch::ScratchDir::named("l2-check-two-docs");
    project(&dir);
    let args = ["check", "scenes/a.lute", "scenes/b.lute"];
    assert_eq!(counts(&dir, &args, 0), "1/1/2");
}

/// §3.3 `check <file>…` over two roots: one base and one vocabulary each.
#[test]
fn check_files_two_roots_build_one_base_each() {
    let dir = lute_scratch::ScratchDir::named("l2-check-two-roots");
    project(&dir.join("one"));
    project(&dir.join("two"));
    let args = [
        "check",
        "one/scenes/a.lute",
        "one/scenes/b.lute",
        "two/scenes/a.lute",
        "two/scenes/b.lute",
    ];
    assert_eq!(counts(&dir, &args, 0), "2/2/4");
}

/// §3.1 "once per cache key": a document selecting another profile keeps
/// its own base, and so its own vocabulary.
#[test]
fn check_files_two_profiles_build_two_bases() {
    let dir = lute_scratch::ScratchDir::named("l2-check-two-profiles");
    project(&dir);
    scene(&dir, "c", Some("other"));
    let args = ["check", "scenes/a.lute", "scenes/b.lute", "scenes/c.lute"];
    assert_eq!(counts(&dir, &args, 0), "2/2/3");
}

/// §3.3 `check <component>`: the callers assemble through the run's cache,
/// so the component (its own directory) and its two callers (`scenes/`)
/// build one base and two vocabularies (0.52: one of each per document).
#[test]
fn component_check_assembles_its_callers_through_the_run_cache() {
    let dir = lute_scratch::ScratchDir::named("l2-check-component");
    project(&dir);
    let args = ["check", "components/bump.component.lute"];
    assert_eq!(counts(&dir, &args, 0), "1/2/3");
}

/// §3.3 `loc export` and `loc report`: one cache for the walk.
#[test]
fn loc_walks_build_one_base_per_key() {
    let dir = lute_scratch::ScratchDir::named("l2-loc");
    project(&dir);
    let export = ["loc", "export", "--project", ".", "-o", "loc.json"];
    let report = ["loc", "report", "--project", "."];
    assert_eq!(counts(&dir, &export, 0), "1/2/3");
    assert_eq!(counts(&dir, &report, 0), "1/2/3");
    scene(&dir, "c", Some("other"));
    assert_eq!(counts(&dir, &export, 0), "2/3/4");
    assert_eq!(counts(&dir, &report, 0), "2/3/4");
}

/// §3.3 `loc` over a walk spanning two subprojects: each root's keys once.
#[test]
fn loc_walk_over_two_roots_builds_one_base_each() {
    let dir = lute_scratch::ScratchDir::named("l2-loc-two-roots");
    project(&dir.join("one"));
    project(&dir.join("two"));
    let args = ["loc", "report", "--project", "."];
    assert_eq!(counts(&dir, &args, 0), "2/4/6");
}

/// §3.3 and §3.4 `doctor`: one cache for the walk, and the `catalogDir`
/// read from it, not from a project model (0.52: a base per document plus
/// a whole model).
#[test]
fn doctor_builds_one_base_per_key_and_no_model() {
    let dir = lute_scratch::ScratchDir::named("l2-doctor");
    project(&dir);
    let args = ["doctor", "--project", "."];
    assert_eq!(counts(&dir, &args, 0), "1/2/3");
    scene(&dir, "c", Some("other"));
    assert_eq!(counts(&dir, &args, 0), "2/3/4");
    let two = lute_scratch::ScratchDir::named("l2-doctor-two-roots");
    project(&two.join("one"));
    project(&two.join("two"));
    assert_eq!(counts(&two, &args, 0), "2/4/6");
}

/// §3.3 standalone `lute test` (no manifest): the documents assemble
/// through the run's one cache.
#[test]
fn standalone_tests_build_one_base() {
    let dir = lute_scratch::ScratchDir::named("l2-test-standalone");
    for name in ["a", "b"] {
        write(
            &dir.join(format!("{name}.lute")),
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n@narrator: a.\n",
        );
        write(
            &dir.join(format!("{name}.test.yaml")),
            &format!("file: {name}.lute\nexpect:\n  end: complete\n"),
        );
    }
    assert_eq!(counts(&dir, &["test", "--project", "."], 0), "1/1/2");
}

/// §3.4 `scenario reach --endings`: the producers read the manifest
/// without a second project model, so the run builds the report's model
/// only (0.52: two models).
#[test]
fn endings_build_one_model() {
    let dir = lute_scratch::ScratchDir::named("l2-endings");
    project(&dir);
    let args = ["scenario", "reach", "--endings", "--project", "."];
    assert_eq!(counts(&dir, &args, 0), "1/2/3");
}

/// §3.4 `catalog refresh`: the `catalogDir` comes from the manifest; no
/// document is assembled.
#[test]
fn catalog_refresh_assembles_no_document() {
    let dir = lute_scratch::ScratchDir::named("l2-catalog");
    project(&dir);
    assert_eq!(
        counts(&dir, &["catalog", "refresh", "--project", "."], 0),
        "0/0/0"
    );
}

/// A root with a custom `catalogDir`, one provider snapshot in it, a clean
/// scene and a sibling `bad.lute` that is not valid UTF-8.
fn custom_catalog_with_unreadable_sibling(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir.join("lute.project.yaml"),
        &format!("catalogDir: providers/\n{MANIFEST}"),
    );
    write(
        &dir.join("scenes/a.lute"),
        "---\nkind: scene\nid: scene.a\n---\n## Shot 1.\n::bg{location=\"a\"}\n@alice: hi\n",
    );
    std::fs::write(
        dir.join("scenes/bad.lute"),
        b"---\nkind: scene\n---\n\xff\xfe\n",
    )
    .unwrap();
    dir
}

/// Spec 0.53.0 §3.9 item 2: `catalog refresh` refreshes the manifest's
/// `catalogDir` also when a document of the root is unreadable. 0.52 built
/// a model to read the field, the unreadable sibling failed it, and the
/// refresh fell back to `catalog/`.
#[test]
fn catalog_refresh_uses_the_custom_catalog_dir_beside_an_unreadable_document() {
    let dir = custom_catalog_with_unreadable_sibling("l2-catalog-custom");
    let out = Command::new(BIN)
        .args(["catalog", "refresh", "--project", "."])
        .current_dir(&dir)
        .env_remove("LUTE_STATS")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(dir.join("providers").is_dir(), "{stderr}");
    assert!(!dir.join("catalog").exists(), "{stderr}");
}

/// Spec 0.53.0 §3.9 item 2: `doctor` reads the provider snapshots at the
/// manifest's `catalogDir` also when a document of the root is unreadable
/// (0.52: `catalog/`, absent here, so "no pinned provider snapshots").
#[test]
fn doctor_uses_the_custom_catalog_dir_beside_an_unreadable_document() {
    let dir = custom_catalog_with_unreadable_sibling("l2-doctor-custom");
    std::fs::create_dir_all(dir.join("providers")).unwrap();
    let out = Command::new(BIN)
        .args(["doctor", "--project", ".", "--json"])
        .current_dir(&dir)
        .env_remove("LUTE_STATS")
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    let detail = report["checks"]["providers"]["detail"]
        .as_str()
        .unwrap_or_else(|| panic!("{stdout}"));
    assert_eq!(detail, "0 snapshot(s) at ./providers/, none stale");
}
