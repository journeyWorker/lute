//! Atomic source patch command.

use std::path::Path;
use std::process::ExitCode;

/// Run `lute patch`; the implementation lands in wave 2.
pub(crate) fn run(_dir: &Path, _patch: &Path, _dry_run: bool, _json: bool) -> ExitCode {
    eprintln!("lute patch: command implementation not yet landed");
    ExitCode::from(2)
}
