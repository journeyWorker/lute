//! `lute fmt`: deterministic, lossless canonical formatting for source inputs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn is_yaml(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("yaml" | "yml"))
}

fn is_selected(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) == Some("lute") {
        return true;
    }
    if !is_yaml(path) { return false; }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if (name == "lute.project.yaml" || name == "lute.project.yml")
        || name.ends_with(".schema.yaml") || name.ends_with(".schema.yml")
    {
        return true;
    }
    configured_plugin_file(path)
}

fn configured_plugin_file(path: &Path) -> bool {
    let mut ancestor = path.parent();
    while let Some(dir) = ancestor {
        if dir.join("lute.project.yaml").is_file() {
            return lute_model::manifest_context(dir)
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
        if is_selected(path) { out.push(path.to_path_buf()); }
        return Ok(());
    }
    if !meta.is_dir() { return Ok(()); }
    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        collect(&entry.path(), out)?;
    }
    Ok(())
}

/// Run `lute fmt [--check] <path…>`.
///
/// Selection and output are sorted by path. Check mode never writes and exits
/// 1 when at least one selected input is noncanonical; I/O, UTF-8, and parser
/// failures use exit 2.
pub(crate) fn run(paths: &[PathBuf], check: bool) -> ExitCode {
    let roots: Vec<PathBuf> = if paths.is_empty() { vec![PathBuf::from(".")] } else { paths.to_vec() };
    let mut files = Vec::new();
    for root in roots {
        if let Err(err) = collect(&root, &mut files) {
            eprintln!("lute fmt: {}: {err}", root.display());
            return ExitCode::from(2);
        }
    }
    files.sort();
    files.dedup();
    let mut changed = false;
    for path in files {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("lute fmt: {}: {err}", path.display());
                return ExitCode::from(2);
            }
        };
        let source = match std::str::from_utf8(&bytes) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("lute fmt: {}: invalid UTF-8 ({err})", path.display());
                return ExitCode::from(2);
            }
        };
        let result = if is_yaml(&path) {
            lute_syntax::format_yaml_source(source)
        } else {
            match lute_syntax::format_source(source, &lute_syntax::FormatOptions::default()) {
                Ok(result) => result,
                Err(err) => {
                    eprintln!("lute fmt: {}: formatting failed: {err:?}", path.display());
                    return ExitCode::from(2);
                }
            }
        };
        if !result.changed { continue; }
        changed = true;
        if check {
            println!("{}", path.display());
        } else if let Err(err) = std::fs::write(&path, result.text.as_bytes()) {
            eprintln!("lute fmt: {}: {err}", path.display());
            return ExitCode::from(2);
        }
    }
    if check && changed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_exit_codes_and_write_only_when_changed() {
        let root = std::env::temp_dir().join(format!("lute-fmt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.lute");
        let original = "## S\n@narrator: hi  \n";
        std::fs::write(&file, original).unwrap();
        assert_eq!(run(std::slice::from_ref(&file), true), ExitCode::from(1));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert_eq!(run(std::slice::from_ref(&file), false), ExitCode::SUCCESS);
        let canonical = std::fs::read_to_string(&file).unwrap();
        assert_ne!(canonical, original);
        assert_eq!(run(std::slice::from_ref(&file), true), ExitCode::SUCCESS);
        assert_eq!(run(&[root.join("missing.lute")], false), ExitCode::from(2));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn collection_skips_symlink_loops() {
        let root = std::env::temp_dir().join(format!("lute-fmt-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
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
        let root = std::env::temp_dir().join(format!("lute-fmt-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("extensions/pkg")).unwrap();
        std::fs::create_dir_all(root.join("plugins/pkg")).unwrap();
        std::fs::write(
            root.join("lute.project.yaml"),
            "defaultProfile: core\npluginsDir: extensions\nprofiles:\n  core:\n    plugins: {}\n",
        ).unwrap();
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
