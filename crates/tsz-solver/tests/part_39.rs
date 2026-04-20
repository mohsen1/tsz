use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_distributive_non_nullable_utility() {
    // NonNullable<T> = T extends null | undefined ? never : T
    // NonNullable<string | null | undefined | number> = string | number
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let null_or_undefined = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    // T extends null | undefined ? never : T
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: null_or_undefined,
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
            TypeId::UNDEFINED,
            TypeId::NUMBER,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_deeply_nested_union() {
    // T extends string ? "s" : (T extends number ? "n" : (T extends boolean ? "b" : "x"))
    // with T = "a" | 1 | true | null
    // Distribution: "a" -> "s", 1 -> "n", true -> "b", null -> "x"
    // Result: "s" | "n" | "b" | "x"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_s = interner.literal_string("s");
    let lit_n = interner.literal_string("n");
    let lit_b = interner.literal_string("b");
    let lit_x = interner.literal_string("x");
    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);

    // Innermost: T extends boolean ? "b" : "x"
    let cond3 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::BOOLEAN,
        true_type: lit_b,
        false_type: lit_x,
        is_distributive: false,
    });

    // Middle: T extends number ? "n" : cond3
    let cond2 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: lit_n,
        false_type: cond3,
        is_distributive: false,
    });

    // Outer: T extends string ? "s" : cond2
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_s,
        false_type: cond2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            lit_a,
            lit_1,
            interner.literal_boolean(true),
            TypeId::NULL,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: "s" | "n" | "b" | "x"
    let expected = interner.union(vec![lit_s, lit_n, lit_b, lit_x]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_never_input() {
    // T extends string ? T : "fallback", with T = never
    // Distribution over never: never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_fallback = interner.literal_string("fallback");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: lit_fallback,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::NEVER);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Distributive over never results in never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distributive_with_any_input() {
    // T extends string ? 1 : 2, with T = any
    // any distributes to both branches, result is 1 | 2
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

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::ANY);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // any distributes to both branches
    let expected = interner.union(vec![lit_1, lit_2]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_single_member_union() {
    // T extends string ? T : never, with T = "a" (single member)
    // Result: "a"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_a = interner.literal_string("a");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    // Single-member union should behave the same as the member itself
    subst.insert(t_name, interner.union(vec![lit_a]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, lit_a);
}

#[test]
fn test_distributive_with_duplicate_results() {
    // T extends string | number ? 1 : 2, with T = "a" | 1 | true
    // Distribution: "a" -> 1, 1 -> 1, true -> 2
    // Result: 1 | 2 (deduplicated)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: string_or_number,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            lit_a,
            interner.literal_number(42.0),
            interner.literal_boolean(true),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // "a" -> 1, 42 -> 1, true -> 2; result = 1 | 2
    let expected = interner.union(vec![lit_1, lit_2]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_preserves_tuple_structure() {
    // T extends [infer R] ? R : never, with T = [string] | [number]
    // Distribution: [string] -> string, [number] -> number
    // Result: string | number
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
    let tuple_number = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, interner.union(vec![tuple_string, tuple_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_constrained_infer() {
    // T extends (infer R extends string)[] ? R : never
    // with T = string[] | number[] | boolean[]
    // Distribution: string[] -> string, number[] -> never (filtered), boolean[] -> never (filtered)
    // Result: string
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
        constraint: Some(TypeId::STRING), // R extends string constraint
        default: None,
        is_const: false,
    }));

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

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let boolean_array = interner.array(TypeId::BOOLEAN);
    subst.insert(
        t_name,
        interner.union(vec![string_array, number_array, boolean_array]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Only string[] satisfies the constraint, so result is string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_distributive_intrinsic_union() {
    // T extends object ? "obj" : "prim", with T = string | number | { x: string }
    // Distribution: string -> "prim", number -> "prim", { x: string } -> "obj"
    // Result: "obj" | "prim"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_obj = interner.literal_string("obj");
    let lit_prim = interner.literal_string("prim");
    let x_atom = interner.intern_string("x");

    let obj_type = interner.object(vec![PropertyInfo::new(x_atom, TypeId::STRING)]);

    // T extends object ? "obj" : "prim"
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::OBJECT,
        true_type: lit_obj,
        false_type: lit_prim,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, obj_type]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: "obj" | "prim"
    let expected = interner.union(vec![lit_obj, lit_prim]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_function_types() {
    // T extends (...args: any[]) => any ? "func" : "other"
    // with T = (() => void) | string | ((x: number) => string)
    // Distribution: () => void -> "func", string -> "other", (x) => string -> "func"
    // Result: "func" | "other"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_func = interner.literal_string("func");
    let lit_other = interner.literal_string("other");

    // Pattern: (...args: any[]) => any
    let args_atom = interner.intern_string("args");
    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::rest(args_atom, interner.array(TypeId::ANY))],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: lit_func,
        false_type: lit_other,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let fn1 = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn2 = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    subst.insert(t_name, interner.union(vec![fn1, TypeId::STRING, fn2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: "func" | "other"
    let expected = interner.union(vec![lit_func, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_readonly_array() {
    // T extends readonly (infer R)[] ? R : never
    // with T = readonly string[] | readonly number[] | boolean
    // Distribution: readonly string[] -> string, readonly number[] -> number, boolean -> never
    // Result: string | number
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
        interner.union(vec![
            readonly_string_array,
            readonly_number_array,
            TypeId::BOOLEAN,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_literal_union_exhaustive() {
    // T extends "a" ? 1 : T extends "b" ? 2 : T extends "c" ? 3 : 0
    // with T = "a" | "b" | "c" | "d"
    // Distribution: "a" -> 1, "b" -> 2, "c" -> 3, "d" -> 0
    // Result: 0 | 1 | 2 | 3
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
    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    // Innermost: T extends "c" ? 3 : 0
    let cond3 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_c,
        true_type: lit_3,
        false_type: lit_0,
        is_distributive: false,
    });

    // Middle: T extends "b" ? 2 : cond3
    let cond2 = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: lit_b,
        true_type: lit_2,
        false_type: cond3,
        is_distributive: false,
    });

    // Outer: T extends "a" ? 1 : cond2
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: cond2,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_a, lit_b, lit_c, lit_d]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: 0 | 1 | 2 | 3
    let expected = interner.union(vec![lit_0, lit_1, lit_2, lit_3]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_multiple_arrays() {
    // T extends (infer R)[][] ? R : never
    // with T = string[][] | number[][] | boolean
    // Distribution: string[][] -> string, number[][] -> number, boolean -> never
    // Result: string | number
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

    // R[][] = Array<Array<R>>
    let extends_nested_array = interner.array(interner.array(infer_r));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_nested_array,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let nested_string_array = interner.array(interner.array(TypeId::STRING));
    let nested_number_array = interner.array(interner.array(TypeId::NUMBER));
    subst.insert(
        t_name,
        interner.union(vec![
            nested_string_array,
            nested_number_array,
            TypeId::BOOLEAN,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_keyof_filter() {
    // T extends keyof any ? T : never, with T = "a" | "b" | 1 | symbol
    // Distribution: "a" -> "a", "b" -> "b", 1 -> 1, symbol -> symbol
    // Result: "a" | "b" | 1 | symbol (all are valid keyof types)
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
    let lit_1 = interner.literal_number(1.0);

    // keyof any = string | number | symbol
    let keyof_any = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: keyof_any,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, lit_b, lit_1, TypeId::SYMBOL]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // All inputs extend string | number | symbol
    let expected = interner.union(vec![lit_a, lit_b, lit_1, TypeId::SYMBOL]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_mixed_primitive_union() {
    // T extends string | boolean ? "primitive" : "other"
    // with T = "a" | 1 | true | null | undefined | {}
    // Distribution: "a" -> "primitive", 1 -> "other", true -> "primitive",
    //               null -> "other", undefined -> "other", {} -> "other"
    // Result: "primitive" | "other"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_primitive = interner.literal_string("primitive");
    let lit_other = interner.literal_string("other");
    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_true = interner.literal_boolean(true);
    let empty_obj = interner.object(Vec::new());

    let string_or_boolean = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: string_or_boolean,
        true_type: lit_primitive,
        false_type: lit_other,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            lit_a,
            lit_1,
            lit_true,
            TypeId::NULL,
            TypeId::UNDEFINED,
            empty_obj,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: "primitive" | "other"
    let expected = interner.union(vec![lit_primitive, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_very_large_union() {
    // Stress test with 50 union members
    // T extends string ? "yes" : "no", with T = mix of 25 strings and 25 numbers
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // 50 members: 25 strings, 25 numbers
    let members: Vec<TypeId> = (0..50)
        .map(|i| {
            if i < 25 {
                interner.literal_string(&format!("str{i}"))
            } else {
                interner.literal_number(i as f64)
            }
        })
        .collect();
    subst.insert(t_name, interner.union(members));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should be "yes" | "no"
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_all_to_same_result() {
    // All union members produce the same result
    // T extends string | number | boolean ? "primitive" : "other"
    // with T = "a" | 1 | true (all primitives)
    // Result: "primitive" (single value, not union)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_primitive = interner.literal_string("primitive");
    let lit_other = interner.literal_string("other");
    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_true = interner.literal_boolean(true);

    let primitives = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: primitives,
        true_type: lit_primitive,
        false_type: lit_other,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_a, lit_1, lit_true]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // All members are primitives, so result should just be "primitive"
    assert_eq!(result, lit_primitive);
}

#[test]
fn test_distributive_identity_preservation() {
    // T extends any ? T : never (identity type)
    // with T = "a" | 1 | true
    // Result: "a" | 1 | true
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_true = interner.literal_boolean(true);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::ANY,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let input = interner.union(vec![lit_a, lit_1, lit_true]);
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Identity: should preserve the original union
    assert_eq!(result, input);
}

#[test]
fn test_distributive_two_infers_different_positions() {
    // T extends { a: infer A, b: infer B } ? [A, B] : never
    // with T = { a: string, b: number } | { a: boolean, b: symbol }
    // Result: [string, number] | [boolean, symbol]
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let prop_a = interner.intern_string("a");
    let prop_b = interner.intern_string("b");

    let extends_obj = interner.object(vec![
        PropertyInfo::new(prop_a, infer_a),
        PropertyInfo::new(prop_b, infer_b),
    ]);

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(prop_a, TypeId::STRING),
        PropertyInfo::new(prop_b, TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(prop_a, TypeId::BOOLEAN),
        PropertyInfo::new(prop_b, TypeId::SYMBOL),
    ]);

    subst.insert(t_name, interner.union(vec![obj1, obj2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let tuple1 = interner.tuple(vec![
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
    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::SYMBOL,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let expected = interner.union(vec![tuple1, tuple2]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_infer_return_type() {
    // T extends () => infer R ? R : never
    // with T = (() => string) | (() => number) | string
    // Expected result is string | number (extracted from function return types)
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

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    let fn_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    subst.insert(
        t_name,
        interner.union(vec![fn_string, fn_number, TypeId::STRING]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Function return type infer pattern extraction works correctly in distributive context
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
