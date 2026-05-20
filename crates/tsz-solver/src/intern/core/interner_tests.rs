use super::*;

#[test]
fn interned_type_limit_fallback_poison_returns_error() {
    let interner = TypeInterner::new();
    interner
        .alloc_counter
        .store((MAX_INTERNED_TYPES + 1) as u32, Ordering::Relaxed);

    assert!(interner.interned_type_limit_exceeded());
    assert_eq!(
        interner.interned_type_limit_context(),
        InternedTypeLimitContext {
            current_count: MAX_INTERNED_TYPES + 1,
            max_interned_types: MAX_INTERNED_TYPES,
            fallback_type: TypeId::ERROR,
        }
    );
    assert_eq!(interner.poison_due_to_interned_type_limit(), TypeId::ERROR);
    assert!(interner.poisoned.load(Ordering::Relaxed));
}

#[test]
fn interned_type_limit_boundary_is_strictly_greater_than_limit() {
    assert!(!TypeInterner::interned_type_limit_exceeded_for_count(
        MAX_INTERNED_TYPES
    ));
    assert!(TypeInterner::interned_type_limit_exceeded_for_count(
        MAX_INTERNED_TYPES + 1
    ));
}

#[test]
fn estimated_size_accounts_for_retained_predicate_caches() {
    let interner = TypeInterner::new();
    let before = interner.estimated_size_bytes();

    interner.contains_this_cache.insert(TypeId::NUMBER, true);
    interner.contains_infer_cache.insert(TypeId::STRING, false);
    interner
        .contains_type_query_cache
        .insert(TypeId::BOOLEAN, true);

    assert!(
        interner.estimated_size_bytes() > before,
        "retained TypeInterner predicate cache entries must be visible to residency estimates",
    );
}
