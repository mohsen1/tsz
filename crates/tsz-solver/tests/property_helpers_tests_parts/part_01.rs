#[test]
fn test_mutable_array_push_still_found() {
    // Regression: mutable T[] must keep push after the fix.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let mutable_num = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(mutable_num, "push");
    assert!(
        result.is_success(),
        "push should still exist on mutable number[]. Got: {result:?}"
    );
}

#[test]
fn test_readonly_tuple_push_not_found() {
    // readonly [T, U] tuples must also reject push via ReadonlyArray resolution.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.readonly_type(tuple);
    assert_property_not_found(&evaluator.resolve_property_access(readonly_tuple, "push"));
}

#[test]
fn test_readonly_tuple_length_accessible() {
    // Fixed-length tuples return a literal length even in readonly context.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.readonly_type(tuple);
    let expected_len = interner.literal_number(2.0);
    assert_property_success(
        &evaluator.resolve_property_access(readonly_tuple, "length"),
        expected_len,
    );
}

#[test]
fn test_non_array_readonly_type_transparent() {
    // ReadonlyType wrapping a non-array object must remain transparent —
    // its properties are still accessible unchanged.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let readonly_obj = interner.readonly_type(obj);

    assert_property_success(
        &evaluator.resolve_property_access(readonly_obj, "x"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_readonly_array_no_lib_falls_back_gracefully() {
    // When no ReadonlyArray lib type is registered, readonly array property
    // access falls back to transparent behaviour (no crash, no false errors).
    let interner = TypeInterner::new();
    // Intentionally do NOT call make_array_and_readonly_array_env — no lib.
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let readonly_num = interner.readonly_array(TypeId::NUMBER);
    // Without lib, the fallback path is used; result may succeed or not-found.
    // The important invariant is that it does not panic.
    let result = evaluator.resolve_property_access(readonly_num, "length");
    // length is handled specially even without lib
    assert!(
        result.is_success(),
        "length should succeed even without lib. Got: {result:?}"
    );
}

#[test]
fn test_readonly_array_no_lib_push_not_found() {
    // Even when no ReadonlyArray lib type is registered, readonly arrays must
    // not expose built-in Array mutators.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let readonly_num = interner.readonly_array(TypeId::NUMBER);
    assert_property_not_found(&evaluator.resolve_property_access(readonly_num, "push"));
}

// =============================================================================
// TypeParameter property access — constraint evaluation
// =============================================================================

/// `T extends { x: number; y: string }` — property access on a type parameter
/// with a direct concrete object constraint resolves via the constraint.
#[test]
fn test_type_param_with_object_constraint_property_found() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let constraint_obj = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);
    // Verify the rule is name-independent (T, K, U all behave the same).
    for name in ["T", "K", "U"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(constraint_obj),
            default: None,
            is_const: false,
        }));
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "x"),
            TypeId::NUMBER,
        );
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "y"),
            TypeId::STRING,
        );
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "z"));
    }
}

/// `T` with no constraint — any property access must return `PropertyNotFound`.
#[test]
fn test_type_param_no_constraint_property_not_found() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    for name in ["T", "P", "X"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
        }));
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "x"));
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "toString"));
    }
}

/// `T extends A | B` where both union members have the same property — the
/// solver evaluates the union constraint and finds the property on all members.
#[test]
fn test_type_param_with_union_constraint_shared_property_found() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x_atom = interner.intern_string("x");
    let a = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(x_atom, TypeId::STRING)]);
    let union_constraint = interner.union(vec![a, b]);

    for name in ["T", "K"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(union_constraint),
            default: None,
            is_const: false,
        }));
        // Property `x` exists on both union members — must resolve.
        let result = evaluator.resolve_property_access(type_param, "x");
        assert!(
            matches!(result, PropertyAccessResult::Success { .. }),
            "Expected Success for shared union property, got {result:?}"
        );
        // Property `z` exists on neither — must be PropertyNotFound.
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "z"));
    }
}

/// `T extends { x: number } & { y: string }` — intersection constraint must
/// expose properties from both sides.
#[test]
fn test_type_param_with_intersection_constraint_finds_both_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let a = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let intersection_constraint = interner.intersection(vec![a, b]);

    for name in ["T", "V"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(intersection_constraint),
            default: None,
            is_const: false,
        }));
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "x"),
            TypeId::NUMBER,
        );
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "y"),
            TypeId::STRING,
        );
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "z"));
    }
}

/// `T extends NoInfer<{ n: number }>` — the `NoInfer` wrapper is transparent
/// for property access; the solver evaluates through it to the inner type.
#[test]
fn test_type_param_noinfer_constraint_resolves_inner_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let n_atom = interner.intern_string("n");
    let obj = interner.object(vec![PropertyInfo::new(n_atom, TypeId::NUMBER)]);
    let no_infer = interner.intern(TypeData::NoInfer(obj));

    for name in ["T", "E", "Item"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(no_infer),
            default: None,
            is_const: false,
        }));
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "n"),
            TypeId::NUMBER,
        );
        assert_property_not_found(&evaluator.resolve_property_access(type_param, "z"));
    }
}

/// `T extends Readonly<{ value: number }>` — the solver evaluates the readonly
/// wrapper and finds the property on the inner object type.
#[test]
fn test_type_param_readonly_constraint_resolves_inner_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let value_atom = interner.intern_string("value");
    let obj = interner.object(vec![PropertyInfo::new(value_atom, TypeId::NUMBER)]);
    let readonly_obj = interner.readonly_type(obj);

    for name in ["T", "R"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(readonly_obj),
            default: None,
            is_const: false,
        }));
        assert_property_success(
            &evaluator.resolve_property_access(type_param, "value"),
            TypeId::NUMBER,
        );
    }
}

/// When a synthetic Application constraint cannot be evaluated (noop resolver,
/// unregistered base DefId), the evaluator must NOT fabricate a false Success.
/// `PropertyNotFound` or `Success{ANY}` are the only acceptable results.
#[test]
fn test_type_param_unresolvable_application_constraint_no_false_success() {
    use crate::def::DefId;

    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let synthetic_base = interner.lazy(DefId(99_001));
    let synthetic_app = interner.application(synthetic_base, vec![TypeId::NUMBER]);

    for name in ["T", "A"] {
        let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(synthetic_app),
            default: None,
            is_const: false,
        }));
        let result = evaluator.resolve_property_access(type_param, "value");
        let is_acceptable = matches!(
            result,
            PropertyAccessResult::PropertyNotFound { .. }
                | PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    ..
                }
        );
        assert!(
            is_acceptable,
            "Unresolvable Application constraint must not produce a wrong concrete type, got {result:?}"
        );
    }
}

/// Nested type parameters: `T extends U` where `U extends { x: number }`.
/// Property access on `T` must walk through both constraints.
#[test]
fn test_nested_type_param_constraint_property_found() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x_atom = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    // U extends { x: number }
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(obj),
        default: None,
        is_const: false,
    }));

    // T extends U
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(u_param),
        default: None,
        is_const: false,
    }));

    assert_property_success(
        &evaluator.resolve_property_access(t_param, "x"),
        TypeId::NUMBER,
    );
    assert_property_not_found(&evaluator.resolve_property_access(t_param, "y"));
}

// =============================================================================
// Deferred conditional type property access (issue #9734)
// Rule: when `T extends U ? A : B` is deferred (type params in check/extends),
// the apparent type for property access is `A | B`. Properties not on all
// branches must produce PropertyNotFound; common properties succeed.
// =============================================================================

#[test]
fn test_deferred_conditional_branch_only_property_not_found() {
    // T extends string ? { a: 1 } : { b: 2 }
    // Accessing `.a` → PropertyNotFound (not on { b: 2 } branch).
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let true_branch = interner.object(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);
    let false_branch = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    });

    // .a exists only on the true branch → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(cond, "a"));
    // .b exists only on the false branch → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(cond, "b"));
    // .zzz exists on neither branch → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(cond, "zzz"));
}

#[test]
fn test_deferred_conditional_common_property_succeeds() {
    // T extends string ? { common: number; a: 1 } : { common: string; b: 2 }
    // Accessing `.common` → Success (present on both branches, result is union).
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let common_name = interner.intern_string("common");
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let true_branch = interner.object(vec![
        PropertyInfo::new(common_name, TypeId::NUMBER),
        PropertyInfo::new(a_name, TypeId::NUMBER),
    ]);
    let false_branch = interner.object(vec![
        PropertyInfo::new(common_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    });

    // .common exists on both branches → Success
    let result = evaluator.resolve_property_access(cond, "common");
    assert!(
        matches!(result, PropertyAccessResult::Success { .. }),
        "Expected Success for common property, got {result:?}"
    );
    // .a is branch-only → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(cond, "a"));
}

#[test]
fn test_deferred_conditional_renamed_type_param_same_behavior() {
    // Structural invariant: renaming T to P must not change property access behavior.
    // P extends string ? { a: 1 } : { b: 2 }  (same shape as T version above)
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let p_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("P"), // renamed from T
        constraint: None,
        default: None,
        is_const: false,
    }));
    let true_branch = interner.object(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);
    let false_branch = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
    let cond = interner.conditional(ConditionalType {
        check_type: p_param,
        extends_type: TypeId::STRING,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    });

    // Same expectations regardless of type-param name
    assert_property_not_found(&evaluator.resolve_property_access(cond, "a"));
    assert_property_not_found(&evaluator.resolve_property_access(cond, "b"));
    assert_property_not_found(&evaluator.resolve_property_access(cond, "zzz"));
}

#[test]
fn test_deferred_conditional_with_any_branch_suppresses_error() {
    // T extends string ? any : { b: 2 }
    // One branch is `any` → apparent type is `any` → suppress TS2339.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let b_name = interner.intern_string("b");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let false_branch = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::ANY, // any branch → suppress errors
        false_type: false_branch,
        is_distributive: true,
    });

    // When a branch is `any`, property access must not report not-found
    let result = evaluator.resolve_property_access(cond, "nonexistent");
    assert!(
        matches!(
            result,
            PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                ..
            }
        ),
        "Expected Success(any) when branch is any, got {result:?}"
    );
}

#[test]
fn test_deferred_conditional_nested_produces_not_found() {
    // T extends string ? (U extends number ? { a: 1 } : { b: 2 }) : { c: 3 }
    // .a is not on the false branch { b: 2 } of inner conditional or { c: 3 }.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let inner_true = interner.object(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);
    let inner_false = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
    let outer_false = interner.object(vec![PropertyInfo::new(c_name, TypeId::NUMBER)]);

    // Inner deferred conditional: U extends number ? { a: 1 } : { b: 2 }
    let inner_cond = interner.conditional(ConditionalType {
        check_type: u_param,
        extends_type: TypeId::NUMBER,
        true_type: inner_true,
        false_type: inner_false,
        is_distributive: true,
    });
    // Outer deferred conditional: T extends string ? inner_cond : { c: 3 }
    let outer_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: outer_false,
        is_distributive: true,
    });

    // .a is not on { b: 2 } (inner false branch) or { c: 3 } → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(outer_cond, "a"));
    // .zzz is on none → PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(outer_cond, "zzz"));
}
