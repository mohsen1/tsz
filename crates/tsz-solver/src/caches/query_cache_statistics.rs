//! Query cache statistics and size-accounting snapshots.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationCacheStats {
    pub subtype_hits: u64,
    pub subtype_misses: u64,
    pub subtype_entries: usize,
    pub assignability_hits: u64,
    pub assignability_misses: u64,
    pub assignability_entries: usize,
}

/// Snapshot of all `QueryCache` sizes for observability.
///
/// Captures entry counts for every memoization cache and relation hit/miss
/// counters. Intended for `--extendedDiagnostics` and performance monitoring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueryCacheStatistics {
    /// Number of memoized `evaluate_type` results.
    pub eval_cache_entries: usize,
    /// Number of memoized substitution-independent evaluation results.
    pub closed_eval_cache_entries: usize,
    /// Number of memoized conditional-branch subtype verdicts (#8356 / #13097).
    pub conditional_branch_verdict_cache_entries: usize,
    /// Number of memoized application evaluation results.
    pub application_eval_cache_entries: usize,
    /// Number of times the application eval cache returned a hit.
    pub application_eval_cache_hits: u64,
    /// Number of times the application eval cache was probed and missed.
    pub application_eval_cache_misses: u64,
    /// Number of local misses satisfied by the opt-in shared application eval cache.
    pub application_eval_cache_shared_hits: u64,
    /// Number of opt-in shared application eval cache probes that missed.
    pub application_eval_cache_shared_misses: u64,
    /// Number of application eval entries promoted into the opt-in shared cache.
    pub application_eval_cache_shared_inserts: u64,
    /// Number of memoized element access results.
    pub element_access_cache_entries: usize,
    /// Number of memoized object spread property lists.
    pub object_spread_cache_entries: usize,
    /// Number of memoized property access results.
    pub property_cache_entries: usize,
    /// Number of memoized variance computations.
    pub variance_cache_entries: usize,
    /// Number of memoized canonical type mappings.
    pub canonical_cache_entries: usize,
    /// Number of memoized intersection-to-merged-object results.
    pub intersection_merge_cache_entries: usize,
    /// Number of times the intersection-merge cache returned a hit.
    pub intersection_merge_cache_hits: u64,
    /// Number of times the intersection-merge cache was probed and missed.
    pub intersection_merge_cache_misses: u64,
    /// Number of memoized `instantiate_type` results.
    pub instantiation_cache_entries: usize,
    /// Number of times the instantiation cache returned a hit.
    pub instantiation_cache_hits: u64,
    /// Number of times the instantiation cache was probed and missed.
    pub instantiation_cache_misses: u64,
    /// Number of local misses satisfied by the opt-in shared instantiation cache.
    pub instantiation_cache_shared_hits: u64,
    /// Number of opt-in shared instantiation cache probes that missed.
    pub instantiation_cache_shared_misses: u64,
    /// Number of instantiation entries promoted into the opt-in shared cache.
    pub instantiation_cache_shared_inserts: u64,
    /// Number of memoized `remove_subtypes_for_bct` results.
    pub subtype_reduction_cache_entries: usize,
    /// Number of times the subtype-reduction cache returned a hit.
    pub subtype_reduction_cache_hits: u64,
    /// Number of times the subtype-reduction cache was probed and missed.
    pub subtype_reduction_cache_misses: u64,
    /// Relation (subtype + assignability) cache statistics.
    pub relation: RelationCacheStats,
}

impl QueryCacheStatistics {
    /// Merge another snapshot into this one (for aggregating per-file caches in parallel builds).
    pub const fn merge(&mut self, other: &QueryCacheStatistics) {
        self.eval_cache_entries += other.eval_cache_entries;
        self.closed_eval_cache_entries += other.closed_eval_cache_entries;
        self.conditional_branch_verdict_cache_entries +=
            other.conditional_branch_verdict_cache_entries;
        self.application_eval_cache_entries += other.application_eval_cache_entries;
        self.application_eval_cache_hits += other.application_eval_cache_hits;
        self.application_eval_cache_misses += other.application_eval_cache_misses;
        self.application_eval_cache_shared_hits += other.application_eval_cache_shared_hits;
        self.application_eval_cache_shared_misses += other.application_eval_cache_shared_misses;
        self.application_eval_cache_shared_inserts += other.application_eval_cache_shared_inserts;
        self.element_access_cache_entries += other.element_access_cache_entries;
        self.object_spread_cache_entries += other.object_spread_cache_entries;
        self.property_cache_entries += other.property_cache_entries;
        self.variance_cache_entries += other.variance_cache_entries;
        self.canonical_cache_entries += other.canonical_cache_entries;
        self.intersection_merge_cache_entries += other.intersection_merge_cache_entries;
        self.intersection_merge_cache_hits += other.intersection_merge_cache_hits;
        self.intersection_merge_cache_misses += other.intersection_merge_cache_misses;
        self.instantiation_cache_entries += other.instantiation_cache_entries;
        self.instantiation_cache_hits += other.instantiation_cache_hits;
        self.instantiation_cache_misses += other.instantiation_cache_misses;
        self.instantiation_cache_shared_hits += other.instantiation_cache_shared_hits;
        self.instantiation_cache_shared_misses += other.instantiation_cache_shared_misses;
        self.instantiation_cache_shared_inserts += other.instantiation_cache_shared_inserts;
        self.subtype_reduction_cache_entries += other.subtype_reduction_cache_entries;
        self.subtype_reduction_cache_hits += other.subtype_reduction_cache_hits;
        self.subtype_reduction_cache_misses += other.subtype_reduction_cache_misses;
        self.relation.subtype_hits += other.relation.subtype_hits;
        self.relation.subtype_misses += other.relation.subtype_misses;
        self.relation.subtype_entries += other.relation.subtype_entries;
        self.relation.assignability_hits += other.relation.assignability_hits;
        self.relation.assignability_misses += other.relation.assignability_misses;
        self.relation.assignability_entries += other.relation.assignability_entries;
    }

    /// Estimate total in-memory size of all caches in bytes.
    ///
    /// Uses conservative per-entry estimates for `FxHashMap` bucket metadata plus
    /// key/value sizes. Heap allocations inside values are intentionally excluded.
    #[must_use]
    pub const fn estimated_size_bytes(&self) -> usize {
        const BUCKET_OVERHEAD: usize = 64;

        let eval = self.eval_cache_entries * (BUCKET_OVERHEAD + 13);
        let closed_eval = self.closed_eval_cache_entries * (BUCKET_OVERHEAD + 13);
        // (TypeId, TypeId, bool, bool) key + bool value ≈ 12 bytes.
        let conditional_verdict =
            self.conditional_branch_verdict_cache_entries * (BUCKET_OVERHEAD + 12);
        let app_eval = self.application_eval_cache_entries * (BUCKET_OVERHEAD + 37);
        let elem = self.element_access_cache_entries * (BUCKET_OVERHEAD + 21);
        let spread = self.object_spread_cache_entries * (BUCKET_OVERHEAD + 4 + 24 + 256);
        let prop = self.property_cache_entries * (BUCKET_OVERHEAD + 25);
        let variance = self.variance_cache_entries * (BUCKET_OVERHEAD + 16);
        let canonical = self.canonical_cache_entries * (BUCKET_OVERHEAD + 8);
        let intersection_merge = self.intersection_merge_cache_entries * (BUCKET_OVERHEAD + 12);
        let subtype = self.relation.subtype_entries * (BUCKET_OVERHEAD + 13);
        let assignability = self.relation.assignability_entries * (BUCKET_OVERHEAD + 13);
        let instantiation = self.instantiation_cache_entries * (BUCKET_OVERHEAD + 65);
        let subtype_reduction = self.subtype_reduction_cache_entries * (BUCKET_OVERHEAD + 73);

        eval + closed_eval
            + conditional_verdict
            + app_eval
            + elem
            + spread
            + prop
            + variance
            + canonical
            + intersection_merge
            + subtype
            + assignability
            + instantiation
            + subtype_reduction
    }
}

impl std::fmt::Display for QueryCacheStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "QueryCache statistics:")?;
        writeln!(f, "  eval_cache:             {}", self.eval_cache_entries)?;
        writeln!(
            f,
            "  closed_eval_cache:      {}",
            self.closed_eval_cache_entries
        )?;
        writeln!(
            f,
            "  cond_branch_verdict:    {}",
            self.conditional_branch_verdict_cache_entries
        )?;
        writeln!(
            f,
            "  application_eval_cache: {} entries ({} hits, {} misses; shared {} hits, {} misses, {} inserts)",
            self.application_eval_cache_entries,
            self.application_eval_cache_hits,
            self.application_eval_cache_misses,
            self.application_eval_cache_shared_hits,
            self.application_eval_cache_shared_misses,
            self.application_eval_cache_shared_inserts,
        )?;
        writeln!(
            f,
            "  element_access_cache:   {}",
            self.element_access_cache_entries
        )?;
        writeln!(
            f,
            "  object_spread_cache:    {}",
            self.object_spread_cache_entries
        )?;
        writeln!(
            f,
            "  property_cache:         {}",
            self.property_cache_entries
        )?;
        writeln!(
            f,
            "  variance_cache:         {}",
            self.variance_cache_entries
        )?;
        writeln!(
            f,
            "  canonical_cache:        {}",
            self.canonical_cache_entries
        )?;
        writeln!(
            f,
            "  intersection_merge:     {} entries ({} hits, {} misses)",
            self.intersection_merge_cache_entries,
            self.intersection_merge_cache_hits,
            self.intersection_merge_cache_misses,
        )?;
        writeln!(
            f,
            "  subtype_cache:          {} entries ({} hits, {} misses)",
            self.relation.subtype_entries, self.relation.subtype_hits, self.relation.subtype_misses,
        )?;
        writeln!(
            f,
            "  assignability_cache:    {} entries ({} hits, {} misses)",
            self.relation.assignability_entries,
            self.relation.assignability_hits,
            self.relation.assignability_misses,
        )?;
        writeln!(
            f,
            "  instantiation_cache:    {} entries ({} hits, {} misses; shared {} hits, {} misses, {} inserts)",
            self.instantiation_cache_entries,
            self.instantiation_cache_hits,
            self.instantiation_cache_misses,
            self.instantiation_cache_shared_hits,
            self.instantiation_cache_shared_misses,
            self.instantiation_cache_shared_inserts,
        )?;
        writeln!(
            f,
            "  subtype_reduction:      {} entries ({} hits, {} misses)",
            self.subtype_reduction_cache_entries,
            self.subtype_reduction_cache_hits,
            self.subtype_reduction_cache_misses,
        )?;
        write!(
            f,
            "  estimated_size:         {} bytes ({:.1} KB)",
            self.estimated_size_bytes(),
            self.estimated_size_bytes() as f64 / 1024.0,
        )
    }
}
