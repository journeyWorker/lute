use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRevision {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRevision {
    pub sha256: String,
    pub files: BTreeMap<PathBuf, FileRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevisionError {
    InvalidPath(PathBuf),
    Io { path: PathBuf, message: String },
}

impl fmt::Display for RevisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "invalid project-relative path `{}`", path.display()),
            Self::Io { path, message } => write!(f, "cannot read {}: {message}", path.display()),
        }
    }
}
impl std::error::Error for RevisionError {}

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => { out.pop(); }
            _ => out.push(component.as_os_str()),
        }
    }
    out
}

fn canonical_path(root: &Path, input: &Path) -> Result<PathBuf, RevisionError> {
    let cwd = std::env::current_dir().map_err(|error| RevisionError::Io {
        path: root.to_path_buf(), message: error.to_string(),
    })?;
    let root_abs = std::fs::canonicalize(root).unwrap_or_else(|_| normalized(&cwd.join(root)));
    // Directory-walk paths can already carry a relative root prefix; bare
    // relative inputs are resolved against the project root instead.
    let path = if input.is_absolute() {
        input.to_path_buf()
    } else if input.starts_with(root) {
        cwd.join(input)
    } else {
        root_abs.join(input)
    };
    let path = std::fs::canonicalize(&path).unwrap_or_else(|_| normalized(&path));
    let root_parts: Vec<_> = root_abs.components().collect();
    let path_parts: Vec<_> = path.components().collect();
    if root_parts.first() != path_parts.first() {
        return Ok(PathBuf::from(format!("abs:{}", path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))));
    }
    let common = root_parts.iter().zip(&path_parts).take_while(|(a, b)| a == b).count();
    let mut relative = PathBuf::new();
    for _ in common..root_parts.len() { relative.push(".."); }
    for part in &path_parts[common..] { relative.push(part.as_os_str()); }
    if relative.as_os_str().is_empty() { return Err(RevisionError::InvalidPath(input.to_path_buf())); }
    Ok(PathBuf::from(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")))
}

/// Hash an explicit closure of exact input bytes. `inputs` is intentionally
/// supplied by the model boundary: revisions never derive their contents from
/// cache keys or from a directory walk.
pub fn project_revision(root: &Path, inputs: &[(PathBuf, Vec<u8>)]) -> Result<ProjectRevision, RevisionError> {
    let mut files = BTreeMap::<PathBuf, FileRevision>::new();
    for (path, bytes) in inputs {
        let path = canonical_path(root, path)?;
        let sha256 = digest(bytes);
        if let Some(previous) = files.get(&path) {
            if previous.sha256 != sha256 {
                return Err(RevisionError::InvalidPath(path));
            }
            continue;
        }
        files.insert(path.clone(), FileRevision { path, sha256 });
    }
    let mut encoded = Vec::new();
    for (path, file) in &files {
        let path = path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let path = path.as_bytes();
        let rev = file.sha256.as_bytes();
        encoded.extend_from_slice(&(path.len() as u64).to_le_bytes());
        encoded.extend_from_slice(path);
        encoded.extend_from_slice(&(rev.len() as u64).to_le_bytes());
        encoded.extend_from_slice(rev);
    }
    Ok(ProjectRevision { sha256: digest(&encoded), files })
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_file_and_stable_project_hash() {
        let a = project_revision(Path::new("/tmp/p"), &[(PathBuf::from("b.lute"), b"b".to_vec()), (PathBuf::from("a.lute"), b"a".to_vec())]).unwrap();
        let b = project_revision(Path::new("/tmp/p"), &[(PathBuf::from("a.lute"), b"a".to_vec()), (PathBuf::from("b.lute"), b"b".to_vec())]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.files[Path::new("a.lute")].sha256, "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb");
    }
    #[test]
    fn normalizes_external_inputs() {
        let a = project_revision(Path::new("/tmp/p"), &[(PathBuf::from("../shared/./schema.yaml"), b"a".to_vec())]).unwrap();
        let b = project_revision(Path::new("/tmp/p"), &[(PathBuf::from("/tmp/shared/schema.yaml"), b"a".to_vec())]).unwrap();
        assert_eq!(a, b);
        assert!(a.files.contains_key(Path::new("../shared/schema.yaml")));
        let changed = project_revision(Path::new("/tmp/p"), &[(PathBuf::from("../shared/schema.yaml"), b"b".to_vec())]).unwrap();
        assert_ne!(a.sha256, changed.sha256);
    }
}
