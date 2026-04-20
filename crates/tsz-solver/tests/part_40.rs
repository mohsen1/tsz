use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_distributive_union_of_unions() {
    // T extends string ? 1 : 2, with T = ("a" | "b") | (1 | 2)
    // The nested unions should be flattened
    // Result: 1 | 2
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Nested unions (should be flattened by union())
    let strings = interner.union(vec![lit_a, lit_b]);
    let numbers = interner.union(vec![
        interner.literal_number(10.0),
        interner.literal_number(20.0),
    ]);
    subst.insert(t_name, interner.union(vec![strings, numbers]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // "a" -> 1, "b" -> 1, 10 -> 2, 20 -> 2; result = 1 | 2
    let expected = interner.union(vec![lit_1, lit_2]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_boolean_literals() {
    // T extends true ? "yes" : T extends false ? "no" : "other"
    // with T = true | false | null
    // Result: "yes" | "no" | "other"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");
    let lit_other = interner.literal_string("other");

    // Inner: T extends false ? "no" : "other"
    let inner = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_false,
        true_type: lit_no,
        false_type: lit_other,
        is_distributive: false,
    });

    // Outer: T extends true ? "yes" : inner
    let outer = ConditionalType {
        check_type: t_param,
        extends_type: lit_true,
        true_type: lit_yes,
        false_type: inner,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_true, lit_false, TypeId::NULL]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![lit_yes, lit_no, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_unknown() {
    // T extends unknown ? T : never
    // with T = string | number | null
    // Everything extends unknown, so result = string | number | null
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::UNKNOWN,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let input = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NULL]);
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Everything extends unknown
    assert_eq!(result, input);
}

#[test]
fn test_distributive_partial_object_match() {
    // T extends { x: any } ? T["x"] : "no-x"
    // with T = { x: string } | { y: number } | { x: boolean, y: symbol }
    // Result: string | "no-x" | boolean
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let lit_no_x = interner.literal_string("no-x");

    let extends_obj = interner.object(vec![PropertyInfo::new(x_atom, TypeId::ANY)]);

    let index_access =
        interner.intern(TypeData::IndexAccess(t_param, interner.literal_string("x")));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: index_access,
        false_type: lit_no_x,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let obj1 = interner.object(vec![PropertyInfo::new(x_atom, TypeId::STRING)]);
    let obj2 = interner.object(vec![PropertyInfo::new(y_atom, TypeId::NUMBER)]);
    let obj3 = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::BOOLEAN),
        PropertyInfo::new(y_atom, TypeId::SYMBOL),
    ]);

    subst.insert(t_name, interner.union(vec![obj1, obj2, obj3]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // obj1 -> string, obj2 -> "no-x", obj3 -> boolean
    let expected = interner.union(vec![TypeId::STRING, lit_no_x, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_hundred_member_union() {
    // Stress test with 100 union members
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_match = interner.literal_string("match");
    let lit_no_match = interner.literal_string("no-match");

    // T extends string ? "match" : "no-match"
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_match,
        false_type: lit_no_match,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // 100 members: 50 strings, 50 numbers
    let members: Vec<TypeId> = (0..100)
        .map(|i| {
            if i < 50 {
                interner.literal_string(&format!("s{i}"))
            } else {
                interner.literal_number(i as f64)
            }
        })
        .collect();
    subst.insert(t_name, interner.union(members));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should be "match" | "no-match"
    let expected = interner.union(vec![lit_match, lit_no_match]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_triple_nested_conditional() {
    // T extends "a" ? 1 : T extends "b" ? 2 : T extends "c" ? 3 : T extends "d" ? 4 : 0
    // with T = "a" | "b" | "c" | "d" | "e"
    // Result: 0 | 1 | 2 | 3 | 4
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_e = interner.literal_string("e");
    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let lit_4 = interner.literal_number(4.0);

    // Build from innermost to outermost
    let cond4 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_d,
        true_type: lit_4,
        false_type: lit_0,
        is_distributive: false,
    });

    let cond3 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_c,
        true_type: lit_3,
        false_type: cond4,
        is_distributive: false,
    });

    let cond2 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_b,
        true_type: lit_2,
        false_type: cond3,
        is_distributive: false,
    });

    let outer = ConditionalType {
        check_type: t_param,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: cond2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, lit_b, lit_c, lit_d, lit_e]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // "a" -> 1, "b" -> 2, "c" -> 3, "d" -> 4, "e" -> 0
    let expected = interner.union(vec![lit_0, lit_1, lit_2, lit_3, lit_4]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_no_false_branch_matches() {
    // T extends string ? T : never
    // with T = 1 | 2 | 3 (all numbers, none match)
    // Result: never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    subst.insert(t_name, interner.union(vec![lit_1, lit_2, lit_3]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // All go to false branch (never), result is never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distributive_empty_object_match() {
    // T extends {} ? "object-like" : "primitive"
    // with T = string | number | { x: 1 } | null
    // In TypeScript, string and number extend {}, but null doesn't
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_object_like = interner.literal_string("object-like");
    let lit_primitive = interner.literal_string("primitive");
    let x_atom = interner.intern_string("x");

    let empty_obj = interner.object(Vec::new());

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: empty_obj,
        true_type: lit_object_like,
        false_type: lit_primitive,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let obj_x = interner.object(vec![PropertyInfo::new(
        x_atom,
        interner.literal_number(1.0),
    )]);

    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, obj_x, TypeId::NULL]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Result should contain both branches
    let expected = interner.union(vec![lit_object_like, lit_primitive]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_literal_type_filter() {
    // T extends "a" | "b" | "c" ? T : never
    // with T = "a" | "b" | "c" | "d" | "e"
    // Result: "a" | "b" | "c"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_e = interner.literal_string("e");

    let allowed = interner.union(vec![lit_a, lit_b, lit_c]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: allowed,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, lit_b, lit_c, lit_d, lit_e]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![lit_a, lit_b, lit_c]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_numeric_literal_filter() {
    // T extends 1 | 2 | 3 ? "low" : T extends 4 | 5 | 6 ? "mid" : "high"
    // with T = 1 | 2 | 5 | 7 | 10
    // Result: "low" | "mid" | "high"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_low = interner.literal_string("low");
    let lit_mid = interner.literal_string("mid");
    let lit_high = interner.literal_string("high");

    let low_set = interner.union(vec![
        interner.literal_number(1.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
    ]);
    let mid_set = interner.union(vec![
        interner.literal_number(4.0),
        interner.literal_number(5.0),
        interner.literal_number(6.0),
    ]);

    let inner = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: mid_set,
        true_type: lit_mid,
        false_type: lit_high,
        is_distributive: false,
    });

    let outer = ConditionalType {
        check_type: t_param,
        extends_type: low_set,
        true_type: lit_low,
        false_type: inner,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.literal_number(1.0),
            interner.literal_number(2.0),
            interner.literal_number(5.0),
            interner.literal_number(7.0),
            interner.literal_number(10.0),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // 1 -> low, 2 -> low, 5 -> mid, 7 -> high, 10 -> high
    let expected = interner.union(vec![lit_low, lit_mid, lit_high]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_void() {
    // T extends void ? "void" : "not-void"
    // with T = void | string | undefined
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_void = interner.literal_string("void");
    let lit_not_void = interner.literal_string("not-void");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::VOID,
        true_type: lit_void,
        false_type: lit_not_void,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::VOID, TypeId::STRING, TypeId::UNDEFINED]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // void -> "void", string -> "not-void", undefined -> could be either depending on semantics
    let expected = interner.union(vec![lit_void, lit_not_void]);
    assert_eq!(result, expected);
}

// =============================================================================
// DISTRIBUTIVE CONDITIONAL TYPE STRESS TESTS
// =============================================================================

#[test]
fn test_distributive_chained_conditionals() {
    // Type chain: T extends string ? "str" : T extends number ? "num" : "other"
    // Tests: multiple conditional evaluation in sequence
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_str = interner.literal_string("str");
    let lit_num = interner.literal_string("num");
    let lit_other = interner.literal_string("other");

    // Inner conditional: T extends number ? "num" : "other"
    let inner_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: lit_num,
        false_type: lit_other,
        is_distributive: true,
    };
    let inner = interner.conditional(inner_cond);

    // Outer conditional: T extends string ? "str" : <inner>
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_str,
        false_type: inner,
        is_distributive: true,
    };
    let outer = interner.conditional(outer_cond);

    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]),
    );

    let instantiated = instantiate_type(&interner, outer, &subst);
    let result = evaluate_type(&interner, instantiated);

    // string -> "str", number -> "num", boolean -> "other"
    let expected = interner.union(vec![lit_str, lit_num, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_intersection_check() {
    // T extends { a: string } & { b: number } ? "match" : "no-match"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let extends_type = interner.intersection(vec![obj_a, obj_b]);

    let lit_match = interner.literal_string("match");
    let lit_no_match = interner.literal_string("no-match");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type,
        true_type: lit_match,
        false_type: lit_no_match,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create an object with both properties
    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![obj_ab, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // obj_ab -> "match", string -> "no-match"
    let expected = interner.union(vec![lit_match, lit_no_match]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_bigint_literals() {
    // T extends bigint ? "bigint" : "not-bigint"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_bigint = interner.literal_string("bigint");
    let lit_not = interner.literal_string("not-bigint");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::BIGINT,
        true_type: lit_bigint,
        false_type: lit_not,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::BIGINT, TypeId::NUMBER, TypeId::STRING]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // bigint -> "bigint", number -> "not-bigint", string -> "not-bigint"
    let expected = interner.union(vec![lit_bigint, lit_not]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_filter_nullables() {
    // NonNullable<T> = T extends null | undefined ? never : T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let nullish = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: nullish,
        true_type: TypeId::NEVER,
        false_type: t_param,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            TypeId::STRING,
            TypeId::NULL,
            TypeId::NUMBER,
            TypeId::UNDEFINED,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // null -> never, undefined -> never, string -> string, number -> number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_symbol() {
    // T extends symbol ? "symbol" : "not-symbol"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_sym = interner.literal_string("symbol");
    let lit_not = interner.literal_string("not-symbol");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::SYMBOL,
        true_type: lit_sym,
        false_type: lit_not,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![TypeId::SYMBOL, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![lit_sym, lit_not]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_object_keyword() {
    // T extends object ? "object" : "primitive"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_obj = interner.literal_string("object");
    let lit_prim = interner.literal_string("primitive");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::OBJECT,
        true_type: lit_obj,
        false_type: lit_prim,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![obj, TypeId::STRING, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // { x: number } -> "object", string -> "primitive", number -> "primitive"
    let expected = interner.union(vec![lit_obj, lit_prim]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_infer_with_fallback() {
    // T extends { value: infer V } ? V : T
    // When T doesn't match, returns T itself
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let v_name = interner.intern_string("V");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let v_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let value_prop = interner.intern_string("value");
    let extends_obj = interner.object(vec![PropertyInfo::new(value_prop, v_infer)]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: v_infer,
        false_type: t_param,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Object with value: number
    let obj_with_value = interner.object(vec![PropertyInfo::new(value_prop, TypeId::NUMBER)]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![obj_with_value, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // { value: number } -> number, string -> string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_tuple_check() {
    // T extends [infer First, ...infer Rest] ? First : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let first_name = interner.intern_string("First");
    let rest_name = interner.intern_string("Rest");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let first_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: first_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let rest_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: first_infer,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: rest_infer,
            optional: false,
            name: None,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: first_infer,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Tuple [string, number]
    let tuple1 = interner.tuple(vec![
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
    ]);

    // Tuple [boolean]
    let tuple2 = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        optional: false,
        name: None,
        rest: false,
    }]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![tuple1, tuple2, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // [string, number] -> string, [boolean] -> boolean, string -> never
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_literal_numbers() {
    // T extends 1 | 2 | 3 ? "low" : "high"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);
    let five = interner.literal_number(5.0);

    let low_set = interner.union(vec![one, two, three]);
    let lit_low = interner.literal_string("low");
    let lit_high = interner.literal_string("high");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: low_set,
        true_type: lit_low,
        false_type: lit_high,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![one, two, four, five]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // 1 -> "low", 2 -> "low", 4 -> "high", 5 -> "high"
    let expected = interner.union(vec![lit_low, lit_high]);
    assert_eq!(result, expected);
}

