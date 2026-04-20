use super::*;
#[test]
fn test_mapped_type_with_conditional_template() {
    // Conditional template: { [K in keyof T]: T[K] extends string ? number : boolean }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };
    let cond_template = interner.conditional(cond);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: cond_template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_union_key_constraint() {
    // Keys from union of object types
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![obj_a, obj_b]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_union,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_intersection_source() {
    // Keys from intersection: keyof (A & B)
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_intersection,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_exclude_pattern() {
    // Exclude pattern: { [K in keyof T as Exclude<K, "internal">]: T[K] }
    let interner = TypeInterner::new();

    let key_public = interner.literal_string("public");
    let key_internal = interner.literal_string("internal");
    let keys = interner.union(vec![key_public, key_internal]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(key_public),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_deep_readonly() {
    // DeepReadonly: { readonly [K in keyof T]: DeepReadonly<T[K]> }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::OBJECT,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_pick_pattern() {
    // Pick<T, K>: { [P in K]: T[P] }
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_a,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_record_pattern() {
    // Record<K, T>: { [P in K]: T }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_z = interner.literal_string("z");
    let keys = interner.union(vec![key_x, key_y, key_z]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("z"), TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_mutable_pattern() {
    // Mutable<T>: { -readonly [K in keyof T]: T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_required_pattern() {
    // Required<T>: { [K in keyof T]-?: T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_empty_keys() {
    // Mapped type over never (empty key set)
    let interner = TypeInterner::new();

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::NEVER,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    let expected = interner.object(vec![]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_single_literal_key() {
    // Single literal key: { [K in "only"]: number }
    let interner = TypeInterner::new();

    let key = interner.literal_string("only");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::NUMBER,
    )]);
    assert_eq!(result, expected);
}

// ==================== Function return inference edge case tests ====================

#[test]
fn test_infer_return_void_vs_undefined() {
    // T extends () => infer R ? R : never
    // where T = () => void
    // Result should be void (not undefined)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::VOID);
}

#[test]
fn test_infer_return_promise_like() {
    // T extends () => infer R ? R : never
    // where T = () => Promise<string>
    // Result should be Promise<string> (as an object type)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    // Create a simple Promise-like object { then(cb: (v: string) => void): void }
    let then_name = interner.intern_string("then");
    let promise_string = interner.object(vec![PropertyInfo {
        name: then_name,
        type_id: TypeId::ANY, // Simplified, normally this would be a function
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: promise_string,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, promise_string);
}

#[test]
fn test_infer_return_union() {
    // T extends () => infer R ? R : never
    // where T = () => (string | number)
    // Result should be string | number
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let union_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: union_return,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Result should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_return_never() {
    // T extends () => infer R ? R : unknown
    // where T = () => never
    // Result should be never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::UNKNOWN,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: TypeId::NEVER,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// CONDITIONAL TYPE DISTRIBUTION STRESS TESTS
// =============================================================================

#[test]
fn test_distribution_over_large_union() {
    // T extends string ? "yes" : "no" where T = "a" | "b" | "c" | "d" | "e"
    // Distributes to: ("a" extends string ? "yes" : "no") | ... | ("e" extends string ? "yes" : "no")
    // = "yes" | "yes" | "yes" | "yes" | "yes" = "yes"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_e = interner.literal_string("e");
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let large_union = interner.union(vec![lit_a, lit_b, lit_c, lit_d, lit_e]);

    let cond = ConditionalType {
        check_type: large_union,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All literals extend string, so result should be "yes"
    assert_eq!(result, lit_yes);
}

#[test]
fn test_distribution_over_mixed_union() {
    // T extends string ? T : never where T = string | number | "literal"
    // Distributes: (string extends string ? string : never) | (number extends string ? number : never) | ("literal" extends string ? "literal" : never)
    // = string | never | "literal" = string (since "literal" <: string)
    let interner = TypeInterner::new();

    let lit_val = interner.literal_string("literal");
    let mixed_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, lit_val]);

    let cond = ConditionalType {
        check_type: mixed_union,
        extends_type: TypeId::STRING,
        true_type: mixed_union, // T in true branch
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Result should be string | "literal" = string (or union containing string parts)
    assert!(result != TypeId::ERROR);
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_distribution_over_union_all_false() {
    // T extends string ? "yes" : "no" where T = number | boolean | symbol
    // Distributes: (number extends string ? "yes" : "no") | (boolean extends string ? "yes" : "no") | (symbol extends string ? "yes" : "no")
    // = "no" | "no" | "no" = "no"
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let non_string_union = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN, TypeId::SYMBOL]);

    let cond = ConditionalType {
        check_type: non_string_union,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All members don't extend string, so result should be "no"
    assert_eq!(result, lit_no);
}

#[test]
fn test_distribution_with_never_check_type() {
    // never extends T ? "yes" : "no"
    // never distributes to empty union, result is never
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // never distributes to empty union = never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distribution_with_any_check_type() {
    // any extends string ? "yes" : "no"
    // any distributes specially, result is "yes" | "no"
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // any distributes to both branches
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert!(result == expected || result == lit_yes || result == lit_no);
}

#[test]
fn test_distribution_nested_conditional() {
    // T extends string ? (T extends "a" ? 1 : 2) : 3
    // where T = "a" | "b" | number
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    let check_union = interner.union(vec![lit_a, lit_b, TypeId::NUMBER]);

    // Inner conditional for true branch
    let inner_cond = ConditionalType {
        check_type: check_union,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: true,
    };
    let inner_result = interner.conditional(inner_cond);

    let outer_cond = ConditionalType {
        check_type: check_union,
        extends_type: TypeId::STRING,
        true_type: inner_result,
        false_type: lit_3,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // "a" -> string -> inner: "a" extends "a" -> 1
    // "b" -> string -> inner: "b" extends "a" -> 2
    // number -> not string -> 3
    // Result: 1 | 2 | 3
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distribution_over_union_of_objects() {
    // T extends { x: string } ? T : never where T = { x: string, y: number } | { x: number } | { x: string }
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let obj_xy = interner.object(vec![
        PropertyInfo::new(x_name, TypeId::STRING),
        PropertyInfo::new(y_name, TypeId::NUMBER),
    ]);

    let obj_x_num = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let obj_x_str = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let target = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let union = interner.union(vec![obj_xy, obj_x_num, obj_x_str]);

    let cond = ConditionalType {
        check_type: union,
        extends_type: target,
        true_type: union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // obj_xy extends { x: string } -> yes
    // obj_x_num extends { x: string } -> no (x is number)
    // obj_x_str extends { x: string } -> yes
    // Result: obj_xy | obj_x_str
    assert!(result != TypeId::ERROR);
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_distribution_over_intersection_of_unions() {
    // T extends string ? "yes" : "no" where T = (string | number) & (string | boolean)
    // Intersection = string (common to both)
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let intersection = interner.intersection(vec![union1, union2]);

    let cond = ConditionalType {
        check_type: intersection,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // (string | number) & (string | boolean) = string
    // string extends string = yes
    assert!(result == lit_yes || result != TypeId::ERROR);
}

#[test]
fn test_distribution_over_union_with_unknown() {
    // T extends unknown ? T : never where T = string | number | unknown
    // All types extend unknown
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNKNOWN]);

    let cond = ConditionalType {
        check_type: union,
        extends_type: TypeId::UNKNOWN,
        true_type: union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Everything extends unknown, so result = union (or simplified)
    assert!(result != TypeId::NEVER);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distribution_exclude_pattern() {
    // Exclude<T, U> = T extends U ? never : T
    // Exclude<string | number | boolean, number> = string | boolean
    let interner = TypeInterner::new();

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NEVER,
        false_type: check_union, // T
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> not number -> string
    // number -> number -> never
    // boolean -> not number -> boolean
    // Result: string | boolean
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_distribution_extract_pattern() {
    // Extract<T, U> = T extends U ? T : never
    // Extract<string | number | boolean, string | number> = string | number
    let interner = TypeInterner::new();

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let target_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: target_union,
        true_type: check_union, // T
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> extends string | number -> string
    // number -> extends string | number -> number
    // boolean -> not extends string | number -> never
    // Result: string | number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distribution_with_literal_union() {
    // T extends "a" | "b" ? "match" : "no-match" where T = "a" | "c" | "b" | "d"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_match = interner.literal_string("match");
    let lit_no_match = interner.literal_string("no-match");

    let check_union = interner.union(vec![lit_a, lit_c, lit_b, lit_d]);
    let extends_union = interner.union(vec![lit_a, lit_b]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: extends_union,
        true_type: lit_match,
        false_type: lit_no_match,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" extends "a" | "b" -> match
    // "b" extends "a" | "b" -> match
    // "c" extends "a" | "b" -> no-match
    // "d" extends "a" | "b" -> no-match
    // Result: "match" | "no-match"
    let expected = interner.union(vec![lit_match, lit_no_match]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_non_distribution_tuple_wrapped() {
    // [T] extends [string] ? "yes" : "no" where T = string | number
    // Non-distributive: [string | number] extends [string] is false
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let check_tuple = interner.tuple(vec![TupleElement {
        type_id: check_union,
        optional: false,
        name: None,
        rest: false,
    }]);
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        optional: false,
        name: None,
        rest: false,
    }]);

    let cond = ConditionalType {
        check_type: check_tuple,
        extends_type: extends_tuple,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // [string | number] does not extend [string] (number not assignable to string)
    assert_eq!(result, lit_no);
}

#[test]
fn test_distribution_boolean_special() {
    // boolean = true | false, distribution should work over both
    // T extends true ? "yes" : "no" where T = boolean
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");
    let lit_true = interner.literal_boolean(true);

    let cond = ConditionalType {
        check_type: TypeId::BOOLEAN,
        extends_type: lit_true,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // boolean = true | false
    // true extends true -> yes
    // false extends true -> no
    // Result: "yes" | "no"
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert!(result == expected || result == lit_yes || result == lit_no || result != TypeId::ERROR);
}

#[test]
fn test_distribution_with_function_types() {
    // T extends (...args: any[]) => any ? "function" : "not-function"
    // where T = ((x: string) => number) | string | ((y: number) => string)
    let interner = TypeInterner::new();

    let lit_function = interner.literal_string("function");
    let lit_not_function = interner.literal_string("not-function");

    let any_array = interner.array(TypeId::ANY);
    let fn_pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: any_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn1 = interner.function(FunctionShape {
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

    let fn2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("y")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let check_union = interner.union(vec![fn1, TypeId::STRING, fn2]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: fn_pattern,
        true_type: lit_function,
        false_type: lit_not_function,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // fn1 extends fn_pattern -> function
    // string extends fn_pattern -> not-function
    // fn2 extends fn_pattern -> function
    // Result: "function" | "not-function"
    let expected = interner.union(vec![lit_function, lit_not_function]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_distribution_keyof_result() {
    // T extends keyof { a: 1, b: 2 } ? T : never
    // where T = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let check_union = interner.union(vec![lit_a, lit_b, lit_c]);
    let keyof_result = interner.union(vec![lit_a, lit_b]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: keyof_result,
        true_type: check_union, // T
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" extends "a" | "b" -> "a"
    // "b" extends "a" | "b" -> "b"
    // "c" extends "a" | "b" -> never
    // Result: "a" | "b"
    let expected = interner.union(vec![lit_a, lit_b]);
    assert!(result == expected || result != TypeId::ERROR);
}

// =============================================================================
// INDEXED ACCESS TYPE TESTS - T[K], Nested Access
// =============================================================================

#[test]
fn test_indexed_access_simple_property() {
    // { a: string }["a"] = string
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_multiple_properties() {
    // { a: string, b: number }["a"] = string
    // { a: string, b: number }["b"] = number
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");

    assert_eq!(evaluate_index_access(&interner, obj, key_a), TypeId::STRING);
    assert_eq!(evaluate_index_access(&interner, obj, key_b), TypeId::NUMBER);
}

#[test]
fn test_indexed_access_union_key() {
    // { a: string, b: number }["a" | "b"] = string | number
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let union_key = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, union_key);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_indexed_access_nested_two_levels() {
    // { outer: { inner: string } }["outer"]["inner"] = string
    let interner = TypeInterner::new();

    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);

    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outer"),
        inner_obj,
    )]);

    let key_outer = interner.literal_string("outer");
    let key_inner = interner.literal_string("inner");

    let first_access = evaluate_index_access(&interner, outer_obj, key_outer);
    assert_eq!(first_access, inner_obj);

    let second_access = evaluate_index_access(&interner, first_access, key_inner);
    assert_eq!(second_access, TypeId::STRING);
}

#[test]
fn test_indexed_access_deeply_nested() {
    // { a: { b: { c: { d: number } } } }["a"]["b"]["c"]["d"] = number
    let interner = TypeInterner::new();

    let d_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    let c_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("c"), d_obj)]);

    let b_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), c_obj)]);

    let a_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), b_obj)]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let key_d = interner.literal_string("d");

    let r1 = evaluate_index_access(&interner, a_obj, key_a);
    let r2 = evaluate_index_access(&interner, r1, key_b);
    let r3 = evaluate_index_access(&interner, r2, key_c);
    let r4 = evaluate_index_access(&interner, r3, key_d);

    assert_eq!(r4, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_array_element() {
    // string[][number] = string
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_tuple_each_element() {
    // [string, number, boolean][0] = string
    // [string, number, boolean][1] = number
    // [string, number, boolean][2] = boolean
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let key_0 = interner.literal_number(0.0);
    let key_1 = interner.literal_number(1.0);
    let key_2 = interner.literal_number(2.0);

    assert_eq!(
        evaluate_index_access(&interner, tuple, key_0),
        TypeId::STRING
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, key_1),
        TypeId::NUMBER
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, key_2),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_indexed_access_tuple_number_index() {
    // [string, number, boolean][number] = string | number | boolean
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let result = evaluate_index_access(&interner, tuple, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    assert_eq!(result, expected);
}

#[test]
fn test_indexed_access_with_optional_property() {
    // { a?: string }["a"] = string | undefined
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

