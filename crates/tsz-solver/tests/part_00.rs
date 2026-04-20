use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_conditional_true_branch() {
    let interner = TypeInterner::new();

    // string extends string ? number : boolean
    // Should resolve to number
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_false_branch() {
    let interner = TypeInterner::new();

    // number extends string ? number : boolean
    // Should resolve to boolean (number is not subtype of string)
    let cond = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_conditional_literal_extends_base() {
    let interner = TypeInterner::new();

    // "hello" extends string ? true : false
    // Should resolve to true (literal is subtype of base)
    let hello = interner.literal_string("hello");
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let cond = ConditionalType {
        check_type: hello,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, lit_true);
}

#[test]
fn test_conditional_distributive() {
    let interner = TypeInterner::new();

    // (string | number) extends string ? true : false
    // Distributes to: (string extends string ? true : false) | (number extends string ? true : false)
    // = true | false
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let cond = ConditionalType {
        check_type: string_or_number,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be true | false (i.e., boolean union of literals)
    let expected = interner.union(vec![lit_true, lit_false]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_non_distributive_union() {
    let interner = TypeInterner::new();

    // (string | number) extends string ? true : false
    // Non-distributive: union is not a subtype of string, so false
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let cond = ConditionalType {
        check_type: string_or_number,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, lit_false);
}

#[test]
fn test_rest_unknown_bivariant_conditional_evaluate_strict() {
    let interner = TypeInterner::new();

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let cond = ConditionalType {
        check_type: source,
        extends_type: target,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, lit_false);
}

#[test]
fn test_conditional_instantiated_param_distributes() {
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

    // T extends string ? true : false, with T = string | number (distributive).
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![lit_true, lit_false]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_instantiated_param_distributes_branch_substitution() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends string ? T : never, with T = string | number
    // Distributes to: (string extends string ? string : never) |
    //                 (number extends string ? number : never)
    // Result should be string.
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_distributive_nested_extends() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends string ? (T extends "a" ? 1 : 2) : 3, with T = "a" | "b"
    // Distributes to 1 | 2.
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_one = interner.literal_number(1.0);
    let lit_two = interner.literal_number(2.0);
    let lit_three = interner.literal_number(3.0);

    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_a,
        true_type: lit_one,
        false_type: lit_two,
        is_distributive: false,
    });

    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: lit_three,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_a, lit_b]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![lit_one, lit_two]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_distributive_infer_extends_nested() {
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
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends infer R extends string ? (R extends "a" ? "yes" : "no") : "fallback"
    // with T = "a" | "b" | number.
    let lit_a = interner.literal_string("a");
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");
    let lit_fallback = interner.literal_string("fallback");

    let inner_cond = interner.conditional(ConditionalType {
        check_type: infer_r,
        extends_type: lit_a,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    });

    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: infer_r,
        true_type: inner_cond,
        false_type: lit_fallback,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, interner.literal_string("b"), TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![lit_yes, lit_no, lit_fallback]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_true_branch_substitution() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // "a" extends infer R extends string ? R : never
    let lit_a = interner.literal_string("a");
    let cond = ConditionalType {
        check_type: lit_a,
        extends_type: infer_r,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, lit_a);
}

#[test]
fn test_conditional_infer_false_branch_substitution() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // number extends infer R extends string ? string : R
    let cond = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: infer_r,
        true_type: TypeId::STRING,
        false_type: infer_r,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_conditional_infer_array_element_extraction() {
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

    // T extends (infer R)[] ? R : never, with T = string[] | number[].
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.array(TypeId::STRING),
            interner.array(TypeId::NUMBER),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_array_element_non_array_union_branch() {
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

    // T extends (infer R)[] ? R : never, with T = string[] | number.
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![interner.array(TypeId::STRING), TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_array_element_non_distributive_union_input() {
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

    // T extends (infer R)[] ? R : never, with T = string[] | number[] (no distribution).
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.array(TypeId::STRING),
            interner.array(TypeId::NUMBER),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_array_element_non_distributive_union_branch() {
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

    // T extends (infer R)[] ? R : never, with T = string[] | number (no distribution).
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![interner.array(TypeId::STRING), TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_array_element_from_tuple_rest() {
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

    // T extends (infer R)[] ? R : never, with T = [string, ...number[]].
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
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
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_array_element_from_tuple_rest_tuple() {
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

    // T extends (infer R)[] ? R : never, with T = [string, ...[number, boolean]].
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
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
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_array_element_from_optional_tuple_element() {
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

    // T extends (infer R)[] ? R : never, with T = [string?].
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let optional_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);
    subst.insert(t_name, optional_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_array_element_with_constraint() {
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
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends (infer R extends string)[] ? R : never, with T = number[] | string[].
    let extends_array = interner.array(infer_r);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.array(TypeId::NUMBER),
            interner.array(TypeId::STRING),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // For number[]: R = number fails constraint, goes to false branch (never)
    // For string[]: R = string satisfies constraint, goes to true branch (string)
    // Union: string | never = string
    assert_eq!(result, TypeId::STRING);
}
