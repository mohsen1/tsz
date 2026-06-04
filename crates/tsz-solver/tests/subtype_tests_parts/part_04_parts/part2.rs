#[test]
fn test_covariant_return_never() {
    // () => never <: () => string
    // never is bottom type, subtype of everything
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

    // never is subtype of any return type
    assert!(checker.is_subtype_of(fn_return_never, fn_return_string));
    // string is not subtype of never
    assert!(!checker.is_subtype_of(fn_return_string, fn_return_never));
}

#[test]
fn test_covariant_return_void_undefined() {
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

    // undefined <: void
    assert!(checker.is_subtype_of(fn_return_undefined, fn_return_void));
}

#[test]
fn test_contravariant_param_wider_is_subtype() {
    // (x: string | number) => void <: (x: string) => void
    // Param type is contravariant: wider param is subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_param_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_string = interner.function(FunctionShape {
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

    // Contravariant: (string | number) => void <: (string) => void
    assert!(checker.is_subtype_of(fn_param_union, fn_param_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_param_string, fn_param_union));
}

#[test]
fn test_contravariant_param_base_class() {
    // (x: Base) => void <: (x: Derived) => void
    // Base is "wider" than Derived
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base_prop = interner.intern_string("base");
    let derived_prop = interner.intern_string("derived");

    // Base has one property
    let base = interner.object(vec![PropertyInfo::new(base_prop, TypeId::STRING)]);

    // Derived extends Base with additional property
    let derived = interner.object(vec![
        PropertyInfo::new(base_prop, TypeId::STRING),
        PropertyInfo::new(derived_prop, TypeId::NUMBER),
    ]);

    let fn_param_base = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: base,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: derived,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contravariant: (Base) => void <: (Derived) => void
    assert!(checker.is_subtype_of(fn_param_base, fn_param_derived));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_param_derived, fn_param_base));
}

#[test]
fn test_contravariant_param_unknown() {
    // (x: unknown) => void <: (x: T) => void for any T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_param_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_string = interner.function(FunctionShape {
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

    // (unknown) => void is subtype of (string) => void
    assert!(checker.is_subtype_of(fn_param_unknown, fn_param_string));
}

#[test]
fn test_contravariant_multiple_params() {
    // (a: A', b: B') => void <: (a: A, b: B) => void when A <: A' and B <: B'
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Wider params
    let fn_wider = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: union,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Narrower params
    let fn_narrower = interner.function(FunctionShape {
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
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contravariant in all params
    assert!(checker.is_subtype_of(fn_wider, fn_narrower));
    assert!(!checker.is_subtype_of(fn_narrower, fn_wider));
}
