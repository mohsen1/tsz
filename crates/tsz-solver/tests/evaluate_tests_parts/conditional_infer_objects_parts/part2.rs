#[test]
fn test_conditional_infer_absent_optional_constrained_union_true_branch() {
    // [T] extends [{ a?: infer R extends string }] ? R : "nope", with
    // T = { a: string } | {} (no distribution).
    // The {} member has absent a: tsc uses the constraint (string) so the
    // true branch is taken for that member too — result is string | string = string.
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_, infer_r) = test_constrained_infer_param(&interner, "R", TypeId::STRING);

    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
    let nope = interner.literal_string("nope");
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_obj,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: nope,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let empty_obj = interner.object(Vec::new());
    subst.insert(t_name, interner.union(vec![obj_string, empty_obj]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Both members take the true branch: { a: string } → R = string, {} → R = string (constraint).
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_absent_optional_multiple_props_union_input() {
    // [T] extends [{ a?: infer R; b?: infer S }] ? [R, S] : never, with
    // T = { a: string; b: number } | {} (no distribution).
    // {} member: a absent → R = undefined, b absent → S = undefined.
    // Result: [string, number] | [undefined, undefined].
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_, infer_r) = test_infer_param(&interner, "R");
    let (_, infer_s) = test_infer_param(&interner, "S");

    let extends_obj = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), infer_r),
        PropertyInfo::opt(interner.intern_string("b"), infer_s),
    ]);
    let true_tuple = interner.tuple(vec![
        TupleElement { type_id: infer_r, name: None, optional: false, rest: false },
        TupleElement { type_id: infer_s, name: None, optional: false, rest: false },
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_obj,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: true_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_full = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let empty_obj = interner.object(Vec::new());
    subst.insert(t_name, interner.union(vec![obj_full, empty_obj]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Non-distributive evaluation: infer bindings are merged across union members.
    // { a: string; b: number } contributes R=string, S=number.
    // {} contributes R=undefined, S=undefined.
    // Merged: R = string | undefined, S = number | undefined.
    // true_type [R, S] → [string | undefined, number | undefined].
    let r_ty = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let s_ty = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let expected = interner.tuple(vec![
        TupleElement { type_id: r_ty, name: None, optional: false, rest: false },
        TupleElement { type_id: s_ty, name: None, optional: false, rest: false },
    ]);

    assert_eq!(result, expected);
}
