//! `TempFixture`: per-test temp directory used across resolver tests.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A temporary directory scoped to a single test.
///
/// The directory lives under the OS temp dir, has a unique randomized name
/// (so two tests can run in parallel safely), and is deleted when the
/// fixture goes out of scope.
pub(super) struct TempFixture {
    dir: TempDir,
}

impl TempFixture {
    /// Create a fresh, empty temp directory.
    pub(super) fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir for module_resolver test fixture");
        Self { dir }
    }

    /// Root of the fixture.
    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Resolve a path inside the fixture (does not create anything).
    pub(super) fn join(&self, rel: impl AsRef<Path>) -> PathBuf {
        self.path().join(rel)
    }

    /// Write `content` to `rel` inside the fixture, creating any missing
    /// parent directories. Returns the absolute path of the written file.
    pub(super) fn write(&self, rel: impl AsRef<Path>, content: &str) -> PathBuf {
        let path = self.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir for fixture write");
        }
        fs::write(&path, content).expect("write fixture file");
        path
    }
}
