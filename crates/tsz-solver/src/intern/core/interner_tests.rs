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

#[test]
fn all_type_parameter_intersections_preserve_ordered_members() {
    let interner = TypeInterner::new();
    let alpha = interner.type_param(TypeParamInfo::simple(interner.intern_string("Alpha")));
    let beta = interner.type_param(TypeParamInfo::simple(interner.intern_string("Beta")));
    let gamma = interner.type_param(TypeParamInfo::simple(interner.intern_string("Gamma")));

    let result = interner.intersection(vec![alpha, beta, gamma, beta]);
    let Some(TypeData::Intersection(list_id)) = interner.lookup(result) else {
        panic!("expected all-type-parameter intersection, got {result:?}");
    };

    assert_eq!(&*interner.type_list(list_id), &[alpha, beta, gamma]);
}

#[test]
fn same_name_type_parameter_intersection_collapses_to_constrained_member() {
    let interner = TypeInterner::new();
    let name = interner.intern_string("T");
    let unconstrained = interner.type_param(TypeParamInfo::simple(name));
    let constrained = interner.type_param(TypeParamInfo {
        name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    assert_eq!(
        interner.intersection(vec![unconstrained, constrained]),
        constrained
    );
}

#[test]
fn same_name_type_parameter_replacement_dedups_non_adjacent_members() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let unconstrained_t = interner.type_param(TypeParamInfo::simple(t_name));
    let constrained_t = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let u = interner.type_param(TypeParamInfo::simple(u_name));

    let result = interner.intersection(vec![unconstrained_t, u, constrained_t]);
    let Some(TypeData::Intersection(list_id)) = interner.lookup(result) else {
        panic!("expected all-type-parameter intersection, got {result:?}");
    };

    assert_eq!(&*interner.type_list(list_id), &[constrained_t, u]);
}
