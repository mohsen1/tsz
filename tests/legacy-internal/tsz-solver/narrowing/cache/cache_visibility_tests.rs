use super::*;
use crate::intern::TypeInterner;
use crate::narrowing::generation_memo::MAX_GENERATIONS_PER_NARROWING_KEY;
use crate::narrowing::guard::{GuardSense, TypeGuard};
use crate::narrowing::request::{NarrowingOptions, NarrowingRequest};
use crate::types::TypeId;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[test]
fn narrowing_cache_statistics_report_entries_and_size() {
    let db = TypeInterner::new();
    let prop = db.intern_string("prop");
    let key = (TypeId::STRING, prop);
    let chain_key = OptionalPropertyChainKey {
        root_type: TypeId::STRING,
        properties: vec![prop],
        optional_mask: 1,
        no_unchecked_indexed_access: true,
    };
    let cache = NarrowingCache::new();
    let empty = cache.cache_statistics();

    assert_eq!(empty.total_entries(), 0);
    assert!(empty.estimated_size_bytes > 0);

    cache
        .resolve_cache
        .borrow_mut()
        .insert(TypeId::STRING, TypeId::NUMBER);
    cache.property_cache.borrow_mut().insert(
        key,
        7,
        Some(CachedPropertyType::explicit(TypeId::BOOLEAN)),
    );
    cache
        .required_property_cache
        .borrow_mut()
        .insert(key, 7, true);
    cache
        .split_nullish_cache
        .borrow_mut()
        .insert(TypeId::STRING, (Some(TypeId::STRING), Some(TypeId::NULL)));
    cache
        .contains_type_parameters_cache
        .borrow_mut()
        .insert(TypeId::STRING, false);
    cache.optional_chain_cache.borrow_mut().insert(
        (TypeId::STRING, prop),
        7,
        CachedChainType::new(TypeId::BOOLEAN, false),
    );
    cache.optional_property_chain_cache.borrow_mut().insert(
        chain_key,
        7,
        CachedChainType::new(TypeId::BOOLEAN, false),
    );
    cache
        .contextual_resolve_cache
        .borrow_mut()
        .insert(TypeId::STRING, TypeId::BOOLEAN);
    let mut discriminants = FxHashMap::default();
    discriminants.insert(TypeId::STRING, vec![TypeId::BOOLEAN]);
    cache
        .discriminant_index
        .borrow_mut()
        .insert((TypeId::STRING, prop), Arc::new(discriminants));
    cache.narrow_type_cache.borrow_mut().insert(
        NarrowingRequest::new(TypeId::STRING, TypeGuard::Truthy, GuardSense::Positive)
            .stable_cache_key(NarrowingOptions::new()),
        0,
        TypeId::STRING,
    );

    let stats = cache.cache_statistics();
    assert_eq!(stats.resolve_cache_entries, 1);
    assert_eq!(stats.narrowed_property_cache_entries, 1);
    assert_eq!(stats.required_property_cache_entries, 1);
    assert_eq!(stats.split_nullish_cache_entries, 1);
    assert_eq!(stats.contains_type_parameters_cache_entries, 1);
    assert_eq!(stats.optional_chain_cache_entries, 1);
    assert_eq!(stats.optional_property_chain_cache_entries, 1);
    assert_eq!(stats.contextual_resolve_cache_entries, 1);
    assert_eq!(stats.discriminant_index_entries, 1);
    assert_eq!(stats.narrow_type_cache_entries, 1);
    assert_eq!(stats.total_entries(), 10);
    assert!(stats.estimated_size_bytes > empty.estimated_size_bytes);
    assert!(cache.estimated_size_bytes() >= stats.estimated_size_bytes);
}

#[test]
fn generation_stamped_narrowing_caches_bound_retained_generations() {
    let db = TypeInterner::new();
    let prop = db.intern_string("prop");
    let cache = NarrowingCache::new();
    let property_key = (TypeId::STRING, prop);
    let chain_key = OptionalPropertyChainKey {
        root_type: TypeId::STRING,
        properties: vec![prop],
        optional_mask: 1,
        no_unchecked_indexed_access: false,
    };
    let request_key =
        NarrowingRequest::new(TypeId::STRING, TypeGuard::Truthy, GuardSense::Positive)
            .stable_cache_key(NarrowingOptions::new());
    let relation_key = NarrowExcludingStableKey {
        source: TypeId::STRING,
        excluded: TypeId::NUMBER,
    };

    for generation in 1..=(MAX_GENERATIONS_PER_NARROWING_KEY as u64 + 3) {
        cache.property_cache.borrow_mut().insert(
            property_key,
            generation,
            Some(CachedPropertyType::explicit(TypeId::BOOLEAN)),
        );
        cache
            .required_property_cache
            .borrow_mut()
            .insert(property_key, generation, true);
        cache.optional_chain_cache.borrow_mut().insert(
            property_key,
            generation,
            CachedChainType::new(TypeId::BOOLEAN, false),
        );
        cache.optional_property_chain_cache.borrow_mut().insert(
            chain_key.clone(),
            generation,
            CachedChainType::new(TypeId::NUMBER, false),
        );
        cache.narrow_type_cache.borrow_mut().insert(
            request_key.clone(),
            generation,
            TypeId::STRING,
        );
        cache
            .narrow_excluding_cache
            .borrow_mut()
            .insert(relation_key, generation, TypeId::BOOLEAN);
        cache
            .narrow_assignable_cache
            .borrow_mut()
            .insert(relation_key, generation, true);
        cache
            .narrow_subtype_cache
            .borrow_mut()
            .insert(relation_key, generation, true);
    }

    let stats = cache.cache_statistics();
    assert_eq!(stats.generation_stamped_cache_keys, 8);
    assert_eq!(
        stats.max_generation_slots_per_cache_key,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.narrowed_property_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.required_property_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.optional_chain_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.optional_property_chain_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.narrow_type_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.narrow_excluding_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.narrow_assignable_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.narrow_subtype_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );

    assert_eq!(cache.property_cache.borrow().get(&property_key, 1), None);
    assert_eq!(
        cache.property_cache.borrow().get(&property_key, 7),
        Some(Some(CachedPropertyType::explicit(TypeId::BOOLEAN)))
    );
    assert_eq!(
        cache.optional_chain_cache.borrow().get(&property_key, 1),
        None
    );
    assert_eq!(
        cache.optional_chain_cache.borrow().get(&property_key, 7),
        Some(CachedChainType::new(TypeId::BOOLEAN, false))
    );
    assert_eq!(
        cache
            .optional_property_chain_cache
            .borrow()
            .get(&chain_key, 1),
        None
    );
    assert_eq!(
        cache
            .optional_property_chain_cache
            .borrow()
            .get(&chain_key, 7),
        Some(CachedChainType::new(TypeId::NUMBER, false))
    );
    assert_eq!(cache.narrow_type_cache.borrow().get(&request_key, 1), None);
    assert_eq!(
        cache.narrow_type_cache.borrow().get(&request_key, 7),
        Some(TypeId::STRING)
    );
}

#[test]
fn optional_chain_caches_serve_only_matching_resolver_generation() {
    let db = TypeInterner::new();
    let prop = db.intern_string("prop");
    let property_key = (TypeId::STRING, prop);
    let chain_key = OptionalPropertyChainKey {
        root_type: TypeId::STRING,
        properties: vec![prop],
        optional_mask: 1,
        no_unchecked_indexed_access: false,
    };
    let cache = NarrowingCache::new();

    cache.optional_chain_cache.borrow_mut().insert(
        property_key,
        3,
        CachedChainType::new(TypeId::BOOLEAN, false),
    );
    cache.optional_property_chain_cache.borrow_mut().insert(
        chain_key.clone(),
        3,
        CachedChainType::new(TypeId::NUMBER, false),
    );

    assert_eq!(
        cache.optional_chain_cache.borrow().get(&property_key, 3),
        Some(CachedChainType::new(TypeId::BOOLEAN, false))
    );
    assert_eq!(
        cache.optional_chain_cache.borrow().get(&property_key, 4),
        None
    );
    assert_eq!(
        cache
            .optional_property_chain_cache
            .borrow()
            .get(&chain_key, 3),
        Some(CachedChainType::new(TypeId::NUMBER, false))
    );
    assert_eq!(
        cache
            .optional_property_chain_cache
            .borrow()
            .get(&chain_key, 4),
        None
    );
}

#[test]
fn exclusion_frame_clears_request_fuel_on_outer_drop() {
    let cache = NarrowingCache::new();
    cache.set_narrow_excluding_budget(3);

    {
        let _frame = cache.enter_exclusion_frame();
        assert_eq!(cache.narrow_excluding_depth.get(), 1);
        assert_eq!(cache.narrow_excluding_fuel.get(), 3);
        assert!(cache.charge_exclusion_work());
        assert_eq!(cache.narrow_excluding_fuel.get(), 2);
    }

    assert_eq!(cache.narrow_excluding_depth.get(), 0);
    assert_eq!(cache.narrow_excluding_fuel.get(), 0);
}

#[test]
fn nested_exclusion_frames_share_fuel_until_outer_drop() {
    let cache = NarrowingCache::new();
    cache.set_narrow_excluding_budget(5);

    {
        let _outer = cache.enter_exclusion_frame();
        assert!(cache.charge_exclusion_work());
        assert_eq!(cache.narrow_excluding_fuel.get(), 4);

        {
            let _inner = cache.enter_exclusion_frame();
            assert_eq!(cache.narrow_excluding_depth.get(), 2);
            assert_eq!(cache.narrow_excluding_fuel.get(), 4);
            assert!(cache.charge_exclusion_work());
            assert_eq!(cache.narrow_excluding_fuel.get(), 3);
        }

        assert_eq!(cache.narrow_excluding_depth.get(), 1);
        assert_eq!(cache.narrow_excluding_fuel.get(), 3);
    }

    assert_eq!(cache.narrow_excluding_depth.get(), 0);
    assert_eq!(cache.narrow_excluding_fuel.get(), 0);
}

#[test]
fn resolve_visit_guard_releases_key_on_drop() {
    let cache = NarrowingCache::new();

    {
        let guard = cache
            .resolve_visit_guard(TypeId::STRING)
            .expect("first visit should enter");
        assert!(cache.resolve_visiting.borrow().contains(&TypeId::STRING));
        assert!(cache.resolve_visit_guard(TypeId::STRING).is_none());
        drop(guard);
    }

    assert!(!cache.resolve_visiting.borrow().contains(&TypeId::STRING));
    assert!(cache.resolve_visit_guard(TypeId::STRING).is_some());
}

#[test]
fn narrow_excluding_visit_guard_releases_key_on_drop() {
    let cache = NarrowingCache::new();
    let key = NarrowExcludingKey {
        source: TypeId::STRING,
        excluded: TypeId::NUMBER,
        resolver_generation: 7,
    };

    {
        let guard = cache
            .narrow_excluding_visit_guard(key)
            .expect("first visit should enter");
        assert!(cache.narrow_excluding_visiting.borrow().contains(&key));
        assert!(cache.narrow_excluding_visit_guard(key).is_none());
        drop(guard);
    }

    assert!(!cache.narrow_excluding_visiting.borrow().contains(&key));
    assert!(cache.narrow_excluding_visit_guard(key).is_some());
}
