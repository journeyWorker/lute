//! `ModelMemo` shares one build per root and options: never across different
//! options (a compiled model's checks carry compile diagnostics an uncompiled
//! one lacks), and a failed build is the same failure for every consumer.

use lute_model::{ModelMemo, ModelOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_model_memo_{tag}_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
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
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_build_is_replayed_to_every_consumer() {
    let dir = temp_dir("fail").join("missing");
    let memo = ModelMemo::default();
    let Err(first) = memo.single_root(&dir, &opts(false)) else { panic!("a missing root fails to build") };
    let Err(second) = memo.single_root(&dir, &opts(false)) else { panic!("the failure is shared") };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(memo.roots_under(&dir, &opts(false)).is_err());
}
