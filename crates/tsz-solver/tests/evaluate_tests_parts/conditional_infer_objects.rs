#[test]
fn test_conditional_infer_template_literal_from_string_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `${infer R}` ? R : never, with T = string.
    // tsc: primitive `string` does NOT extend a template literal pattern → never.
    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_template_literal_from_template_string_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `${infer R}` ? R : never, with T = `${string}`.
    // `${string}` spans the full string domain and collapses to `string`, and
    // tsc treats `string extends `${infer R}`` as the false branch (a bare
    // primitive does not match a template pattern) → never. This mirrors
    // `test_conditional_infer_template_literal_from_string_input`, since
    // `${string}` and `string` are the same type.
    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let template_string = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);
    assert_eq!(template_string, TypeId::STRING);
    subst.insert(t_name, template_string);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_template_literal_with_middle_infer_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `foo${infer R}bar` ? R : never, with T = "foobazbar" | "bar".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("bar")),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_match = interner.literal_string("foobazbar");
    let lit_other = interner.literal_string("bar");
    subst.insert(t_name, interner.union(vec![lit_match, lit_other]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("baz");
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_template_literal_two_infers_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_a_name, infer_a) = test_infer_param(&interner, "A");
    let (_infer_b_name, infer_b) = test_infer_param(&interner, "B");

    // T extends `${infer A}-${infer B}` ? A | B : never, with T = "foo-bar" | "baz-qux".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: interner.union(vec![infer_a, infer_b]),
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_left = interner.literal_string("foo-bar");
    let lit_right = interner.literal_string("baz-qux");
    subst.insert(t_name, interner.union(vec![lit_left, lit_right]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("foo"),
        interner.literal_string("baz"),
        interner.literal_string("bar"),
        interner.literal_string("qux"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_template_literal_with_constrained_infer_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends `foo${infer R extends string}` ? R : never, with T = "foo1" | "foo2".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_foo1 = interner.literal_string("foo1");
    let lit_foo2 = interner.literal_string("foo2");
    subst.insert(t_name, interner.union(vec![lit_foo1, lit_foo2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("1"),
        interner.literal_string("2"),
    ]);
    assert_eq!(result, expected);
}

/// Primitive string does NOT match a template literal infer pattern.
///
/// tsc rule: when `check_type` is the primitive string, the false branch is taken.
/// Only concrete string literals and template literal source types can match.
#[test]
fn test_conditional_primitive_string_does_not_match_template_infer_pattern() {
    let interner = TypeInterner::new();

    // Infer variable name R (test: rule holds regardless of name choice)
    let (_infer_name_r, infer_r) = test_infer_param(&interner, "R");

    // `string extends \`${infer R}\` ? true : false` — non-distributive direct check
    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: extends_template,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_type);

    assert_eq!(
        result,
        TypeId::BOOLEAN_FALSE,
        "primitive string should NOT match template literal pattern"
    );
}

/// Same rule with infer var named X — proves the rule is structural, not name-dependent.
#[test]
fn test_conditional_primitive_string_does_not_match_template_infer_pattern_any_name() {
    let interner = TypeInterner::new();

    let (_infer_name_x, infer_x) = test_infer_param(&interner, "X");

    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_x)]);
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: extends_template,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(result, TypeId::BOOLEAN_FALSE);
}

/// Primitive string against a template with a text prefix — regression guard.
/// Verifies that primitive string stays false for prefixed templates (pre-existing behavior).
#[test]
fn test_conditional_primitive_string_prefixed_template_stays_false() {
    let interner = TypeInterner::new();

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    let prefix = interner.intern_string("prefix_");
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: extends_template,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(result, TypeId::BOOLEAN_FALSE);
}

/// String literal "hello" against a template infer pattern — should still yield "hello".
/// String literals continue to match template patterns correctly.
#[test]
fn test_conditional_string_literal_still_matches_template_infer_pattern() {
    let interner = TypeInterner::new();

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: interner.literal_string("hello"),
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(result, interner.literal_string("hello"));
}

/// A genuine (non-collapsing) template literal source matches a structurally
/// aligned template infer pattern, capturing the `${string}` segment.
/// `` `x${string}` extends `x${infer R}` ? R : never `` yields `string` in tsc.
/// (A bare `` `${string}` `` collapses to `string`, which does NOT match a
/// template infer pattern — covered by the `from_string_input` tests.)
#[test]
fn test_conditional_template_literal_source_still_matches_template_infer_pattern() {
    let interner = TypeInterner::new();

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    let prefix = interner.intern_string("x");
    let source_template = interner.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: source_template,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_nested_object_property_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { b: number } } (no distribution).
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | number (no distribution).
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_nested_object_property_with_constraint() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R extends string } } ? R : never, with T = { a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { readonly a: { b: infer R } } ? R : never, with T = { readonly a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | { a: { b: number } }.
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | { a: { b: number } } (no distribution).
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | number (no distribution).
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_nested_object_property_non_matching_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { c: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::NUMBER,
    )]);
    let obj_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_non_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_match, obj_non_match]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    // For { a: { b: string } }: matches, R = string
    // For { a: { c: number } }: doesn't match (no 'b' property), goes to false branch = never
    // Union: string | never = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_nested_object_property_union_value() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string | number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let b_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_union,
    )]);
    let obj = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), obj_a)]);
    subst.insert(t_name, obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, b_union);
}

#[test]
fn test_conditional_infer_object_property_non_object_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a: infer R } ? R : never, with T = { a: string } | number.
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_match, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    // For { a: string }: matches, R = string
    // For number: doesn't match (not an object), goes to false branch = never
    // Union: string | never = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_object_property_non_distributive_non_object_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ a: infer R }] ? R : never, with T = { a: string } | number (no distribution).
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_r,
    )]);
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
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_match, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_object_index_signature_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: string]: infer R } ? R : never, with T = { a: string } | { b: number }.
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_number_index_signature_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: number]: infer R } ? R : never, with T = { 0: string } | { 1: number }.
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("1"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_number_index_signature_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: number]: infer R } ? R : never, with T = { 0: string } | { 1: number } (no distribution).
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("1"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_number_index_signature_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: number]: infer R } ? R : never, with T = { 0: string } | number (no distribution).
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_object_index_signature_non_object_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: string]: infer R } ? R : never, with T = { a: string } | number.
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_object_index_signature_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: string]: infer R } ? R : never, with T = { a: string } | { b: number } (no distribution).
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_index_signature_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { [key: string]: infer R } ? R : never, with T = { a: string } | number (no distribution).
    let extends_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_optional_property_missing_object() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a?: infer R } ? R : never, with T = {}.
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let empty_obj = interner.object(Vec::new());
    subst.insert(t_name, empty_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // `{}` matches `{ a?: infer R }` (the optional `a` is simply absent), so the
    // conditional takes its true branch. `a` is absent in the source, so it
    // contributes no inference candidate; an unconstrained `R` therefore defaults
    // to `unknown` (tsc's `getInferredType`), not `undefined`.
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_conditional_infer_optional_property_present_distributive() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends { a?: infer R } ? R : never, with T = { a?: string } | { a?: number }.
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_optional_property_with_constraint() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends { a?: infer R extends string } ? R : never, with T = { a?: string } | { a?: number }.
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_optional_property_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ a?: infer R }] ? R : never, with T = { a: string } | {} (no distribution).
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
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
        false_type: TypeId::NEVER,
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
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_optional_property_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // [T] extends [{ a?: infer R }] ? R : never, with T = { a: string } | number (no distribution).
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        infer_r,
    )]);
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
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// Absent optional property infer tests — general path (tuple-wrapped to prevent
// the fast path from handling it, proving the fix covers the general pattern).
// =============================================================================

#[test]
fn test_conditional_infer_absent_optional_renamed_var_union_input() {
    // Same as test_conditional_infer_optional_property_non_distributive_union_input
    // but with infer variable named `K` instead of `R` to verify the fix is not
    // hardcoded to a specific identifier.
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_, infer_k) = test_infer_param(&interner, "K");

    // [T] extends [{ v?: infer K }] ? K : never, with T = { v: number } | {} (no distribution).
    let extends_obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("v"),
        infer_k,
    )]);
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
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("v"),
        TypeId::NUMBER,
    )]);
    let empty_obj = interner.object(Vec::new());
    subst.insert(t_name, interner.union(vec![obj_number, empty_obj]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

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
