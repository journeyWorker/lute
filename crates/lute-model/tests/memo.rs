//! `ModelMemo` shares one build per root and options: never across different
//! options (a compiled model's checks carry compile diagnostics an uncompiled
//! one lacks), and a failed build is the same failure for every consumer.

use lute_model::{ModelMemo, ModelOptions};
use std::sync::Arc;

fn project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    std::fs::write(dir.join("lute.project.yaml"), "defaultProfile: default\n").unwrap();
    std::fs::write(
        dir.join("scene.lute"),
        "---\nkind: scene\ncharacter: n\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@n: line\n",
    )
    .unwrap();
    dir
}

fn opts(compile: bool) -> ModelOptions {
    ModelOptions {
        compile,
        ..ModelOptions::default()
    }
}

#[test]
fn one_build_per_root_and_options() {
    let dir = project("share");
    let memo = ModelMemo::default();
    let compiled = memo.single_root(&dir, &opts(true)).unwrap();
    assert!(Arc::ptr_eq(
        &compiled,
        &memo.single_root(&dir, &opts(true)).unwrap()
    ));
    let checked = memo.single_root(&dir, &opts(false)).unwrap();
    assert!(!Arc::ptr_eq(&compiled, &checked));
    // `roots_under` reaches the same per-root builds.
    let roots = memo.roots_under(&dir, &opts(false)).unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].0, dir.to_path_buf());
    assert!(Arc::ptr_eq(&roots[0].1, &checked));
    assert_eq!(memo.builds(), 2);
}

/// A selecting memo serves every request for a root, however spelled and
/// whatever `compile`/`mode` it asks for, from its one selected build; the
/// analysis-only results are what a `compile: false` model reports.
#[test]
fn a_selecting_memo_builds_each_root_once() {
    let dir = project("select");
    let memo = ModelMemo::selecting(opts(true));
    let selected = memo.single_root(&dir, &opts(true)).unwrap();
    let respelled = dir.join(".");
    let checked = memo
        .single_root(
            &respelled,
            &ModelOptions {
                mode: lute_check::Mode::Author,
                ..opts(false)
            },
        )
        .unwrap();
    assert!(Arc::ptr_eq(&selected, &checked));
    let roots = memo.roots_under(&respelled, &opts(false)).unwrap();
    assert_eq!(roots[0].0, respelled);
    assert!(Arc::ptr_eq(&roots[0].1, &selected));
    assert_eq!(memo.builds(), 1);

    let plain = lute_model::ProjectModel::build_single_root(&dir, &opts(false)).unwrap();
    let json = |checks: Vec<(&std::path::PathBuf, &lute_check::CheckResult)>| {
        serde_json::to_value(checks).unwrap()
    };
    assert_eq!(
        json(selected.analysis_checks().collect()),
        json(
            plain
                .reconciled()
                .checks
                .iter()
                .map(|(p, c)| (p, c))
                .collect()
        )
    );
    assert_eq!(selected.analysis_diagnostics(), plain.project_diagnostics());
}

#[test]
fn a_failed_build_is_replayed_to_every_consumer() {
    let dir_scratch = lute_scratch::ScratchDir::named("fail");
    let dir = dir_scratch.join("missing");
    let memo = ModelMemo::default();
    let Err(first) = memo.single_root(&dir, &opts(false)) else {
        panic!("a missing root fails to build")
    };
    let Err(second) = memo.single_root(&dir, &opts(false)) else {
        panic!("the failure is shared")
    };
    assert!(Arc::ptr_eq(&first, &second));
    memo.roots_under(&dir, &opts(false)).unwrap_err();
}
