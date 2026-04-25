//! Residency accounting for the parallel compilation pipeline.
//!
//! Centralizes memory estimation and residency statistics for
//! `MergedProgram`, enabling LSP eviction budgeting and
//! `--extendedDiagnostics` reporting.

use super::core::MergedProgram;

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
    /// Total estimated size of unique `NodeArena` allocations in bytes.
    /// Computed by calling `estimated_size_bytes()` on each deduplicated arena.
    pub unique_arena_estimated_bytes: usize,
    /// Whether a dependency graph was computed from skeleton import sources.
    pub has_dep_graph: bool,
    /// Number of import edges in the dependency graph.
    pub dep_graph_edge_count: usize,
    /// Number of root files (no in-graph dependencies) in the dep graph.
    pub dep_graph_root_count: usize,
    /// Whether the dependency graph is acyclic (a DAG).
    pub dep_graph_is_acyclic: bool,
    /// Number of cycle groups (SCCs with >1 member) in the dep graph.
    pub dep_graph_cycle_count: usize,
    /// Number of unresolved import specifiers in the dep graph.
    pub dep_graph_unresolved_count: usize,
}

impl MergedProgram {
    /// Return residency-oriented counters for the current merged program.
    #[must_use]
    pub fn residency_stats(&self) -> MergedProgramResidencyStats {
        use rustc_hash::FxHashMap;
        use tsz_parser::parser::NodeArena;

        // Collect unique arenas by pointer identity, keeping a reference to
        // each for size estimation.
        let mut unique_arena_map: FxHashMap<usize, &Arc<NodeArena>> = FxHashMap::default();

        for file in &self.files {
            let ptr = Arc::as_ptr(&file.arena) as usize;
            unique_arena_map.entry(ptr).or_insert(&file.arena);
        }
        for arena in self.symbol_arenas.values() {
            let ptr = Arc::as_ptr(arena) as usize;
            unique_arena_map.entry(ptr).or_insert(arena);
        }
        for arenas in self.declaration_arenas.values() {
            for arena in arenas {
                let ptr = Arc::as_ptr(arena) as usize;
                unique_arena_map.entry(ptr).or_insert(arena);
            }
        }

        let unique_arena_count = unique_arena_map.len();
        let unique_arena_estimated_bytes: usize = unique_arena_map
            .values()
            .map(|a| a.estimated_size_bytes())
            .sum();

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

        let (
            has_dep_graph,
            dep_edge_count,
            dep_root_count,
            dep_is_acyclic,
            dep_cycle_count,
            dep_unresolved,
        ) = if let Some(ref dg) = self.dep_graph {
            let topo = dg.topological_order();
            (
                true,
                dg.edge_count,
                dg.roots().len(),
                topo.is_acyclic,
                topo.cycles.len(),
                dg.unresolved_specifiers.len(),
            )
        } else {
            (false, 0, 0, true, 0, 0)
        };

        MergedProgramResidencyStats {
            file_count: self.files.len(),
            bound_file_arena_count: self.files.len(),
            unique_arena_count,
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
            unique_arena_estimated_bytes,
            has_dep_graph,
            dep_graph_edge_count: dep_edge_count,
            dep_graph_root_count: dep_root_count,
            dep_graph_is_acyclic: dep_is_acyclic,
            dep_graph_cycle_count: dep_cycle_count,
            dep_graph_unresolved_count: dep_unresolved,
        }
    }
}

/// Memory pressure assessment for eviction decisions.
///
/// Converts raw byte estimates into actionable signals. The LSP project layer
/// can use this to decide whether to evict parsed/bound ASTs and rely on
/// skeletons for merge decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Well within budget — no action needed.
    Low,
    /// Approaching budget — consider evicting cold files.
    Medium,
    /// Over budget — should evict aggressively.
    High,
}

/// Budget configuration for residency-based eviction.
///
/// Thresholds are in bytes. The defaults are conservative starting points
/// for a language server handling medium-sized projects (~500 files).
#[derive(Debug, Clone, Copy)]
pub struct ResidencyBudget {
    /// Threshold below which memory pressure is Low.
    pub low_watermark_bytes: usize,
    /// Threshold above which memory pressure is High.
    pub high_watermark_bytes: usize,
}

/// Default memory pressure thresholds for `MergedProgram` residency tracking.
const DEFAULT_LOW_WATERMARK_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_HIGH_WATERMARK_BYTES: usize = 512 * 1024 * 1024;

impl Default for ResidencyBudget {
    fn default() -> Self {
        Self {
            low_watermark_bytes: DEFAULT_LOW_WATERMARK_BYTES,
            high_watermark_bytes: DEFAULT_HIGH_WATERMARK_BYTES,
        }
    }
}

impl ResidencyBudget {
    /// Assess memory pressure from residency stats.
    #[must_use]
    pub const fn assess(&self, stats: &MergedProgramResidencyStats) -> MemoryPressure {
        let total = stats.pre_merge_bind_total_bytes + stats.total_bound_file_bytes;
        if total >= self.high_watermark_bytes {
            MemoryPressure::High
        } else if total >= self.low_watermark_bytes {
            MemoryPressure::Medium
        } else {
            MemoryPressure::Low
        }
    }

    /// Returns whether the skeleton index alone could serve merge decisions,
    /// allowing full `BindResult` eviction for memory recovery.
    #[must_use]
    pub const fn skeleton_can_replace_full_arenas(stats: &MergedProgramResidencyStats) -> bool {
        stats.has_skeleton_index && stats.skeleton_merge_candidate_count > 0
    }

    /// Estimate how many bytes would be freed by evicting pre-merge `BindResults`
    /// and relying on the skeleton index for merge topology.
    #[must_use]
    pub const fn eviction_savings(stats: &MergedProgramResidencyStats) -> usize {
        if !stats.has_skeleton_index {
            return 0;
        }
        // Pre-merge bind data can be freed; skeleton retains merge topology
        stats
            .pre_merge_bind_total_bytes
            .saturating_sub(stats.skeleton_estimated_size_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pressure_below_watermark() {
        let budget = ResidencyBudget {
            low_watermark_bytes: 1000,
            high_watermark_bytes: 2000,
        };
        let stats = MergedProgramResidencyStats {
            file_count: 1,
            bound_file_arena_count: 1,
            unique_arena_count: 1,
            symbol_arena_count: 0,
            declaration_arena_bucket_count: 0,
            declaration_arena_mapping_count: 0,
            has_skeleton_index: false,
            skeleton_merge_candidate_count: 0,
            skeleton_total_symbol_count: 0,
            skeleton_estimated_size_bytes: 0,
            pre_merge_bind_total_bytes: 300,
            total_bound_file_bytes: 200,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(budget.assess(&stats), MemoryPressure::Low);
    }

    #[test]
    fn high_pressure_above_watermark() {
        let budget = ResidencyBudget {
            low_watermark_bytes: 1000,
            high_watermark_bytes: 2000,
        };
        let stats = MergedProgramResidencyStats {
            file_count: 100,
            bound_file_arena_count: 100,
            unique_arena_count: 50,
            symbol_arena_count: 100,
            declaration_arena_bucket_count: 50,
            declaration_arena_mapping_count: 200,
            has_skeleton_index: true,
            skeleton_merge_candidate_count: 10,
            skeleton_total_symbol_count: 500,
            skeleton_estimated_size_bytes: 50,
            pre_merge_bind_total_bytes: 1500,
            total_bound_file_bytes: 1000,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(budget.assess(&stats), MemoryPressure::High);
    }

    #[test]
    fn eviction_savings_estimates_freed_bytes() {
        let stats = MergedProgramResidencyStats {
            file_count: 10,
            bound_file_arena_count: 10,
            unique_arena_count: 5,
            symbol_arena_count: 10,
            declaration_arena_bucket_count: 5,
            declaration_arena_mapping_count: 20,
            has_skeleton_index: true,
            skeleton_merge_candidate_count: 3,
            skeleton_total_symbol_count: 50,
            skeleton_estimated_size_bytes: 1000,
            pre_merge_bind_total_bytes: 50_000,
            total_bound_file_bytes: 20_000,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        // Savings = pre_merge - skeleton = 50000 - 1000 = 49000
        assert_eq!(ResidencyBudget::eviction_savings(&stats), 49_000);
    }

    #[test]
    fn no_eviction_without_skeleton() {
        let stats = MergedProgramResidencyStats {
            file_count: 10,
            bound_file_arena_count: 10,
            unique_arena_count: 5,
            symbol_arena_count: 10,
            declaration_arena_bucket_count: 5,
            declaration_arena_mapping_count: 20,
            has_skeleton_index: false,
            skeleton_merge_candidate_count: 0,
            skeleton_total_symbol_count: 0,
            skeleton_estimated_size_bytes: 0,
            pre_merge_bind_total_bytes: 50_000,
            total_bound_file_bytes: 20_000,
            unique_arena_estimated_bytes: 0,
            has_dep_graph: false,
            dep_graph_edge_count: 0,
            dep_graph_root_count: 0,
            dep_graph_is_acyclic: true,
            dep_graph_cycle_count: 0,
            dep_graph_unresolved_count: 0,
        };
        assert_eq!(ResidencyBudget::eviction_savings(&stats), 0);
    }
}
