/// `x instanceof RHS` where `RHS` has `[Symbol.hasInstance](v: unknown): value is STRING`
/// narrows by `STRING` and ignores the construct signature return type (`NUMBER`).
#[test]
fn test_narrow_by_instanceof_uses_symbol_has_instance_predicate() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let constructor = make_constructor_with_has_instance(
        &interner,
        Some(TypeId::NUMBER), // construct sig says new (): NUMBER
        Some(TypeId::STRING), // hasInstance says value is STRING
        false,
        "value",
    );

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let narrowed = ctx.narrow_by_instanceof(source, constructor, true);

    assert_eq!(
        narrowed,
        TypeId::STRING,
        "narrow_by_instanceof must use the [Symbol.hasInstance] predicate target \
         (STRING) instead of the construct signature return (NUMBER)"
    );
}

/// The structural rule is parameter-name-independent: renaming `value` to `x`
/// must not change the narrowed result. Locks in §25 of `CLAUDE.md` (no
/// hardcoded user-chosen names).
#[test]
fn test_narrow_by_instanceof_has_instance_independent_of_param_name() {
    for param_name in ["value", "x", "v"] {
        let interner = TypeInterner::new();
        let ctx = NarrowingContext::new(&interner);

        let constructor = make_constructor_with_has_instance(
            &interner,
            None,
            Some(TypeId::NUMBER),
            false,
            param_name,
        );

        let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed = ctx.narrow_by_instanceof(source, constructor, true);

        assert_eq!(
            narrowed,
            TypeId::NUMBER,
            "predicate narrowing must not depend on parameter name (got param={param_name})"
        );
    }
}

/// `asserts value is T` predicates do NOT participate in instanceof narrowing
/// per tsc — only non-asserting predicates carry through.
#[test]
fn test_narrow_by_instanceof_ignores_asserts_has_instance_predicate() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let constructor = make_constructor_with_has_instance(
        &interner,
        Some(TypeId::NUMBER),
        Some(TypeId::STRING),
        true, // asserts
        "value",
    );

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed = ctx.narrow_by_instanceof(source, constructor, true);

    assert_eq!(
        narrowed,
        TypeId::NUMBER,
        "asserts-only predicate must NOT drive instanceof narrowing — \
         construct signature return must be used instead"
    );
}

/// When the constructor has no `[Symbol.hasInstance]` method, narrowing falls
/// back to the construct signature return type.
#[test]
fn test_narrow_by_instanceof_without_has_instance_uses_construct_return() {
    use crate::types::{CallSignature, CallableShape};

    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let constructor = interner.callable(CallableShape {
        construct_signatures: vec![CallSignature::new(vec![], TypeId::NUMBER)],
        ..CallableShape::default()
    });

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed = ctx.narrow_by_instanceof(source, constructor, true);

    assert_eq!(
        narrowed,
        TypeId::NUMBER,
        "Without Symbol.hasInstance, narrowing must use the construct signature return"
    );
}

/// Union of constructors where EVERY member has `[Symbol.hasInstance]` —
/// `instance_type_from_symbol_has_instance` returns the union of predicate
/// targets, and narrowing must filter by that union.
///
/// Uses primitive predicate targets (STRING / NUMBER) so the assertion is
/// unaffected by interface-overlap intersection fallbacks.
#[test]
fn test_narrow_by_instanceof_union_constructor_both_have_has_instance() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Member A: [Symbol.hasInstance]: value is STRING.
    let a_constructor =
        make_constructor_with_has_instance(&interner, None, Some(TypeId::STRING), false, "value");

    // Member B: [Symbol.hasInstance]: value is NUMBER. Renamed param ("v")
    // ensures the rule isn't keyed on parameter name across union members.
    let b_constructor =
        make_constructor_with_has_instance(&interner, None, Some(TypeId::NUMBER), false, "v");

    let union_constructor = interner.union2(a_constructor, b_constructor);
    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let narrowed = ctx.narrow_by_instanceof(source, union_constructor, true);

    // Predicate union STRING | NUMBER, applied to STRING | NUMBER | BOOLEAN.
    let expected_union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(
        narrowed, expected_union,
        "Union constructor where both members carry Symbol.hasInstance must \
         narrow by the union of predicate targets"
    );
}

/// When the `[Symbol.hasInstance]` predicate target erases to `any` (e.g., the
/// predicate is generic and its type parameter collapses), tsc's
/// `getInstanceType` falls back to the erased generic construct return rather
/// than letting `any` widen the source. This test pins that precedence at the
/// narrowing layer so the solver entry point can't diverge from
/// `instance_type_from_constructor` (see #8670 review feedback).
#[test]
fn test_narrow_by_instanceof_collapsed_any_predicate_falls_back_to_generic_construct() {
    use crate::def::DefId;
    use crate::types::{
        CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo,
        TypePredicate, TypePredicateTarget,
    };

    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let value_atom = interner.intern_string("value");
    let t_name = interner.intern_string("T");
    let t_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_type = interner.type_param(t_info);
    let box_base = interner.lazy(DefId(4242));
    let box_t = interner.application(box_base, vec![t_type]);
    let box_any = interner.application(box_base, vec![TypeId::ANY]);
    let has_instance_atom = interner.intern_string("[Symbol.hasInstance]");

    // hasInstance predicate collapses to `any`.
    let has_instance_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(value_atom, TypeId::UNKNOWN)],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(value_atom),
            type_id: Some(TypeId::ANY),
            parameter_index: Some(0),
        }),
        is_constructor: false,
        is_method: true,
    });

    // Constructor with both `any`-collapsing predicate AND a generic construct
    // signature returning Box<T>. The any-fallback rule should select Box<any>
    // (the erased generic construct return) rather than letting `any` widen.
    let constructor = interner.callable(CallableShape {
        construct_signatures: vec![CallSignature {
            type_params: vec![t_info],
            params: vec![],
            this_type: None,
            return_type: box_t,
            type_predicate: None,
            is_method: false,
            declaration_group: 0,
        }],
        properties: vec![PropertyInfo::method(has_instance_atom, has_instance_fn)],
        ..CallableShape::default()
    });

    let source = interner.union2(TypeId::STRING, box_any);
    let narrowed = ctx.narrow_by_instanceof(source, constructor, true);

    assert_eq!(
        narrowed, box_any,
        "Collapsed-any predicate must defer to the erased generic construct \
         return (Box<any>) rather than narrowing source by `any`"
    );
}

// =============================================================================
// Enum narrowing tests (narrow_to_type for enum sources)
// =============================================================================

#[test]
fn test_narrow_to_type_enum_preserves_nominal_wrapper() {
    // When v: E1 (enum) and we narrow to literal 1, the result should be Enum(E1_def, 1)
    // not raw literal 1. This preserves the nominal identity of the enum.
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let enum_def = crate::def::DefId(100);
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let inner_union = interner.union(vec![lit1, lit2]);

    // E1 = Enum(E1_def, 1 | 2)
    let e1 = interner.intern(crate::types::TypeData::Enum(enum_def, inner_union));

    // narrow_to_type(E1, 1) should yield Enum(E1_def, 1), not raw literal 1
    let narrowed = ctx.narrow_to_type(e1, lit1);
    let expected = interner.intern(crate::types::TypeData::Enum(enum_def, lit1));
    assert_eq!(
        narrowed, expected,
        "narrow_to_type(Enum(D,1|2), 1) should produce Enum(D,1), not raw 1"
    );

    // Verify that the result is NOT the raw literal (the regression we fixed)
    assert_ne!(
        narrowed, lit1,
        "narrow_to_type on an enum source must not drop the nominal wrapper"
    );
}

#[test]
fn test_narrow_to_type_enum_value_not_in_enum_returns_never() {
    // When v: E1 = {a=1,b=2} and we narrow to 3, result is NEVER (3 not in E1)
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let enum_def = crate::def::DefId(100);
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let lit3 = interner.literal_number(3.0);
    let inner_union = interner.union(vec![lit1, lit2]);
    let e1 = interner.intern(crate::types::TypeData::Enum(enum_def, inner_union));

    let narrowed = ctx.narrow_to_type(e1, lit3);
    assert_eq!(
        narrowed,
        TypeId::NEVER,
        "narrow_to_type(E1, 3) where 3 is not in E1 should be NEVER"
    );
}

#[test]
fn test_enum_union_parts_merge_on_join() {
    // When control flow produces Enum(D,2) | Enum(D,1), the union should
    // merge to Enum(D, 1|2) rather than staying as two separate enum types.
    // This verifies the merge_same_enum_parts step in normalize_union.
    let interner = TypeInterner::new();

    let enum_def = crate::def::DefId(100);
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);

    let part_a = interner.intern(crate::types::TypeData::Enum(enum_def, lit2));
    let part_b = interner.intern(crate::types::TypeData::Enum(enum_def, lit1));

    // Building Enum(D,2) | Enum(D,1) should give Enum(D, 1|2) = E1
    let joined = interner.union(vec![part_a, part_b]);

    let inner_12 = interner.union(vec![lit1, lit2]);
    let e1 = interner.intern(crate::types::TypeData::Enum(enum_def, inner_12));

    assert_eq!(
        joined, e1,
        "Enum(D,2) | Enum(D,1) should merge to Enum(D, 1|2)"
    );
}

#[test]
fn test_enum_narrowing_join_roundtrip() {
    // Full roundtrip: E1 excluding 1 | narrow_to(E1, 1) should recover E1.
    // This is the join after `if (v: E1) { v !== 1 } {}`.
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let enum_def = crate::def::DefId(200);
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let inner_union = interner.union(vec![lit1, lit2]);
    let e1 = interner.intern(crate::types::TypeData::Enum(enum_def, inner_union));

    // True branch: v !== 1 → exclude 1 → Enum(D, 2)
    let true_branch = ctx.narrow_excluding_type(e1, lit1);
    // False branch: v === 1 → narrow to 1 → Enum(D, 1)
    let false_branch = ctx.narrow_to_type(e1, lit1);

    // Join: Enum(D,2) | Enum(D,1) → should merge to E1
    let joined = interner.union(vec![true_branch, false_branch]);
    assert_eq!(
        joined, e1,
        "join(E1 excl 1, narrow_to(E1, 1)) should recover E1"
    );
}

#[test]
fn test_enum_narrowing_two_names_same_fix() {
    // Regression coverage: the fix must not depend on any specific variable
    // name, enum name, or type parameter name. Verify with different DefIds.
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    for def_raw in [77u32, 888, 12345] {
        let enum_def = crate::def::DefId(def_raw);
        let inner_a = interner.literal_number(10.0);
        let inner_b = interner.literal_number(20.0);
        let inner = interner.union(vec![inner_a, inner_b]);
        let e = interner.intern(crate::types::TypeData::Enum(enum_def, inner));

        let narrowed_to_a = ctx.narrow_to_type(e, inner_a);
        let expected = interner.intern(crate::types::TypeData::Enum(enum_def, inner_a));
        assert_eq!(
            narrowed_to_a, expected,
            "narrow_to_type with DefId={def_raw} should produce Enum(D,10)"
        );
    }
}

// =============================================================================
// Array.isArray narrowing - ReadonlyArray<T> application form
// =============================================================================

/// Register a dummy `ReadonlyArray` base in the interner and return its `TypeId`.
fn register_readonly_array_base(interner: &TypeInterner) -> TypeId {
    let base = interner.object(vec![]);
    interner.set_readonly_array_base_type(base);
    base
}

#[test]
fn array_isarray_narrows_readonly_array_application_truthy() {
    let interner = TypeInterner::new();
    let base = register_readonly_array_base(&interner);
    let readonly_numbers = interner.application(base, vec![TypeId::NUMBER]);
    let union = interner.union2(readonly_numbers, TypeId::NUMBER);
    let ctx = NarrowingContext::new(&interner);

    let narrowed = ctx.narrow_type(union, &TypeGuard::Array, GuardSense::Positive);

    assert_eq!(
        narrowed, readonly_numbers,
        "Array.isArray truthy branch should keep ReadonlyArray<number>"
    );
}

#[test]
fn array_isarray_narrows_readonly_array_application_different_element_types() {
    let interner = TypeInterner::new();
    let base = register_readonly_array_base(&interner);
    let ctx = NarrowingContext::new(&interner);

    let readonly_strings = interner.application(base, vec![TypeId::STRING]);
    let string_union = interner.union2(readonly_strings, TypeId::STRING);
    let narrowed_strings = ctx.narrow_type(string_union, &TypeGuard::Array, GuardSense::Positive);
    assert_eq!(
        narrowed_strings, readonly_strings,
        "Array.isArray truthy branch should keep ReadonlyArray<string>"
    );

    let readonly_booleans = interner.application(base, vec![TypeId::BOOLEAN]);
    let boolean_union = interner.union2(readonly_booleans, TypeId::BOOLEAN);
    let narrowed_booleans = ctx.narrow_type(boolean_union, &TypeGuard::Array, GuardSense::Positive);
    assert_eq!(
        narrowed_booleans, readonly_booleans,
        "Array.isArray truthy branch should keep ReadonlyArray<boolean>"
    );
}

#[test]
fn array_isarray_narrows_readonly_array_application_falsy() {
    let interner = TypeInterner::new();
    let base = register_readonly_array_base(&interner);
    let readonly_numbers = interner.application(base, vec![TypeId::NUMBER]);
    let union = interner.union2(readonly_numbers, TypeId::NUMBER);
    let ctx = NarrowingContext::new(&interner);

    let narrowed = ctx.narrow_type(union, &TypeGuard::Array, GuardSense::Negative);

    // `Array.isArray`'s predicate type is the mutable `any[]`; a
    // `ReadonlyArray<number>` is not assignable to it, so the negative branch
    // removes nothing and the union is preserved verbatim (tsc narrows `x` to
    // `number | readonly number[]`; cf #14782/#15070).
    assert_eq!(
        narrowed, union,
        "!Array.isArray keeps ReadonlyArray<number>: readonly arrays are not assignable to the mutable any[] predicate"
    );
}

#[test]
fn array_isarray_narrows_readonly_array_application_alone() {
    let interner = TypeInterner::new();
    let base = register_readonly_array_base(&interner);
    let readonly_numbers = interner.application(base, vec![TypeId::NUMBER]);
    let ctx = NarrowingContext::new(&interner);

    let truthy = ctx.narrow_type(readonly_numbers, &TypeGuard::Array, GuardSense::Positive);
    let falsy = ctx.narrow_type(readonly_numbers, &TypeGuard::Array, GuardSense::Negative);

    assert_eq!(
        truthy, readonly_numbers,
        "Array.isArray should keep a bare ReadonlyArray<number>"
    );
    // A bare readonly array is not assignable to the mutable `any[]` predicate,
    // so the negative branch removes nothing and keeps it unchanged (tsc
    // narrows `x` to `readonly number[]`, not `never`; cf #14782/#15070).
    assert_eq!(
        falsy, readonly_numbers,
        "!Array.isArray keeps a bare ReadonlyArray<number>: it is not assignable to the mutable any[] predicate"
    );
}

#[test]
fn array_isarray_keeps_mutable_and_readonly_array_members() {
    let interner = TypeInterner::new();
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let base = register_readonly_array_base(&interner);
    let readonly_strings = interner.application(base, vec![TypeId::STRING]);
    let union = interner.union(vec![mutable_numbers, readonly_strings, TypeId::BOOLEAN]);
    let ctx = NarrowingContext::new(&interner);

    let narrowed = ctx.narrow_type(union, &TypeGuard::Array, GuardSense::Positive);
    let expected = interner.union2(mutable_numbers, readonly_strings);

    assert_eq!(
        narrowed, expected,
        "Array.isArray should keep mutable and readonly array members"
    );
}

/// A union of two *distinct* mutable arrays must keep both members with their
/// concrete element types: each is a subtype of the predicate `any[]`, so tsc
/// keeps it rather than substituting `any[]`. Guards the regression where a
/// mutable array `number[]` was collapsed to `any[]` because
/// `any[] <: number[]` (its element `any <: number`) tripped the
/// any-array-compat substitution that is meant only for non-array members.
#[test]
fn array_isarray_keeps_multiple_distinct_mutable_arrays() {
    let interner = TypeInterner::new();
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let mutable_strings = interner.array(TypeId::STRING);
    let union = interner.union(vec![mutable_numbers, mutable_strings, TypeId::BOOLEAN]);
    let ctx = NarrowingContext::new(&interner);

    let narrowed = ctx.narrow_type(union, &TypeGuard::Array, GuardSense::Positive);
    let expected = interner.union2(mutable_numbers, mutable_strings);
    let any_array = interner.array(TypeId::ANY);

    assert_eq!(
        narrowed, expected,
        "Array.isArray should keep both distinct mutable array members, not \
         collapse them to any[]"
    );
    assert_ne!(
        narrowed, any_array,
        "the mutable array members must not be substituted with any[]"
    );
}

/// A mutable array alongside a non-array, any-array-compatible member: tsc
/// keeps the genuine array (`number[] <: any[]`) and substitutes the predicate
/// `any[]` only for the structurally-array-like-but-not-array member
/// (`{ [k: string]: any }`). The fix must preserve the concrete `number[]`
/// element type while still substituting the index-signature member.
#[test]
fn array_isarray_substitutes_only_nonarray_any_compat_member() {
    let interner = TypeInterner::new();
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let any_string_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: None,
        symbol: None,
    });
    let union = interner.union(vec![mutable_numbers, any_string_index, TypeId::BOOLEAN]);
    let ctx = NarrowingContext::new(&interner);

    let narrowed = ctx.narrow_type(union, &TypeGuard::Array, GuardSense::Positive);
    let any_array = interner.array(TypeId::ANY);

    let members = match interner.lookup(narrowed) {
        Some(TypeData::Union(list)) => interner.type_list(list).to_vec(),
        _ => vec![narrowed],
    };
    assert!(
        members.contains(&mutable_numbers),
        "the concrete mutable array `number[]` must be preserved, got {members:?}"
    );
    assert!(
        members.contains(&any_array),
        "the non-array `{{ [k: string]: any }}` member must be substituted with any[], \
         got {members:?}"
    );
}

// =============================================================================
// `in`-operator narrowing of the `object` intrinsic chained with `typeof`
// =============================================================================

/// `'k' in x` over the `object` intrinsic narrows to `object & Record<"k",
/// unknown>`. Re-applying a `typeof x === "object"` guard to that intersection
/// must keep it, not collapse it to `never`. The collapse came from
/// `is_object_like_type_through_type_constraints` rejecting the `object`
/// intrinsic member of the intersection (its intrinsic fast-path returned
/// `false` for every intrinsic), so `narrow_to_type(_, object)` judged the
/// whole intersection un-assignable to `object`. Witnessed by ts-rest's
/// `response-error.ts` (`typeof body === 'object' && ... && 'message' in body
/// && typeof body.message === 'string'`).
#[test]
fn test_in_then_typeof_object_keeps_intersection() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let key = interner.intern_string("message");
    let in_narrowed = ctx.narrow_by_property_presence(TypeId::OBJECT, key, true);
    assert!(
        matches!(
            interner.lookup(in_narrowed),
            Some(crate::types::TypeData::Intersection(_))
        ),
        "'message' in object must narrow to object & Record<\"message\", unknown>"
    );

    let re_typeof = ctx.narrow_by_typeof(in_narrowed, "object");
    assert_ne!(
        re_typeof,
        TypeId::NEVER,
        "re-applying typeof === 'object' to (object & Record) must not collapse to never"
    );

    let to_object = ctx.narrow_to_type(in_narrowed, TypeId::OBJECT);
    assert_ne!(
        to_object,
        TypeId::NEVER,
        "(object & Record) is assignable to object; narrow_to_type must not yield never"
    );
}

/// Name-independence: the property atom used by the `in` guard must not change
/// the outcome (no hardcoded property name in the structural rule).
#[test]
fn test_in_then_typeof_object_independent_of_property_name() {
    for prop in ["message", "a", "__brand", "status"] {
        let interner = TypeInterner::new();
        let ctx = NarrowingContext::new(&interner);

        let key = interner.intern_string(prop);
        let in_narrowed = ctx.narrow_by_property_presence(TypeId::OBJECT, key, true);
        let re_typeof = ctx.narrow_by_typeof(in_narrowed, "object");
        assert_ne!(
            re_typeof,
            TypeId::NEVER,
            "typeof object after 'in' must hold for property {prop}"
        );
    }
}

// =============================================================================
// narrow_excluding_type per-request cumulative work budget (issue #13806, theme C)
// =============================================================================

/// A constrained type parameter whose constraint loses a member is refined to
/// `T & <narrowed constraint>` under a normal budget, but the nested constraint
/// narrow that performs that refinement is the recursion the per-request work
/// budget bounds. With the budget spent on the outer union narrow, the nested
/// refinement bails to the unchanged source instead of recursing — the result
/// still terminates and conservatively equals the un-narrowed union.
///
/// `narrow_type_param_excluding` re-mints a fresh `T & constraint'` intersection
/// at each level, so the `(source, excluded)` `narrow_excluding_visiting` guard
/// (keyed on a stable pair) cannot catch a self-referential constraint; the
/// cumulative counter is what guarantees termination.
#[test]
fn test_narrow_excluding_budget_bounds_constraint_refinement() {
    let interner = TypeInterner::new();
    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));
    let union = interner.union(vec![param, TypeId::BOOLEAN]);

    // Default budget refines the constrained parameter: (T & number) | boolean.
    let expected_param = interner.intersection(vec![param, TypeId::NUMBER]);
    let expected_full = interner.union(vec![expected_param, TypeId::BOOLEAN]);
    let full_ctx = NarrowingContext::new(&interner);
    assert_eq!(
        full_ctx.narrow_excluding_type(union, TypeId::STRING),
        expected_full,
    );

    // Budget 1 is consumed by the outer union narrow, so the nested constraint
    // narrow bails to the unchanged source. A fresh context (and therefore a
    // fresh memo) is required so the cached full result is not reused.
    let bounded_ctx = NarrowingContext::new(&interner);
    bounded_ctx.set_narrow_excluding_budget(1);
    assert_eq!(
        bounded_ctx.narrow_excluding_type(union, TypeId::STRING),
        union,
    );
}

/// Name-independence: the bound is structural, not keyed on the type-parameter
/// name. Refinement happens under a normal budget and bails under a starved one
/// regardless of how the parameter is spelled.
#[test]
fn test_narrow_excluding_budget_bound_is_name_independent() {
    for name in ["T", "U", "Element", "__x"] {
        let interner = TypeInterner::new();
        let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }));
        let union = interner.union(vec![param, TypeId::BOOLEAN]);

        let full_ctx = NarrowingContext::new(&interner);
        let refined = full_ctx.narrow_excluding_type(union, TypeId::STRING);
        assert_ne!(refined, union, "param `{name}` must refine under full budget");

        let bounded_ctx = NarrowingContext::new(&interner);
        bounded_ctx.set_narrow_excluding_budget(1);
        assert_eq!(
            bounded_ctx.narrow_excluding_type(union, TypeId::STRING),
            union,
            "param `{name}` must bail to the source under a starved budget",
        );
    }
}

/// The default budget is generous enough that a legitimately wide (but finite)
/// type narrows fully without a false bail. A 4,000-member intersection forces
/// one exclusion narrow per member; excluding `string` removes nothing, so the
/// result must terminate and equal the source unchanged.
#[test]
fn test_narrow_excluding_default_budget_handles_wide_intersection() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let mut members = Vec::new();
    for i in 0..4000 {
        let name = interner.intern_string(&format!("p{i}"));
        members.push(interner.object(vec![PropertyInfo::new(name, TypeId::NUMBER)]));
    }
    let wide = interner.intersection(members);

    assert_eq!(ctx.narrow_excluding_type(wide, TypeId::STRING), wide);
}

/// The per-request budget is shared across the exclusion families: the
/// `typeof x !== "function"` path (`narrow_excluding_function`) re-mints
/// `T & narrowed` through `narrow_type_param_excluding_function`, so it must be
/// bounded by the same counter. Under a normal budget a constrained parameter
/// refines; under a starved one it bails to the unchanged source.
#[test]
fn test_narrow_excluding_function_shares_the_budget() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let constraint = interner.union(vec![func, TypeId::NUMBER]);
    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));

    // Full budget: the callable constituent is stripped, refining T to T & number.
    let full_ctx = NarrowingContext::new(&interner);
    let refined = full_ctx.narrow_excluding_function(param);
    assert_ne!(refined, param, "T must refine away its callable constraint constituent");

    // Starved budget: the nested constraint narrow bails, leaving T unchanged.
    let bounded_ctx = NarrowingContext::new(&interner);
    bounded_ctx.set_narrow_excluding_budget(1);
    assert_eq!(bounded_ctx.narrow_excluding_function(param), param);
}

/// Regression (#14739): the false branch of a `x is Function` guard exclude-
/// narrows the source by the global `Function`. When a top-level union member is
/// itself an alias (`Lazy`/`Application`) whose body is a *union* carrying both a
/// non-callable and a callable constituent (`Updater<P, R> = R | ((p) => R)`),
/// the callable must be stripped from *inside* that member — tsc's `filterType`
/// excludes per top-level constituent. Before the fix the whole alias member
/// survived because it was not, *as a whole*, assignable to `Function`, leaving
/// the callable in the false branch (false TS2322 in tanstack-router
/// `functionalUpdate`). The descent is structural, so two differently numbered
/// alias `DefId`s narrow identically.
#[test]
fn test_narrow_excluding_function_descends_into_union_bodied_alias_members() {
    use crate::def::DefId;

    let interner = TypeInterner::new();

    let callable = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Two distinct aliases, each `<non-callable> | <callable>`.
    let def_a = DefId(101);
    let def_b = DefId(102);
    let alias_a = interner.intern(TypeData::Lazy(def_a));
    let alias_b = interner.intern(TypeData::Lazy(def_b));
    let body_a = interner.union(vec![TypeId::STRING, callable]);
    let body_b = interner.union(vec![TypeId::NUMBER, callable]);

    struct AliasResolver {
        entries: [(DefId, TypeId); 2],
    }
    impl TypeResolver for AliasResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }
        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            self.entries
                .iter()
                .find_map(|&(d, t)| (d == def_id).then_some(t))
        }
    }

    let resolver = AliasResolver {
        entries: [(def_a, body_a), (def_b, body_b)],
    };
    let ctx = NarrowingContext::new(&interner).with_resolver(&resolver);

    let source = interner.union(vec![alias_a, alias_b]);
    let function_type = ctx.function_type();

    // The callable constituents are stripped from inside each alias body,
    // leaving only the non-callable residual `string | number`.
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(ctx.narrow_excluding_type(source, function_type), expected);
}
