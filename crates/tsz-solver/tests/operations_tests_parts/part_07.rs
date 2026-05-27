/// Test that type parameters inside intersection parameter types are inferred correctly.
///
/// Reproduces the bug from intersectionTypeInference1.ts:
///   <OwnProps>(f: (p: {dispatch: number} & `OwnProps`) => void) => (o: `OwnProps`) => `OwnProps`
/// Called with (props: {store: string}) => void should infer `OwnProps` = {store: string}.
#[test]
fn test_call_generic_intersection_param_inference() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Type parameter OwnProps (unconstrained)
    let own_props_param = TypeParamInfo {
        name: interner.intern_string("OwnProps"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let own_props_type = interner.intern(TypeData::TypeParameter(own_props_param));

    // {dispatch: number}
    let dispatch_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("dispatch"),
        TypeId::NUMBER,
    )]);

    // {dispatch: number} & OwnProps
    let intersection_param = interner.intersection(vec![dispatch_obj, own_props_type]);

    // (p: {dispatch: number} & OwnProps) => void
    let inner_fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("p")),
            type_id: intersection_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Generic function: <OwnProps>(f: inner_fn_type) => OwnProps
    let generic_func = interner.function(FunctionShape {
        type_params: vec![own_props_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: inner_fn_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: own_props_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Argument: (props: {store: string}) => void
    let store_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("store"),
        TypeId::STRING,
    )]);
    let arg_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("props")),
            type_id: store_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call generic_func(arg_fn) — should succeed (OwnProps inferred as {store: string})
    let result = evaluator.resolve_call(generic_func, &[arg_fn]);
    match result {
        CallResult::Success(_ret) => {
            // OwnProps should be inferred as {store: string}, and the call should succeed
        }
        other => panic!(
            "Expected success for intersection param inference, got {other:?}. \
             OwnProps should be inferred from the intersection decomposition."
        ),
    }
}

/// Tests that the trivial single-type-param fast path preserves literal types
/// when a contextual return type contains those literals.
///
/// Reproduces: `let v: 'A' | 'B' = identity('A')` where `identity<T>(x: T): T`.
/// Without the fix, `T` is inferred as `string` (widened from `"A"`), causing
/// a spurious TS2322. With the fix, the contextual type `'A' | 'B'` prevents
/// widening, keeping `T = "A"`.
#[test]
fn test_trivial_identity_preserves_literal_with_contextual_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // type DooDad = 'SOMETHING' | 'ELSE'
    let lit_something = interner.literal_string("SOMETHING");
    let lit_else = interner.literal_string("ELSE");
    let doodad = interner.union(vec![lit_something, lit_else]);

    // declare function identity<T>(x: T): T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let identity = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call identity("ELSE") with contextual type DooDad
    evaluator.set_contextual_type(Some(doodad));
    let result = evaluator.resolve_call(identity, &[lit_else]);

    match result {
        CallResult::Success(ret) => {
            // The return type should be "ELSE" (the literal), not string.
            // With the contextual type DooDad, the solver should preserve the
            // literal instead of widening to string.
            assert_ne!(
                ret,
                TypeId::STRING,
                "identity('ELSE') with contextual DooDad should NOT widen to string"
            );
            assert_eq!(
                ret, lit_else,
                "identity('ELSE') with contextual DooDad should return literal \"ELSE\""
            );
        }
        other => panic!("Expected success for identity call with contextual type, got {other:?}"),
    }
}

/// Tests that without a contextual type, the identity fast path still preserves
/// scalar literal arguments: `identity('ELSE')` should infer T = "ELSE".
#[test]
fn test_trivial_identity_preserves_unconstrained_literal_without_contextual_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let lit_else = interner.literal_string("ELSE");
    let result = infer_generic_function(
        &interner,
        &mut checker,
        &make_identity_shape(&interner, "T", "x"),
        &[lit_else],
    );
    assert_eq!(result, lit_else);
}

/// Test that a union of a single-overload function and a multi-overload callable
/// remains callable when the actual receiver satisfies the single signature and
/// one overload's `this` type.
///
/// Corresponds to tsc behavior for:
///   type F1 = (this: A) => void;
///   interface F4 { (this: C): void; (this: D): void; }
///   declare var x: A & C & { f: F1 | F4 };
///   `x.f()`;  // OK: selected overload has this A & C
#[test]
fn test_union_call_mixed_overloads_intersects_this_types_callable() {
    let interner = TypeInterner::new();

    // Create distinct `this` types: A = { a: string }, C = { c: string }, D = { d: number }
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);
    let type_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    // F1 = (this: A) => void — single overload
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // F4 = { (this: C): void; (this: D): void; } — multi-overload
    let f4 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_d),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union = F1 | F4
    let union_type = interner.union(vec![f1, f4]);

    // Create a `this` context that satisfies A (from F1's this type)
    // so Phase 0 passes and we reach the 1-multi compatibility check.
    let actual_this = interner.intersection(vec![type_a, type_c]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    evaluator.set_actual_this_type(Some(actual_this));
    let result = evaluator.resolve_call(union_type, &[]);

    assert!(
        matches!(result, CallResult::Success(_)),
        "Union of single-overload (this: A) and multi-overload (this: C / this: D) \
         should be callable when actual `this` satisfies A & C. Got: {result:?}"
    );
}

/// Test that a union of a single-overload function and a multi-overload callable
/// IS callable when the single-overload member's `this` type matches one of the
/// multi-overload member's `this` types.
///
/// Corresponds to tsc behavior for:
///   type F1 = (this: A) => void;
///   interface F3 { (this: A): void; (this: B): void; }
///   type Union = F1 | F3;  // callable (F1 matches F3's first overload)
#[test]
fn test_union_call_mixed_overloads_compatible_this_callable() {
    let interner = TypeInterner::new();

    // Create `this` types using SAME TypeId for A
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    // F1 = (this: A) => void — single overload
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // F3 = { (this: A): void; (this: B): void; } — multi-overload, sharing `this: A`
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_a), // Same TypeId as F1's this
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union = F1 | F3
    let union_type = interner.union(vec![f1, f3]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    // Set actual_this to a type that satisfies A (the shared this type)
    evaluator.set_actual_this_type(Some(type_a));
    let result = evaluator.resolve_call(union_type, &[]);

    assert!(
        matches!(result, CallResult::Success(_)),
        "Union of single-overload (this: A) and multi-overload (this: A / this: B) \
         should be callable when `this` types match. Got: {result:?}"
    );
}

/// Test that multi-overload union call merging compares `this` types by
/// semantic identity, not raw TypeId. Checker lowering can produce distinct
/// `TypeIds` for the same source alias across interface call signatures; tsc
/// still treats those overloads as merge-compatible.
#[test]
fn test_union_call_multi_overloads_structurally_identical_this_callable() {
    let interner = TypeInterner::new();

    let prop_a = interner.intern_string("a");
    let type_a = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(prop_a, TypeId::STRING)],
        ObjectFlags::empty(),
        Some(tsz_binder::SymbolId(1)),
    );
    let type_a_shadow = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(prop_a, TypeId::STRING)],
        ObjectFlags::empty(),
        Some(tsz_binder::SymbolId(2)),
    );
    assert_ne!(
        type_a, type_a_shadow,
        "test setup needs distinct TypeIds for structurally identical `this` types"
    );

    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);

    let left = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_a_shadow),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    let right = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_a),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    let union_type = interner.union(vec![left, right]);
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    evaluator.set_actual_this_type(Some(type_a));
    let result = evaluator.resolve_call(union_type, &[]);

    assert!(
        matches!(result, CallResult::Success(_)),
        "Union call should merge signatures whose `this` types are semantically \
         identical even when their TypeIds differ. Got: {result:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// resolve_union_new — combined construct-signature tests
// These lock the Phase 1/2/3 algorithm used by resolve_union_new.
// ──────────────────────────────────────────────────────────────────────────────

/// Helper: build a one-construct-signature Callable type.
#[cfg(test)]
fn make_construct_callable(
    interner: &TypeInterner,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    interner.callable(CallableShape {
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }],
        ..Default::default()
    })
}

#[test]
fn test_union_new_different_param_types_rejects_any_arg() {
    // { new(a: number): number } | { new(a: string): Date }
    // Combined param type = number & string = never → every arg fails.
    let interner = TypeInterner::new();
    let num_param = ParamInfo {
        name: None,
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };
    let str_param = ParamInfo {
        name: None,
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let m1 = make_construct_callable(&interner, vec![num_param], TypeId::NUMBER);
    let m2 = make_construct_callable(&interner, vec![str_param], TypeId::STRING);
    let union_type = interner.union(vec![m1, m2]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Passing `10` (number) should fail with AtM { expected: never }
    let result = evaluator.resolve_new(union_type, &[TypeId::NUMBER]);
    assert!(
        matches!(
            result,
            CallResult::ArgumentTypeMismatch {
                index: 0,
                expected,
                ..
            } if expected == TypeId::NEVER
        ),
        "union new with incompatible param types should report AtM(never). Got: {result:?}"
    );

    // Passing `"hello"` (string) should also fail with AtM { expected: never }
    let result2 = evaluator.resolve_new(union_type, &[TypeId::STRING]);
    assert!(
        matches!(
            result2,
            CallResult::ArgumentTypeMismatch {
                index: 0,
                expected,
                ..
            } if expected == TypeId::NEVER
        ),
        "union new with incompatible param types should report AtM(never). Got: {result2:?}"
    );
}

#[test]
fn test_union_new_different_param_counts_requires_max_args() {
    // { new(a: string): string } | { new(a: string, b: number): number }
    // Combined min_required = 2 (max of 1 and 2).
    let interner = TypeInterner::new();
    let str_param = || ParamInfo {
        name: None,
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let num_param = ParamInfo {
        name: None,
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };
    let m1 = make_construct_callable(&interner, vec![str_param()], TypeId::STRING);
    let m2 = make_construct_callable(&interner, vec![str_param(), num_param], TypeId::NUMBER);
    let union_type = interner.union(vec![m1, m2]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // 0 args → arity error: expected_min = 2
    let result = evaluator.resolve_new(union_type, &[]);
    assert!(
        matches!(
            result,
            CallResult::ArgumentCountMismatch {
                expected_min: 2,
                expected_max: Some(2),
                actual: 0
            }
        ),
        "0 args should require expected_min=2. Got: {result:?}"
    );

    // 1 arg → arity error: expected_min = 2
    let result = evaluator.resolve_new(union_type, &[TypeId::STRING]);
    assert!(
        matches!(
            result,
            CallResult::ArgumentCountMismatch {
                expected_min: 2,
                ..
            }
        ),
        "1 arg should still fail (min=2). Got: {result:?}"
    );

    // 2 args → success
    let result = evaluator.resolve_new(union_type, &[TypeId::STRING, TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "2 args should succeed. Got: {result:?}"
    );
}

#[test]
fn test_union_new_same_return_types_correct_union() {
    // { new(a: number): string } | { new(a: number): number }
    // Combined: param = number, return = string | number.
    let interner = TypeInterner::new();
    let num_param = || ParamInfo {
        name: None,
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };
    let m1 = make_construct_callable(&interner, vec![num_param()], TypeId::STRING);
    let m2 = make_construct_callable(&interner, vec![num_param()], TypeId::NUMBER);
    let union_type = interner.union(vec![m1, m2]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let result = evaluator.resolve_new(union_type, &[TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "compatible construct sigs should succeed. Got: {result:?}"
    );

    // Wrong arg type → AtM at index 0
    let result = evaluator.resolve_new(union_type, &[TypeId::STRING]);
    assert!(
        matches!(result, CallResult::ArgumentTypeMismatch { index: 0, .. }),
        "wrong arg type should give AtM at index 0. Got: {result:?}"
    );
}

#[test]
fn test_union_new_all_fail_requires_all_member_success() {
    // { new(a: number): number } | { new(a: number): Date; new(a: string): boolean }
    // Member 2 has multiple construct sigs → combined = None → strict per-member.
    // If member 1 fails (string arg), whole union fails.
    let interner = TypeInterner::new();
    let num_param = || ParamInfo {
        name: None,
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };
    let str_param = ParamInfo {
        name: None,
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let m1 = make_construct_callable(&interner, vec![num_param()], TypeId::NUMBER);

    // member2 has TWO construct signatures
    let m2 = interner.callable(CallableShape {
        construct_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![num_param()],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![str_param],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    let union_type = interner.union(vec![m1, m2]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // `10` (number) → member1 succeeds, member2 succeeds (first overload) → union succeeds
    let result = evaluator.resolve_new(union_type, &[TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "number arg where both members can construct should succeed. Got: {result:?}"
    );

    // `"hello"` (string) → member1 fails, member2 succeeds (second overload)
    // Strict semantics: member1 fails → whole union fails.
    let result = evaluator.resolve_new(union_type, &[TypeId::STRING]);
    assert!(
        !matches!(result, CallResult::Success(_)),
        "string arg where member1 fails should fail the union. Got: {result:?}"
    );
}
