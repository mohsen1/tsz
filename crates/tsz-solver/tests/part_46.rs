use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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
