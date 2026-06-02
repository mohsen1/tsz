// Regression coverage for the distributive-conditional / tuple-union family
// (issue #12175 and witnesses #10799, #10815, #10823, #10831, #10848, #10856,
// #10864, #10872). The structural rule under test:
//
// > When conditional types distribute over tuple-like union inputs, each union
// > arm's tuple constraints must be preserved and its inferred results merged
// > independently, matching `tsc` behavior.
//
// Tests vary binder names (`T`/`U`, `H`/`F`/`Hd`, `R`/`Rest`/`Tl`) so a
// fixture-name fast path cannot pass them, and cover both distributive
// (`T extends ...`) and non-distributive (`[T] extends [...]`) forms so the
// per-arm-merge path and the all-arms-must-match path stay distinct.

fn make_tuple(interner: &TypeInterner, elems: Vec<TypeId>) -> TypeId {
    interner.tuple(
        elems
            .into_iter()
            .map(|t| TupleElement {
                type_id: t,
                name: None,
                optional: false,
                rest: false,
            })
            .collect(),
    )
}

fn make_tuple_with_rest_tail(interner: &TypeInterner, head: Vec<TypeId>, rest: TypeId) -> TypeId {
    let mut elems: Vec<TupleElement> = head
        .into_iter()
        .map(|t| TupleElement {
            type_id: t,
            name: None,
            optional: false,
            rest: false,
        })
        .collect();
    elems.push(TupleElement {
        type_id: rest,
        name: None,
        optional: false,
        rest: true,
    });
    interner.tuple(elems)
}

#[test]
fn distributive_tuple_union_head_only_pattern_unions_each_variant() {
    // U extends [infer Hd, ...unknown[]] ? Hd : never
    //   with U = [string, number] | [boolean] | [].
    // Distribute -> { string, boolean, never } -> string | boolean.
    let interner = TypeInterner::new();
    let (u_name, u_param) = test_type_param(&interner, "U");
    let (_, infer_hd) = test_infer_param(&interner, "Hd");

    let unknown_arr = interner.array(TypeId::UNKNOWN);
    let extends = make_tuple_with_rest_tail(&interner, vec![infer_hd], unknown_arr);
    let cond = ConditionalType {
        check_type: u_param,
        extends_type: extends,
        true_type: infer_hd,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let cond_type = interner.conditional(cond);

    let arm_a = make_tuple(&interner, vec![TypeId::STRING, TypeId::NUMBER]);
    let arm_b = make_tuple(&interner, vec![TypeId::BOOLEAN]);
    let arm_c = make_tuple(&interner, vec![]);
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, interner.union(vec![arm_a, arm_b, arm_c]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn distributive_tuple_union_head_and_rest_preserves_per_variant_rest() {
    // U extends [infer F, ...infer Tl] ? Tl : never
    //   with U = [string, number, boolean] | [string, number] | [string].
    // Distribute -> [number, boolean] | [number] | [].
    let interner = TypeInterner::new();
    let (u_name, u_param) = test_type_param(&interner, "U");
    let (_, infer_f) = test_infer_param(&interner, "F");
    let (_, infer_tl) = test_infer_param(&interner, "Tl");

    let extends = make_tuple_with_rest_tail(&interner, vec![infer_f], infer_tl);
    let cond = ConditionalType {
        check_type: u_param,
        extends_type: extends,
        true_type: infer_tl,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let cond_type = interner.conditional(cond);

    let arm_three = make_tuple(
        &interner,
        vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN],
    );
    let arm_two = make_tuple(&interner, vec![TypeId::STRING, TypeId::NUMBER]);
    let arm_one = make_tuple(&interner, vec![TypeId::STRING]);
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, interner.union(vec![arm_three, arm_two, arm_one]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        make_tuple(&interner, vec![TypeId::NUMBER, TypeId::BOOLEAN]),
        make_tuple(&interner, vec![TypeId::NUMBER]),
        make_tuple(&interner, vec![]),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn non_distributive_tuple_wrapper_union_arms_merge_via_contravariance_helper() {
    // [T] extends [[infer Hd, ...unknown[]]] ? Hd : never
    //   with T = [string, number] | [boolean]. Non-distributive but the union
    //   still flows through the contravariance-aware merge — expected
    //   string | boolean.
    let interner = TypeInterner::new();
    let (t_name, t_param) = test_type_param(&interner, "T");
    let (_, infer_hd) = test_infer_param(&interner, "Hd");

    let unknown_arr = interner.array(TypeId::UNKNOWN);
    let inner_pattern = make_tuple_with_rest_tail(&interner, vec![infer_hd], unknown_arr);
    let outer_pattern = make_tuple(&interner, vec![inner_pattern]);
    let outer_check = make_tuple(&interner, vec![t_param]);

    let cond = ConditionalType {
        check_type: outer_check,
        extends_type: outer_pattern,
        true_type: infer_hd,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let arm_a = make_tuple(&interner, vec![TypeId::STRING, TypeId::NUMBER]);
    let arm_b = make_tuple(&interner, vec![TypeId::BOOLEAN]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![arm_a, arm_b]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn non_distributive_tuple_wrapper_union_with_missing_arm_takes_false_branch() {
    // [T] extends [[infer Hd, ...unknown[]]] ? Hd : never
    //   with T = [string, number] | []. The empty arm doesn't fit the head
    //   pattern, so the whole conditional fails and returns the false branch.
    let interner = TypeInterner::new();
    let (t_name, t_param) = test_type_param(&interner, "T");
    let (_, infer_hd) = test_infer_param(&interner, "Hd");

    let unknown_arr = interner.array(TypeId::UNKNOWN);
    let inner_pattern = make_tuple_with_rest_tail(&interner, vec![infer_hd], unknown_arr);
    let outer_pattern = make_tuple(&interner, vec![inner_pattern]);
    let outer_check = make_tuple(&interner, vec![t_param]);

    let cond = ConditionalType {
        check_type: outer_check,
        extends_type: outer_pattern,
        true_type: infer_hd,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let arm_a = make_tuple(&interner, vec![TypeId::STRING, TypeId::NUMBER]);
    let arm_b = make_tuple(&interner, vec![]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![arm_a, arm_b]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn distributive_tuple_union_box_pattern_rebuilds_each_arm() {
    // Rebuild<Seq> = Seq extends [infer F, ...infer Tl] ? [F, ...Tl] : [];
    //   with Seq = [string, string] | [number, number] | [].
    // Distribute -> [string, string] | [number, number] | [] (the [] arm
    //   takes the false branch).
    let interner = TypeInterner::new();
    let (seq_name, seq_param) = test_type_param(&interner, "Seq");
    let (_, infer_f) = test_infer_param(&interner, "F");
    let (_, infer_tl) = test_infer_param(&interner, "Tl");

    let extends = make_tuple_with_rest_tail(&interner, vec![infer_f], infer_tl);
    let true_branch = make_tuple_with_rest_tail(&interner, vec![infer_f], infer_tl);
    let false_branch = make_tuple(&interner, vec![]);

    let cond = ConditionalType {
        check_type: seq_param,
        extends_type: extends,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    };
    let cond_type = interner.conditional(cond);

    let arm_strings = make_tuple(&interner, vec![TypeId::STRING, TypeId::STRING]);
    let arm_numbers = make_tuple(&interner, vec![TypeId::NUMBER, TypeId::NUMBER]);
    let arm_empty = make_tuple(&interner, vec![]);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        seq_name,
        interner.union(vec![arm_strings, arm_numbers, arm_empty]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        make_tuple(&interner, vec![TypeId::STRING, TypeId::STRING]),
        make_tuple(&interner, vec![TypeId::NUMBER, TypeId::NUMBER]),
        make_tuple(&interner, vec![]),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn distributive_tuple_union_readonly_arm_is_recognised_as_tuple() {
    // U extends [infer Hd] ? Hd : never with U = readonly [string] | [number].
    // The readonly variant must be peeled before the tuple shape is observed,
    // otherwise its arm would be lost and the result would be number only.
    let interner = TypeInterner::new();
    let (u_name, u_param) = test_type_param(&interner, "U");
    let (_, infer_hd) = test_infer_param(&interner, "Hd");

    let extends = make_tuple(&interner, vec![infer_hd]);
    let cond = ConditionalType {
        check_type: u_param,
        extends_type: extends,
        true_type: infer_hd,
        false_type: TypeId::NEVER,
        is_distributive: false, // exercise the eval_conditional_tuple_infer Union branch
    };
    let cond_type = interner.conditional(cond);

    let readonly_str = interner.intern(TypeData::ReadonlyType(make_tuple(
        &interner,
        vec![TypeId::STRING],
    )));
    let arm_num = make_tuple(&interner, vec![TypeId::NUMBER]);
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, interner.union(vec![readonly_str, arm_num]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn distributive_tuple_union_no_infer_wrapper_arm_is_recognised_as_tuple() {
    // Same single-element-pattern path, but one arm is `NoInfer<[number]>`.
    // The wrapper must peel before the tuple shape is observed.
    let interner = TypeInterner::new();
    let (u_name, u_param) = test_type_param(&interner, "U");
    let (_, infer_hd) = test_infer_param(&interner, "Hd");

    let extends = make_tuple(&interner, vec![infer_hd]);
    let cond = ConditionalType {
        check_type: u_param,
        extends_type: extends,
        true_type: infer_hd,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let arm_str = make_tuple(&interner, vec![TypeId::STRING]);
    let noinfer_num = interner.intern(TypeData::NoInfer(make_tuple(
        &interner,
        vec![TypeId::NUMBER],
    )));
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, interner.union(vec![arm_str, noinfer_num]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn distributive_tuple_union_renamed_binders_match_canonical_form() {
    // Same pattern with arbitrary binder names to confirm the result is
    // identical (fixture-name independence).
    let interner = TypeInterner::new();
    let (zeta_name, zeta_param) = test_type_param(&interner, "Zeta");
    let (_, infer_alpha) = test_infer_param(&interner, "Alpha");
    let (_, infer_omega) = test_infer_param(&interner, "Omega");

    let extends = make_tuple_with_rest_tail(&interner, vec![infer_alpha], infer_omega);
    let cond = ConditionalType {
        check_type: zeta_param,
        extends_type: extends,
        true_type: infer_alpha,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let cond_type = interner.conditional(cond);

    let arm_a = make_tuple(&interner, vec![TypeId::STRING, TypeId::NUMBER]);
    let arm_b = make_tuple(&interner, vec![TypeId::BOOLEAN]);
    let mut subst = TypeSubstitution::new();
    subst.insert(zeta_name, interner.union(vec![arm_a, arm_b]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}
