#[test]
fn test_conditional_infer_readonly_array_element_non_array_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly (infer R)[] ? R : never, with T = readonly string[] | number.
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
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_array, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_readonly_tuple_element_extraction() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly [infer R] ? R : never, with T = readonly [string] | readonly [number].
    let extends_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        }])));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }])));
    let readonly_number_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }])));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_tuple, readonly_number_tuple]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_readonly_tuple_element_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly [infer R] ? R : never, with T = readonly [string] | readonly [number] (no distribution).
    let extends_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        }])));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }])));
    let readonly_number_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }])));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_tuple, readonly_number_tuple]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_readonly_tuple_element_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly [infer R] ? R : never, with T = readonly [string] | number (no distribution).
    let extends_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        }])));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }])));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_tuple, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_readonly_tuple_element_non_tuple_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly [infer R] ? R : never, with T = readonly [string] | number.
    let extends_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        }])));
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let readonly_string_tuple =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }])));
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_tuple, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_readonly_array_mixed_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends readonly (infer R)[] ? R : never, with T = readonly string[] | number[].
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
    let number_array = interner.array(TypeId::NUMBER);
    subst.insert(
        t_name,
        interner.union(vec![readonly_string_array, number_array]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_instantiated_param_tuple_wrapper_no_distribution() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let tuple_check = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_extends = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // [T] extends [string] ? true : false, with T = string | number (no distribution).
    let cond = ConditionalType {
        check_type: tuple_check,
        extends_type: tuple_extends,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, lit_false);
}

#[test]
fn test_conditional_any_produces_union() {
    let interner = TypeInterner::new();

    // any extends string ? number : boolean
    // any produces union of branches
    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_any_error_poisoning() {
    let interner = TypeInterner::new();

    // any extends string ? error : number
    // any produces union of branches, which should poison to error.
    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: TypeId::ERROR,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn test_conditional_distributive_never() {
    let interner = TypeInterner::new();

    // T extends string ? number : boolean, with T = never (distributive)
    // Distributes over empty union -> never
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_deferred_type_parameter() {
    let interner = TypeInterner::new();

    // T extends string ? number : boolean
    // Should remain deferred when T is an unsubstituted type parameter
    let type_param_t = test_type_param(&interner, "T").1;

    let cond = ConditionalType {
        check_type: type_param_t,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let result = evaluate_conditional(&interner, &cond);

    // Should return the same conditional (deferred)
    assert_eq!(result, cond_type);
}

#[test]
fn test_conditional_deferred_type_parameter_with_constraint() {
    let interner = TypeInterner::new();

    // T extends string ? number : boolean
    // where T has constraint `string`
    // Should remain deferred even when constraint satisfies extends type.
    // tsc does NOT eagerly resolve conditionals based on constraint alone —
    // the type parameter could be instantiated with different subtypes.
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let cond = ConditionalType {
        check_type: type_param_t,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let result = evaluate_conditional(&interner, &cond);

    // Should return the same conditional (deferred), NOT eagerly resolve to NUMBER
    assert_eq!(result, cond_type);
}

#[test]
fn test_conditional_deferred_type_parameter_constraint_not_satisfying() {
    let interner = TypeInterner::new();

    // T extends number ? "yes" : "no"
    // where T has constraint `string` (disjoint from number)
    // Should remain deferred — tsc does not eagerly resolve to false branch.
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: type_param_t,
        extends_type: TypeId::NUMBER,
        true_type: yes,
        false_type: no,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let result = evaluate_conditional(&interner, &cond);

    // Should return the same conditional (deferred), NOT eagerly resolve to "no"
    assert_eq!(result, cond_type);
}

#[test]
fn test_conditional_infer_direct_match() {
    let interner = TypeInterner::new();

    let (_r_name, infer_r) = test_infer_param(&interner, "R");

    // string extends infer R ? R : never -> string
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: infer_r,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_constraint_mismatch() {
    let interner = TypeInterner::new();

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));
    let no = interner.literal_string("no");

    // string extends infer R extends number ? R : "no" -> "no"
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: infer_r,
        true_type: infer_r,
        false_type: no,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, no);
}

#[test]
fn test_conditional_distributive_infer_array_extends() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_r_name, infer_r) = test_infer_param(&interner, "R");

    // T extends Array<infer R> ? R : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: interner.array(infer_r),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let union_arrays = interner.union(vec![string_array, number_array]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union_arrays);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_nested_distributive_infer() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_r_name, infer_r) = test_infer_param(&interner, "R");

    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");
    let outer_no = interner.literal_string("outer-no");

    let inner_cond = interner.conditional(ConditionalType {
        check_type: infer_r,
        extends_type: TypeId::STRING,
        true_type: yes,
        false_type: no,
        is_distributive: false,
    });

    // T extends infer R ? (R extends string ? "yes" : "no") : "outer-no"
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: infer_r,
        true_type: inner_cond,
        false_type: outer_no,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![yes, no]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_property() {
    let interner = TypeInterner::new();

    let (_r_name, infer_r) = test_infer_param(&interner, "R");

    let prop_name = interner.intern_string("a");
    let source = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);
    let pattern = interner.object(vec![PropertyInfo::new(prop_name, infer_r)]);

    // { a: string } extends { a: infer R } ? R : never -> string
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_object_string_index_signature() {
    let interner = TypeInterner::new();

    let (_r_name, infer_r) = test_infer_param(&interner, "R");

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let pattern = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // { [key: string]: number } extends { [key: string]: infer R } ? R : never -> number
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_object_literal() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["x"] -> number
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_x = interner.literal_string("x");

    let result = evaluate_index_access(&interner, obj, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_object_string_key() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["y"] -> string
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_y = interner.literal_string("y");

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_object_missing_key() {
    let interner = TypeInterner::new();

    // { x: number }["z"] -> undefined
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let key_z = interner.literal_string("z");

    let result = evaluate_index_access(&interner, obj, key_z);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_object_union_key() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["x" | "y"] -> number | string
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let result = evaluate_index_access(&interner, obj, key_union);

    // Should be number | string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_union_object_literal_key() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");

    let result = evaluate_index_access(&interner, union_obj, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_union_object_union_key() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let result = evaluate_index_access(&interner, union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_correlated_union_index_access_cross_product() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_a = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("a")),
        PropertyInfo::new(key_a, TypeId::NUMBER),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("b")),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let result = evaluate_index_access(&interner, union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_index_union_of_generic_key_intersections_preserves_key() {
    let interner = TypeInterner::new();

    let mapped_key_name = interner.intern_string("P");
    let generic_key_name = interner.intern_string("K");
    let one = interner.literal_string("one");
    let two = interner.literal_string("two");
    let key_space = interner.union(vec![one, two]);

    let mapped_key = interner.type_param(TypeParamInfo {
        name: mapped_key_name,
        constraint: Some(key_space),
        default: None,
        is_const: false,
    });
    let generic_key = interner.type_param(TypeParamInfo {
        name: generic_key_name,
        constraint: Some(key_space),
        default: None,
        is_const: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: mapped_key_name,
            constraint: Some(key_space),
            default: None,
            is_const: false,
        },
        constraint: key_space,
        template: mapped_key,
        name_type: None,
        optional_modifier: None,
        readonly_modifier: None,
    });

    let index_type = interner.union(vec![
        interner.intersection2(generic_key, one),
        interner.intersection2(generic_key, two),
    ]);

    let result = evaluate_index_access(&interner, mapped, index_type);
    assert_eq!(
        result,
        generic_key,
        "a union covering every constrained `K & key` member should index mapped templates as K, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_index_access_union_object_union_key_no_unchecked() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);
    let result = evaluator.evaluate_index_access(union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_union_object_literal_key_no_unchecked() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(union_obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.literal_string("y");

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(key_x, TypeId::STRING)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_x_literal = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x_literal);
    assert_eq!(result, TypeId::STRING);

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::NUMBER);

    let result = evaluate_index_access(&interner, obj, TypeId::STRING);
    assert_eq!(result, TypeId::NUMBER);

    let key_union = interner.union(vec![key_x_literal, key_y]);
    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature_optional_property() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.literal_string("y");

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(key_x, TypeId::NUMBER)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_x_literal = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x_literal);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::BOOLEAN);

    let key_union = interner.union(vec![key_x_literal, key_y]);
    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature_optional_property_no_unchecked() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let key_x = interner.literal_string("x");
    let result = evaluator.evaluate_index_access(obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let key_y = interner.literal_string("y");
    let result = evaluator.evaluate_index_access(obj, key_y);
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluator.evaluate_index_access(obj, TypeId::STRING);
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let key_union = interner.union(vec![key_x, key_y]);
    let result = evaluator.evaluate_index_access(obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_no_unchecked_object_index_signature_evaluate() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(obj, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_number_index_signature() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_index_access(&interner, obj, TypeId::NUMBER);
    assert_eq!(result, TypeId::BOOLEAN);

    let one = interner.literal_number(1.0);
    let result = evaluate_index_access(&interner, obj, one);
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_index_access_object_with_number_index_signature_no_unchecked() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(obj, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let zero = interner.literal_number(0.0);
    let result = evaluator.evaluate_index_access(obj, zero);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let zero_str = interner.literal_string("0");
    let result = evaluator.evaluate_index_access(obj, zero_str);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_resolves_ref() {
    use crate::def::DefId;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let def_id = DefId(1);
    env.insert_def(def_id, obj);

    let ref_type = interner.lazy(def_id);
    let key_x = interner.literal_string("x");

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate_index_access(ref_type, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_type_param_constraint() {
    let interner = TypeInterner::new();

    let constraint = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, type_param, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_type_param_no_constraint_deferred() {
    let interner = TypeInterner::new();

    let type_param = test_type_param(&interner, "T").1;

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, type_param, key_x);

    match interner.lookup(result) {
        Some(TypeData::IndexAccess(obj, idx)) => {
            assert_eq!(obj, type_param);
            assert_eq!(idx, key_x);
        }
        other => panic!("Expected deferred IndexAccess, got {other:?}"),
    }
}

#[test]
fn test_index_access_optional_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_any_is_any() {
    let interner = TypeInterner::new();

    let result = evaluate_index_access(&interner, TypeId::ANY, TypeId::STRING);
    assert_eq!(result, TypeId::ANY);

    let result = evaluate_index_access(&interner, TypeId::NUMBER, TypeId::ANY);
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_index_access_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let array = interner.array(TypeId::STRING);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(indexed, TypeId::STRING);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluator.evaluate_index_access(array, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_with_options_helper_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluate_index_access_with_options(&interner, indexed, TypeId::STRING, true);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array_literal_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let zero = interner.literal_number(0.0);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(array, zero);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array() {
    let interner = TypeInterner::new();

    // string[][number] -> string
    let string_array = interner.array(TypeId::STRING);

    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_no_unchecked_indexed_access_array_union_key() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let length_key = interner.literal_string("length");
    let key_union = interner.union(vec![TypeId::NUMBER, length_key]);

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_index_access(string_array, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);

    evaluator.set_no_unchecked_indexed_access(true);
    let result = evaluator.evaluate_index_access(string_array, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array_string_index() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    // tsc: Array<T>[string] returns T (the element type) because the
    // numeric index signature (returning T) is available under string
    // indexing.  String keys subsume numeric keys.
    let result = evaluate_index_access(&interner, string_array, TypeId::STRING);
    assert_eq!(
        result,
        TypeId::STRING,
        "string[][string] should be string (element type)"
    );
}

#[test]
fn test_index_access_array_string_index_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    // tsc: Array<T>[string] returns T | undefined with noUncheckedIndexedAccess.
    let result = evaluator.evaluate_index_access(string_array, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for array[string] with noUncheckedIndexedAccess");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(
                members.contains(&TypeId::STRING),
                "should contain STRING (element type)"
            );
            assert!(
                members.contains(&TypeId::UNDEFINED),
                "should contain UNDEFINED (noUncheckedIndexedAccess)"
            );
        }
        other => panic!("Expected union (string | undefined), got {other:?}"),
    }
}

#[test]
fn test_index_access_array_string_literal_length() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let length_key = interner.literal_string("length");

    let result = evaluate_index_access(&interner, string_array, length_key);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_array_string_literal_method() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let includes_key = interner.literal_string("includes");

    let result = evaluate_index_access(&interner, string_array, includes_key);
    match interner.lookup(result) {
        Some(TypeData::Function(func_id)) => {
            let func = interner.function_shape(func_id);
            assert_eq!(func.return_type, TypeId::BOOLEAN);
            assert_eq!(func.params.len(), 1);
            assert!(func.params[0].rest);
        }
        other => panic!("Expected function type, got {other:?}"),
    }
}

#[test]
fn test_index_access_array_string_literal_numeric_key_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let zero = interner.literal_string("0");

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(string_array, zero);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_readonly_array() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(array));

    let result = evaluate_index_access(&interner, readonly_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_tuple_literal() {
    let interner = TypeInterner::new();

    // [string, number][0] -> string
    let tuple = interner.tuple(vec![
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
    let zero = interner.literal_number(0.0);

    let result = evaluate_index_access(&interner, tuple, zero);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_tuple_rest_array_literal() {
    let interner = TypeInterner::new();

    // [string, ...number[]][1] -> number
    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    assert_eq!(evaluate_index_access(&interner, tuple, one), TypeId::NUMBER);
    assert_eq!(evaluate_index_access(&interner, tuple, two), TypeId::NUMBER);
}

#[test]
fn test_index_access_tuple_rest_tuple_literal() {
    let interner = TypeInterner::new();

    // [string, ...[number, boolean]][1] -> number
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
    let tuple = interner.tuple(vec![
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

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    assert_eq!(evaluate_index_access(&interner, tuple, one), TypeId::NUMBER);
    assert_eq!(
        evaluate_index_access(&interner, tuple, two),
        TypeId::BOOLEAN
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, three),
        TypeId::UNDEFINED
    );
}

#[test]
fn test_index_access_tuple_optional_literal() {
    let interner = TypeInterner::new();

    // [string, number?][1] -> number | undefined
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);
    let one = interner.literal_number(1.0);

    let result = evaluate_index_access(&interner, tuple, one);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_tuple_negative_literal() {
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let negative = interner.literal_number(-1.0);

    let result = evaluate_index_access(&interner, tuple, negative);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_fractional_literal() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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
    let fractional = interner.literal_number(1.5);

    let result = evaluate_index_access(&interner, tuple, fractional);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_negative_string_literal() {
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let negative = interner.literal_string("-1");

    let result = evaluate_index_access(&interner, tuple, negative);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_fractional_string_literal() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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
    let fractional = interner.literal_string("1.5");

    let result = evaluate_index_access(&interner, tuple, fractional);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_string_index() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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
    let map_key = interner.literal_string("map");
    let map_type = evaluate_index_access(&interner, tuple, map_key);

    let result = evaluate_index_access(&interner, tuple, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for tuple[string]");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&map_type));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_index_access_tuple_string_index_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(tuple, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for tuple[string]");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}
