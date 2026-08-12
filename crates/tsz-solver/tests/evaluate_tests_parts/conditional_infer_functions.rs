#[test]
fn test_conditional_infer_object_call_signature_optional_param_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { (x?: infer R): void } ? R : never, with T = { (x?: string): void }
    // | { (x?: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_optional_param_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ (x?: infer R): void }] ? R : never, with T = { (x?: string): void }
    // | { (x?: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in call signature optional parameter from union source
    let expected = TypeId::NEVER;
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_rest_param_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { (...args: infer R): void } ? R : never, with T = { (...args: string[]): void }
    // | { (...args: number[]): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::STRING),
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::NUMBER),
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_rest_param_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ (...args: infer R): void }] ? R : never, with T = { (...args: string[]): void }
    // | { (...args: number[]): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::STRING),
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::NUMBER),
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in call signature rest parameter from union source
    let expected = interner.intersection(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_non_callable_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { (x: infer R): void } ? R : never, with T = { (x: string): void } | number.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_object_call_signature_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ (x: infer R): void }] ? R : never, with T = { (x: string): void } | number.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_object_call_signature_overload_source_non_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ (x: infer R): void }] ? R : never, with T = { (x: string): void; (x: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let overload_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::STRING)],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                type_params: Vec::new(),
                is_method: false,
            },
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                type_params: Vec::new(),
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(t_name, overload_callable);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_infer_object_property_non_distributive_union_all_match() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ a: infer R }] ? R : never, with T = { a: string } | { a: number }.
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_obj,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_property_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ a: infer R }] ? R : never, with T = { a: string } | number.
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_obj,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_match, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_tuple_element_extraction() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R] ? R : never, with T = [string] | [number].
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.tuple(vec![TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            }]),
            interner.tuple(vec![TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            }]),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_tuple_optional_element_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R?] ? R : never, with T = [string] | [].
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: true,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let empty_tuple = interner.tuple(Vec::new());
    subst.insert(t_name, interner.union(vec![string_tuple, empty_tuple]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_tuple_optional_element_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R?] ? R : never, with T = [string] | [] (no distribution).
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: true,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let empty_tuple = interner.tuple(Vec::new());
    subst.insert(t_name, interner.union(vec![string_tuple, empty_tuple]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_tuple_optional_element_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R?] ? R : never, with T = [string] | number (no distribution).
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: true,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, interner.union(vec![string_tuple, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_tuple_element_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R] ? R : never, with T = [string] | [number] (no distribution).
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.tuple(vec![TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            }]),
            interner.tuple(vec![TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            }]),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_tuple_element_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R] ? R : never, with T = [string] | number (no distribution).
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, interner.union(vec![tuple_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_tuple_element_non_tuple_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer R] ? R : never, with T = [string] | number.
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, interner.union(vec![tuple_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_tuple_element_with_constraint() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));

    // T extends [infer R extends string] ? R : never, with T = [number] | [string].
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.tuple(vec![TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            }]),
            interner.tuple(vec![TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            }]),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_optional_tuple_element_with_constraint() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));

    // T extends [infer R extends string] ? R : never, with T = [string?] | [number?].
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: infer_r,
        name: None,
        optional: true,
        rest: false,
    }]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.tuple(vec![TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: true,
                rest: false,
            }]),
            interner.tuple(vec![TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: true,
                rest: false,
            }]),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_tuple_rest_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [string, ...infer R] ? R : never, with T = [string, number] | [string].
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let tuple_string_number = interner.tuple(vec![
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
    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(
        t_name,
        interner.union(vec![tuple_string_number, tuple_string]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }]),
        interner.tuple(Vec::new()),
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_tuple_rest_with_head_infer_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_h_name, infer_h) = test_infer_param(&interner, "H");
    let (_infer_r_name, infer_r) = test_infer_param(&interner, "R");

    // T extends [infer H, ...infer R] ? R : never, with T = [string, number] | [boolean].
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_h,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let tuple_string_number = interner.tuple(vec![
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
    let tuple_boolean = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(
        t_name,
        interner.union(vec![tuple_string_number, tuple_boolean]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }]),
        interner.tuple(Vec::new()),
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_union_true_branch_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends string ? R | number : never, with T = string | boolean.
    // Infer appears only in the true branch; ensure it is preserved.
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: interner.union(vec![infer_r, TypeId::NUMBER]),
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, interner.union(vec![infer_r, TypeId::NUMBER]));
}

#[test]
fn test_conditional_infer_union_false_branch_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends string ? never : R | number, with T = string | boolean.
    // Infer appears only in the false branch; ensure it is preserved.
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: interner.union(vec![infer_r, TypeId::NUMBER]),
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, interner.union(vec![infer_r, TypeId::NUMBER]));
}

#[test]
fn test_conditional_infer_any_check_type_distributive() {
    let interner = TypeInterner::new();

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // any extends string ? infer R : never
    // any produces union of branches; infer should survive in true branch.
    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, infer_r);
}

#[test]
fn test_conditional_infer_readonly_array_element_extraction() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly (infer R)[] ? R : never, with T = readonly string[] | readonly number[].
    let extends_array = interner.intern(TypeData::ReadonlyType(interner.array(infer_r)));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::STRING)));
    let readonly_number_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::NUMBER)));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_array, readonly_number_array]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_readonly_array_element_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly (infer R)[] ? R : never, with T = readonly string[] | readonly number[] (no distribution).
    let extends_array = interner.intern(TypeData::ReadonlyType(interner.array(infer_r)));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::STRING)));
    let readonly_number_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::NUMBER)));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_array, readonly_number_array]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_readonly_array_element_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly (infer R)[] ? R : never, with T = readonly string[] | number (no distribution).
    let extends_array = interner.intern(TypeData::ReadonlyType(interner.array(infer_r)));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::STRING)));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_array, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

/// Helper: build `(params...) => return_type` as a `Function` type.
fn make_fn(interner: &TypeInterner, params: Vec<ParamInfo>, return_type: TypeId) -> TypeId {
    interner.function(FunctionShape {
        params,
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

/// Regression (issue #14323, mined from type-zoo): a fixed-arity `infer`
/// function pattern (no trailing rest) must not match a higher-arity source.
///
/// ```ts
/// type ParamTypes<F> =
///   F extends (p0: infer P0) => any ? [P0]
///   : F extends (p0: infer P0, p1: infer P1) => any ? [P0, P1] : never;
/// // F = (a: string, b: number) => {}  =>  [string, number]
/// ```
///
/// `tsc` fails `(a, b) => {}` against the 1-arg pattern (the source demands
/// more arguments than the pattern supplies), so the conditional falls through
/// to the 2-arg pattern. Before the arity gate, tsz truncated the source to the
/// pattern prefix and wrongly picked `[string]`.
#[test]
fn test_conditional_infer_fixed_arity_rejects_higher_arity_source() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let (_p0, infer_p0) = test_infer_param(&interner, "P0");
    let (_p1, infer_p1) = test_infer_param(&interner, "P1");

    // Source: (a: string, b: number) => {}
    let source = make_fn(
        &interner,
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::required(b, TypeId::NUMBER),
        ],
        TypeId::VOID,
    );

    // Pattern 1: (p0: infer P0) => any  ->  [P0]
    let pattern1 = make_fn(&interner, vec![ParamInfo::required(a, infer_p0)], TypeId::ANY);
    let true1 = interner.tuple(vec![TupleElement::fixed(infer_p0)]);

    // Pattern 2: (p0: infer P0, p1: infer P1) => any  ->  [P0, P1]
    let pattern2 = make_fn(
        &interner,
        vec![
            ParamInfo::required(a, infer_p0),
            ParamInfo::required(b, infer_p1),
        ],
        TypeId::ANY,
    );
    let true2 = interner.tuple(vec![
        TupleElement::fixed(infer_p0),
        TupleElement::fixed(infer_p1),
    ]);

    let inner = interner.conditional(ConditionalType {
        check_type: source,
        extends_type: pattern2,
        true_type: true2,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let outer = ConditionalType {
        check_type: source,
        extends_type: pattern1,
        true_type: true1,
        false_type: inner,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer);
    let expected = interner.tuple(vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::fixed(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

/// Adjacent: an exact-arity source still matches the fixed-arity pattern.
#[test]
fn test_conditional_infer_fixed_arity_matches_exact_arity_source() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let (_p0, infer_p0) = test_infer_param(&interner, "P0");

    // Source: (a: string) => void
    let source = make_fn(&interner, vec![ParamInfo::required(a, TypeId::STRING)], TypeId::VOID);
    // Pattern: (p0: infer P0) => any -> P0
    let pattern = make_fn(&interner, vec![ParamInfo::required(a, infer_p0)], TypeId::ANY);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_p0,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(evaluate_conditional(&interner, &cond), TypeId::STRING);
}

/// Adjacent: a fixed-arity source with fewer params than the pattern still
/// matches (extra positions default to `unknown`), matching `tsc`.
#[test]
fn test_conditional_infer_fixed_arity_matches_lower_arity_source() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let (_p0, infer_p0) = test_infer_param(&interner, "P0");
    let (_p1, infer_p1) = test_infer_param(&interner, "P1");

    // Source: (a: string) => void
    let source = make_fn(&interner, vec![ParamInfo::required(a, TypeId::STRING)], TypeId::VOID);
    // Pattern: (p0: infer P0, p1: infer P1) => any -> [P0, P1]
    let pattern = make_fn(
        &interner,
        vec![
            ParamInfo::required(a, infer_p0),
            ParamInfo::required(b, infer_p1),
        ],
        TypeId::ANY,
    );
    let true_branch = interner.tuple(vec![
        TupleElement::fixed(infer_p0),
        TupleElement::fixed(infer_p1),
    ]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: true_branch,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.tuple(vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::fixed(TypeId::UNKNOWN),
    ]);
    assert_eq!(result, expected);
}

/// Adjacent: a trailing-rest pattern imposes no arity cap, so a higher-arity
/// source still matches `(...args: infer P) => any`.
#[test]
fn test_conditional_infer_rest_pattern_matches_higher_arity_source() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let args = interner.intern_string("args");
    let (_p, infer_p) = test_infer_param(&interner, "P");

    // Source: (a: string, b: number) => void
    let source = make_fn(
        &interner,
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::required(b, TypeId::NUMBER),
        ],
        TypeId::VOID,
    );
    // Pattern: (...args: infer P) => any -> P
    let pattern = make_fn(
        &interner,
        vec![ParamInfo {
            name: Some(args),
            type_id: infer_p,
            optional: false,
            rest: true,
arity_only_optional: false,
        }],
        TypeId::ANY,
    );

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    // The rest pattern absorbs both source params, so the true branch is taken
    // (`P` binds to the `[string, number]` parameter tuple). The exact tuple
    // reification (element names/readonly) is not what this case guards — only
    // that the arity cap does not fire for a trailing-rest pattern.
    let result = evaluate_conditional(&interner, &cond);
    assert_ne!(result, TypeId::NEVER);
    assert!(matches!(interner.lookup(result), Some(TypeData::Tuple(_))));
}

/// Adjacent: an optional trailing param does not count toward required arity,
/// so `(a, b?) => {}` still matches the 1-arg pattern.
#[test]
fn test_conditional_infer_fixed_arity_optional_trailing_param_matches() {
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let (_p0, infer_p0) = test_infer_param(&interner, "P0");

    // Source: (a: string, b?: number) => void  (1 required param)
    let source = make_fn(
        &interner,
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo {
                name: Some(b),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
arity_only_optional: false,
            },
        ],
        TypeId::VOID,
    );
    // Pattern: (p0: infer P0) => any -> P0
    let pattern = make_fn(&interner, vec![ParamInfo::required(a, infer_p0)], TypeId::ANY);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_p0,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(evaluate_conditional(&interner, &cond), TypeId::STRING);
}
