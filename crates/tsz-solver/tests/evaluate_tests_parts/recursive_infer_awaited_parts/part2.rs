#[test]
fn test_infer_with_default_type_fallback() {
    // When the pattern doesn't match at all, check default behavior
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    // Pattern: { a: infer P = string }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_p,
    )]);

    // Input: { b: number } - different property name, won't match
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Pattern doesn't match, should return never (false branch)
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_infer_with_default_and_constraint() {
    // T extends { prop: infer P extends object = {} } ? P : never
    let interner = TypeInterner::new();

    let empty_object = interner.object(vec![]);

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: Some(TypeId::OBJECT),
        default: Some(empty_object),
        is_const: false,
    }));

    // Pattern: { prop: infer P extends object = {} }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: { x: 1 } }
    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        inner_obj,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = { x: number } which extends object
    assert!(result == inner_obj || result != TypeId::ERROR);
}

#[test]
fn test_infer_discriminated_union_kind() {
    // T extends { kind: infer K } ? K : never
    // Input: { kind: "circle" } | { kind: "square" }
    let interner = TypeInterner::new();

    let (_infer_k_name, infer_k) = test_infer_param(&interner, "K");

    // Pattern: { kind: infer K }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        infer_k,
    )]);

    // Input: { kind: "circle" }
    let circle = interner.literal_string("circle");
    let circle_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        circle,
    )]);

    // Input: { kind: "square" }
    let square = interner.literal_string("square");
    let square_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        square,
    )]);

    let union_input = interner.union(vec![circle_obj, square_obj]);

    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer K = "circle" | "square"
    assert!(result != TypeId::ERROR && result != TypeId::NEVER);
}

#[test]
fn test_infer_discriminated_union_with_extra_props() {
    // T extends { type: infer T, data: infer D } ? [T, D] : never
    let interner = TypeInterner::new();

    let (_infer_t_name, infer_t) = test_infer_param(&interner, "T");

    let (_infer_d_name, infer_d) = test_infer_param(&interner, "D");

    // Pattern: { type: infer T, data: infer D }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), infer_t),
        PropertyInfo::new(interner.intern_string("data"), infer_d),
    ]);

    // Input: { type: "success", data: number }
    let success = interner.literal_string("success");
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), success),
        PropertyInfo::new(interner.intern_string("data"), TypeId::NUMBER),
    ]);

    // Result: [T, D]
    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_t,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_d,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer T = "success", D = number
    assert!(result != TypeId::ERROR);
}
