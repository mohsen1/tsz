use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_conditional_infer_readonly_array_element_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

#[test]
fn test_conditional_infer_readonly_array_element_non_array_union_branch() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
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

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

