//! Generation-bounded memo for intersection-to-merged-object results.
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`.

use crate::caches::db::IntersectionMergeCacheEntry;
use crate::types::TypeId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

const MAX_GENERATIONS_PER_INTERSECTION: usize = 4;

type GenerationSlots = SmallVec<[(u64, IntersectionMergeCacheEntry); 1]>;

#[derive(Default)]
pub(super) struct IntersectionMergeMemo {
    entries: FxHashMap<TypeId, GenerationSlots>,
}

impl IntersectionMergeMemo {
    pub(super) fn get(
        &self,
        intersection_id: TypeId,
        generation: u64,
    ) -> Option<IntersectionMergeCacheEntry> {
        self.entries
            .get(&intersection_id)?
            .iter()
            .find(|(slot_generation, _)| *slot_generation == generation)
            .map(|(_, result)| *result)
    }

    pub(super) fn insert(
        &mut self,
        intersection_id: TypeId,
        generation: u64,
        result: Option<TypeId>,
    ) {
        let entry = IntersectionMergeCacheEntry::from_result(result);
        let slots = self.entries.entry(intersection_id).or_default();

        if let Some(slot) = slots
            .iter_mut()
            .find(|(slot_generation, _)| *slot_generation == generation)
        {
            slot.1 = entry;
            return;
        }

        if slots.len() >= MAX_GENERATIONS_PER_INTERSECTION
            && let Some(oldest) = slots
                .iter()
                .enumerate()
                .min_by_key(|(_, (slot_generation, _))| *slot_generation)
                .map(|(index, _)| index)
        {
            slots.swap_remove(oldest);
        }

        slots.push((generation, entry));
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn total_entries(&self) -> usize {
        self.entries.values().map(SmallVec::len).sum()
    }

    pub(super) fn estimated_size_bytes(&self, bucket_overhead: usize) -> usize {
        let mut size = self.entries.capacity()
            * (bucket_overhead
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<GenerationSlots>());
        for slots in self.entries.values() {
            if slots.spilled() {
                size +=
                    slots.capacity() * std::mem::size_of::<(u64, IntersectionMergeCacheEntry)>();
            }
        }
        size
    }
}
