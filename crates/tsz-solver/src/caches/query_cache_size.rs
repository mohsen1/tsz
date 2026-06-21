//! In-memory size estimation for [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`, so it keeps
//! access to the cache's private fields.

use super::*;

impl QueryCache<'_> {
    /// Estimate the in-memory size of all caches in bytes.
    ///
    /// Accounts for `FxHashMap` bucket overhead, key/value sizes, and heap
    /// allocations inside cached values (e.g., `Vec<PropertyInfo>` in the
    /// object-spread cache, `Arc<[Variance]>` in the variance cache).
    ///
    /// This is more accurate than `QueryCacheStatistics::estimated_size_bytes()`
    /// because it reads actual map capacities and heap contents.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // FxHashMap per-bucket overhead: hash + key + value + alignment padding.
        const BUCKET_OVERHEAD: usize = 64;

        let mut size = std::mem::size_of::<Self>();

        // eval_cache: (TypeId, bool) -> TypeId
        {
            let map = self.eval_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<EvaluationCacheKey>()
                    + std::mem::size_of::<TypeId>());
        }

        // closed_eval_cache: (TypeId, bool) -> TypeId
        {
            let map = self.closed_eval_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<EvaluationCacheKey>()
                    + std::mem::size_of::<TypeId>());
        }

        // application_eval_cache: (DefId, SmallVec<[TypeId; 4]>, bool) -> TypeId
        {
            let map = self.application_eval_cache.borrow();
            let base_entry = BUCKET_OVERHEAD
                + std::mem::size_of::<ApplicationEvalCacheKey>()
                + std::mem::size_of::<TypeId>();
            size += map.capacity() * base_entry;
            // SmallVec spills to heap when > 4 elements; account for spilled entries.
            for key in map.keys() {
                if key.1.spilled() {
                    size += key.1.capacity() * std::mem::size_of::<TypeId>();
                }
            }
        }

        // element_access_cache
        {
            let map = self.element_access_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<ElementAccessTypeCacheKey>()
                    + std::mem::size_of::<TypeId>());
        }

        // object_spread_properties_cache: TypeId -> Vec<PropertyInfo>
        {
            let map = self.object_spread_properties_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Vec<PropertyInfo>>());
            for props in map.values() {
                size += props.capacity() * std::mem::size_of::<PropertyInfo>();
            }
        }

        // collect_properties_result_cache: TypeId -> bounded (generation, result) slots
        size += self
            .collect_properties_result_cache
            .borrow()
            .estimated_size_bytes(BUCKET_OVERHEAD);

        // subtype_cache
        {
            let map = self.subtype_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<RelationCacheKey>()
                    + std::mem::size_of::<RelationCacheValue>());
        }

        // assignability_cache
        {
            let map = self.assignability_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<RelationCacheKey>()
                    + std::mem::size_of::<RelationCacheValue>());
        }

        // property_cache
        {
            let map = self.property_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<PropertyAccessCacheKey>()
                    + std::mem::size_of::<PropertyAccessResult>());
        }

        // variance_cache: DefId -> Arc<[Variance]>
        {
            let map = self.variance_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<DefId>()
                    + std::mem::size_of::<Arc<[Variance]>>());
            // Account for the Arc-allocated slice contents
            for arc in map.values() {
                size += arc.len() * std::mem::size_of::<Variance>();
            }
        }

        // canonical_cache
        {
            let map = self.canonical_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + 2 * std::mem::size_of::<TypeId>());
        }

        // intersection_merge_cache: TypeId -> Option<TypeId>
        {
            let map = self.intersection_merge_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Option<TypeId>>());
        }

        // instantiation_cache: (TypeId, CanonicalSubst, u8, Option<TypeId>) -> TypeId
        // CanonicalSubst's inline SmallVec buffer is included in the
        // `InstantiationCacheKey` size; spilled entries pay extra heap.
        size += self.instantiation_cache.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<InstantiationCacheKey>()
                + std::mem::size_of::<TypeId>());

        // subtype_reduction_cache: (SortedTypeIds, u8) -> Arc<[TypeId]>
        // Inline buffer is part of `SubtypeReductionKey`; the cached value
        // is `Arc<[TypeId]>` (16 bytes) plus the heap slice it points at.
        size += self.subtype_reduction_cache.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<SubtypeReductionKey>()
                + std::mem::size_of::<std::sync::Arc<[TypeId]>>());

        size
    }
}
