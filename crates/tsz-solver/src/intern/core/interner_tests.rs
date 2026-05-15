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
