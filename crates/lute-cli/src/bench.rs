//! In-process entry points for `lute-bench` (dsl 0.36.0 §5): the play runner
//! `lute play` and `lute test` use, without spawning the binary. Not a stable
//! API; only the benchmark harness calls it.

use std::path::Path;

use crate::play::{run_play_for_test, PlayProject};

/// A project compiled for play, as `lute test` compiles it once per run.
#[derive(Debug)]
pub struct Project(PlayProject);

/// Compile the project at `dir` for play: model build, manifest gate and
/// `ExecProject` assembly.
///
/// # Errors
///
/// [`BenchError::Compile`]: why no play can run over the project.
pub fn compile(dir: &Path) -> Result<Project, BenchError> {
    let project = PlayProject::compile(dir, &crate::play::Build::default());
    match project.error() {
        Some(e) => Err(BenchError::Compile(e.to_string())),
        None => Ok(Project(project)),
    }
}

/// Run the play script at `script` over `project`, judging its `expect:`
/// blocks. Returns the number of missed expectations.
///
/// # Errors
///
/// [`BenchError::Play`]: the script does not load or plan.
pub fn play(project: &Project, script: &Path) -> Result<usize, BenchError> {
    run_play_for_test(&project.0, script, true)
        .map(|run| run.misses.len())
        .map_err(BenchError::Play)
}

/// Why a benchmark sample could not run. Each holds the message `lute test`
/// reports for the same failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BenchError {
    /// The project does not compile for play.
    Compile(String),
    /// A play script does not load or plan.
    Play(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchError::Compile(message) | BenchError::Play(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BenchError {}
