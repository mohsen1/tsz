/// Tuple element contextual type for rest-at-end tuples:
/// `[A, ...B[]]` — index 0 gets A, index 1+ gets B (not B[]).
#[test]
fn test_tuple_rest_element_extracts_array_element_type() {
    let interner = TypeInterner::new();

    // [(x: number) => number, ...((x: string) => number)[]]
    let num_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_callback_array = interner.array(str_callback);

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: num_callback,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: str_callback_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let ctx = ContextualTypeContext::with_expected(&interner, tuple_type);

    // Index 0: should get (x: number) => number
    let elem0 = ctx.get_tuple_element_type_with_count(0, 3);
    assert_eq!(elem0, Some(num_callback));

    // Index 1: should get (x: string) => number (the ELEMENT type, not the array)
    let elem1 = ctx.get_tuple_element_type_with_count(1, 3);
    assert_eq!(elem1, Some(str_callback));

    // Index 2: should also get (x: string) => number (overflow into rest)
    let elem2 = ctx.get_tuple_element_type_with_count(2, 3);
    assert_eq!(elem2, Some(str_callback));
}

/// Rest parameter with tuple type extracts element types correctly:
/// `f(...a: [A, ...B[]])` — arg 0 gets A, arg 1+ gets B.
#[test]
fn test_rest_param_tuple_extracts_element_type_for_call() {
    let interner = TypeInterner::new();

    let num_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_callback_array = interner.array(str_callback);

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: num_callback,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: str_callback_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // f2(...a: [(x: number) => number, ...((x: string) => number)[]]): void
    let f2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
            type_id: tuple_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, f2);

    // Arg 0: should get (x: number) => number
    assert_eq!(ctx.get_parameter_type_for_call(0, 3), Some(num_callback));

    // Arg 1: should get (x: string) => number (element type, not array)
    assert_eq!(ctx.get_parameter_type_for_call(1, 3), Some(str_callback));

    // Arg 2: should also get (x: string) => number
    assert_eq!(ctx.get_parameter_type_for_call(2, 3), Some(str_callback));
}

fn make_fn(interner: &TypeInterner, param_ty: TypeId, is_constructor: bool) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(param_ty)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor,
        is_method: false,
    })
}

#[test]
fn get_parameter_type_union_skips_constructor_only_when_callable_exists() {
    let interner = TypeInterner::new();
    let callable = make_fn(&interner, TypeId::STRING, false);
    let ctor = make_fn(&interner, TypeId::NUMBER, true);
    let union_ty = interner.union(vec![TypeId::STRING, callable, ctor]);
    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));
}

// Same rule with a different param type, proving the fix is structural not spelling-specific.
#[test]
fn get_parameter_type_union_skips_constructor_only_different_param_shape() {
    let interner = TypeInterner::new();
    let callable = make_fn(&interner, TypeId::BOOLEAN, false);
    let ctor = make_fn(&interner, TypeId::NUMBER, true);
    let union_ty = interner.union(vec![callable, ctor]);
    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::BOOLEAN));
}

// JSX-like union: string | ComponentClass (construct-only) | StatelessComponent (callable).
#[test]
fn get_parameter_type_jsx_like_string_ctor_callable_union() {
    let interner = TypeInterner::new();
    let component_class = make_fn(&interner, TypeId::NUMBER, true);
    let stateless = make_fn(&interner, TypeId::STRING, false);
    let union_ty = interner.union(vec![TypeId::STRING, component_class, stateless]);
    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));
}

// No regression: construct-only union still returns None for call-context param inference.
#[test]
fn get_parameter_type_constructor_only_union_returns_none() {
    let interner = TypeInterner::new();
    let ctor1 = make_fn(&interner, TypeId::STRING, true);
    let ctor2 = make_fn(&interner, TypeId::NUMBER, true);
    let union_ty = interner.union(vec![ctor1, ctor2]);
    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_parameter_type(0), None);
}

// `ThisType<T>` marker extraction across union members (#8537).

/// Build a `ThisType<inner>` application via the type-parameter-named form so
/// the tests do not depend on `register_this_type_def_id` lib.d.ts setup.
fn make_this_type_marker(interner: &TypeInterner, inner: TypeId) -> TypeId {
    let marker_param = TypeParamInfo::simple(interner.intern_string("ThisType"));
    let marker_base = interner.intern(TypeData::TypeParameter(marker_param));
    interner.application(marker_base, vec![inner])
}

fn make_named_object(interner: &TypeInterner, name: &str, value: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(interner.intern_string(name), value)])
}

#[test]
fn this_type_marker_union_is_order_independent() {
    let interner = TypeInterner::new();
    let a = make_named_object(&interner, "a", TypeId::STRING);
    let b = make_named_object(&interner, "b", TypeId::NUMBER);
    let x = TypeId::STRING;
    let y = TypeId::NUMBER;
    let marker_x = make_this_type_marker(&interner, x);
    let marker_y = make_this_type_marker(&interner, y);
    let a_with_marker = interner.intersection(vec![a, marker_x]);
    let b_with_marker = interner.intersection(vec![b, marker_y]);

    let union_forward = interner.union(vec![a_with_marker, b_with_marker]);
    let union_reversed = interner.union(vec![b_with_marker, a_with_marker]);
    let expected = interner.union(vec![x, y]);

    let ctx_forward = ContextualTypeContext::with_expected(&interner, union_forward);
    let ctx_reversed = ContextualTypeContext::with_expected(&interner, union_reversed);

    assert_eq!(ctx_forward.get_this_type_from_marker(), Some(expected));
    assert_eq!(ctx_reversed.get_this_type_from_marker(), Some(expected));
}

#[test]
fn this_type_marker_union_renamed_targets_still_order_independent() {
    // Same structural rule with different `ThisType<T>` arguments — the targets
    // here are boolean and bigint instead of string/number — to prove the fix
    // is not keyed on a particular spelling or pair of payload types.
    let interner = TypeInterner::new();
    let a = make_named_object(&interner, "first", TypeId::NUMBER);
    let b = make_named_object(&interner, "second", TypeId::STRING);
    let x = TypeId::BOOLEAN;
    let y = TypeId::BIGINT;
    let marker_x = make_this_type_marker(&interner, x);
    let marker_y = make_this_type_marker(&interner, y);
    let union_forward = interner.union(vec![
        interner.intersection(vec![a, marker_x]),
        interner.intersection(vec![b, marker_y]),
    ]);
    let union_reversed = interner.union(vec![
        interner.intersection(vec![b, marker_y]),
        interner.intersection(vec![a, marker_x]),
    ]);
    let expected = interner.union(vec![x, y]);

    let ctx_forward = ContextualTypeContext::with_expected(&interner, union_forward);
    let ctx_reversed = ContextualTypeContext::with_expected(&interner, union_reversed);

    assert_eq!(ctx_forward.get_this_type_from_marker(), Some(expected));
    assert_eq!(ctx_reversed.get_this_type_from_marker(), Some(expected));
}

#[test]
fn this_type_marker_union_skips_members_without_marker() {
    // `(A & ThisType<X>) | B` should yield `X` — members without a marker
    // contribute nothing to the result, matching tsc's `mapType` behavior of
    // dropping undefined-mapped members.
    let interner = TypeInterner::new();
    let a = make_named_object(&interner, "a", TypeId::STRING);
    let b = make_named_object(&interner, "b", TypeId::NUMBER);
    let x = TypeId::STRING;
    let marker_x = make_this_type_marker(&interner, x);
    let union_ty = interner.union(vec![interner.intersection(vec![a, marker_x]), b]);

    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_this_type_from_marker(), Some(x));
}

#[test]
fn this_type_marker_union_no_markers_returns_none() {
    // A union of objects with no `ThisType<T>` member must produce no marker —
    // this is the negative case proving the fix does not synthesize one.
    let interner = TypeInterner::new();
    let a = make_named_object(&interner, "a", TypeId::STRING);
    let b = make_named_object(&interner, "b", TypeId::NUMBER);
    let union_ty = interner.union(vec![a, b]);

    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_this_type_from_marker(), None);
}

#[test]
fn this_type_marker_union_collapses_duplicate_targets() {
    // Two union members carrying `ThisType<X>` with the same `X` must produce a
    // single target — `db.union` deduplicates members so the result is `X`,
    // not `X | X`.
    let interner = TypeInterner::new();
    let a = make_named_object(&interner, "a", TypeId::STRING);
    let b = make_named_object(&interner, "b", TypeId::NUMBER);
    let x = TypeId::STRING;
    let marker_x = make_this_type_marker(&interner, x);
    let union_ty = interner.union(vec![
        interner.intersection(vec![a, marker_x]),
        interner.intersection(vec![b, marker_x]),
    ]);

    let ctx = ContextualTypeContext::with_expected(&interner, union_ty);
    assert_eq!(ctx.get_this_type_from_marker(), Some(x));
}

#[test]
fn this_type_marker_intersection_is_unchanged_and_order_independent() {
    // Intersection extraction collects all `ThisType<T>` markers and intersects
    // their targets — this has always been deterministic (the result `TypeId`
    // is independent of source order because `db.intersection` canonicalizes
    // member order). Pin the behavior so the union fix doesn't regress it.
    let interner = TypeInterner::new();
    let methods = make_named_object(&interner, "greet", TypeId::VOID);
    let x = TypeId::STRING;
    let y = TypeId::NUMBER;
    let marker_x = make_this_type_marker(&interner, x);
    let marker_y = make_this_type_marker(&interner, y);

    let forward = interner.intersection(vec![methods, marker_x, marker_y]);
    let reversed = interner.intersection(vec![marker_y, marker_x, methods]);
    let expected = interner.intersection(vec![x, y]);

    let ctx_forward = ContextualTypeContext::with_expected(&interner, forward);
    let ctx_reversed = ContextualTypeContext::with_expected(&interner, reversed);

    assert_eq!(ctx_forward.get_this_type_from_marker(), Some(expected));
    assert_eq!(ctx_reversed.get_this_type_from_marker(), Some(expected));
}

// =============================================================================
// Homomorphic mapped per-element contextual typing (deferred sources)
// =============================================================================
//
// These tests cover the case where the contextual type is a homomorphic mapped
// type `{ [K in keyof X]: F<X[K]> }` whose source cannot be reduced to a
// concrete shape — typically because X is still a generic type parameter. For
// per-element contextual typing of array literals tsc substitutes K with each
// position's literal so closures receive `F<X[index]>` rather than the
// position-agnostic union of every member.

/// Helper: build a homomorphic mapped type `{ [<iter_name> in keyof <source>]: <template> }`.
fn build_homomorphic_mapped(
    interner: &TypeInterner,
    iter_name: &str,
    source: TypeId,
    template_builder: impl FnOnce(&TypeInterner, TypeId) -> TypeId,
) -> TypeId {
    let iter_atom = interner.intern_string(iter_name);
    let iter_info = TypeParamInfo {
        name: iter_atom,
        constraint: None,
        default: None,
        is_const: false,
    };
    let iter_type = interner.intern(TypeData::TypeParameter(iter_info));
    let template = template_builder(interner, iter_type);
    interner.mapped(MappedType {
        type_param: iter_info,
        constraint: interner.keyof(source),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    })
}

/// Helper: build a function shape `(v: <param_type>) => void`.
fn build_consumer_fn(interner: &TypeInterner, param_type: TypeId) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("v")),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

/// Helper: build a generic type parameter named `name` with no constraint.
fn build_unbounded_type_param(interner: &TypeInterner, name: &str) -> TypeId {
    make_iter_param(interner, name).0
}

/// Helper: build a `(TypeId, TypeParamInfo)` pair for an unconstrained iteration
/// variable named `name`. The info is needed for `MappedType { type_param, .. }`
/// construction; the type is needed when referencing K in the template.
fn make_iter_param(interner: &TypeInterner, name: &str) -> (TypeId, TypeParamInfo) {
    let info = TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };
    (interner.intern(TypeData::TypeParameter(info)), info)
}

/// `{ [K in keyof T]: (v: T[K]) => void }` over a deferred type parameter T —
/// per-index contextual typing must substitute K with the numeric index literal
/// so closures at positions 0 and 1 see different parameter types.
#[test]
fn test_homomorphic_mapped_per_index_contextual_deferred_type_param() {
    let interner = TypeInterner::new();

    let t_param = build_unbounded_type_param(&interner, "T");
    let mapped = build_homomorphic_mapped(&interner, "K", t_param, |db, k| {
        let t_index_k = db.index_access(t_param, k);
        build_consumer_fn(db, t_index_k)
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);

    let elem0 = ctx
        .get_tuple_element_type_with_count(0, 2)
        .expect("position 0 contextual missing");
    let elem1 = ctx
        .get_tuple_element_type_with_count(1, 2)
        .expect("position 1 contextual missing");

    assert_ne!(
        elem0, elem1,
        "homomorphic mapped contextual must differ per position"
    );

    let zero_lit = interner.literal_number(0.0);
    let one_lit = interner.literal_number(1.0);
    let expected0 = build_consumer_fn(&interner, interner.index_access(t_param, zero_lit));
    let expected1 = build_consumer_fn(&interner, interner.index_access(t_param, one_lit));
    assert_eq!(elem0, expected0);
    assert_eq!(elem1, expected1);
}

/// Rename the iteration variable from K to P — per-index substitution must
/// remain correct, proving the fix isn't keyed on a specific identifier name.
#[test]
fn test_homomorphic_mapped_per_index_contextual_renamed_iter_var() {
    let interner = TypeInterner::new();

    let t_param = build_unbounded_type_param(&interner, "T");
    let mapped = build_homomorphic_mapped(&interner, "P", t_param, |db, p| {
        let t_index_p = db.index_access(t_param, p);
        build_consumer_fn(db, t_index_p)
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    let elem2 = ctx
        .get_tuple_element_type_with_count(2, 3)
        .expect("position 2 contextual missing");

    let two_lit = interner.literal_number(2.0);
    let expected2 = build_consumer_fn(&interner, interner.index_access(t_param, two_lit));
    assert_eq!(elem2, expected2);
}

/// Mapped over a different source name (`X` instead of `T`) — substitution
/// must follow the source through the constraint, not be keyed on `T`.
#[test]
fn test_homomorphic_mapped_per_index_contextual_renamed_source() {
    let interner = TypeInterner::new();

    let x_param = build_unbounded_type_param(&interner, "X");
    let mapped = build_homomorphic_mapped(&interner, "K", x_param, |db, k| {
        let x_index_k = db.index_access(x_param, k);
        build_consumer_fn(db, x_index_k)
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    let elem0 = ctx
        .get_tuple_element_type_with_count(0, 1)
        .expect("position 0 contextual missing");

    let zero_lit = interner.literal_number(0.0);
    let expected0 = build_consumer_fn(&interner, interner.index_access(x_param, zero_lit));
    assert_eq!(elem0, expected0);
}

/// When the source type parameter and mapped key have the same name, nested
/// templates cannot prove which `P` should be substituted by name alone. Refuse
/// the per-index contextual fast path so callers fall back instead of turning
/// both occurrences in `P[P]` into the numeric key literal.
#[test]
fn test_homomorphic_mapped_per_index_contextual_refuses_same_name_nested_collision() {
    let interner = TypeInterner::new();

    let source_p = build_unbounded_type_param(&interner, "P");
    let mapped = build_homomorphic_mapped(&interner, "P", source_p, |db, key_p| {
        let p_index_p = db.index_access(source_p, key_p);
        build_consumer_fn(db, p_index_p)
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    assert_eq!(
        ctx.get_tuple_element_type_with_count(0, 2),
        None,
        "same-name source/key collision must fall back instead of substituting both P bindings"
    );
}

/// Identity name type (`as K`) is functionally equivalent to no name type and
/// must still allow per-index substitution.
#[test]
fn test_homomorphic_mapped_per_index_contextual_identity_name_type() {
    let interner = TypeInterner::new();

    let t_param = build_unbounded_type_param(&interner, "T");
    let (k_type, k_info) = make_iter_param(&interner, "K");
    let template = build_consumer_fn(&interner, interner.index_access(t_param, k_type));
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: interner.keyof(t_param),
        name_type: Some(k_type), // identity remapping
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    let elem0 = ctx
        .get_tuple_element_type_with_count(0, 2)
        .expect("identity name_type should not block per-index substitution");

    let zero_lit = interner.literal_number(0.0);
    let expected0 = build_consumer_fn(&interner, interner.index_access(t_param, zero_lit));
    assert_eq!(elem0, expected0);
}

/// Non-identity `as` remapping rewrites keys away from their positional
/// alignment — per-index substitution must refuse so callers fall back to the
/// existing (positionally-unaware) handling instead of returning misaligned
/// templates.
#[test]
fn test_homomorphic_mapped_per_index_contextual_skips_key_remap() {
    let interner = TypeInterner::new();

    let t_param = build_unbounded_type_param(&interner, "T");
    let (k_type, k_info) = make_iter_param(&interner, "K");
    let template = build_consumer_fn(&interner, interner.index_access(t_param, k_type));
    // A non-identity remap (`as never`) breaks positional alignment.
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: interner.keyof(t_param),
        name_type: Some(TypeId::NEVER),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    assert_eq!(
        ctx.get_tuple_element_type_with_count(0, 2),
        None,
        "non-identity name_type must not produce a positional substitution"
    );
}

/// `{ [K in keyof T]: F<T[K]> }` wrapped in a `keyof T` intersection — the
/// constraint test recognises the intersected `keyof T` so per-index
/// substitution still applies.
#[test]
fn test_homomorphic_mapped_per_index_contextual_intersection_constraint() {
    let interner = TypeInterner::new();

    let t_param = build_unbounded_type_param(&interner, "T");
    let keyof_t = interner.keyof(t_param);
    // Constraint: keyof T & string (a common idiom for narrowing keys)
    let intersected_constraint = interner.intersection(vec![keyof_t, TypeId::STRING]);

    let (k_type, k_info) = make_iter_param(&interner, "K");
    let template = build_consumer_fn(&interner, interner.index_access(t_param, k_type));
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: intersected_constraint,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    let elem0 = ctx
        .get_tuple_element_type_with_count(0, 1)
        .expect("intersected keyof constraint should still substitute per-index");

    let zero_lit = interner.literal_number(0.0);
    let expected0 = build_consumer_fn(&interner, interner.index_access(t_param, zero_lit));
    assert_eq!(elem0, expected0);
}

/// Mapped iteration over literal-key unions like `"a" | "b"` has no notion of
/// a positional ith key — per-index substitution must refuse so existing
/// finite-key handling at the property level keeps owning that case.
#[test]
fn test_homomorphic_mapped_per_index_contextual_skips_finite_literal_keys() {
    let interner = TypeInterner::new();

    let a_lit = interner.literal_string_atom(interner.intern_string("a"));
    let b_lit = interner.literal_string_atom(interner.intern_string("b"));
    let literal_union = interner.union(vec![a_lit, b_lit]);

    let (_, k_info) = make_iter_param(&interner, "K");
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: literal_union,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, mapped);
    assert_eq!(
        ctx.get_tuple_element_type_with_count(0, 2),
        None,
        "literal-key constraint should not trigger positional substitution"
    );
}
