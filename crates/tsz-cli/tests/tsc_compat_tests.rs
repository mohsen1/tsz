//! Integration tests that compare tsz output against tsc (TypeScript compiler) output
//! to ensure they match character-by-character.
//!
//! These tests require `tsc` to be installed and available in PATH.
//! They compare the diagnostic output format (non-pretty mode) between tsz and tsc
//! to verify that tsz produces identical output to tsc for identical inputs.
//!
//! Note: Some tests compare output structure only (ignoring error span positions)
//! because tsz's type checker may report errors on different AST nodes than tsc.
//! Tests that use error codes/types where both compilers agree on spans will
//! verify exact char-by-char matches.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_tsc_compat_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of tsc_compat_tests tests.
include!("tsc_compat_tests_parts/part_00.rs");
include!("tsc_compat_tests_parts/part_01.rs");
include!("tsc_compat_tests_parts/part_02.rs");
