#[test]
fn test_distributive_large_union_all_match() {
    // T extends string ? T : never, with T = all string literals
    // Result: union of all input strings
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Create a union of 20 string literals
    let members: Vec<TypeId> = (0..20)
        .map(|i| interner.literal_string(&format!("str{i}")))
        .collect();
    let input_union = interner.union(members.clone());
    subst.insert(t_name, input_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Result should be the same union of string literals
    let expected = interner.union(members);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_large_union_none_match() {
    // T extends string ? T : never, with T = all numbers
    // Result: never
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Create a union of 15 number literals
    let members: Vec<TypeId> = (0..15).map(|i| interner.literal_number(i as f64)).collect();
    subst.insert(t_name, interner.union(members));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // All members are numbers, none match string, so result is never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distributive_nested_conditional() {
    // T extends string ? (T extends "a" | "b" ? 1 : 2) : 3
    // with T = "a" | "b" | "c" | 1 | 2
    // Distribution: "a" -> 1, "b" -> 1, "c" -> 2, 1 -> 3, 2 -> 3
    // Result: 1 | 2 | 3
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    // Inner conditional: T extends "a" | "b" ? 1 : 2
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: interner.union(vec![lit_a, lit_b]),
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: false, // Inner is non-distributive
    });

    // Outer conditional: T extends string ? inner : 3
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: lit_3,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, lit_b, lit_c, lit_1, lit_2]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: 1 | 2 | 3
    let expected = interner.union(vec![lit_1, lit_2, lit_3]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_infer_filter() {
    // T extends (infer R)[] ? R : never, with T = string[] | number[] | boolean
    // Distribution: string[] -> string, number[] -> number, boolean -> never
    // Result: string | number
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

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
    subst.insert(
        t_name,
        interner.union(vec![string_array, number_array, TypeId::BOOLEAN]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: string | number (boolean is filtered to never)
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_mapped_branches() {
    // T extends string ? T : T extends number ? "num" : "other"
    // with T = "a" | 1 | true
    // Distribution: "a" -> "a", 1 -> "num", true -> "other"
    // Result: "a" | "num" | "other"
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let lit_a = interner.literal_string("a");
    let lit_num = interner.literal_string("num");
    let lit_other = interner.literal_string("other");
    let lit_1 = interner.literal_number(1.0);

    // Inner conditional: T extends number ? "num" : "other"
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: lit_num,
        false_type: lit_other,
        is_distributive: false,
    });

    // Outer conditional: T extends string ? T : inner
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: inner_cond,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![lit_a, lit_1, interner.literal_boolean(true)]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Expected: "a" | "num" | "other"
    let expected = interner.union(vec![lit_a, lit_num, lit_other]);
    assert_eq!(result, expected);
}
