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

    assert_eq!(
        narrowed,
        TypeId::NUMBER,
        "!Array.isArray should exclude ReadonlyArray<number>"
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
    assert_eq!(
        falsy,
        TypeId::NEVER,
        "!Array.isArray should exclude a bare ReadonlyArray<number>"
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
