use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::hash::Hash;

pub(super) const MAX_GENERATIONS_PER_NARROWING_KEY: usize = 4;

type GenerationSlots<V> = SmallVec<[(u64, V); 1]>;

#[derive(Clone, Debug)]
pub struct GenerationMemo<K, V> {
    entries: FxHashMap<K, GenerationSlots<V>>,
}

impl<K, V> Default for GenerationMemo<K, V> {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }
}

impl<K, V> GenerationMemo<K, V>
where
    K: Eq + Hash,
    V: Copy,
{
    pub fn get(&self, key: &K, generation: u64) -> Option<V> {
        self.entries
            .get(key)?
            .iter()
            .find(|(slot_generation, _)| *slot_generation == generation)
            .map(|(_, value)| *value)
    }

    pub fn insert(&mut self, key: K, generation: u64, value: V) {
        let slots = self.entries.entry(key).or_default();

        if let Some((_, slot_value)) = slots
            .iter_mut()
            .find(|(slot_generation, _)| *slot_generation == generation)
        {
            *slot_value = value;
            return;
        }

        if slots.len() >= MAX_GENERATIONS_PER_NARROWING_KEY
            && let Some(oldest) = slots
                .iter()
                .enumerate()
                .min_by_key(|(_, (slot_generation, _))| *slot_generation)
                .map(|(index, _)| index)
        {
            slots.swap_remove(oldest);
        }

        slots.push((generation, value));
    }

    pub fn len(&self) -> usize {
        self.entries.values().map(SmallVec::len).sum()
    }

    pub fn key_count(&self) -> usize {
        self.entries.len()
    }

    pub fn max_slots_per_key(&self) -> usize {
        self.entries.values().map(SmallVec::len).max().unwrap_or(0)
    }

    pub fn estimated_size_bytes(&self, bucket_overhead: usize) -> usize {
        let mut size = self.entries.capacity()
            * (bucket_overhead
                + std::mem::size_of::<K>()
                + std::mem::size_of::<GenerationSlots<V>>());
        for slots in self.entries.values() {
            if slots.spilled() {
                size += slots.capacity() * std::mem::size_of::<(u64, V)>();
            }
        }
        size
    }

    pub fn key_extra_size_bytes(&self, extra_size: impl FnMut(&K) -> usize) -> usize {
        self.entries.keys().map(extra_size).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeId;

    #[test]
    fn serves_only_the_matching_generation() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        memo.insert(TypeId::STRING, 7, TypeId::NUMBER);

        assert_eq!(memo.get(&TypeId::STRING, 7), Some(TypeId::NUMBER));
        assert_eq!(memo.get(&TypeId::STRING, 8), None);
        assert_eq!(memo.get(&TypeId::BOOLEAN, 7), None);
    }

    #[test]
    fn reinsert_at_same_generation_updates_in_place() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        memo.insert(TypeId::STRING, 3, TypeId::NUMBER);
        memo.insert(TypeId::STRING, 3, TypeId::BOOLEAN);

        assert_eq!(memo.len(), 1);
        assert_eq!(memo.get(&TypeId::STRING, 3), Some(TypeId::BOOLEAN));
    }

    #[test]
    fn bounds_retained_generations_per_key_and_evicts_oldest() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        for generation in 1..=(MAX_GENERATIONS_PER_NARROWING_KEY as u64 + 3) {
            memo.insert(TypeId::STRING, generation, TypeId::NUMBER);
        }

        assert_eq!(memo.len(), MAX_GENERATIONS_PER_NARROWING_KEY);

        for generation in 1..=3 {
            assert_eq!(memo.get(&TypeId::STRING, generation), None);
        }
        for generation in 4..=7 {
            assert_eq!(memo.get(&TypeId::STRING, generation), Some(TypeId::NUMBER));
        }
    }

    #[test]
    fn bounds_each_key_independently() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        for generation in 1..=6 {
            memo.insert(TypeId::STRING, generation, TypeId::NUMBER);
            memo.insert(TypeId::BOOLEAN, generation, TypeId::STRING);
        }

        assert_eq!(memo.len(), MAX_GENERATIONS_PER_NARROWING_KEY * 2);
        assert_eq!(memo.get(&TypeId::STRING, 6), Some(TypeId::NUMBER));
        assert_eq!(memo.get(&TypeId::BOOLEAN, 6), Some(TypeId::STRING));
        assert_eq!(memo.get(&TypeId::STRING, 1), None);
        assert_eq!(memo.get(&TypeId::BOOLEAN, 1), None);
    }
}
