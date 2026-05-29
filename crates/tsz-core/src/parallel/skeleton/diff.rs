//! Skeleton invalidation: compare two skeleton snapshots to detect
//! merge-relevant file changes.
//!
//! Pure functions over [`FileSkeleton`] slices. Used by LSP and incremental
//! drivers to decide which files need re-merging after a file change. Kept
//! separate from `extract_skeleton`/`reduce_skeletons` in `skeleton/mod.rs`
//! because diffing is a consumer of skeletons, not part of their production.

use rustc_hash::FxHashMap;

use super::{FileSkeleton, reduce_skeletons};

/// Result of comparing two skeleton snapshots for incremental invalidation.
///
/// Used by LSP and incremental drivers to determine which files need
/// re-merging after a file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonDiff {
    /// Files whose merge-relevant skeleton changed (need re-merge).
    pub changed: Vec<String>,
    /// Files that are new (not present in the previous snapshot).
    pub added: Vec<String>,
    /// Files that were removed (present before but not now).
    pub removed: Vec<String>,
    /// Whether the aggregate project topology changed.
    pub topology_changed: bool,
}

impl SkeletonDiff {
    /// Returns true if no merge-relevant changes were detected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }

    /// Total number of affected files.
    #[must_use]
    pub const fn affected_count(&self) -> usize {
        self.changed.len() + self.added.len() + self.removed.len()
    }
}

/// Compare two sets of file skeletons to identify merge-relevant changes.
///
/// Compares fingerprints per file to detect which files changed their
/// merge-relevant topology (exported symbols, augmentations, re-exports).
/// Files with identical fingerprints are guaranteed unchanged.
///
/// This is a pure function suitable for incremental invalidation drivers.
pub fn diff_skeletons(previous: &[FileSkeleton], current: &[FileSkeleton]) -> SkeletonDiff {
    let prev_map: FxHashMap<&str, u64> = previous
        .iter()
        .map(|s| (s.file_name.as_str(), s.fingerprint))
        .collect();
    let curr_map: FxHashMap<&str, u64> = current
        .iter()
        .map(|s| (s.file_name.as_str(), s.fingerprint))
        .collect();

    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    // Check current files against previous
    for skel in current {
        match prev_map.get(skel.file_name.as_str()) {
            Some(&prev_fp) if prev_fp != skel.fingerprint => {
                changed.push(skel.file_name.clone());
            }
            None => {
                added.push(skel.file_name.clone());
            }
            _ => {} // unchanged
        }
    }

    // Check for removed files
    for skel in previous {
        if !curr_map.contains_key(skel.file_name.as_str()) {
            removed.push(skel.file_name.clone());
        }
    }

    let prev_index = reduce_skeletons(previous);
    let curr_index = reduce_skeletons(current);
    let topology_changed = prev_index.fingerprint != curr_index.fingerprint;

    SkeletonDiff {
        changed,
        added,
        removed,
        topology_changed,
    }
}
