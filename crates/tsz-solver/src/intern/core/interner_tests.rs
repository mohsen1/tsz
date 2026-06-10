use super::*;
use crate::caches::db::TypeDatabase;

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
fn poisoned_interner_keeps_existing_types_readable_and_rejects_new_ones() {
    let interner = TypeInterner::new();

    // Intern a couple of structurally distinct types before the limit hits.
    let pre_limit_union = TypeDatabase::union2(&interner, TypeId::STRING, TypeId::NUMBER);
    let pre_limit_data = interner
        .lookup(pre_limit_union)
        .expect("freshly interned type must be readable");

    // Simulate crossing the type-count limit.
    interner
        .alloc_counter
        .store((MAX_INTERNED_TYPES + 1) as u32, Ordering::Relaxed);
    assert_eq!(interner.poison_due_to_interned_type_limit(), TypeId::ERROR);
    assert!(interner.poisoned.load(Ordering::Relaxed));

    // Already-interned ids stay readable: graceful degradation must not
    // collapse previously computed program types (or the shared cross-file
    // caches holding their ids) into opaque misses.
    assert_eq!(interner.lookup(pre_limit_union), Some(pre_limit_data));

    // Re-interning an existing key resolves to the existing id.
    let reinterned = TypeDatabase::union2(&interner, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(reinterned, pre_limit_union);

    // Brand-new structural keys degrade to ERROR.
    let fresh = TypeDatabase::union2(&interner, TypeId::BOOLEAN, TypeId::NUMBER);
    assert_eq!(fresh, TypeId::ERROR);
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
fn boxed_def_id_registration_is_idempotent() {
    let interner = TypeInterner::new();
    let def_id = DefId(7);

    interner.register_boxed_def_id(IntrinsicKind::Function, def_id);
    interner.register_boxed_def_id(IntrinsicKind::Function, def_id);

    assert!(interner.is_boxed_def_id(def_id, IntrinsicKind::Function));
    assert_eq!(
        interner
            .boxed_def_ids
            .get(&IntrinsicKind::Function)
            .map(|entry| entry.len()),
        Some(1)
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
