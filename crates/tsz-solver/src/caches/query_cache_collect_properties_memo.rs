//! Generation-bounded memo for context-free `collect_properties` results.
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`.
//!
//! # Why a bounded memo (issue #14347)
//!
//! `collect_properties_cached` (`objects/collect.rs`) produces a result that is
//! valid only for the resolver *generation* at which it was computed: a later
//! `Lazy(DefId)` resolution can change the answer, so the legacy memo stamped
//! every result with `(TypeId, resolver_generation)`. The resolver generation
//! is a program-wide epoch (`def/resolver.rs`: env-local counter plus the
//! shared `DefinitionStore` epoch), bumped at ~44 sites — every cross-file
//! def-body publish and every `set_this_type` during class-scope relation
//! checks. Each bump advances the generation a later lookup will request, so
//! the prior generation's entry is stranded: never read again, never evicted,
//! growing the flat `(TypeId, generation)` map by one mostly-dead entry per
//! cacheable collection until the per-file `clear()`.
//!
//! Resolver generations advance **monotonically**, so an entry stamped with a
//! superseded generation can never be served again — a lookup always supplies
//! the caller's *current* generation. This memo therefore retains, per
//! `TypeId`, only the few most-recent generations and evicts the oldest once
//! that bound is reached. The eviction is value-preserving: a dropped entry can
//! only force an identical recomputation, never a stale answer, because the
//! served result is still a pure function of `(TypeId, current generation)`.
//!
//! This is the parity-safe residency slice of #14347. Replacing the generation
//! epoch with per-dependency invalidation (so unrelated def publishes stop
//! invalidating the entry at all) rides on canonical type identity (#14344) and
//! is tracked separately.

use crate::objects::PropertyCollectionResult;
use crate::types::TypeId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Distinct generations retained per `TypeId`.
///
/// A per-file `QueryCache` is driven by only a handful of live resolvers at any
/// instant — the checker's two `TypeEnvironment`s plus the generation-0 `NOOP`
/// resolver — and each advances monotonically, so its superseded generations
/// are dead the moment it moves on. Four slots cover every generation a live
/// resolver can still request while capping the dead-entry growth the flat
/// `(TypeId, generation)` map suffered.
const MAX_GENERATIONS_PER_TYPE: usize = 4;

/// Per-`TypeId` ring of recent `(generation, result)` pairs.
///
/// `SmallVec` keeps the common single-generation case inline (no heap
/// allocation) while bounding the worst case to [`MAX_GENERATIONS_PER_TYPE`].
type GenerationSlots = SmallVec<[(u64, PropertyCollectionResult); 1]>;

/// Generation-bounded store for context-free `collect_properties` results.
#[derive(Default)]
pub(super) struct CollectPropertiesMemo {
    entries: FxHashMap<TypeId, GenerationSlots>,
}

impl CollectPropertiesMemo {
    /// Look up the result cached for `type_id` at exactly `generation`.
    ///
    /// A stamp mismatch is a miss: the caller's generation is authoritative, so
    /// a result from any other generation must be recomputed rather than served.
    pub(super) fn get(&self, type_id: TypeId, generation: u64) -> Option<PropertyCollectionResult> {
        self.entries
            .get(&type_id)?
            .iter()
            .find(|(slot_generation, _)| *slot_generation == generation)
            .map(|(_, result)| result.clone())
    }

    /// Record `result` for `type_id` at `generation`, evicting the oldest
    /// retained generation for that `type_id` once the per-type bound is hit.
    pub(super) fn insert(
        &mut self,
        type_id: TypeId,
        generation: u64,
        result: PropertyCollectionResult,
    ) {
        let slots = self.entries.entry(type_id).or_default();

        if let Some(slot) = slots
            .iter_mut()
            .find(|(slot_generation, _)| *slot_generation == generation)
        {
            // A re-collection at the same generation must be identical; refresh
            // in place so a generation is never represented twice.
            slot.1 = result;
            return;
        }

        if slots.len() >= MAX_GENERATIONS_PER_TYPE
            && let Some(oldest) = slots
                .iter()
                .enumerate()
                .min_by_key(|(_, (slot_generation, _))| *slot_generation)
                .map(|(index, _)| index)
        {
            // Generations only advance, so the smallest stamp is the one a live
            // resolver is least likely to request again.
            slots.swap_remove(oldest);
        }

        slots.push((generation, result));
    }

    /// Drop every entry. Shares the per-file `QueryCache::clear` lifecycle.
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Estimated heap footprint in bytes, for `QueryCache::estimated_size_bytes`.
    pub(super) fn estimated_size_bytes(&self, bucket_overhead: usize) -> usize {
        let mut size = self.entries.capacity()
            * (bucket_overhead
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<GenerationSlots>());
        for slots in self.entries.values() {
            // Only spilled `SmallVec`s allocate; the inline element is already
            // counted in `size_of::<GenerationSlots>()` above.
            if slots.spilled() {
                size += slots.capacity() * std::mem::size_of::<(u64, PropertyCollectionResult)>();
            }
        }
        size
    }

    #[cfg(test)]
    pub(super) fn total_entries(&self) -> usize {
        self.entries.values().map(SmallVec::len).sum()
    }

    #[cfg(test)]
    pub(super) fn type_count(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(super) fn generations_for(&self, type_id: TypeId) -> usize {
        self.entries.get(&type_id).map_or(0, SmallVec::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(tag: u8) -> PropertyCollectionResult {
        // Distinct, cheaply-comparable results so a lookup proves it returned
        // the value stored for *that* generation, not a neighbour's.
        match tag {
            0 => PropertyCollectionResult::Any,
            1 => PropertyCollectionResult::NonObject,
            _ => PropertyCollectionResult::Properties {
                properties: Vec::new(),
                string_index: None,
                number_index: None,
            },
        }
    }

    #[test]
    fn serves_only_the_matching_generation() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        memo.insert(t, 7, result(0));

        assert_eq!(memo.get(t, 7), Some(result(0)));
        // The caller's generation is authoritative: a different stamp misses
        // even though a result for this `TypeId` exists.
        assert_eq!(memo.get(t, 8), None);
        assert_eq!(memo.get(TypeId::NUMBER, 7), None);
    }

    #[test]
    fn re_collection_at_same_generation_updates_in_place() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        memo.insert(t, 3, result(0));
        memo.insert(t, 3, result(1));

        assert_eq!(memo.generations_for(t), 1);
        assert_eq!(memo.get(t, 3), Some(result(1)));
    }

    #[test]
    fn bounds_retained_generations_and_evicts_oldest() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        // Insert more distinct generations than the per-type bound allows.
        for generation in 1..=(MAX_GENERATIONS_PER_TYPE as u64 + 3) {
            memo.insert(t, generation, result(2));
        }

        assert_eq!(memo.generations_for(t), MAX_GENERATIONS_PER_TYPE);

        // The oldest generations were evicted; only the most recent survive,
        // and each still serves the value it was stored with.
        let highest = MAX_GENERATIONS_PER_TYPE as u64 + 3;
        for generation in 1..=3 {
            assert_eq!(memo.get(t, generation), None, "stale generation evicted");
        }
        for generation in (highest - MAX_GENERATIONS_PER_TYPE as u64 + 1)..=highest {
            assert_eq!(memo.get(t, generation), Some(result(2)));
        }
    }

    #[test]
    fn generations_are_tracked_per_type() {
        let mut memo = CollectPropertiesMemo::default();

        memo.insert(TypeId::STRING, 1, result(0));
        memo.insert(TypeId::NUMBER, 1, result(1));
        memo.insert(TypeId::STRING, 2, result(2));

        assert_eq!(memo.type_count(), 2);
        assert_eq!(memo.get(TypeId::STRING, 1), Some(result(0)));
        assert_eq!(memo.get(TypeId::STRING, 2), Some(result(2)));
        assert_eq!(memo.get(TypeId::NUMBER, 1), Some(result(1)));
    }

    #[test]
    fn clear_drops_all_entries() {
        let mut memo = CollectPropertiesMemo::default();
        memo.insert(TypeId::STRING, 1, result(0));
        memo.insert(TypeId::NUMBER, 2, result(1));

        memo.clear();

        assert_eq!(memo.total_entries(), 0);
        assert_eq!(memo.get(TypeId::STRING, 1), None);
    }
}
