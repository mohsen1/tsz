#[test]
fn test_template_literal_hyphen_two_part_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `${infer First}-${infer Rest}` ? [First, Rest] : never
    // Input: "foo-bar-baz" => First = "foo", Rest = "bar-baz"

    let (t_name, t_param) = test_type_param(&interner, "T");

    let infer_first = test_infer_param(&interner, "First").1;
    let infer_rest = test_infer_param(&interner, "Rest").1;

    // T extends `${infer First}-${infer Rest}` ? [First, Rest] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_first),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_rest),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_first,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_rest,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("foo-bar-baz"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // TypeScript uses first-match semantics: First = "foo", Rest = "bar-baz"
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("foo"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("bar-baz"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_suffix_pattern() {
    let interner = TypeInterner::new();

    // Pattern: T extends `${infer R}-handler` ? R : never
    // Input: "click-handler" => R = "click"

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `${infer R}-handler` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("-handler")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("click-handler"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("click");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_distributive_union() {
    let interner = TypeInterner::new();

    // Pattern: T extends `event-${infer R}` ? R : never (distributive)
    // Input: "event-click" | "event-load" => "click" | "load"

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `event-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("event-")),
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
    let lit_click = interner.literal_string("event-click");
    let lit_load = interner.literal_string("event-load");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_click, lit_load]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("click"),
        interner.literal_string("load"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_no_match_returns_never() {
    let interner = TypeInterner::new();

    // Pattern: T extends `prefix-${infer R}` ? R : never
    // Input: "other-value" (doesn't start with "prefix-") => never

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_r) = test_infer_param(&interner, "R");

    // T extends `prefix-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("other-value"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // "other-value" doesn't match pattern "prefix-${infer R}", so returns never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_template_literal_prefix_infer_suffix_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `start-${infer M}-end` ? M : never
    // Input: "start-middle-end" => M = "middle"

    let (t_name, t_param) = test_type_param(&interner, "T");

    let (_infer_name, infer_m) = test_infer_param(&interner, "M");

    // T extends `start-${infer M}-end` ? M : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("start-")),
        TemplateSpan::Type(infer_m),
        TemplateSpan::Text(interner.intern_string("-end")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_m,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("start-middle-end"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("middle");
    assert_eq!(result, expected);
}
