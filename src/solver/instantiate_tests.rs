use super::*;
use crate::solver::instantiate::MAX_INSTANTIATION_DEPTH;

#[test]
fn test_substitution_basic() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let mut subst = TypeSubstitution::new();

    // Initially empty
    assert!(subst.is_empty());
    assert_eq!(subst.len(), 0);

    // Add a substitution
    subst.insert(t_name, TypeId::STRING);
    assert_eq!(subst.get(t_name), Some(TypeId::STRING));
    assert_eq!(subst.get(u_name), None);
    assert_eq!(subst.len(), 1);
}

#[test]
fn test_substitution_from_args() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let type_params = vec![
        TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        },
        TypeParamInfo {
            name: u_name,
            constraint: None,
            default: None,
        },
    ];
    let type_args = vec![TypeId::STRING, TypeId::NUMBER];

    let subst = TypeSubstitution::from_args(&type_params, &type_args);

    assert_eq!(subst.get(t_name), Some(TypeId::STRING));
    assert_eq!(subst.get(u_name), Some(TypeId::NUMBER));
    assert_eq!(subst.get(interner.intern_string("V")), None);
}

#[test]
fn test_instantiate_type_parameter() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create a type parameter T
    let type_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // No substitution - should stay as is
    let empty_subst = TypeSubstitution::new();
    let result = instantiate_type(&interner, type_param, &empty_subst);
    assert_eq!(result, type_param);

    // With substitution T = string
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    let result = instantiate_type(&interner, type_param, &subst);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_instantiate_array() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Array<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let array_t = interner.array(type_param_t);

    // Substitute T = number -> Array<number>
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, array_t, &subst);

    // Result should be Array<number>
    let expected = interner.array(TypeId::NUMBER);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_union() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create T | null
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let union = interner.union(vec![type_param_t, TypeId::NULL]);

    // Substitute T = string -> string | null
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    let result = instantiate_type(&interner, union, &subst);

    // Result should be string | null
    let expected = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_object() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create { value: T }
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: type_param_t,
        write_type: type_param_t,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Substitute T = number -> { value: number }
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, obj, &subst);

    // Result should be { value: number }
    let expected = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_function() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create (x: T) => T
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: type_param_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: type_param_t,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Substitute T = string -> (x: string) => string
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    let result = instantiate_type(&interner, func, &subst);

    // Result should be (x: string) => string
    let expected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_function_shadowed_type_params() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));
    let func = interner.function(FunctionShape {
        type_params: vec![t_param.clone()],
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

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    let result = instantiate_type(&interner, func, &subst);

    let expected = interner.function(FunctionShape {
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
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_tuple() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create [T, U]
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let type_param_u = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: type_param_t,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: type_param_u,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Substitute T = string, U = number -> [string, number]
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    subst.insert(u_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, tuple, &subst);

    // Result should be [string, number]
    let expected = interner.tuple(vec![
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
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_generic_convenience() {
    let interner = TypeInterner::new();

    // Create Array<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let array_t = interner.array(type_param_t);

    // Use convenience function
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }];
    let type_args = vec![TypeId::STRING];

    let result = instantiate_generic(&interner, array_t, &type_params, &type_args);

    // Result should be Array<string>
    let expected = interner.array(TypeId::STRING);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_nested() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Array<Array<T>>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let inner_array = interner.array(type_param_t);
    let outer_array = interner.array(inner_array);

    // Substitute T = number -> Array<Array<number>>
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, outer_array, &subst);

    // Result should be Array<Array<number>>
    let inner_expected = interner.array(TypeId::NUMBER);
    let expected = interner.array(inner_expected);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_application_promise() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));

    let promise_base = interner.reference(SymbolRef(1));
    let promise_t = interner.application(promise_base, vec![t_type]);

    let result = instantiate_generic(&interner, promise_t, &[t_param], &[TypeId::STRING]);
    let expected = interner.application(promise_base, vec![TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_application_map_nested() {
    let interner = TypeInterner::new();

    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
    };
    let v_param = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
    };
    let k_type = interner.intern(TypeKey::TypeParameter(k_param.clone()));
    let v_type = interner.intern(TypeKey::TypeParameter(v_param.clone()));
    let array_v = interner.array(v_type);

    let map_base = interner.reference(SymbolRef(2));
    let map_kv = interner.application(map_base, vec![k_type, array_v]);

    let result = instantiate_generic(
        &interner,
        map_kv,
        &[k_param, v_param],
        &[TypeId::STRING, TypeId::NUMBER],
    );
    let expected = interner.application(
        map_base,
        vec![TypeId::STRING, interner.array(TypeId::NUMBER)],
    );
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_intrinsics_unchanged() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Intrinsics should not be affected by substitution
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);

    assert_eq!(
        instantiate_type(&interner, TypeId::STRING, &subst),
        TypeId::STRING
    );
    assert_eq!(
        instantiate_type(&interner, TypeId::NUMBER, &subst),
        TypeId::NUMBER
    );
    assert_eq!(
        instantiate_type(&interner, TypeId::BOOLEAN, &subst),
        TypeId::BOOLEAN
    );
    assert_eq!(
        instantiate_type(&interner, TypeId::NULL, &subst),
        TypeId::NULL
    );
    assert_eq!(
        instantiate_type(&interner, TypeId::UNDEFINED, &subst),
        TypeId::UNDEFINED
    );
}

#[test]
fn test_instantiate_conditional() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create T extends string ? T : never
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let cond = interner.conditional(ConditionalType {
        check_type: type_param_t,
        extends_type: TypeId::STRING,
        true_type: type_param_t,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Substitute T = "hello" (a string literal)
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, cond, &subst);

    // Result should be "hello" extends string ? "hello" : never
    let expected = interner.conditional(ConditionalType {
        check_type: hello_lit,
        extends_type: TypeId::STRING,
        true_type: hello_lit,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_mapped_type_shadowed_param() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));

    let mapped = interner.mapped(MappedType {
        type_param: t_param.clone(),
        constraint: TypeId::STRING,
        name_type: None,
        template: t_type,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, mapped, &subst);

    let expected = interner.mapped(MappedType {
        type_param: t_param,
        constraint: TypeId::STRING,
        name_type: None,
        template: t_type,
        readonly_modifier: None,
        optional_modifier: None,
    });
    assert_eq!(result, expected);
}

#[test]
fn test_instantiation_depth_limit_returns_error() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let mut deep_type = type_param_t;
    let limit = (MAX_INSTANTIATION_DEPTH + 5) as usize;
    for _ in 0..limit {
        deep_type = interner.array(deep_type);
    }

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, deep_type, &subst);

    assert_eq!(result, TypeId::ERROR);
}
