#[test]
fn test_fn_return_covariance_never_return() {
    // () => never <: () => T for any T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_never = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NEVER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // never is subtype of everything
    assert!(checker.is_subtype_of(fn_return_never, fn_return_string));
    assert!(checker.is_subtype_of(fn_return_never, fn_return_number));
}

#[test]
fn test_fn_return_covariance_object_return() {
    // () => { a: string, b: number } <: () => { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let fn_return_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_ab = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // { a, b } is subtype of { a }, so fn_return_ab is subtype
    assert!(checker.is_subtype_of(fn_return_ab, fn_return_a));
    // { a } is NOT subtype of { a, b }
    assert!(!checker.is_subtype_of(fn_return_a, fn_return_ab));
}

#[test]
fn test_fn_return_covariance_void_return() {
    // () => undefined <: () => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_undefined = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::UNDEFINED,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // undefined is subtype of void
    assert!(checker.is_subtype_of(fn_return_undefined, fn_return_void));
}

#[test]
fn test_fn_return_covariance_unknown_return() {
    // () => string is NOT subtype of () => unknown in strict sense
    // But () => unknown accepts any return
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // string is subtype of unknown, so fn_return_string is subtype
    assert!(checker.is_subtype_of(fn_return_string, fn_return_unknown));
}

#[test]
fn test_fn_optional_param_fewer_params_is_subtype() {
    // () => void <: (x?: string) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with no params can be used where optional param is expected
    assert!(checker.is_subtype_of(fn_no_params, fn_optional_param));
}

#[test]
fn test_fn_optional_param_required_to_optional() {
    // (x: string) => void is NOT subtype of (x?: string) => void
    // TypeScript widens optional params to string|undefined, so
    // contravariant check: string|undefined <: string fails.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Required param IS subtype of optional — tsc compares declared types,
    // not the | undefined widened type, so (x: string) => void <: (x?: string) => void.
    assert!(checker.is_subtype_of(fn_required, fn_optional));
}

#[test]
fn test_fn_optional_param_optional_to_required_is_subtype() {
    // (x?: string) => void IS subtype of (x: string) => void
    // Contravariant: string <: string|undefined → YES
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Optional IS subtype of required (contravariant: string <: string|undefined)
    assert!(checker.is_subtype_of(fn_optional, fn_required));
}
