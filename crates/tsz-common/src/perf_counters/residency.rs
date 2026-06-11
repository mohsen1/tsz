// ─── residency breakdown (snapshot-time byte gauges) ─────────────────────
//
// Per-category resident-bytes accounting for issue #13249 step 1 ("account
// first"). Unlike the event counters in `runtime.rs`, these are *gauges*:
// each owning layer records its category once, at end of run, after the
// structure has reached its final size:
//
// - `MergedProgram` (tsz-core) records ASTs/`NodeArena`s, per-file binder
//   state, lib binders, program-wide symbol state, the `DefinitionStore`,
//   the `TypeInterner` (including its per-`TypeId` predicate `DashMap`s),
//   and the skeleton index.
// - The CLI driver records the `SharedQueryCache` (before it drops at the
//   end of the check phase) and the retained per-file `TypeCache` map.
//
// All values are *estimates* (capacity-based byte math, not allocator
// truth), hence the `_bytes_est` suffixes. The walks that produce them are
// gated on [`enabled_fast`] at every call site, so disabled runs pay one
// branch and never touch these statics.

/// Process-wide residency gauges. Written once per category at snapshot
/// time; read by [`PerfCounters::snapshot`].
pub struct ResidencyGauges {
    /// True once any category has been recorded. Distinguishes "not
    /// measured" (`residency: null` in the JSON) from "measured zero".
    recorded_any: AtomicBool,
    ast_unique_arena_count: AtomicU64,
    ast_unique_arena_bytes_est: AtomicU64,
    bound_file_count: AtomicU64,
    bound_file_state_bytes_est: AtomicU64,
    lib_binder_count: AtomicU64,
    lib_binder_symbol_bytes_est: AtomicU64,
    program_symbol_state_bytes_est: AtomicU64,
    definition_store_bytes_est: AtomicU64,
    type_interner_bytes_est: AtomicU64,
    skeleton_index_bytes_est: AtomicU64,
    pre_merge_bind_total_bytes_est: AtomicU64,
    shared_query_cache_entries: AtomicU64,
    shared_query_cache_bytes_est: AtomicU64,
    type_cache_count: AtomicU64,
    type_cache_bytes_est: AtomicU64,
}

static RESIDENCY_GAUGES: OnceLock<ResidencyGauges> = OnceLock::new();

fn residency_gauges() -> &'static ResidencyGauges {
    RESIDENCY_GAUGES.get_or_init(|| ResidencyGauges {
        recorded_any: AtomicBool::new(false),
        ast_unique_arena_count: AtomicU64::new(0),
        ast_unique_arena_bytes_est: AtomicU64::new(0),
        bound_file_count: AtomicU64::new(0),
        bound_file_state_bytes_est: AtomicU64::new(0),
        lib_binder_count: AtomicU64::new(0),
        lib_binder_symbol_bytes_est: AtomicU64::new(0),
        program_symbol_state_bytes_est: AtomicU64::new(0),
        definition_store_bytes_est: AtomicU64::new(0),
        type_interner_bytes_est: AtomicU64::new(0),
        skeleton_index_bytes_est: AtomicU64::new(0),
        pre_merge_bind_total_bytes_est: AtomicU64::new(0),
        shared_query_cache_entries: AtomicU64::new(0),
        shared_query_cache_bytes_est: AtomicU64::new(0),
        type_cache_count: AtomicU64::new(0),
        type_cache_bytes_est: AtomicU64::new(0),
    })
}

/// Merged-program residency categories, recorded by
/// `MergedProgram::record_residency_breakdown` in tsz-core once the program
/// has been fully checked. Field semantics mirror [`ResidencySnapshot`].
#[derive(Debug, Clone, Copy, Default)]
pub struct MergedProgramResidencyRecord {
    pub ast_unique_arena_count: u64,
    pub ast_unique_arena_bytes_est: u64,
    pub bound_file_count: u64,
    pub bound_file_state_bytes_est: u64,
    pub lib_binder_count: u64,
    pub lib_binder_symbol_bytes_est: u64,
    pub program_symbol_state_bytes_est: u64,
    pub definition_store_bytes_est: u64,
    pub type_interner_bytes_est: u64,
    pub skeleton_index_bytes_est: u64,
    pub pre_merge_bind_total_bytes_est: u64,
}

/// Record the merged-program residency categories. Gated on
/// [`enabled_fast`]; callers should also gate the (much more expensive)
/// arena walk that computes the record on the same check.
pub fn record_merged_program_residency(record: &MergedProgramResidencyRecord) {
    if !enabled_fast() {
        return;
    }
    let g = residency_gauges();
    g.ast_unique_arena_count
        .store(record.ast_unique_arena_count, Ordering::Relaxed);
    g.ast_unique_arena_bytes_est
        .store(record.ast_unique_arena_bytes_est, Ordering::Relaxed);
    g.bound_file_count
        .store(record.bound_file_count, Ordering::Relaxed);
    g.bound_file_state_bytes_est
        .store(record.bound_file_state_bytes_est, Ordering::Relaxed);
    g.lib_binder_count
        .store(record.lib_binder_count, Ordering::Relaxed);
    g.lib_binder_symbol_bytes_est
        .store(record.lib_binder_symbol_bytes_est, Ordering::Relaxed);
    g.program_symbol_state_bytes_est
        .store(record.program_symbol_state_bytes_est, Ordering::Relaxed);
    g.definition_store_bytes_est
        .store(record.definition_store_bytes_est, Ordering::Relaxed);
    g.type_interner_bytes_est
        .store(record.type_interner_bytes_est, Ordering::Relaxed);
    g.skeleton_index_bytes_est
        .store(record.skeleton_index_bytes_est, Ordering::Relaxed);
    g.pre_merge_bind_total_bytes_est
        .store(record.pre_merge_bind_total_bytes_est, Ordering::Relaxed);
    g.recorded_any.store(true, Ordering::Relaxed);
}

/// Record the shared cross-file query cache's resident size. Called by the
/// CLI driver at the end of the check phase, while the cache is still alive.
pub fn record_shared_query_cache_residency(entries: u64, bytes_est: u64) {
    if !enabled_fast() {
        return;
    }
    let g = residency_gauges();
    g.shared_query_cache_entries
        .store(entries, Ordering::Relaxed);
    g.shared_query_cache_bytes_est
        .store(bytes_est, Ordering::Relaxed);
    g.recorded_any.store(true, Ordering::Relaxed);
}

/// Record the retained per-file `TypeCache` map's resident size (emit runs
/// pin one `TypeCache` per file until emit completes; pure `--noEmit` runs
/// skip extraction and record zero here).
pub fn record_type_cache_residency(count: u64, bytes_est: u64) {
    if !enabled_fast() {
        return;
    }
    let g = residency_gauges();
    g.type_cache_count.store(count, Ordering::Relaxed);
    g.type_cache_bytes_est.store(bytes_est, Ordering::Relaxed);
    g.recorded_any.store(true, Ordering::Relaxed);
}

/// JSON view of the residency gauges. All `*_bytes_est` values are
/// capacity-based estimates, not allocator ground truth; compare against
/// `/usr/bin/time -l` max RSS for the unaccounted remainder.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ResidencySnapshot {
    /// Unique `NodeArena` allocations (user files + lib files, deduplicated
    /// by pointer identity across all retaining maps).
    pub ast_unique_arena_count: u64,
    pub ast_unique_arena_bytes_est: u64,
    /// `BoundFile` entries retained by `MergedProgram.files`.
    pub bound_file_count: u64,
    /// Per-file binder state (`node_symbols`, scopes, flow graph, …)
    /// excluding the arenas counted above.
    pub bound_file_state_bytes_est: u64,
    /// Lib `BinderState`s retained for global type resolution.
    pub lib_binder_count: u64,
    /// Lib binder symbol-arena bytes (lib `NodeArena`s are deduplicated
    /// into `ast_unique_arena_bytes_est`).
    pub lib_binder_symbol_bytes_est: u64,
    /// Program-wide symbol state: merged `SymbolArena`, globals/file-locals
    /// tables, module exports, semantic defs, declaration-arena map
    /// overhead.
    pub program_symbol_state_bytes_est: u64,
    /// Shared solver `DefinitionStore`.
    pub definition_store_bytes_est: u64,
    /// `TypeInterner` storage including its per-`TypeId` predicate
    /// `DashMap` caches.
    pub type_interner_bytes_est: u64,
    /// Skeleton index (zero when not computed).
    pub skeleton_index_bytes_est: u64,
    /// Pre-merge `BindResult` footprint captured before the merge consumed
    /// per-file data (informational; this memory is released after merge).
    pub pre_merge_bind_total_bytes_est: u64,
    /// `SharedQueryCache` entries across eval/subtype/assignability maps.
    pub shared_query_cache_entries: u64,
    pub shared_query_cache_bytes_est: u64,
    /// Per-file `TypeCache` snapshots retained for emit.
    pub type_cache_count: u64,
    pub type_cache_bytes_est: u64,
    /// Sum of every `*_bytes_est` category above except
    /// `pre_merge_bind_total_bytes_est` (which is transient, not resident
    /// at snapshot time).
    pub tracked_total_bytes_est: u64,
}

/// Load the residency gauges into a snapshot. Returns `None` when no
/// category has been recorded (counters disabled, or a driver that does
/// not wire residency recording).
pub(crate) fn snapshot_residency() -> Option<ResidencySnapshot> {
    let g = RESIDENCY_GAUGES.get()?;
    if !g.recorded_any.load(Ordering::Relaxed) {
        return None;
    }
    let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
    let ast_unique_arena_bytes_est = load(&g.ast_unique_arena_bytes_est);
    let bound_file_state_bytes_est = load(&g.bound_file_state_bytes_est);
    let lib_binder_symbol_bytes_est = load(&g.lib_binder_symbol_bytes_est);
    let program_symbol_state_bytes_est = load(&g.program_symbol_state_bytes_est);
    let definition_store_bytes_est = load(&g.definition_store_bytes_est);
    let type_interner_bytes_est = load(&g.type_interner_bytes_est);
    let skeleton_index_bytes_est = load(&g.skeleton_index_bytes_est);
    let shared_query_cache_bytes_est = load(&g.shared_query_cache_bytes_est);
    let type_cache_bytes_est = load(&g.type_cache_bytes_est);
    let tracked_total_bytes_est = ast_unique_arena_bytes_est
        + bound_file_state_bytes_est
        + lib_binder_symbol_bytes_est
        + program_symbol_state_bytes_est
        + definition_store_bytes_est
        + type_interner_bytes_est
        + skeleton_index_bytes_est
        + shared_query_cache_bytes_est
        + type_cache_bytes_est;
    Some(ResidencySnapshot {
        ast_unique_arena_count: load(&g.ast_unique_arena_count),
        ast_unique_arena_bytes_est,
        bound_file_count: load(&g.bound_file_count),
        bound_file_state_bytes_est,
        lib_binder_count: load(&g.lib_binder_count),
        lib_binder_symbol_bytes_est,
        program_symbol_state_bytes_est,
        definition_store_bytes_est,
        type_interner_bytes_est,
        skeleton_index_bytes_est,
        pre_merge_bind_total_bytes_est: load(&g.pre_merge_bind_total_bytes_est),
        shared_query_cache_entries: load(&g.shared_query_cache_entries),
        shared_query_cache_bytes_est,
        type_cache_count: load(&g.type_cache_count),
        type_cache_bytes_est,
        tracked_total_bytes_est,
    })
}
