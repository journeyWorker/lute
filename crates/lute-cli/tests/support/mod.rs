// Each integration-test binary compiles this module independently and uses a different subset.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::ops::Deref;

pub const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A unique temporary project directory removed when the test finishes.
pub struct TempProject {
    dir: lute_scratch::ScratchDir,
}

impl TempProject {
    pub fn new(tag: &str) -> Self {
        Self { dir: lute_scratch::ScratchDir::named(tag) }
    }

    pub fn path(&self) -> &Path { &self.dir }

    pub fn write(&self, relative: impl AsRef<Path>, text: &str) -> PathBuf {
        let path = self.dir.join(relative);
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
