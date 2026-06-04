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
