//! Incremental file-change coordination for the definition store.
//!
//! Provides [`FileChangeSet`] — a batch descriptor that captures which files
//! changed and orchestrates the invalidation/re-registration cycle on a
//! [`DefinitionStore`].
//!
//! ## Usage
//!
//! ```text
//! // 1. Detect which files changed (via skeleton fingerprint comparison)
//! let mut changeset = FileChangeSet::new();
//! changeset.mark_changed(file_id_a, old_fingerprint_a, new_fingerprint_a);
//! changeset.mark_changed(file_id_b, old_fingerprint_b, new_fingerprint_b);
//! changeset.mark_removed(file_id_c);
//!
//! // 2. Apply invalidation to the definition store
//! let summary = changeset.apply_invalidation(&def_store);
//!
//! // 3. Re-bind changed files and re-register their definitions
//! //    (caller's responsibility — this module doesn't own the binder)
//! ```
//!
//! ## Design Rationale
//!
//! This struct is deliberately *not* responsible for re-binding or
//! re-registering definitions. It only handles the invalidation side:
//! removing stale `DefId`s and their index entries. The re-population is
//! left to the driver (LSP, CLI watch) because it depends on parser/binder
//! infrastructure that lives outside the solver crate.
//!
//! The separation keeps the solver crate free of parser/binder dependencies
//! while providing a reusable invalidation unit.

use super::core::DefinitionStore;

// =============================================================================
// FileChange — description of a single file's change
// =============================================================================

/// Describes how a single file changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChange {
    /// File content changed (different skeleton fingerprint).
    Modified {
        /// Skeleton fingerprint before the change.
        old_fingerprint: u64,
        /// Skeleton fingerprint after the change.
        new_fingerprint: u64,
    },
    /// File was removed from the project.
    Removed,
    /// File was added to the project (no prior definitions to invalidate).
    Added,
}

// =============================================================================
// FileChangeSet — batch of file changes
// =============================================================================

/// A batch of file changes ready for invalidation.
///
/// Collects file-level change descriptors and applies them to a
/// [`DefinitionStore`] in a single pass. This is the reusable unit that
/// both the LSP `update_file` path and a future CLI watch-mode would use.
#[derive(Clone, Debug, Default)]
pub struct FileChangeSet {
    /// Changes keyed by `file_id`.
    changes: Vec<(u32, FileChange)>,
}

impl FileChangeSet {
    /// Create an empty changeset.
    pub const fn new() -> Self {
        Self {
            changes: Vec::new(),
        }
    }

    /// Create a changeset with pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            changes: Vec::with_capacity(cap),
        }
    }

    /// Mark a file as modified (skeleton fingerprint changed).
    ///
    /// Only files whose fingerprint actually changed should be marked —
    /// the caller is responsible for comparing old vs new fingerprints.
    pub fn mark_changed(&mut self, file_id: u32, old_fingerprint: u64, new_fingerprint: u64) {
        self.changes.push((
            file_id,
            FileChange::Modified {
                old_fingerprint,
                new_fingerprint,
            },
        ));
    }

    /// Mark a file as removed from the project.
    pub fn mark_removed(&mut self, file_id: u32) {
        self.changes.push((file_id, FileChange::Removed));
    }

    /// Mark a file as newly added to the project.
    ///
    /// Added files have no prior definitions to invalidate; this entry
    /// exists so the summary can report them and the caller knows which
    /// files need initial binding + registration.
    pub fn mark_added(&mut self, file_id: u32) {
        self.changes.push((file_id, FileChange::Added));
    }

    /// Number of file changes in this batch.
    pub const fn len(&self) -> usize {
        self.changes.len()
    }

    /// Whether the changeset is empty (no files changed).
    pub const fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Iterate over the changes.
    pub fn iter(&self) -> impl Iterator<Item = &(u32, FileChange)> {
        self.changes.iter()
    }

    /// File IDs that need re-binding (modified or added).
    ///
    /// Returns file IDs in the order they were added to the changeset.
    /// Removed files are excluded since they don't need re-binding.
    pub fn files_needing_rebind(&self) -> Vec<u32> {
        self.changes
            .iter()
            .filter_map(|(file_id, change)| match change {
                FileChange::Removed => None,
                _ => Some(*file_id),
            })
            .collect()
    }

    /// File IDs that need invalidation (modified or removed).
    ///
    /// Added files are excluded since they have no prior definitions.
    pub fn files_needing_invalidation(&self) -> Vec<u32> {
        self.changes
            .iter()
            .filter_map(|(file_id, change)| match change {
                FileChange::Added => None,
                _ => Some(*file_id),
            })
            .collect()
    }

    /// Apply invalidation to the definition store.
    ///
    /// For each modified or removed file, calls [`DefinitionStore::invalidate_file`]
    /// to remove stale `DefId`s and clean up all reverse indices.
    ///
    /// Added files are skipped (nothing to invalidate).
    ///
    /// Returns an [`InvalidationSummary`] describing what was done.
    pub fn apply_invalidation(&self, store: &DefinitionStore) -> InvalidationSummary {
        let mut total_invalidated = 0;
        let mut per_file = Vec::with_capacity(self.changes.len());

        for (file_id, change) in &self.changes {
            match change {
                FileChange::Modified { .. } | FileChange::Removed => {
                    let count = store.invalidate_file(*file_id);
                    per_file.push((*file_id, count));
                    total_invalidated += count;
                }
                FileChange::Added => {
                    per_file.push((*file_id, 0));
                }
            }
        }

        InvalidationSummary {
            files_modified: self
                .changes
                .iter()
                .filter(|(_, c)| matches!(c, FileChange::Modified { .. }))
                .count(),
            files_removed: self
                .changes
                .iter()
                .filter(|(_, c)| matches!(c, FileChange::Removed))
                .count(),
            files_added: self
                .changes
                .iter()
                .filter(|(_, c)| matches!(c, FileChange::Added))
                .count(),
            total_defs_invalidated: total_invalidated,
            per_file,
        }
    }
}

// =============================================================================
// InvalidationSummary — result of applying a changeset
// =============================================================================

/// Summary of an invalidation pass.
///
/// Returned by [`FileChangeSet::apply_invalidation`] so the caller can
/// log, assert, or display what happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidationSummary {
    /// Number of files that were modified (fingerprint changed).
    pub files_modified: usize,
    /// Number of files that were removed.
    pub files_removed: usize,
    /// Number of files that were added.
    pub files_added: usize,
    /// Total number of `DefId`s invalidated across all files.
    pub total_defs_invalidated: usize,
    /// Per-file invalidation counts: `(file_id, defs_invalidated)`.
    pub per_file: Vec<(u32, usize)>,
}

impl InvalidationSummary {
    /// Total number of files in the changeset.
    pub const fn total_files(&self) -> usize {
        self.files_modified + self.files_removed + self.files_added
    }

    /// Whether any definitions were actually invalidated.
    pub const fn had_invalidations(&self) -> bool {
        self.total_defs_invalidated > 0
    }

    /// File IDs that need re-binding and re-registration.
    ///
    /// This is `files_modified` + `files_added` (removed files don't need re-binding).
    pub const fn files_needing_repopulation(&self) -> usize {
        self.files_modified + self.files_added
    }
}

// =============================================================================
// Fingerprint-based change detection helper
// =============================================================================

/// Compare old and new skeleton fingerprints to build a [`FileChangeSet`].
///
/// Given two fingerprint maps (old and new), determines which files were
/// modified, added, or removed. This is the bridge between the skeleton
/// fingerprint infrastructure and the invalidation coordinator.
///
/// Both maps use `file_id` as the key and `fingerprint` as the value.
pub fn diff_fingerprints(old: &[(u32, u64)], new: &[(u32, u64)]) -> FileChangeSet {
    use rustc_hash::FxHashMap;

    let old_map: FxHashMap<u32, u64> = old.iter().copied().collect();
    let new_map: FxHashMap<u32, u64> = new.iter().copied().collect();

    let mut changeset = FileChangeSet::new();

    // Check for modified and removed files.
    for (&file_id, &old_fp) in &old_map {
        match new_map.get(&file_id) {
            Some(&new_fp) if new_fp != old_fp => {
                changeset.mark_changed(file_id, old_fp, new_fp);
            }
            Some(_) => {
                // Fingerprint unchanged — skip.
            }
            None => {
                changeset.mark_removed(file_id);
            }
        }
    }

    // Check for added files.
    for &file_id in new_map.keys() {
        if !old_map.contains_key(&file_id) {
            changeset.mark_added(file_id);
        }
    }

    changeset
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::core::{DefinitionInfo, DefinitionStore};
    use crate::types::TypeId;
    use tsz_common::interner::Atom;

    fn make_store_with_file(file_id: u32) -> (DefinitionStore, Vec<crate::def::DefId>) {
        let store = DefinitionStore::new();
        let mut ids = Vec::new();

        let mut info = DefinitionInfo::type_alias(Atom(file_id * 1000), vec![], TypeId::NUMBER);
        info.file_id = Some(file_id);
        info.symbol_id = Some(100 + file_id);
        ids.push(store.register(info));

        let mut info2 =
            DefinitionInfo::type_alias(Atom(file_id * 1000 + 1), vec![], TypeId::STRING);
        info2.file_id = Some(file_id);
        info2.symbol_id = Some(200 + file_id);
        ids.push(store.register(info2));

        (store, ids)
    }

    #[test]
    fn test_empty_changeset() {
        let cs = FileChangeSet::new();
        assert!(cs.is_empty());
        assert_eq!(cs.len(), 0);

        let store = DefinitionStore::new();
        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.total_files(), 0);
        assert_eq!(summary.total_defs_invalidated, 0);
        assert!(!summary.had_invalidations());
    }

    #[test]
    fn test_mark_changed_invalidates_defs() {
        let (store, ids) = make_store_with_file(5);

        assert!(store.contains(ids[0]));
        assert!(store.contains(ids[1]));

        let mut cs = FileChangeSet::new();
        cs.mark_changed(5, 0xAAAA, 0xBBBB);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.total_defs_invalidated, 2);
        assert!(summary.had_invalidations());

        // Definitions should be gone.
        assert!(!store.contains(ids[0]));
        assert!(!store.contains(ids[1]));
    }

    #[test]
    fn test_mark_removed_invalidates_defs() {
        let (store, ids) = make_store_with_file(3);

        let mut cs = FileChangeSet::new();
        cs.mark_removed(3);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_removed, 1);
        assert_eq!(summary.total_defs_invalidated, 2);

        assert!(!store.contains(ids[0]));
        assert!(!store.contains(ids[1]));
    }

    #[test]
    fn test_mark_added_does_not_invalidate() {
        let store = DefinitionStore::new();

        let mut cs = FileChangeSet::new();
        cs.mark_added(99);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_added, 1);
        assert_eq!(summary.total_defs_invalidated, 0);
        assert!(!summary.had_invalidations());
    }

    #[test]
    fn test_files_needing_rebind() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);

        let rebind = cs.files_needing_rebind();
        assert_eq!(rebind.len(), 2);
        assert!(rebind.contains(&1));
        assert!(rebind.contains(&3));
        // Removed file should NOT need rebind.
        assert!(!rebind.contains(&2));
    }

    #[test]
    fn test_files_needing_invalidation() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);

        let invalidate = cs.files_needing_invalidation();
        assert_eq!(invalidate.len(), 2);
        assert!(invalidate.contains(&1));
        assert!(invalidate.contains(&2));
        // Added file should NOT need invalidation.
        assert!(!invalidate.contains(&3));
    }

    #[test]
    fn test_mixed_changeset() {
        let store = DefinitionStore::new();

        // File 1: 2 defs
        let mut info1a = DefinitionInfo::type_alias(Atom(100), vec![], TypeId::NUMBER);
        info1a.file_id = Some(1);
        store.register(info1a);

        let mut info1b = DefinitionInfo::type_alias(Atom(101), vec![], TypeId::STRING);
        info1b.file_id = Some(1);
        store.register(info1b);

        // File 2: 1 def
        let mut info2 = DefinitionInfo::type_alias(Atom(200), vec![], TypeId::BOOLEAN);
        info2.file_id = Some(2);
        let id_c = store.register(info2);

        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0xAA, 0xBB); // file 1 modified
        cs.mark_added(3); // file 3 added (nothing to invalidate)

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.files_added, 1);
        assert_eq!(summary.total_defs_invalidated, 2); // only file 1's defs

        // File 2's def should be preserved.
        assert!(store.contains(id_c));
    }

    #[test]
    fn test_diff_fingerprints_no_change() {
        let old = vec![(1, 0xAA), (2, 0xBB)];
        let new = vec![(1, 0xAA), (2, 0xBB)];

        let cs = diff_fingerprints(&old, &new);
        assert!(cs.is_empty());
    }

    #[test]
    fn test_diff_fingerprints_modified() {
        let old = vec![(1, 0xAA), (2, 0xBB)];
        let new = vec![(1, 0xAA), (2, 0xCC)]; // file 2 changed

        let cs = diff_fingerprints(&old, &new);
        assert_eq!(cs.len(), 1);

        let (file_id, change) = &cs.changes[0];
        assert_eq!(*file_id, 2);
        assert_eq!(
            *change,
            FileChange::Modified {
                old_fingerprint: 0xBB,
                new_fingerprint: 0xCC,
            }
        );
    }

    #[test]
    fn test_diff_fingerprints_added_and_removed() {
        let old = vec![(1, 0xAA)];
        let new = vec![(2, 0xBB)];

        let cs = diff_fingerprints(&old, &new);
        assert_eq!(cs.len(), 2);

        // Should have one removed (file 1) and one added (file 2).
        let removed: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Removed))
            .collect();
        let added: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Added))
            .collect();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 1);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, 2);
    }

    #[test]
    fn test_diff_fingerprints_mixed() {
        let old = vec![(1, 0xAA), (2, 0xBB), (3, 0xCC)];
        let new = vec![(1, 0xAA), (2, 0xDD), (4, 0xEE)];
        // file 1: unchanged, file 2: modified, file 3: removed, file 4: added

        let cs = diff_fingerprints(&old, &new);

        let modified: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Modified { .. }))
            .collect();
        let removed: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Removed))
            .collect();
        let added: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Added))
            .collect();

        assert_eq!(modified.len(), 1);
        assert_eq!(modified[0].0, 2);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 3);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, 4);
    }

    #[test]
    fn test_changeset_with_capacity() {
        let cs = FileChangeSet::with_capacity(10);
        assert!(cs.is_empty());
        assert_eq!(cs.len(), 0);
    }

    #[test]
    fn test_summary_files_needing_repopulation() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);
        cs.mark_added(4);

        let store = DefinitionStore::new();
        let summary = cs.apply_invalidation(&store);

        // 1 modified + 2 added = 3 needing repopulation
        assert_eq!(summary.files_needing_repopulation(), 3);
    }
}
