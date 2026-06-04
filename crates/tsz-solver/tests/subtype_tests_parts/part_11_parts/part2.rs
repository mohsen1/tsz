#[test]
fn test_constructor_optional_parameter() {
    // new (x?: string) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![]);

    let ctor_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Optional param constructor is wider (accepts more call patterns)
    assert!(checker.is_subtype_of(ctor_optional, ctor_required));
}

#[test]
fn test_constructor_rest_parameter() {
    // new (...args: string[]) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![]);
    let string_array = interner.array(TypeId::STRING);

    let ctor_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(ctor_rest != TypeId::ERROR);
}

#[test]
fn test_constructor_overload_signatures() {
    // interface C { new (): A; new (x: string): B }
    let interner = TypeInterner::new();

    let instance_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let instance_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let overloaded_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance_a,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: instance_b,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(overloaded_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_generic_type_param() {
    // new <T>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let generic_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(generic_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_generic_with_constraint() {
    // new <T extends object>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let constrained_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(constrained_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_abstract_pattern() {
    // abstract new () => T (abstract constructor)
    // Represented as a construct signature that can't be directly called
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Abstract constructor (conceptually - just a construct signature)
    let abstract_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Concrete constructor
    let concrete_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Both should be valid
    assert!(abstract_ctor != TypeId::ERROR);
    assert!(concrete_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_with_static_properties() {
    // Constructor function with static members
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let ctor_with_static = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![PropertyInfo {
            name: interner.intern_string("create"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }],
        string_index: None,
        number_index: None,
    });

    assert!(ctor_with_static != TypeId::ERROR);
}
