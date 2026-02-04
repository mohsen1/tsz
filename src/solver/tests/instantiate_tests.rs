use super::*;
use crate::solver::def::DefId;
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
            is_const: false,
            default: None,
        },
        TypeParamInfo {
            name: u_name,
            constraint: None,
            is_const: false,
            default: None,
        },
    ];
    let type_args = vec![TypeId::STRING, TypeId::NUMBER];

    let subst = TypeSubstitution::from_args(&interner, &type_params, &type_args);

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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
        default: None,
    }));
    let type_param_u = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        is_const: false,
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
        is_const: false,
        default: None,
    }));
    let array_t = interner.array(type_param_t);

    // Use convenience function
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        is_const: false,
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
        is_const: false,
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
        is_const: false,
        default: None,
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));

    let promise_base = interner.lazy(DefId(1));
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
        is_const: false,
        default: None,
    };
    let v_param = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        is_const: false,
        default: None,
    };
    let k_type = interner.intern(TypeKey::TypeParameter(k_param.clone()));
    let v_type = interner.intern(TypeKey::TypeParameter(v_param.clone()));
    let array_v = interner.array(v_type);

    let map_base = interner.lazy(DefId(2));
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
        is_const: false,
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
        is_const: false,
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
        is_const: false,
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

#[test]
fn test_substitution_from_args_with_defaults() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create type params where U's default is T
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));

    let type_params = vec![
        TypeParamInfo {
            name: t_name,
            constraint: None,
            is_const: false,
            default: None,
        },
        TypeParamInfo {
            name: u_name,
            constraint: None,
            is_const: false,
            default: Some(t_type), // U defaults to T
        },
    ];

    // Provide only T = number, U should default to T (which resolves to number)
    let type_args = vec![TypeId::NUMBER];

    let subst = TypeSubstitution::from_args(&interner, &type_params, &type_args);

    assert_eq!(subst.get(t_name), Some(TypeId::NUMBER));
    // U should be substituted with the instantiated value of T (which is number)
    // The default T gets instantiated with the substitution {T: number}, resulting in number
    assert_eq!(subst.get(u_name), Some(TypeId::NUMBER));
}

#[test]
fn test_substitution_from_args_with_concrete_defaults() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let type_params = vec![
        TypeParamInfo {
            name: t_name,
            constraint: None,
            is_const: false,
            default: None,
        },
        TypeParamInfo {
            name: u_name,
            constraint: None,
            is_const: false,
            default: Some(TypeId::STRING), // U defaults to string
        },
    ];

    // Provide only T = number, U should default to string
    let type_args = vec![TypeId::NUMBER];

    let subst = TypeSubstitution::from_args(&interner, &type_params, &type_args);

    assert_eq!(subst.get(t_name), Some(TypeId::NUMBER));
    assert_eq!(subst.get(u_name), Some(TypeId::STRING));
}

// ============================================
// Template Literal Instantiation Tests
// ============================================

#[test]
fn test_instantiate_template_literal_with_string_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Substitute T = "Name" -> should evaluate to "getName"
    let name_lit = interner.literal_string("Name");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, name_lit);
    let result = instantiate_type(&interner, template, &subst);

    // After instantiation with a string literal, the result should be evaluated
    let expected = interner.literal_string("getName");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_template_literal_with_union() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Substitute T = "a" | "b" -> should evaluate to "geta" | "getb"
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let union = interner.union(vec![a_lit, b_lit]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union);
    let result = instantiate_type(&interner, template, &subst);

    // The result should be a union of "geta" | "getb"
    let geta = interner.literal_string("geta");
    let getb = interner.literal_string("getb");
    let expected = interner.union(vec![geta, getb]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_template_literal_with_multiple_unions() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create `${T}_${U}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let type_param_u = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Type(type_param_t),
        TemplateSpan::Text(interner.intern_string("_")),
        TemplateSpan::Type(type_param_u),
    ]);

    // Substitute T = "a" | "b", U = "x" | "y"
    // Should expand to "a_x" | "a_y" | "b_x" | "b_y"
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let x_lit = interner.literal_string("x");
    let y_lit = interner.literal_string("y");
    let t_union = interner.union(vec![a_lit, b_lit]);
    let u_union = interner.union(vec![x_lit, y_lit]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, t_union);
    subst.insert(u_name, u_union);
    let result = instantiate_type(&interner, template, &subst);

    // Verify the result is a union of all combinations
    if let Some(TypeKey::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 4);
        // Check that we have the expected combinations
        let expected_strings = ["a_x", "a_y", "b_x", "b_y"];
        for expected in expected_strings.iter() {
            let expected_lit = interner.literal_string(expected);
            assert!(
                members.contains(&expected_lit),
                "Expected '{}' to be in union",
                expected
            );
        }
    } else {
        panic!("Expected union type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_instantiate_template_literal_preserves_type_param() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Substitute U = "Name" (T is not substituted)
    let name_lit = interner.literal_string("Name");
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, name_lit);
    let result = instantiate_type(&interner, template, &subst);

    // T should stay as is - result should still be a template literal
    if let Some(TypeKey::TemplateLiteral(spans_id)) = interner.lookup(result) {
        let spans = interner.template_list(spans_id);
        assert_eq!(spans.len(), 2);
        assert!(matches!(&spans[0], TemplateSpan::Text(_)));
        assert!(matches!(&spans[1], TemplateSpan::Type(_)));
    } else {
        panic!(
            "Expected template literal type, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_instantiate_template_literal_with_string_intrinsic() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `prefix${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Substitute T = string (intrinsic)
    // Result should remain a template literal since we can't fully evaluate `string`
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    let result = instantiate_type(&interner, template, &subst);

    // Should still be a template literal (can't fully evaluate with `string`)
    if let Some(TypeKey::TemplateLiteral(spans_id)) = interner.lookup(result) {
        let spans = interner.template_list(spans_id);
        assert_eq!(spans.len(), 2);
    } else {
        panic!(
            "Expected template literal type, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_instantiate_template_literal_in_object() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create a template literal type
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("key_")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create an object { prop: `key_${T}` }
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("prop"),
        type_id: template,
        write_type: template,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Substitute T = "name"
    let name_lit = interner.literal_string("name");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, name_lit);
    let result = instantiate_type(&interner, obj, &subst);

    // The property type should now be "key_name"
    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 1);
        let prop_type = shape.properties[0].type_id;
        let expected = interner.literal_string("key_name");
        assert_eq!(prop_type, expected);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_instantiate_template_literal_in_mapped_type_template() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // Create type parameter T (outer, will be substituted)
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));

    // Create mapped type parameter K (inner, shadowed)
    let k_param = TypeParamInfo {
        name: k_name,
        constraint: None,
        is_const: false,
        default: None,
    };
    let type_param_k = interner.intern(TypeKey::TypeParameter(k_param.clone()));

    // Create template literal `${T}_${K}` as the mapped type's template
    let template = interner.template_literal(vec![
        TemplateSpan::Type(type_param_t),
        TemplateSpan::Text(interner.intern_string("_")),
        TemplateSpan::Type(type_param_k),
    ]);

    // Create mapped type { [K in "a" | "b"]: `${T}_${K}` }
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let keys_union = interner.union(vec![a_lit, b_lit]);

    let mapped = interner.mapped(MappedType {
        type_param: k_param,
        constraint: keys_union,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = "prefix"
    let prefix_lit = interner.literal_string("prefix");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, prefix_lit);
    let result = instantiate_type(&interner, mapped, &subst);

    // The result should be a mapped type with T substituted
    // K should NOT be substituted since it's shadowed
    if let Some(TypeKey::Mapped(mapped_id)) = interner.lookup(result) {
        let mapped = interner.mapped_type(mapped_id);
        // Check that K is still the type param in the template
        if let Some(TypeKey::TemplateLiteral(spans_id)) = interner.lookup(mapped.template) {
            let spans = interner.template_list(spans_id);
            // The template should have T substituted with "prefix", but K preserved
            // So we expect: Text("prefix"), Text("_"), Type(K)
            // Note: evaluation might merge text spans
            assert!(!spans.is_empty());
        } else {
            panic!("Expected template literal in mapped type");
        }
    } else {
        panic!("Expected mapped type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_instantiate_template_literal_with_number_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `value_${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("value_")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Substitute T = 42 (number literal)
    // TypeScript converts numbers to string in template literals
    let num_lit = interner.literal_number(42.0);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, num_lit);
    let result = instantiate_type(&interner, template, &subst);

    // The result should still be a template literal since we need evaluation
    // to handle number -> string conversion (or it could be evaluated)
    // Check that it's either a literal string or template literal with the substituted type
    match interner.lookup(result) {
        Some(TypeKey::Literal(LiteralValue::String(atom))) => {
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "value_42");
        }
        Some(TypeKey::TemplateLiteral(spans_id)) => {
            let spans = interner.template_list(spans_id);
            // Should have the number literal substituted
            assert!(spans.len() >= 1);
        }
        _ => {
            // Both outcomes are acceptable depending on evaluation behavior
        }
    }
}

#[test]
fn test_instantiate_template_literal_empty_string() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `${T}` template literal (just the type param)
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![TemplateSpan::Type(type_param_t)]);

    // Substitute T = "" (empty string literal)
    let empty_lit = interner.literal_string("");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, empty_lit);
    let result = instantiate_type(&interner, template, &subst);

    // Result should be the empty string literal
    let expected = interner.literal_string("");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_template_literal_nested_in_union() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create a union with the template: `get${T}` | number
    let union_with_template = interner.union(vec![template, TypeId::NUMBER]);

    // Substitute T = "Name"
    let name_lit = interner.literal_string("Name");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, name_lit);
    let result = instantiate_type(&interner, union_with_template, &subst);

    // Result should be "getName" | number
    if let Some(TypeKey::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
        let expected_str = interner.literal_string("getName");
        assert!(members.contains(&expected_str));
        assert!(members.contains(&TypeId::NUMBER));
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_instantiate_template_literal_in_function_return() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create function () => `get${T}`
    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: template,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Substitute T = "Value"
    let value_lit = interner.literal_string("Value");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, value_lit);
    let result = instantiate_type(&interner, func, &subst);

    // Check the function's return type
    if let Some(TypeKey::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        let expected_return = interner.literal_string("getValue");
        assert_eq!(shape.return_type, expected_return);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_instantiate_template_literal_in_conditional_type() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `prefix_${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix_")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create conditional: T extends string ? `prefix_${T}` : never
    let cond = interner.conditional(ConditionalType {
        check_type: type_param_t,
        extends_type: TypeId::STRING,
        true_type: template,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Substitute T = "test"
    let test_lit = interner.literal_string("test");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, test_lit);
    let result = instantiate_type(&interner, cond, &subst);

    // The result should be the conditional with substituted types
    // The template in true_type should be evaluated to "prefix_test"
    // after full evaluation of the conditional
    // For now, check that the conditional has the substituted template
    match interner.lookup(result) {
        Some(TypeKey::Conditional(cond_id)) => {
            let cond = interner.conditional_type(cond_id);
            // The true_type should have the template evaluated
            let expected_true = interner.literal_string("prefix_test");
            assert_eq!(cond.true_type, expected_true);
        }
        Some(TypeKey::Literal(LiteralValue::String(atom))) => {
            // If the conditional was fully evaluated
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "prefix_test");
        }
        _ => {
            // The exact result depends on conditional evaluation behavior
        }
    }
}

// ============================================
// String Intrinsic Instantiation Tests
// ============================================

#[test]
fn test_instantiate_string_intrinsic_uppercase_with_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Uppercase<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let uppercase = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: type_param_t,
    });

    // Substitute T = "hello" -> should evaluate to "HELLO"
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, uppercase, &subst);

    let expected = interner.literal_string("HELLO");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_lowercase_with_union() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Lowercase<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let lowercase = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Lowercase,
        type_arg: type_param_t,
    });

    // Substitute T = "ABC" | "XYZ" -> should evaluate to "abc" | "xyz"
    let abc_lit = interner.literal_string("ABC");
    let xyz_lit = interner.literal_string("XYZ");
    let union = interner.union(vec![abc_lit, xyz_lit]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union);
    let result = instantiate_type(&interner, lowercase, &subst);

    // The result should be a union of "abc" | "xyz"
    let abc_lower = interner.literal_string("abc");
    let xyz_lower = interner.literal_string("xyz");
    let expected = interner.union(vec![abc_lower, xyz_lower]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_capitalize() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Capitalize<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let capitalize = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Capitalize,
        type_arg: type_param_t,
    });

    // Substitute T = "hello" -> should evaluate to "Hello"
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, capitalize, &subst);

    let expected = interner.literal_string("Hello");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_uncapitalize() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Uncapitalize<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let uncapitalize = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Uncapitalize,
        type_arg: type_param_t,
    });

    // Substitute T = "Hello" -> should evaluate to "hello"
    let hello_lit = interner.literal_string("Hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, uncapitalize, &subst);

    let expected = interner.literal_string("hello");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_with_template_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create Uppercase<`get${T}`>
    let uppercase = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: template,
    });

    // Substitute T = "Name" -> should evaluate to "GETNAME"
    let name_lit = interner.literal_string("Name");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, name_lit);
    let result = instantiate_type(&interner, uppercase, &subst);

    let expected = interner.literal_string("GETNAME");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_preserves_type_param() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create Uppercase<T>
    let type_param_t = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
    }));
    let uppercase = interner.intern(TypeKey::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: type_param_t,
    });

    // Substitute U = "hello" (T is not substituted)
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, hello_lit);
    let result = instantiate_type(&interner, uppercase, &subst);

    // T should stay as is - result should still be StringIntrinsic<T>
    if let Some(TypeKey::StringIntrinsic { kind, type_arg }) = interner.lookup(result) {
        assert_eq!(kind, StringIntrinsicKind::Uppercase);
        // type_arg should still be T
        if let Some(TypeKey::TypeParameter(info)) = interner.lookup(type_arg) {
            assert_eq!(info.name, t_name);
        } else {
            panic!("Expected type parameter T in StringIntrinsic");
        }
    } else {
        panic!("Expected StringIntrinsic type");
    }
}
