//! In-process entry points for `lute-bench` (dsl 0.36.0 §5): the play runner
//! `lute play` and `lute test` use, without spawning the binary. Not a stable
//! API; only the benchmark harness calls it.

use std::path::Path;

use crate::play::{run_play_for_test, PlayProject};

/// A project compiled for play, as `lute test` compiles it once per run.
pub struct Project(PlayProject);

/// Compile the project at `dir` for play: model build, manifest gate and
/// `ExecProject` assembly. `Err` names why no play can run over it.
pub fn compile(dir: &Path) -> Result<Project, String> {
    let project = PlayProject::compile(dir);
    match project.error() {
        Some(e) => Err(e.to_string()),
        None => Ok(Project(project)),
    }
}

/// Run the play script at `script` over `project`, judging its `expect:`
/// blocks. Returns the number of missed expectations.
pub fn play(project: &Project, script: &Path) -> Result<usize, String> {
    run_play_for_test(&project.0, script, true).map(|run| run.misses.len())
}
