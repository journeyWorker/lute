//! Semantic project diff command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use lute_model::{diff, ModelOptions, ProjectModel};

struct Materialized {
    root: PathBuf,
    cleanup: bool,
}
impl Drop for Materialized {
    fn drop(&mut self) {
        if self.cleanup { let _ = std::fs::remove_dir_all(&self.root); }
    }
}

fn validate_archive_entries(listing: &[u8], detailed: &[u8]) -> Result<(), String> {
    for raw in String::from_utf8_lossy(listing).lines() {
        let name = raw.trim_end_matches('/');
        let path = Path::new(name);
        if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("unsafe git archive entry `{name}`"));
        }
    }
    for raw in String::from_utf8_lossy(detailed).lines() {
        let Some(kind) = raw.as_bytes().first().copied() else { continue };
        if !matches!(kind, b'd' | b'-') {
            return Err("git archive contains a symlink, submodule, or special file".into());
        }
    }
    Ok(())
}

fn materialize(side: &Path) -> Result<Materialized, String> {
    let text = side.to_string_lossy();
    if !text.starts_with("git:") {
        if !side.is_dir() { return Err(format!("{} is not a project directory", side.display())); }
        return Ok(Materialized { root: side.to_path_buf(), cleanup: false });
    }
    let revision = text.strip_prefix("git:").filter(|s| !s.is_empty()).ok_or_else(|| "empty git revision".to_string())?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let root = std::env::temp_dir().join(format!("lute-diff-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&root).map_err(|e| format!("cannot create temporary directory: {e}"))?;
    let archive = root.join("snapshot.tar");
    let status = Command::new("git")
        .args(["archive", "--format=tar", revision])
        .output()
        .map_err(|e| format!("cannot invoke git archive: {e}"))?;
    if !status.status.success() {
        let _ = std::fs::remove_dir_all(&root);
        return Err(format!("git archive {revision} failed: {}", String::from_utf8_lossy(&status.stderr).trim()));
    }
    std::fs::write(&archive, status.stdout).map_err(|e| format!("cannot write git archive: {e}"))?;
    let listing = Command::new("tar").args(["-tf"]).arg(&archive).output().map_err(|e| e.to_string())?;
    if !listing.status.success() { let _ = std::fs::remove_dir_all(&root); return Err("cannot inspect git archive".into()); }
    let detailed = Command::new("tar").args(["-tvf"]).arg(&archive).output().map_err(|e| e.to_string())?;
    if !detailed.status.success() { let _ = std::fs::remove_dir_all(&root); return Err("cannot inspect git archive entry types".into()); }
    if let Err(error) = validate_archive_entries(&listing.stdout, &detailed.stdout) {
        let _ = std::fs::remove_dir_all(&root);
        return Err(error);
    }
    let status = Command::new("tar").args(["-xf"]).arg(&archive).args(["-C"]).arg(&root).status().map_err(|e| e.to_string())?;
    if !status.success() { let _ = std::fs::remove_dir_all(&root); return Err("cannot extract git archive".into()); }
    let _ = std::fs::remove_file(&archive);
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
                let _ = std::fs::remove_dir_all(&root);
                return Err(format!("unsafe materialized archive entry `{}`", path.display()));
            }
            if metadata.is_dir() { stack.push(path); }
        }
    }
    Ok(Materialized { root, cleanup: true })
}

pub(crate) fn run(before: &Path, after: &Path, json: bool) -> ExitCode {
    let before = match materialize(before) { Ok(side) => side, Err(e) => { eprintln!("lute diff: {e}"); return ExitCode::from(2); } };
    let after = match materialize(after) { Ok(side) => side, Err(e) => { eprintln!("lute diff: {e}"); return ExitCode::from(2); } };
    let opts = ModelOptions { compile: true, ..ModelOptions::default() };
    let before_model = match ProjectModel::build_single_root(&before.root, &opts) { Ok(model) => model, Err(e) => { eprintln!("lute diff: cannot build before model: {e}"); return ExitCode::from(2); } };
    let after_model = match ProjectModel::build_single_root(&after.root, &opts) { Ok(model) => model, Err(e) => { eprintln!("lute diff: cannot build after model: {e}"); return ExitCode::from(2); } };
    let report = match diff::diff_models(&before_model, &after_model) { Ok(report) => report, Err(e) => { eprintln!("lute diff: {e}"); return ExitCode::from(2); } };
    let output: Result<String, String> = if json {
        serde_json::to_string_pretty(&report).map(|text| format!("{text}\n")).map_err(|e| e.to_string())
    } else {
        Ok(diff::human(&report))
    };
    match output {
        Ok(text) => { if crate::output::write_stdout(&text).is_ok() { ExitCode::SUCCESS } else { ExitCode::from(2) } }
        Err(e) => { eprintln!("lute diff: cannot serialize report: {e}"); ExitCode::from(2) }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_archive_entries;

    #[test]
    fn archive_validator_rejects_unsafe_names_and_types() {
        assert!(validate_archive_entries(b"/absolute\n", b"-rw-r--r-- x x 1 file\n").is_err());
        assert!(validate_archive_entries(b"../escape\n", b"-rw-r--r-- x x 1 file\n").is_err());
        assert!(validate_archive_entries(b"safe\n", b"lrwxrwxrwx x x 1 link\n").is_err());
        assert!(validate_archive_entries(b"safe/\n", b"drwxr-xr-x x x 1 safe/\n").is_ok());
    }
}
