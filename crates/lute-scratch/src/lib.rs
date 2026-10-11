//! Self-removing scratch directories for tests.
//!
//! Tests that write projects, artifacts or binaries to disk take a
//! [`ScratchDir`]: a fresh, empty directory under the system temp dir that is
//! deleted when the value drops. It derefs to [`Path`], so it is used where
//! the path is.

use std::ops::Deref;
use std::path::Path;

/// A fresh empty directory (`lute-…` under the system temp dir), removed with
/// everything in it when this value drops. Keep it alive as long as the
/// directory is used.
#[derive(Debug)]
pub struct ScratchDir(tempfile::TempDir);

impl ScratchDir {
    pub fn new() -> Self {
        Self::named("scratch")
    }

    /// A scratch directory whose name starts `lute-<tag>-`, so a test's
    /// directories are recognizable while it runs.
    pub fn named(tag: &str) -> Self {
        Self(
            tempfile::Builder::new()
                .prefix(&format!("lute-{tag}-"))
                .tempdir()
                .expect("create a scratch directory under the system temp dir"),
        )
    }
}

impl Default for ScratchDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for ScratchDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<std::ffi::OsStr> for ScratchDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.path().as_os_str()
    }
}
