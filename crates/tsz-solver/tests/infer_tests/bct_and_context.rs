use super::*;

// =============================================================================
// Best Common Type Tests
// =============================================================================

#[test]
fn test_best_common_type_single() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_union() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_best_common_type_dedup() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // Duplicate types should be deduped
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_best_common_type_reuses_subtype_cache() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let empty_stats = ctx.cache_statistics();
    assert_eq!(empty_stats.subtype_entries, 0);
    assert_eq!(empty_stats.estimated_size_bytes(), 0);

    let input = [TypeId::STRING, TypeId::NUMBER, TypeId::STRING];
    let _ = ctx.best_common_type(&input);
    let first_stats = ctx.cache_statistics();
    assert!(first_stats.subtype_entries > 0);
    assert!(
        first_stats.estimated_size_bytes() > empty_stats.estimated_size_bytes(),
        "populated inference subtype cache should report nonzero estimated residency"
    );

    let _ = ctx.best_common_type(&input);
    let second_stats = ctx.cache_statistics();
    assert_eq!(second_stats.subtype_entries, first_stats.subtype_entries);
    assert_eq!(
        second_stats.estimated_size_bytes(),
        first_stats.estimated_size_bytes()
    );
}

#[test]
fn test_best_common_type_empty() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[]);
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_best_common_type_never_ignored() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // never doesn't contribute to union
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_all_never() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NEVER, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// Full Inference Scenario Tests
// =============================================================================

#[test]
fn test_resolve_all_with_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Simulate: function foo<T, U>(a: T, b: U) called with foo("hello", 42)
    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, forty_two);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}

#[test]
fn test_resolve_all_with_circular_extends_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Simulate: <T extends U, U extends T>
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
}

// =============================================================================
// Additional Circular Generic Constraint Tests
// =============================================================================

#[test]
fn test_circular_extends_three_way_cycle() {
    // Test: <T extends U, U extends V, V extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All three resolve to unknown due to circular dependency with no concrete bounds
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_self_reference() {
    // Test: <T extends T> - self-referential constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends T (self-reference)
    ctx.add_upper_bound(var_t, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // Self-reference with no other bounds resolves to unknown
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_with_lower_bound() {
    // Test: <T extends U, U extends T> with T having a lower bound of string
    // Lower bounds propagate through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Add a lower bound to T
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to string (its lower bound)
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U also resolves to string - lower bounds propagate through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_both_have_lower_bounds() {
    // Test: <T extends U, U extends T> with both having the same lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have the same lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Both resolve to string
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_unify_propagates() {
    // Test: <T extends U, U extends T> then unify T with number
    // Unification propagates through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Unify T directly with number
    ctx.unify_var_type(var_t, TypeId::NUMBER).unwrap();

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to number (unified)
    assert_eq!(results[0], (t_name, TypeId::NUMBER));
    // U also resolves to number - unification propagates through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}

#[test]
fn test_circular_extends_conflicting_lower_bounds() {
    // Test: <T extends U, U extends T> with T: string and U: number
    // Cycle propagation causes both to get union of all lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Conflicting lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // After SCC unification, both lower bounds are merged into a union.
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(results[0], (t_name, expected_union));
    assert_eq!(results[1], (u_name, expected_union));
}

#[test]
fn test_circular_extends_three_way_with_one_lower_bound() {
    // Test: <T extends U, U extends V, V extends T> with V having lower bound
    // Bounds propagate through adjacent connections in the cycle
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    // Only V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to boolean (its direct lower bound)
    assert_eq!(results[2], (v_name, TypeId::BOOLEAN));
    // U extends V, so U gets boolean through propagation
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
    // T extends U, and with fixed-point propagation T also gets boolean
    // (previous impl limitation: propagation stopped at one level, T was UNKNOWN)
    assert_eq!(results[0], (t_name, TypeId::BOOLEAN));
}

#[test]
fn test_circular_extends_with_union_lower_bound() {
    // Test: <T extends U, U extends T> with T having union type as lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has a union type as lower bound
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to the union type
    assert_eq!(results[0], (t_name, union_type));
    // U also resolves to the union through propagation
    assert_eq!(results[1], (u_name, union_type));
}

#[test]
fn test_circular_extends_with_literal_types() {
    // Test: <T extends U, U extends T> with literal type lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have literal string lower bounds
    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, world);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Both T and U get string (widened from union of "hello" | "world")
    // In a cycle with conflicting literals, both unify to the common primitive type
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_four_way_cycle() {
    // Test: <T extends U, U extends V, V extends W, W extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");
    let w_name = interner.intern_string("W");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);
    let var_w = ctx.fresh_type_param(w_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let w_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: w_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends W, W extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, w_type);
    ctx.add_upper_bound(var_w, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All four resolve to unknown with no lower bounds
    assert_eq!(results.len(), 4);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
    assert_eq!(results[3], (w_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_with_concrete_upper_and_lower() {
    // Test: <T extends U, U extends T> with T having both upper and lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has both upper bound (string) and lower bound (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to its lower bound (hello literal)
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U gets hello through propagation
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_chain_with_endpoint_bound() {
    // Test: <T extends U, U extends V> (not circular) with V having lower bound
    // Chain propagation: upper bounds become resolved types when no lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V (chain, not cycle)
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);

    // V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to number (its lower bound)
    assert_eq!(results[2], (v_name, TypeId::NUMBER));
    // U has upper bound V but no lower bound, so resolves to its upper bound (V type param)
    assert_eq!(results[1].0, u_name);
    assert!(matches!(
        interner.lookup(results[1].1),
        Some(TypeData::TypeParameter(_))
    ));
    // T has upper bound U but no lower bound, resolves to its upper bound (U type param)
    assert_eq!(results[0].0, t_name);
    assert!(matches!(
        interner.lookup(results[0].1),
        Some(TypeData::TypeParameter(_))
    ));
}

#[test]
fn test_circular_extends_multiple_lower_bounds_same_param() {
    // Test: <T extends U, U extends T> with T having multiple lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has multiple lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Multiple lower bounds are unioned: string | number | boolean
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(results[0], (t_name, expected_union));
    // U gets the same result through SCC propagation
    assert_eq!(results[1], (u_name, expected_union));
}

// =============================================================================
// Context-Sensitive Typing Tests
// =============================================================================

#[test]
fn test_context_sensitive_callback_param_from_upper_bound() {
    // Test: When a callback parameter has an upper bound from context,
    // the parameter type is inferred from that context.
    // e.g., arr.map((x) => x + 1) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context provides: T must be a subtype of number (from array element type)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound and no lower bound, resolves to the upper bound
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_return_type_from_usage() {
    // Test: Return type inference from how the result is used
    // e.g., const x: string = identity(value) infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Usage site provides: result is assigned to string variable
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Call site provides: argument is a string literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound wins (more specific)
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_context_sensitive_multiple_usage_sites() {
    // Test: Multiple usage sites provide constraints that must be unified
    // e.g., function used in two places with different argument types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // First usage: called with string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Second usage: called with number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_context_sensitive_literal_widening_prevented() {
    // Test: Fresh literals are always widened during inference resolution.
    // With upper bound STRING, widened literal satisfies constraint.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    // Lower bound is the literal
    ctx.add_lower_bound(var_t, hello);
    // Upper bound is the base type (widened literal satisfies this)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Fresh literal is widened to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_context_sensitive_object_property_inference() {
    // Test: Object property types inferred from contextual type
    // e.g., const obj: {x: number} = {x: getValue()}
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects number for property x
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Value provides a specific number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal number from lower bound
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_array_element_inference() {
    // Test: Array element types inferred from array context
    // e.g., const arr: string[] = [getValue()]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context: array of strings means elements must be strings
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_context_sensitive_conditional_branch_types() {
    // Test: Type from conditional branches — both branches contribute lower bounds
    // e.g., condition ? stringValue : numberValue
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // True branch contributes string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // False branch contributes number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions both branch types: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_context_sensitive_function_param_from_callback_context() {
    // Test: Function parameter type inferred from callback signature context
    // e.g., arr.filter((x) => x > 0) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Contextual callback type says param must be number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // No explicit annotation, so no lower bound from declaration

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Infers from context
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_rest_param_inference() {
    // Test: Rest parameter type inference from spread arguments
    // e.g., fn(...args: T) called with (1, 2, 3)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple arguments of same type contribute to rest param type
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    ctx.add_lower_bound(var_t, one);
    ctx.add_lower_bound(var_t, two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple number literals widen to number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_default_param_inference() {
    // Test: Default parameter provides lower bound for type param
    // e.g., function fn<T>(x: T = "default")
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Default value is a string literal
    let default_val = interner.literal_string("default");
    ctx.add_lower_bound(var_t, default_val);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Callback Parameter Inference Tests
// =============================================================================

#[test]
fn test_callback_param_inferred_from_array_map() {
    // Test: arr.map((x) => x.toUpperCase()) where arr: string[]
    // The callback parameter x should be inferred as string from array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array<string>.map provides callback with (element: string) => U
    // So T (the callback param type) has upper bound string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Callback param inferred from array element type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callback_param_inferred_with_index() {
    // Test: arr.forEach((item, index) => ...) where arr: number[]
    // First param is number (element), second is number (index)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T is the element type from Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // U is the index type (always number)
    ctx.add_upper_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_callback_param_inferred_from_generic_higher_order() {
    // Test: Generic higher-order function like filter<T>(arr: T[], pred: (x: T) => boolean)
    // When called with string[], T is inferred as string, so callback param is string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bound from argument: array contains strings
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound from callback usage: predicate receives T
    // (callback param type flows from T)

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // T inferred as string, so callback param is string
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Generic Default Type Inference Tests
// =============================================================================

#[test]
fn test_generic_default_used_when_no_inference() {
    // Test: <T = string> with no inference constraints, T defaults to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No constraints added - should use default if available
    // Note: defaults are typically handled during type param registration,
    // but here we test the inference context behavior with no constraints
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Without any constraints, resolves to unknown
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_generic_default_overridden_by_lower_bound() {
    // Test: <T = string> with lower bound number, inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Inferred lower bound takes precedence
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound overrides any potential default
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_default_with_constraint() {
    // Test: <T extends object = {}> - constraint with default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound, resolves to the upper bound
    assert_eq!(result, TypeId::OBJECT);
}

#[test]
fn test_generic_default_with_literal_inference() {
    // Test: <T = string> called with literal "hello", infers literal not default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Inferred literal takes precedence over default
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_multiple_params_with_defaults() {
    // Test: <T = string, U = number> with only U having lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let _var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Only U has a lower bound
    ctx.add_lower_bound(var_u, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T has no constraints, resolves to unknown
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    // U has lower bound, resolves to boolean
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
}

// =============================================================================
// Generic Constraint Propagation Tests
// =============================================================================

#[test]
fn test_constraint_propagation_upper_to_lower() {
    // Test: Upper bound on one param propagates to lower bound check
    // <T extends string> called with T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Lower bound from argument: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound satisfies upper bound, resolves to literal
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_through_unification() {
    // Test: Unifying two vars propagates constraints from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T has lower bound string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has lower bound number
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    // Unify T and U
    ctx.unify_vars(var_t, var_u).unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // After unification, both lower bounds are merged into a union
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_transitive_upper_bounds() {
    // Test: T extends string with lower bound "hello"
    // Lower bound must satisfy upper bound constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Add lower bound to T (literal satisfies string upper bound)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();

    // T resolves to its lower bound (literal "hello")
    assert_eq!(result_t, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_multiple_upper_bounds() {
    // Test: T extends A & B (multiple upper bounds create intersection)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple upper bounds
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple upper bounds create intersection
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_lower_bounds_union() {
    // Test: Multiple lower bounds produce a union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple lower bounds from different call sites
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple lower bounds: string | number | boolean
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_with_never_lower_bound() {
    // Test: never as lower bound doesn't contribute to union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bounds including never
    ctx.add_lower_bound(var_t, TypeId::NEVER);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // never is filtered out, only string remains
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_any_lower_with_concrete() {
    // Test: any as lower bound with concrete type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bounds: any and string
    ctx.add_lower_bound(var_t, TypeId::ANY);
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound constrains
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With upper bound, any is filtered from lower bounds
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_object_properties() {
    // Test: Object type constraint propagation
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Create object type with property
    let prop_name = interner.intern_string("x");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::NUMBER)]);

    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}
