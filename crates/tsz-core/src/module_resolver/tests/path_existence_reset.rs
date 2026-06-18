//! Public `reset_path_existence_caches` boundary contract.
//!
//! The resolver memoizes `is_file` / `is_dir` in a `thread_local!` for the
//! duration of one compilation. A long-lived batch/merge-group worker reuses
//! one thread across many compilations, so the per-compilation boundary reset
//! (`clear_batch_iteration_state`) must drop these existence caches through the
//! public [`reset_path_existence_caches`] entry point — otherwise a later
//! compilation reads a stale existence answer for a path whose on-disk state
//! changed between compilations (#13368 / #13255 worker-reuse isolation).

use super::super::reset_path_existence_caches;
use super::fixtures::TempFixture;
use crate::resolution::helpers::{cached_is_dir, cached_is_file};

#[test]
fn public_reset_drops_stale_file_and_dir_existence_after_fs_change() {
    // Start from a clean slate on this test thread.
    reset_path_existence_caches();

    let fixture = TempFixture::new();
    let file = fixture.write("present.ts", "export {};");
    let dir = fixture.join("present-dir");
    std::fs::create_dir(&dir).expect("create probed directory");

    // First probes record both as present and seed the thread-local caches.
    assert!(cached_is_file(&file), "file should be observed present");
    assert!(cached_is_dir(&dir), "directory should be observed present");

    // Remove both on disk. Within one compilation the filesystem is assumed
    // stable, so the cached answers are intentionally reused even though the
    // paths are now gone — this is what collapses repeated `stat()` syscalls.
    std::fs::remove_file(&file).expect("remove probed file");
    std::fs::remove_dir(&dir).expect("remove probed directory");
    assert!(
        cached_is_file(&file),
        "cache is stable until the boundary reset"
    );
    assert!(
        cached_is_dir(&dir),
        "cache is stable until the boundary reset"
    );

    // The public boundary reset (called from `clear_batch_iteration_state`)
    // drops both caches, so the next compilation re-reads the real filesystem.
    reset_path_existence_caches();
    assert!(
        !cached_is_file(&file),
        "post-reset re-read must observe the file is gone"
    );
    assert!(
        !cached_is_dir(&dir),
        "post-reset re-read must observe the directory is gone"
    );
}
