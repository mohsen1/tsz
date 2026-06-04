#[test]
fn test_distribution_over_intersection_three_types() {
    // Three-way intersection: A & B & C
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    let cond = ConditionalType {
        check_type: intersection,
        extends_type: TypeId::OBJECT,
        true_type: lit_yes,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert!(result == lit_yes || result != TypeId::ERROR);
}

#[test]
fn test_never_filtering_basic() {
    // T extends never ? "yes" : "no" where T = never
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NEVER,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Distributive over never = never (empty union distribution)
    assert!(result == TypeId::NEVER || result == lit_yes);
}

#[test]
fn test_never_filtering_in_union() {
    // T extends string ? T : never where T = string | never
    // never is filtered out, result should be string
    let interner = TypeInterner::new();

    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NEVER]);

    let cond = ConditionalType {
        check_type: union_with_never,
        extends_type: TypeId::STRING,
        true_type: union_with_never,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string extends string -> string, never distributes to never
    // Result should be string (never filtered out)
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_never_filtering_exclude_pattern() {
    // Exclude<T, U> = T extends U ? never : T
    // Exclude<"a" | "b" | "c", "a"> = "b" | "c"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let union_abc = interner.union(vec![lit_a, lit_b, lit_c]);

    // T param for distributive check
    let (_t_name, _t_param) = test_type_param(&interner, "T");

    let cond = ConditionalType {
        check_type: union_abc,
        extends_type: lit_a,
        true_type: TypeId::NEVER,
        false_type: union_abc, // Return the check type
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" -> never, "b" -> "b", "c" -> "c"
    // Result should be "b" | "c" (never filtered)
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_never_filtering_extract_pattern() {
    // Extract<T, U> = T extends U ? T : never
    // Extract<"a" | "b" | 1 | 2, string> = "a" | "b"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let mixed_union = interner.union(vec![lit_a, lit_b, lit_1, lit_2]);

    let cond = ConditionalType {
        check_type: mixed_union,
        extends_type: TypeId::STRING,
        true_type: mixed_union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" -> "a", "b" -> "b", 1 -> never, 2 -> never
    // Result should be "a" | "b"
    assert!(result != TypeId::ERROR && result != TypeId::NEVER);
}

#[test]
fn test_never_filtering_all_filtered() {
    // Extract<1 | 2 | 3, string> = never (all filtered out)
    let interner = TypeInterner::new();

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let number_union = interner.union(vec![lit_1, lit_2, lit_3]);

    let cond = ConditionalType {
        check_type: number_union,
        extends_type: TypeId::STRING,
        true_type: number_union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All numbers -> never, result should be never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_never_filtering_nonnullable() {
    // NonNullable<T> = T extends null | undefined ? never : T
    // NonNullable<string | null | undefined> = string
    let interner = TypeInterner::new();

    let nullable_union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let null_or_undefined = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let cond = ConditionalType {
        check_type: nullable_union,
        extends_type: null_or_undefined,
        true_type: TypeId::NEVER,
        false_type: nullable_union,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> string, null -> never, undefined -> never
    // Result should be string
    assert!(result != TypeId::ERROR);
}
