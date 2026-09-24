//! `lute tag` / `lute fix` — the in-place source rewrites, over one `.lute`
//! file or every `.lute` file under a directory (dsl 0.22.0 §13).
//!
//! A directory is walked recursively with the SAME walk `check-project` uses
//! ([`crate::find_lute_files`]: byte-sorted, symlink aliases deduplicated), so
//! the files rewritten are exactly the files the project checks, in a
//! deterministic order. Each file is rewritten independently: a refused or
//! unreadable file is reported and the walk continues, so one draft with
//! structural errors does not leave the rest of the tree untagged.
//!
//! Exit codes: `0` when every file succeeded (changed or not), `1` when some
//! file was refused (`--force` on a `codesLocked:` or structurally broken
//! document), `2` on an I/O failure (an unreadable path or an unwritable
//! file) — the worst outcome across the files wins.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How one file fared. Ordered by severity so a tree's exit code is the max.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    Unchanged,
    Changed,
    Refused,
    Failed,
}

impl Outcome {
    fn exit_code(self) -> ExitCode {
        match self {
            Outcome::Unchanged | Outcome::Changed => ExitCode::SUCCESS,
            Outcome::Refused => ExitCode::from(1),
            Outcome::Failed => ExitCode::from(2),
        }
    }
}

/// Where a file's messages go: a lone file keeps the historical bare
/// `lute: <message>` lines; inside a tree every line names its file, and a
/// file with nothing to do stays silent (the summary counts it).
#[derive(Clone, Copy)]
enum Scope<'a> {
    Single,
    Tree(&'a Path),
}

impl Scope<'_> {
    fn say(self, msg: &str) {
        match self {
            Scope::Single => println!("lute: {msg}"),
            Scope::Tree(file) => println!("lute: {}: {msg}", file.display()),
        }
    }

    /// A no-op report: printed for a lone file only.
    fn say_unchanged(self, msg: &str) {
        if let Scope::Single = self {
            println!("lute: {msg}");
        }
    }
}

/// The files a `tag`/`fix` invocation rewrites: `path` itself, or every
/// `.lute` file under it when it is a directory. `Err` is the exit code.
fn targets(path: &Path) -> Result<Option<Vec<PathBuf>>, ExitCode> {
    if !path.is_dir() {
        return Ok(None);
    }
    crate::find_lute_files(path).map(Some).map_err(|e| {
        eprintln!("lute: cannot walk {}: {e}", path.display());
        ExitCode::from(2)
    })
}

/// Run `one` over `path` (a file) or every `.lute` file under it (a
/// directory), then — for a directory — print `summary(changed_files,
/// total_files, units)` where `units` sums what each changed file reported.
fn over_path(
    path: &Path,
    mut one: impl FnMut(&Path, Scope<'_>) -> (Outcome, usize),
    summary: impl Fn(usize, usize, usize) -> String,
) -> ExitCode {
    let files = match targets(path) {
        Ok(Some(files)) => files,
        Ok(None) => return one(path, Scope::Single).0.exit_code(),
        Err(code) => return code,
    };
    let mut worst = Outcome::Unchanged;
    let (mut changed, mut units) = (0, 0);
    for file in &files {
        let (outcome, n) = one(file, Scope::Tree(file));
        if outcome == Outcome::Changed {
            changed += 1;
            units += n;
        }
        worst = worst.max(outcome);
    }
    println!("lute: {}", summary(changed, files.len(), units));
    worst.exit_code()
}

fn read(file: &Path) -> Option<String> {
    match std::fs::read_to_string(file) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            None
        }
    }
}

fn write(file: &Path, text: &str) -> bool {
    match std::fs::write(file, text) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("lute: cannot write {}: {e}", file.display());
            false
        }
    }
}

/// Back-fill a stable `code` into every untagged `:line` (dsl §12), rewriting
/// each file in place. A thin shell over [`lute_check::tag_document`] (the
/// pure core that owns the tagging logic): read, tag, and — only when at least
/// one line was tagged — write the result back, so an already-tagged document
/// is left byte-identical (idempotent).
///
/// With `--force`, FORCE-renumber instead ([`lute_check::retag_document`]):
/// every line's code is rewritten in clean document order — a drafting tool.
/// Refused when frontmatter declares `codesLocked:` (published codes are
/// `lineId`/`voiceKey` identity; renumbering severs the localization/voice
/// join) or when the document has structural errors.
pub fn run_tag(path: &Path, force: bool) -> ExitCode {
    over_path(
        path,
        |file, scope| tag_file(file, force, scope),
        |changed, total, lines| {
            let verb = if force { "renumbered" } else { "tagged" };
            format!("{verb} {lines} line(s) in {changed} of {total} file(s)")
        },
    )
}

fn tag_file(file: &Path, force: bool, scope: Scope<'_>) -> (Outcome, usize) {
    let Some(text) = read(file) else {
        return (Outcome::Failed, 0);
    };

    if force {
        return match lute_check::retag_document(&text) {
            lute_check::RetagOutcome::Locked => {
                eprintln!(
                    "lute: {} declares `codesLocked:` — its codes are published identity \
                     (lineId/voiceKey); refusing to renumber. Remove the key or set it \
                     `false` to renumber a draft.",
                    file.display()
                );
                (Outcome::Refused, 0)
            }
            lute_check::RetagOutcome::Broken => {
                eprintln!(
                    "lute: {} has structural errors — fix `lute check` findings first; \
                     nothing was rewritten",
                    file.display()
                );
                (Outcome::Refused, 0)
            }
            lute_check::RetagOutcome::Renumbered {
                text: out,
                renumbered,
                skipped,
            } => {
                let outcome = if renumbered > 0 {
                    if !write(file, &out) {
                        return (Outcome::Failed, 0);
                    }
                    scope.say(&format!("renumbered {renumbered} line(s)"));
                    Outcome::Changed
                } else {
                    scope.say_unchanged("codes already in order");
                    Outcome::Unchanged
                };
                if skipped > 0 {
                    scope.say(&format!("{skipped} line(s) skipped (non-string `code` value)"));
                }
                (outcome, renumbered)
            }
        };
    }

    let out = lute_check::tag_document(&text);
    if out.added == 0 {
        scope.say_unchanged("already tagged");
        return (Outcome::Unchanged, 0);
    }
    if !write(file, &out.text) {
        return (Outcome::Failed, 0);
    }
    scope.say(&format!("tagged {} line(s)", out.added));
    (Outcome::Changed, out.added)
}

/// Apply `lute fix`'s mechanical migrations in place (dsl §7.1, §7.3, 0.18.0
/// §3), rewriting a file only when a span was actually changed. A thin shell
/// over [`lute_check::fix_document`] (the pure core that owns every rule), so
/// an already-migrated document is left byte-identical (idempotent).
pub fn run_fix(path: &Path) -> ExitCode {
    over_path(path, fix_file, |changed, total, fixes| {
        format!("applied {fixes} fix(es) in {changed} of {total} file(s)")
    })
}

fn fix_file(file: &Path, scope: Scope<'_>) -> (Outcome, usize) {
    let Some(text) = read(file) else {
        return (Outcome::Failed, 0);
    };
    let out = lute_check::fix_document(&text);
    if out.changed == 0 {
        scope.say_unchanged("nothing to fix");
        return (Outcome::Unchanged, 0);
    }
    if !write(file, &out.text) {
        return (Outcome::Failed, 0);
    }
    scope.say(&format!("applied {} fix(es)", out.changed));
    (Outcome::Changed, out.changed)
}
