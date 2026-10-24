//! spec 0.49.0 §9.2: on the #173 generator's project, every document folds
//! against the shared project vocab exactly as the forced full merge (the
//! 0.48.1 path) folds it. The project is the one the gate measures
//! (`scripts/vocab-grid.py`), at a size a test can afford.

use std::path::{Path, PathBuf};
use std::process::Command;

use lute_check::{fold_env, fold_env_full_merge};
use lute_load::{assemble_input, InputCache};

fn lute_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lute") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_generator_project_folds_identically() {
    let scratch = lute_scratch::ScratchDir::named("bench-fold-eq");
    let project = scratch.join("proj");
    let status = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args(["generate-vocab", project.to_str().unwrap()])
        .args(["-S", "4", "-N", "8", "-M", "40", "-K", "5", "-P", "15"])
        .status()
        .unwrap();
    assert!(status.success());
    let files = lute_files(&project);
    assert_eq!(files.len(), 12, "{files:?}");
    let cache = InputCache::default();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let (built, (doc, _)) = assemble_input(&cache, file, text, None, Some(&project), None);
        let shared = format!("{:#?}", fold_env(&doc, &built.input));
        let full = format!("{:#?}", fold_env_full_merge(&doc, &built.input));
        assert!(
            shared == full,
            "{}: the shared fold differs from the full merge",
            file.display()
        );
    }
}
