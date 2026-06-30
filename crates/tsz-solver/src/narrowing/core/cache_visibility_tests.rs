use super::*;
use crate::intern::TypeInterner;

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
    cache
        .optional_chain_cache
        .borrow_mut()
        .insert((TypeId::STRING, prop), TypeId::BOOLEAN);
    cache
        .optional_property_chain_cache
        .borrow_mut()
        .insert(chain_key, TypeId::BOOLEAN);
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
    assert_eq!(
        stats.narrowed_property_cache_entries,
        MAX_GENERATIONS_PER_NARROWING_KEY
    );
    assert_eq!(
        stats.required_property_cache_entries,
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
    assert_eq!(cache.narrow_type_cache.borrow().get(&request_key, 1), None);
    assert_eq!(
        cache.narrow_type_cache.borrow().get(&request_key, 7),
        Some(TypeId::STRING)
    );
}
