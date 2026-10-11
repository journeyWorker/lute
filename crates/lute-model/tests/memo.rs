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
    ModelOptions { compile, ..ModelOptions::default() }
}

#[test]
fn one_build_per_root_and_options() {
    let dir = project("share");
    let memo = ModelMemo::default();
    let compiled = memo.single_root(&dir, &opts(true)).unwrap();
    assert!(Arc::ptr_eq(&compiled, &memo.single_root(&dir, &opts(true)).unwrap()));
    let checked = memo.single_root(&dir, &opts(false)).unwrap();
    assert!(!Arc::ptr_eq(&compiled, &checked));
    // `roots_under` reaches the same per-root builds.
    let roots = memo.roots_under(&dir, &opts(false)).unwrap();
    assert_eq!(roots.len(), 1);
    assert!(Arc::ptr_eq(&roots[0], &checked));
}

#[test]
fn a_failed_build_is_replayed_to_every_consumer() {
    let dir_scratch = lute_scratch::ScratchDir::named("fail");
    let dir = dir_scratch.join("missing");
    let memo = ModelMemo::default();
    let Err(first) = memo.single_root(&dir, &opts(false)) else { panic!("a missing root fails to build") };
    let Err(second) = memo.single_root(&dir, &opts(false)) else { panic!("the failure is shared") };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(memo.roots_under(&dir, &opts(false)).is_err());
}
