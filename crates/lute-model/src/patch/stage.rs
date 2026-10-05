use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Stage {
    pub(crate) root: PathBuf,
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn rel(root: &Path, path: &Path) -> PathBuf {
    PathBuf::from(
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
    )
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let kind = std::fs::symlink_metadata(&from).map_err(|error| error.to_string())?;
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            std::fs::copy(&from, &to).map_err(|error| error.to_string())?;
        } else {
            return Err(format!("unsupported special file {}", from.display()));
        }
    }
    Ok(())
}

pub(crate) fn make_stage(root: &Path) -> Result<Stage, String> {
    static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let serial = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "lute-patch-{}-{stamp}-{serial}",
        std::process::id()
    ));
    copy_tree(root, &path)?;
    Ok(Stage { root: path })
}

pub(crate) fn all_files(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut files = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.insert(rel(root, &path));
            }
        }
    }
    Ok(files)
}

pub(crate) fn changed_files(root: &Path, stage: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = all_files(root)?;
    paths.extend(all_files(stage)?);
    let mut changed = Vec::new();
    for path in paths {
        let before = std::fs::read(root.join(&path)).ok();
        let after = std::fs::read(stage.join(&path)).ok();
        if before != after {
            changed.push(path);
        }
    }
    Ok(changed)
}
