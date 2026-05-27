#[test]
fn test_ref_to_ref_resolution() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Two refs that should be equal when resolved
    let ref1 = interner.lazy(DefId(1));
    let ref2 = interner.lazy(DefId(2));

    // Both resolve to string
    env.insert_def(DefId(1), TypeId::STRING);
    env.insert_def(DefId(2), TypeId::STRING);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(ref1, ref2));
    assert!(checker.is_subtype_of(ref2, ref1));
}

#[test]
fn test_ref_to_object_resolution() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Create an object type: { x: number }
    let obj_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Create a Ref that resolves to { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let ref_type = interner.lazy(DefId(100));
    env.insert_def(DefId(100), obj_xy);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Ref resolves to { x: number, y: string } which is subtype of { x: number }
    assert!(checker.is_subtype_of(ref_type, obj_x));
}

#[test]
fn test_unresolved_ref_behavior() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new(); // Empty environment

    let ref_type = interner.lazy(DefId(999));

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Unresolved ref to itself should be true (same TypeId)
    assert!(checker.is_subtype_of(ref_type, ref_type));

    // Unresolved ref to something else should be false
    assert!(!checker.is_subtype_of(ref_type, TypeId::STRING));
}

#[test]
fn test_function_rest_parameter_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Create any[] type for rest parameter
    let any_array = interner.array(TypeId::ANY);

    // (a: string, b: any, c: any) => any - 3 fixed params
    let fixed_params = FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("c")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let fixed_fn = interner.function(fixed_params);

    // (a: string, b: any, ...args: any[]) => any - 2 fixed + rest
    let rest_params = FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let rest_fn = interner.function(rest_params);

    // Function with 3 fixed params IS assignable to function with 2 fixed + rest
    // Because (a, b, c) can be called as (a, b, ...args) where args = [c]
    assert!(checker.is_subtype_of(fixed_fn, rest_fn));

    // Function with rest is NOT assignable to function with fixed params
    // (because rest can accept 0 or more args, but fixed expects exactly 3)
    // This depends on semantics - TypeScript actually allows this in some cases
    // For now, test the basic case
}

#[test]
fn test_rest_unknown_bivariant_subtyping_toggle() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));

    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_rest_any_bivariant_subtyping_toggle() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: number) => void IS subtype of (...args: any[]) => void
    // because `any` in the target rest parameter is always compatible.
    assert!(checker.is_subtype_of(source, target));

    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_never_param_is_not_subtype_of_any_rest_target() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NEVER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_extra_elements() {
    // CRITICAL: [number, string] is NOT assignable to [number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // [number, string]
    let source = interner.tuple(vec![
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

    // [number]
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Source has extra elements, target is closed -> should FAIL
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_with_rest_target() {
    // [number, string] IS assignable to [number, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, string]
    let source = interner.tuple(vec![
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

    // [number, ...string[]]
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Target has rest -> should accept extra elements
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_expansion() {
    // [number, string, boolean] IS assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_missing_element() {
    // [number, string] is NOT assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let source = interner.tuple(vec![
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

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_extra_element() {
    // [number, string, boolean, boolean] is NOT assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_variadic_tail() {
    // [number, string, boolean, boolean] IS assignable to [number, ...[string, ...boolean[]]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let boolean_array = interner.array(TypeId::BOOLEAN);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: boolean_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_source_rest_closed_target() {
    // [number, ...string[]] is NOT assignable to [number, string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // [number, string]
    let target = interner.tuple(vec![
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

    // Source has rest but target is closed -> should FAIL
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_optional_elements() {
    // [number, string?] IS assignable to [number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // [number, string?]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    // [number]
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Optional elements don't count as "extra" if they're beyond target length
    // This is actually a borderline case - TypeScript may reject this
    // For strictness, we reject tuples with more elements even if optional
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_to_rest() {
    // [number, ...string[]] IS assignable to [number, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // [number, ...string[]]
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Both have rest, same types -> should succeed
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_with_rest() {
    // BLOCKER fix: [number, ...string[]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // string[]
    let target = string_array;

    // This should FAIL because first element is number, not string
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_with_rest_tuple() {
    // [string, ...[string, string]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_with_rest_tuple_mismatch() {
    // [string, ...[string, number]] is NOT assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_with_rest_tuple_variadic() {
    // [string, ...[string, ...string[]]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_all_matching_with_rest() {
    // [string, ...string[]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [string, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // string[]
    let target = string_array;

    // This should SUCCEED - all elements (including rest) are strings
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_no_rest() {
    // [string, string] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [string, string]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // string[]
    let target = string_array;

    // This should SUCCEED - all fixed elements are strings
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_mixed_types() {
    // [number, string] is NOT assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, string]
    let source = interner.tuple(vec![
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

    // string[]
    let target = string_array;

    // This should FAIL - first element is number, not string
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_number_number() {
    // [number, number] IS assignable to number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    // [number, number]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // number[]
    let target = number_array;

    // This should SUCCEED - all fixed elements are numbers
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_array_assignment_tuple_to_union_array() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(checker.is_subtype_of(source, union_array));
}

#[test]
fn test_array_to_variadic_tuple() {
    // `string[]` IS assignable to `[...string[]]` — a variadic tuple with a
    // single rest element of array type is structurally equivalent to that
    // array type. tsc accepts this and tsz must too; the symmetric case is
    // `[...T] <: T` (handled in `visit_tuple`).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(checker.is_subtype_of(string_array, target));

    // The reduction must still respect element-type compatibility: a mismatched
    // element type still rejects.
    let number_array_target = interner.tuple(vec![TupleElement {
        type_id: interner.array(TypeId::NUMBER),
        name: None,
        optional: false,
        rest: true,
    }]);
    assert!(!checker.is_subtype_of(string_array, number_array_target));
}

#[test]
fn test_tuple_array_assignment_array_to_tuple_rejected() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_variadic_tuple_with_required_prefix() {
    // string[] is NOT assignable to [string, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_variadic_tuple_with_optional_prefix() {
    // string[] is NOT assignable to [string?, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_fixed_optional_tuple() {
    // string[] is NOT assignable to [string?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_tuple_array_assignment_empty_array_optional_tuple() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_array = interner.array(TypeId::NEVER);
    let optional_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    assert!(checker.is_subtype_of(empty_array, optional_tuple));
}

#[test]
fn test_never_array_to_optional_tuple() {
    // never[] IS assignable to [] and [string?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let empty_tuple = interner.tuple(Vec::new());
    let optional_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);
    let required_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(checker.is_subtype_of(never_array, empty_tuple));
    assert!(checker.is_subtype_of(never_array, optional_tuple));
    assert!(!checker.is_subtype_of(never_array, required_tuple));
}

#[test]
fn test_never_array_to_variadic_tuple() {
    // never[] IS assignable to [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(checker.is_subtype_of(never_array, target));
}

#[test]
fn test_number_index_signature_numeric_property() {
    // CRITICAL: { 0: string } should match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string }
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - numeric property "0" matches number index signature
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_number_index_signature_type_mismatch() {
    // { 0: number } should NOT match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: number }
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::NUMBER,
    )]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should FAIL - numeric property has wrong type
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_anonymous_number_index_signature_vacuously_compatible_with_no_numeric_keys() {
    // Anonymous object types are allowed to satisfy numeric index signatures
    // structurally when they have no numeric members.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("one"),
        TypeId::NUMBER,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_named_object_without_number_index_does_not_satisfy_number_index_target() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(
            interner.intern_string("one"),
            TypeId::NUMBER,
        )],
        ObjectFlags::empty(),
        Some(SymbolId(1)),
    );

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(!checker.is_subtype_of(source, target));
    assert!(matches!(
        checker.explain_failure(source, target),
        Some(SubtypeFailureReason::MissingIndexSignature {
            index_kind: "number"
        })
    ));
}

#[test]
fn test_number_index_signature_method_bivariant_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_param = TypeId::STRING;
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source_method = interner.object(vec![PropertyInfo::method(
        interner.intern_string("0"),
        narrow_method,
    )]);

    let source_prop = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        narrow_method,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: wide_fn,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source_method, target));
    assert!(!checker.is_subtype_of(source_prop, target));
}

#[test]
fn test_named_class_not_assignable_to_string_indexed_without_explicit_index_sig() {
    // Named class/interface types are nominally opaque for index-signature satisfaction:
    // having only compatible properties is not sufficient — an explicit matching index
    // signature must be declared.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let class_def = DefId(4100);
    let class_symbol = SymbolId(4100);
    env.register_def_symbol_mapping(class_def, class_symbol);
    env.insert_def_kind(class_def, crate::def::DefKind::Class);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::method(
            interner.intern_string("foo"),
            interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: true,
            }),
        )],
        ObjectFlags::empty(),
        Some(class_symbol),
    );

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
            param_name: None,
        }),
    });

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_namespace_object_can_satisfy_string_index_structurally() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let namespace_def = DefId(4101);
    let namespace_symbol = SymbolId(4101);
    env.register_def_symbol_mapping(namespace_def, namespace_symbol);
    env.insert_def_kind(namespace_def, crate::def::DefKind::Namespace);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(
            interner.intern_string("unrelated"),
            TypeId::NUMBER,
        )],
        ObjectFlags::empty(),
        Some(namespace_symbol),
    );

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
            param_name: None,
        }),
    });

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        checker.is_subtype_of(source, target),
        "Namespace value objects should keep their implicit structural compatibility"
    );
}

#[test]
fn test_string_index_signature_method_bivariant_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_param = TypeId::STRING;
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source_method = interner.object(vec![PropertyInfo::method(
        interner.intern_string("foo"),
        narrow_method,
    )]);

    let source_prop = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        narrow_method,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: wide_fn,
            readonly: false,
            param_name: None,
        }),
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source_method, target));
    assert!(!checker.is_subtype_of(source_prop, target));
}

#[test]
fn test_number_index_signature_multiple_numeric_props() {
    // { 0: string, 1: string, 2: string } should match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string, 1: string, 2: string }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("2"), TypeId::STRING),
    ]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - all numeric properties match
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_number_and_string_index_signatures() {
    // { 0: string, foo: string } should match { [x: number]: string; [y: string]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string, foo: string }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
    ]);

    // { [x: number]: string; [y: string]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - "0" satisfies number index, both satisfy string index
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_index_signature_consistency_number_vs_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_readonly_index_signature_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    let mutable_target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let readonly_target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    // Per tsc behavior, readonly on index signatures does NOT affect assignability.
    // A readonly index signature IS assignable to a mutable index signature.
    assert!(checker.is_subtype_of(readonly_source, mutable_target));
    assert!(checker.is_subtype_of(mutable_target, readonly_target));
}

#[test]
fn test_readonly_property_with_mutable_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let mutable_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let readonly_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    // tsc allows readonly properties to be assigned to writable index signatures.
    // The readonly constraint prevents writing through the source reference, but
    // doesn't prevent the type from satisfying a writable index signature target.
    assert!(checker.is_subtype_of(source, mutable_index));
    assert!(checker.is_subtype_of(source, readonly_index));
}

#[test]
fn test_object_with_index_properties_match_target_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![
            PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        ],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(checker.is_subtype_of(source, target));
}

