// Each integration-test binary compiles this module independently and uses a different subset.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::ops::Deref;

pub const BIN: &str = env!("CARGO_BIN_EXE_lute");
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A unique temporary project directory removed when the test finishes.
pub struct TempProject {
    path: PathBuf,
}

impl TempProject {
    pub fn new(tag: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temporary project directory");
        Self { path }
    }

    pub fn path(&self) -> &Path { &self.path }

    pub fn write(&self, relative: impl AsRef<Path>, text: &str) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create temporary project parent");
        }
        std::fs::write(&path, text).expect("write temporary project file");
        path
    }
}

impl Deref for TempProject {
    type Target = Path;
    fn deref(&self) -> &Path { self.path() }
}

impl AsRef<Path> for TempProject {
    fn as_ref(&self) -> &Path { self.path() }
}

impl Drop for TempProject {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); }
}

pub fn run_cli<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(BIN).args(args).output().expect("run lute CLI")
}

pub fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("invalid CLI JSON ({error}): {}", String::from_utf8_lossy(&output.stdout))
    })
}

pub fn json_text(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("valid JSON")
}
