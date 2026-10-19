//! `lute fmt`: deterministic, lossless canonical formatting for source inputs.

use crate::envelope::{self, CliError};
use lute_load::manifest_context;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml")
    )
}

fn is_selected(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) == Some("lute") {
        return true;
    }
    if !is_yaml(path) {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if (name == "lute.project.yaml" || name == "lute.project.yml")
        || name.ends_with(".schema.yaml")
        || name.ends_with(".schema.yml")
    {
        return true;
    }
    configured_plugin_file(path)
}

fn configured_plugin_file(path: &Path) -> bool {
    let mut ancestor = path.parent();
    while let Some(dir) = ancestor {
        if dir.join("lute.project.yaml").is_file() {
            return manifest_context(dir)
                .ok()
                .and_then(|context| context.project)
                .is_some_and(|config| path.starts_with(&config.plugins_dir));
        }
        ancestor = dir.parent();
    }
    false
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_file() {
        if is_selected(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !meta.is_dir() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        collect(&entry.path(), out)?;
    }
    Ok(())
}

/// Run `lute fmt [--check] [<PATH>]… [--json]`.
///
/// Selection and output are sorted by path. Check mode never writes and exits
/// 1 when at least one selected input is noncanonical; I/O, UTF-8, and parser
/// failures use exit 2 (the error envelope with `--json`). `--json` prints the
/// `0.48.0.fmt` report over every selected file; the check-mode path lines go
/// to stderr then.
pub(crate) fn run(paths: &[PathBuf], check: bool, json: bool) -> ExitCode {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    for root in roots {
        if let Err(err) = collect(&root, &mut files) {
            let message = format!("lute fmt: {}: {err}", root.display());
            eprintln!("{message}");
            return envelope::fail_quietly(CliError::io(&root, message));
        }
    }
    files.sort();
    files.dedup();
    let mut rows: Vec<(PathBuf, bool)> = Vec::new();
    for path in files {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                let message = format!("lute fmt: {}: {err}", path.display());
                eprintln!("{message}");
                return envelope::fail_quietly(CliError::io(&path, message));
            }
        };
        let source = match std::str::from_utf8(&bytes) {
            Ok(source) => source,
            Err(err) => {
                let message = format!("lute fmt: {}: invalid UTF-8 ({err})", path.display());
                eprintln!("{message}");
                return envelope::fail_quietly(CliError::input(&path, message));
            }
        };
        let result = if is_yaml(&path) {
            lute_syntax::format_yaml_source(source)
        } else {
            match lute_syntax::format_source(source, &lute_syntax::FormatOptions::default()) {
                Ok(result) => result,
                Err(err) => {
                    let message =
                        format!("lute fmt: {}: formatting failed: {err:?}", path.display());
                    eprintln!("{message}");
                    return envelope::fail_quietly(CliError::input(&path, message));
                }
            }
        };
        rows.push((path.clone(), result.changed));
        if !result.changed {
            continue;
        }
        if check {
            if json {
                eprintln!("{}", path.display());
            } else {
                println!("{}", path.display());
            }
        } else if let Err(err) = std::fs::write(&path, result.text.as_bytes()) {
            let message = format!("lute fmt: {}: {err}", path.display());
            eprintln!("{message}");
            return envelope::fail_quietly(CliError::io(&path, message));
        }
    }
    let changed = rows.iter().any(|(_, changed)| *changed);
    let exit = u8::from(check && changed);
    if !json {
        return ExitCode::from(exit);
    }
    envelope::emit_report(
        envelope::version::FMT,
        exit,
        serde_json::json!({
            "applied": !check && changed,
            "files": rows
                .iter()
                .map(|(path, changed)| serde_json::json!({
                    "path": path.display().to_string(),
                    "changed": changed,
                }))
                .collect::<Vec<_>>(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_exit_codes_and_write_only_when_changed() {
        let root_scratch = lute_scratch::ScratchDir::new();
        let root = root_scratch.join("lute-fmt");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.lute");
        let original = "## S\n@narrator: hi  \n";
        std::fs::write(&file, original).unwrap();
        assert_eq!(
            run(std::slice::from_ref(&file), true, false),
            ExitCode::from(1)
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert_eq!(
            run(std::slice::from_ref(&file), false, false),
            ExitCode::SUCCESS
        );
        let canonical = std::fs::read_to_string(&file).unwrap();
        assert_ne!(canonical, original);
        assert_eq!(
            run(std::slice::from_ref(&file), true, false),
            ExitCode::SUCCESS
        );
        assert_eq!(
            run(&[root.join("missing.lute")], false, false),
            ExitCode::from(2)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn collection_skips_symlink_loops() {
        let root_scratch = lute_scratch::ScratchDir::new();
        let root = root_scratch.join("lute-fmt-loop");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.lute");
        std::fs::write(&file, "## S\n@narrator: hi\n").unwrap();
        std::os::unix::fs::symlink(".", root.join("loop")).unwrap();
        let mut files = Vec::new();
        collect(&root, &mut files).unwrap();
        assert_eq!(files, vec![file]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plugin_yaml_uses_manifest_plugins_dir() {
        let root_scratch = lute_scratch::ScratchDir::new();
        let root = root_scratch.join("lute-fmt-plugin");
        std::fs::create_dir_all(root.join("extensions/pkg")).unwrap();
        std::fs::create_dir_all(root.join("plugins/pkg")).unwrap();
        std::fs::write(
            root.join("lute.project.yaml"),
            "defaultProfile: core\npluginsDir: extensions\nprofiles:\n  core:\n    plugins: {}\n",
        )
        .unwrap();
        let configured = root.join("extensions/pkg/plugin.yaml");
        let legacy = root.join("plugins/pkg/plugin.yaml");
        std::fs::write(&configured, "id: pkg\n").unwrap();
        std::fs::write(&legacy, "id: legacy\n").unwrap();
        let mut files = Vec::new();
        collect(&root, &mut files).unwrap();
        assert!(files.contains(&configured));
        assert!(!files.contains(&legacy));
        std::fs::remove_dir_all(root).unwrap();
    }
}
