// ============================================================================
// Tuple-to-Array Union-of-Elements Tests
//
// tsc relates a tuple to an array element type by unioning the tuple's element
// types and relating that union *once* (not element-by-element). This matters
// whenever the union normalizes to something the individual members are not
// subtypes of — most importantly `any | undefined` collapses to `any`, so
// `[any, undefined] <: IObject[]` holds even though `undefined <: IObject`
// fails on its own. These tests vary binder names (`IObject`/`Bag`/`Dict`,
// `T`/`U`) to confirm the rule is structural, with negative controls that must
// keep rejecting.
// ============================================================================

/// Build a structural object type like `interface IObject { <prop>: number }`.
/// The binder name is irrelevant to the structural rule; only the shape (a
/// non-`undefined`, non-`any` object) matters.
fn object_with_prop(interner: &TypeInterner, prop: &str) -> TypeId {
    let name = interner.intern_string(prop);
    interner.object(vec![PropertyInfo::new(name, TypeId::NUMBER)])
}

fn fixed(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

// --- Negative controls: heterogeneous tuples to a too-narrow array stay rejected ---

#[test]
fn test_tuple_union_string_number_to_string_array_rejects() {
    // [string, number] <: string[] must REJECT (number is not string).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![fixed(TypeId::STRING), fixed(TypeId::NUMBER)]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[string, number] should NOT be assignable to string[]"
    );
}

#[test]
fn test_tuple_union_number_string_to_number_array_rejects() {
    // [number, string] <: number[] must REJECT (string is not number).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::STRING)]);

    assert!(
        !checker.is_subtype_of(source, number_array),
        "[number, string] should NOT be assignable to number[]"
    );
}

// --- Positive: union widening / coverage cases ---

#[test]
fn test_tuple_string_literal_to_string_array_accepts() {
    // [string, "x"] <: string[] ACCEPT ("x" widens into the string union).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_lit = interner.literal_string("x");
    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![fixed(TypeId::STRING), fixed(x_lit)]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, \"x\"] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_number_string_to_union_array_accepts() {
    // [number, string] <: (number | string)[] ACCEPT.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::STRING)]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[number, string] should be assignable to (number | string)[]"
    );
}

// --- The fix: `any` absorbs `undefined` in the element union ---

#[test]
fn test_tuple_any_undefined_to_object_array_accepts() {
    // [any, undefined] <: IObject[] ACCEPT.
    // The element union `any | undefined` collapses to `any`, and `any` relates
    // to any element type. Previously tsz checked element-wise and `undefined <:
    // IObject` failed, producing a spurious TS2345 (ts-deepmerge witness).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let iobject_array = interner.array(object_with_prop(&interner, "value"));
    let source = interner.tuple(vec![fixed(TypeId::ANY), fixed(TypeId::UNDEFINED)]);

    assert!(
        checker.is_subtype_of(source, iobject_array),
        "[any, undefined] should be assignable to IObject[] (any|undefined = any)"
    );
}

#[test]
fn test_tuple_undefined_any_to_object_array_accepts() {
    // [undefined, any] <: Bag[] ACCEPT (order-independent: union absorbs `any`).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let bag_array = interner.array(object_with_prop(&interner, "item"));
    let source = interner.tuple(vec![fixed(TypeId::UNDEFINED), fixed(TypeId::ANY)]);

    assert!(
        checker.is_subtype_of(source, bag_array),
        "[undefined, any] should be assignable to Bag[] (undefined|any = any)"
    );
}

#[test]
fn test_tuple_any_rest_undefined_array_to_object_array_accepts() {
    // [any, ...undefined[]] <: Dict[] ACCEPT.
    // The rest spread's element type `undefined` folds into the union with
    // `any`, again collapsing to `any`.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let dict_array = interner.array(object_with_prop(&interner, "entry"));
    let undefined_array = interner.array(TypeId::UNDEFINED);
    let source = interner.tuple(vec![fixed(TypeId::ANY), rest(undefined_array)]);

    assert!(
        checker.is_subtype_of(source, dict_array),
        "[any, ...undefined[]] should be assignable to Dict[] (any|undefined = any)"
    );
}

#[test]
fn test_tuple_any_undefined_to_readonly_object_array_accepts() {
    // [any, undefined] <: readonly IObject[] ACCEPT (readonly target form).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let iobject_readonly = interner.readonly_array(object_with_prop(&interner, "value"));
    let source = interner.tuple(vec![fixed(TypeId::ANY), fixed(TypeId::UNDEFINED)]);

    assert!(
        checker.is_subtype_of(source, iobject_readonly),
        "[any, undefined] should be assignable to readonly IObject[] (any|undefined = any)"
    );
}

// --- Regression canary: no `any` present => still rejects (matches tsc) ---

#[test]
fn test_tuple_object_undefined_to_object_array_still_rejects() {
    // [IObject, undefined] <: IObject[] must STILL REJECT when there is no `any`
    // to absorb `undefined`. This is the `merge(o, undefined)` shape WITHOUT an
    // `any` argument — both compilers reject it.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let iobject = object_with_prop(&interner, "value");
    let iobject_array = interner.array(iobject);
    let source = interner.tuple(vec![fixed(iobject), fixed(TypeId::UNDEFINED)]);

    assert!(
        !checker.is_subtype_of(source, iobject_array),
        "[IObject, undefined] should NOT be assignable to IObject[] (no any to absorb undefined)"
    );
}

// --- Generic element: `[any, T] <: T[]` accepts via `any` absorption ---

#[test]
fn test_tuple_any_typeparam_to_typeparam_array_accepts() {
    // [any, T] <: T[] ACCEPT. The union `any | T` collapses to `any`, which is
    // assignable to the element type `T`.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_param_info));
    let t_array = interner.array(t_param);
    let source = interner.tuple(vec![fixed(TypeId::ANY), fixed(t_param)]);

    assert!(
        checker.is_subtype_of(source, t_array),
        "[any, T] should be assignable to T[] (any|T = any)"
    );
}

#[test]
fn test_tuple_object_typeparam_to_typeparam_array_rejects() {
    // [U, T] <: T[] must REJECT when U is an unrelated object type with no `any`
    // to absorb it (negative control for the generic case).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_param_info));
    let t_array = interner.array(t_param);
    let u_object = object_with_prop(&interner, "u");
    let source = interner.tuple(vec![fixed(u_object), fixed(t_param)]);

    assert!(
        !checker.is_subtype_of(source, t_array),
        "[U, T] should NOT be assignable to T[] (U is unrelated, no any to absorb)"
    );
}

#[test]
fn generic_alpha_rename_preserves_captured_same_named_binder() {
    let interner = TypeInterner::new();
    let file = interner.intern_string("generic-alpha-capture.ts");
    let u = interner.intern_string("U");
    let v = interner.intern_string("V");
    let owned_u = TypeParamInfo {
        name: u,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 1 },
    };
    let foreign_u = TypeParamInfo {
        origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        ..owned_u
    };
    let target_v = TypeParamInfo {
        name: v,
        origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        ..owned_u
    };
    let owned_u_type = interner.fresh_type_param(owned_u);
    let foreign_u_type = interner.fresh_type_param(foreign_u);
    let target_v_type = interner.fresh_type_param(target_v);

    let function = |type_param, param_type, return_type| {
        interner.function(FunctionShape {
            type_params: vec![type_param],
            params: vec![ParamInfo::unnamed(param_type)],
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };
    let source = function(
        owned_u,
        owned_u_type,
        interner.readonly_tuple(vec![
            TupleElement::fixed(owned_u_type),
            TupleElement::fixed(foreign_u_type),
        ]),
    );
    let good_target = function(
        target_v,
        target_v_type,
        interner.readonly_tuple(vec![
            TupleElement::fixed(target_v_type),
            TupleElement::fixed(foreign_u_type),
        ]),
    );
    let bad_target = function(
        target_v,
        target_v_type,
        interner.readonly_tuple(vec![
            TupleElement::fixed(target_v_type),
            TupleElement::fixed(target_v_type),
        ]),
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, good_target));
    assert!(!checker.is_subtype_of(source, bad_target));
}

#[test]
fn unstamped_type_parameter_relation_retains_name_fallback() {
    let interner = TypeInterner::new();
    let name = interner.intern_string("Legacy");
    let legacy = TypeParamInfo::simple(name);
    let left = interner.fresh_type_param(legacy);
    let right = interner.fresh_type_param(legacy);

    assert_ne!(left, right, "the control must compare distinct type ids");
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(left, right));
}
