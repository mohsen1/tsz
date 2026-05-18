//! Shared fixture builders for the module resolver test suite.
//!
//! The historical test file open-coded its temp-directory plumbing in every
//! integration test:
//!
//! ```ignore
//! let dir = std::env::temp_dir().join("tsz_test_xxx");
//! let _ = fs::remove_dir_all(&dir);
//! fs::create_dir_all(&dir).unwrap();
//! fs::write(dir.join("file"), "...").unwrap();
//! // ... assertions ...
//! let _ = fs::remove_dir_all(&dir);
//! ```
//!
//! That pattern is brittle (parallel runs of a test that share the same
//! hardcoded path collide), noisy (every test re-implements the same setup),
//! and easy to get wrong (an early `panic!` skips the final `remove_dir_all`
//! and leaks a directory). The helpers here replace it with a single
//! `TempFixture` whose `Drop` removes the directory exactly once.
//!
//! Each helper is `pub(super)` because the `tests` module is the only
//! consumer; production code must not depend on test infrastructure.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::super::{ModuleResolver, ResolvedCompilerOptions};
use crate::config::ModuleResolutionKind;

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

    /// Create a (possibly nested) directory inside the fixture.
    pub(super) fn mkdir(&self, rel: impl AsRef<Path>) -> PathBuf {
        let path = self.join(rel);
        fs::create_dir_all(&path).expect("create fixture dir");
        path
    }
}

/// `ModuleResolver` configured with `moduleResolution: node`.
pub(super) fn node_resolver() -> ModuleResolver {
    ModuleResolver::node_resolver()
}

/// `ModuleResolver` configured with `moduleResolution: bundler` and
/// otherwise-default compiler options.
pub(super) fn bundler_resolver() -> ModuleResolver {
    resolver_with(ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        ..Default::default()
    })
}

/// `ModuleResolver` built from an arbitrary `ResolvedCompilerOptions`.
pub(super) fn resolver_with(options: ResolvedCompilerOptions) -> ModuleResolver {
    ModuleResolver::new(&options)
}
