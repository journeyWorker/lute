//! #240: the project pass gives its final answer in one reconciliation. The
//! beat shadow and guard passes read the per-file results after the
//! project-reclassified `E-MAYBE-UNSET` is dropped, so reconciling the
//! reconciled results again changes nothing — and `check --project` reports
//! the shadow `lute beats` always reported.

use std::path::{Path, PathBuf};

use lute_check::CheckResult;
use lute_core_span::Diagnostic;
use lute_model::{reconcile_collected, ByRoot, ModelOptions, ProjectModel};

fn write(dir: &Path, rel: &str, text: &str) {
    std::fs::write(dir.join(rel), text).unwrap();
}

/// `y` sets `run.z`; `x` (after `y`, on `dayEnd`) reads it, so its per-file
/// `E-MAYBE-UNSET` is settled by the project envelope; `a`, earlier on
/// `dayEnd`, is always eligible and never spent, so `x` never wins.
fn shadow_project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "y.lute",
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\nstate:\n  run.z: { type: int }\n\
         ---\n## Shot 1.\n::set{run.z = 1}\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 2\non: dayEnd\nonce: false\n---\n\
         ## Shot 1.\n@narrator: always\n",
    );
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 3\non: dayEnd\n\
         after: 'visited(\"y.s01ep01\")'\nstate:\n  run.z: { type: int }\n  run.out: { type: int }\n\
         ---\n## Shot 1.\n::set{run.out = run.z}\n::end\n",
    );
    dir
}

fn model(dir: &Path) -> ProjectModel {
    let opts = ModelOptions {
        compile: false,
        ..ModelOptions::default()
    };
    ProjectModel::build_single_root(dir, &opts).expect("the project builds")
}

type Checks = Vec<(PathBuf, bool, Vec<Diagnostic>)>;

fn comparable(checks: &[(PathBuf, CheckResult)]) -> Checks {
    checks
        .iter()
        .map(|(p, r)| (p.clone(), r.ok, r.diagnostics.clone()))
        .collect()
}

#[test]
fn check_project_reports_a_shadow_found_after_reclassification() {
    let dir = shadow_project("reconcile-shadow");
    let model = model(&dir);
    let shadows: Vec<_> = model
        .project_diagnostics()
        .iter()
        .filter(|(_, d)| d.code == "W-BEAT-SHADOWED")
        .collect();
    assert_eq!(shadows.len(), 1, "{shadows:?}");
    assert!(shadows[0].0.ends_with("x.lute"));
    // The read the envelope settles is not reported per file.
    assert!(model
        .reconciled()
        .checks
        .iter()
        .all(|(_, r)| r.diagnostics.iter().all(|d| d.code != "E-MAYBE-UNSET")));
}

#[test]
fn reconciling_twice_changes_nothing() {
    let dir = shadow_project("reconcile-idempotent");
    let model = model(&dir);
    let once = model.reconciled();
    let mut by_root = ByRoot::new();
    by_root.insert(
        dir.to_path_buf(),
        model
            .documents()
            .iter()
            .map(|d| (d.path.clone(), d.doc.clone(), d.folded.clone()))
            .collect(),
    );
    let (checks, diagnostics, _, _) = reconcile_collected(once.checks.clone(), &by_root, false);
    assert_eq!(comparable(&checks), comparable(&once.checks));
    assert_eq!(diagnostics, once.diagnostics);
}
