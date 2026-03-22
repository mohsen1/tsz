//! Residency accounting for the parallel compilation pipeline.
//!
//! Centralizes memory estimation and residency statistics for
//! `MergedProgram`, enabling LSP eviction budgeting and
//! `--extendedDiagnostics` reporting.

use super::core::MergedProgram;
use super::skeleton::SkeletonIndex;
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// High-level residency counters for `MergedProgram` state.
///
/// These numbers give a stable baseline for large-repo performance work without
/// pretending to be exact heap accounting. The important question is how many
/// arenas and declaration mappings the current pipeline retains after merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedProgramResidencyStats {
    /// Number of user files in the merged program.
    pub file_count: usize,
    /// Number of bound-file arena handles retained directly by `program.files`.
    pub bound_file_arena_count: usize,
    /// Number of unique `NodeArena` allocations retained across all arena maps.
    pub unique_arena_count: usize,
    /// Number of entries in the symbol -> arena lookup table.
    pub symbol_arena_count: usize,
    /// Number of declaration -> arena buckets retained for cross-file lookup.
    pub declaration_arena_bucket_count: usize,
    /// Total number of declaration -> arena edges across all buckets.
    pub declaration_arena_mapping_count: usize,
    /// Whether the skeleton index was computed alongside the legacy merge.
    pub has_skeleton_index: bool,
    /// Number of merge candidates identified by the skeleton (symbols in >1 file).
    pub skeleton_merge_candidate_count: usize,
    /// Total top-level symbols tracked by the skeleton (before merge).
    pub skeleton_total_symbol_count: usize,
    /// Estimated in-memory size of the skeleton index in bytes.
    /// Zero if no skeleton index is present.
    pub skeleton_estimated_size_bytes: usize,
    /// Sum of `BindResult::estimated_size_bytes()` across all input files,
    /// captured before the merge. Useful for comparing pre-merge vs post-merge
    /// memory footprint and for LSP eviction budgeting.
    pub pre_merge_bind_total_bytes: usize,
    /// Total estimated in-memory size of all `BoundFile` entries in bytes.
    pub total_bound_file_bytes: usize,
}

impl MergedProgram {
    /// Return residency-oriented counters for the current merged program.
    #[must_use]
    pub fn residency_stats(&self) -> MergedProgramResidencyStats {
        let mut unique_arenas: FxHashSet<usize> = FxHashSet::default();

        for file in &self.files {
            unique_arenas.insert(Arc::as_ptr(&file.arena) as usize);
        }
        for arena in self.symbol_arenas.values() {
            unique_arenas.insert(Arc::as_ptr(arena) as usize);
        }
        for arenas in self.declaration_arenas.values() {
            for arena in arenas {
                unique_arenas.insert(Arc::as_ptr(arena) as usize);
            }
        }

        let (has_skeleton, skel_merge_count, skel_sym_count, skel_size) =
            if let Some(ref idx) = self.skeleton_index {
                (
                    true,
                    idx.merge_candidates.len(),
                    idx.total_symbol_count,
                    idx.estimated_size_bytes(),
                )
            } else {
                (false, 0, 0, 0)
            };

        let total_bound_file_bytes: usize =
            self.files.iter().map(|f| f.estimated_size_bytes()).sum();

        MergedProgramResidencyStats {
            file_count: self.files.len(),
            bound_file_arena_count: self.files.len(),
            unique_arena_count: unique_arenas.len(),
            symbol_arena_count: self.symbol_arenas.len(),
            declaration_arena_bucket_count: self.declaration_arenas.len(),
            declaration_arena_mapping_count: self
                .declaration_arenas
                .values()
                .map(|arenas| arenas.len())
                .sum(),
            has_skeleton_index: has_skeleton,
            skeleton_merge_candidate_count: skel_merge_count,
            skeleton_total_symbol_count: skel_sym_count,
            skeleton_estimated_size_bytes: skel_size,
            pre_merge_bind_total_bytes: self.pre_merge_bind_total_bytes,
            total_bound_file_bytes,
        }
    }
}
