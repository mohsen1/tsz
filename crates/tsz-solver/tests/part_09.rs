use super::*;
/// Test distributive conditional over intersection type.
///
/// `(string & { length: number }) extends string ? true : false`
/// The intersection should extend string, so result is true.
#[test]
fn test_conditional_intersection_check_type() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // Create intersection: string & { length: number }
    let length_name = interner.intern_string("length");
    let length_obj = interner.object(vec![PropertyInfo::new(length_name, TypeId::NUMBER)]);
    let string_intersection = interner.intersection(vec![TypeId::STRING, length_obj]);

    // (string & { length: number }) extends string ? true : false
    let cond = ConditionalType {
        check_type: string_intersection,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string & {...} extends string should be true (intersection is more specific)
    assert_eq!(
        result, lit_true,
        "string intersection extends string should be true"
    );
}

/// Test conditional with `never` as check type (non-distributive).
///
/// `never extends T ? A : B` should be `never` when distributive is false
/// and check type is exactly `never`.
#[test]
fn test_conditional_never_check_type_non_distributive() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // never extends string ? true : false (non-distributive)
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // With non-distributive, never extends T should evaluate normally
    // never is assignable to everything, so true branch
    assert_eq!(
        result, lit_true,
        "never extends string (non-distributive) should be true"
    );
}

/// Test conditional with `never` extends type.
///
/// `string extends never ? true : false` should be `false`
/// because string is not assignable to never.
#[test]
fn test_conditional_extends_never() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // string extends never ? true : false
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NEVER,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string is not assignable to never, so false branch
    assert_eq!(result, lit_false, "string extends never should be false");
}

/// Test conditional with `never` extends `never`.
///
/// `never extends never ? true : false` should be `true`
/// because never is assignable to never.
#[test]
fn test_conditional_never_extends_never() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // never extends never ? true : false
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NEVER,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // never extends never should be true
    assert_eq!(result, lit_true, "never extends never should be true");
}

/// Test multiple `infer` in tuple pattern.
///
/// `[string, number] extends [infer A, infer B] ? [B, A] : never`
/// Should extract both elements and swap them.
#[test]
fn test_conditional_infer_tuple_multiple_positions() {
    let interner = TypeInterner::new();

    // Create tuple: [string, number]
    let tuple_type = interner.tuple(vec![
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

    // Create infer placeholders for A and B
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create extends pattern: [infer A, infer B]
    let pattern = interner.tuple(vec![
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

    // Create true branch: [B, A] - swapped
    // We reference the inferred types using their positions
    let swapped = interner.tuple(vec![
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: tuple_type,
        extends_type: pattern,
        true_type: swapped,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Expected: [number, string] (swapped)
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert_eq!(
        result, expected,
        "[string, number] with [infer A, infer B] should swap to [number, string]"
    );
}

/// Test nested conditional types (conditional in true branch).
///
/// `T extends string ? (T extends "hello" ? "greeting" : "other") : "not string"`
#[test]
fn test_conditional_nested_in_true_branch() {
    let interner = TypeInterner::new();

    let hello_lit = interner.literal_string("hello");
    let greeting_lit = interner.literal_string("greeting");
    let other_lit = interner.literal_string("other");
    let not_string_lit = interner.literal_string("not string");

    // Inner conditional: "hello" extends "hello" ? "greeting" : "other"
    let inner_cond = interner.conditional(ConditionalType {
        check_type: hello_lit,
        extends_type: hello_lit,
        true_type: greeting_lit,
        false_type: other_lit,
        is_distributive: false,
    });

    // Outer: "hello" extends string ? <inner> : "not string"
    let outer_cond = ConditionalType {
        check_type: hello_lit,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: not_string_lit,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer_cond);

    // "hello" extends string -> true, then "hello" extends "hello" -> "greeting"
    assert_eq!(
        result, greeting_lit,
        "nested conditional should resolve to 'greeting'"
    );
}

/// Test distributive conditional with literal union.
///
/// `("a" | "b" | "c") extends "a" ? "yes" : "no"`
/// Distributes to: ("a" extends "a" ? "yes" : "no") | ("b" extends "a" ? "yes" : "no") | ...
#[test]
fn test_conditional_distributive_literal_union() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let yes_lit = interner.literal_string("yes");
    let no_lit = interner.literal_string("no");

    let abc_union = interner.union(vec![lit_a, lit_b, lit_c]);

    // ("a" | "b" | "c") extends "a" ? "yes" : "no"
    let cond = ConditionalType {
        check_type: abc_union,
        extends_type: lit_a,
        true_type: yes_lit,
        false_type: no_lit,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // "a" -> "yes", "b" -> "no", "c" -> "no"
    // Result: "yes" | "no"
    let expected = interner.union(vec![yes_lit, no_lit]);
    assert_eq!(
        result, expected,
        "distributive over literal union should produce 'yes' | 'no'"
    );
}

/// Test conditional with `any` in extends position.
///
/// `string extends any ? true : false` should be `true`
/// because everything extends any.
#[test]
fn test_conditional_extends_any() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // string extends any ? true : false
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::ANY,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string extends any is always true
    assert_eq!(result, lit_true, "string extends any should be true");
}

/// Test infer with constraint that doesn't match.
///
/// `{ x: number } extends { x: infer T extends string } ? T : never`
/// The constraint `T extends string` doesn't match `number`, so false branch.
#[test]
fn test_conditional_infer_constraint_mismatch_edge() {
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");
    let t_name = interner.intern_string("T");

    // Create { x: number }
    let obj_number = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    // Create infer T extends string
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Create pattern { x: infer T extends string }
    let pattern = interner.object(vec![PropertyInfo::new(x_name, infer_t)]);

    let cond = ConditionalType {
        check_type: obj_number,
        extends_type: pattern,
        true_type: infer_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // number doesn't satisfy constraint `extends string`, so false branch
    assert_eq!(
        result,
        TypeId::NEVER,
        "infer with mismatched constraint should produce never"
    );
}

// =========================================================================
// Template Literal Type Inference - Hyphen Pattern Tests
// =========================================================================
// Tests for template literal patterns using hyphen separators like `hello-${string}`

#[test]
fn test_template_literal_hyphen_prefix_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `hello-${infer R}` ? R : never
    // Input: "hello-world" => R = "world"

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

    // T extends `hello-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello-")),
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
    subst.insert(t_name, interner.literal_string("hello-world"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("world");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_two_part_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `${infer First}-${infer Rest}` ? [First, Rest] : never
    // Input: "foo-bar-baz" => First = "foo", Rest = "bar-baz"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("First"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Rest"),
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("M");
    let infer_m = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

#[test]
fn test_template_literal_prefix_infer_suffix_multiple_hyphens() {
    let interner = TypeInterner::new();

    // Pattern: T extends `api-${infer Route}-handler` ? Route : never
    // Input: "api-user-profile-handler" => Route = "user-profile"
    // The infer captures everything between "api-" and "-handler"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Route");
    let infer_route = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `api-${infer Route}-handler` ? Route : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("api-")),
        TemplateSpan::Type(infer_route),
        TemplateSpan::Text(interner.intern_string("-handler")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_route,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("api-user-profile-handler"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Captures everything between "api-" and "-handler"
    let expected = interner.literal_string("user-profile");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_prefix_infer_suffix_distributive() {
    let interner = TypeInterner::new();

    // Pattern: T extends `on-${infer E}-event` ? E : never (distributive)
    // Input: "on-click-event" | "on-load-event" => "click" | "load"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `on-${infer E}-event` ? E : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("on-")),
        TemplateSpan::Type(infer_e),
        TemplateSpan::Text(interner.intern_string("-event")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let lit_click = interner.literal_string("on-click-event");
    let lit_load = interner.literal_string("on-load-event");
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

// =========================================================================
// Template Literal Type Inference - Number Extraction Pattern Tests
// =========================================================================
// Tests for template literal patterns that extract numeric strings

#[test]
fn test_template_literal_extract_numeric_id() {
    let interner = TypeInterner::new();

    // Pattern: T extends `user-${infer Id}` ? Id : never
    // Input: "user-42" => Id = "42"
    // Common pattern for extracting numeric IDs from string keys

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Id");
    let infer_id = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `user-${infer Id}` ? Id : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("user-")),
        TemplateSpan::Type(infer_id),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_id,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("user-42"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts "42" as a string literal
    let expected = interner.literal_string("42");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_version_numbers() {
    let interner = TypeInterner::new();

    // Pattern: T extends `v${infer Major}.${infer Minor}` ? [Major, Minor] : never
    // Input: "v1.2" => [Major, Minor] = ["1", "2"]
    // Common pattern for parsing version strings

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_major = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Major"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_minor = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Minor"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `v${infer Major}.${infer Minor}` ? [Major, Minor] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("v")),
        TemplateSpan::Type(infer_major),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(infer_minor),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_major,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_minor,
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
    subst.insert(t_name, interner.literal_string("v1.2"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts ["1", "2"]
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("1"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("2"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_index_from_array_key() {
    let interner = TypeInterner::new();

    // Pattern: T extends `item[${infer Index}]` ? Index : never
    // Input: "item[0]" | "item[1]" | "item[2]" => "0" | "1" | "2"
    // Common pattern for extracting array indices from bracket notation

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Index");
    let infer_index = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `item[${infer Index}]` ? Index : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item[")),
        TemplateSpan::Type(infer_index),
        TemplateSpan::Text(interner.intern_string("]")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_index,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let lit_0 = interner.literal_string("item[0]");
    let lit_1 = interner.literal_string("item[1]");
    let lit_2 = interner.literal_string("item[2]");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_0, lit_1, lit_2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts "0" | "1" | "2"
    let expected = interner.union(vec![
        interner.literal_string("0"),
        interner.literal_string("1"),
        interner.literal_string("2"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_port_number() {
    let interner = TypeInterner::new();

    // Pattern: T extends `localhost:${infer Port}` ? Port : never
    // Input: "localhost:3000" => Port = "3000"
    // Common pattern for extracting port numbers from host strings

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Port");
    let infer_port = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `localhost:${infer Port}` ? Port : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("localhost:")),
        TemplateSpan::Type(infer_port),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_port,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("localhost:3000"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("3000");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_coordinates() {
    let interner = TypeInterner::new();

    // Pattern: T extends `(${infer X},${infer Y})` ? [X, Y] : never
    // Input: "(10,20)" => [X, Y] = ["10", "20"]
    // Common pattern for parsing coordinate pairs

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_x = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("X"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Y"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `(${infer X},${infer Y})` ? [X, Y] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("(")),
        TemplateSpan::Type(infer_x),
        TemplateSpan::Text(interner.intern_string(",")),
        TemplateSpan::Type(infer_y),
        TemplateSpan::Text(interner.intern_string(")")),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_x,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_y,
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
    subst.insert(t_name, interner.literal_string("(10,20)"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("10"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("20"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

// =============================================================================
// Variadic Tuple Type Tests
// =============================================================================

#[test]
fn test_variadic_tuple_spread_at_end() {
    // Test: [string, ...number[]] - variadic tuple with spread at end
    let interner = TypeInterner::new();

    // Create [string, ...number[]]
    let number_array = interner.array(TypeId::NUMBER);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Verify the tuple was created as a tuple type
    assert!(matches!(
        interner.lookup(variadic_tuple),
        Some(TypeData::Tuple(_))
    ));
    assert_ne!(variadic_tuple, TypeId::NEVER);
    assert_ne!(variadic_tuple, TypeId::UNKNOWN);
}

#[test]
fn test_variadic_tuple_spread_at_start() {
    // Test: [...string[], number] - variadic tuple with spread at start
    let interner = TypeInterner::new();

    // Create [...string[], number]
    let string_array = interner.array(TypeId::STRING);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Verify the tuple was created as a tuple type
    assert!(matches!(
        interner.lookup(variadic_tuple),
        Some(TypeData::Tuple(_))
    ));
    assert_ne!(variadic_tuple, TypeId::NEVER);
    assert_ne!(variadic_tuple, TypeId::UNKNOWN);
}

#[test]
fn test_variadic_tuple_infer_rest_elements() {
    // Test: T extends [first, ...infer Rest] ? Rest : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let rest_name = interner.intern_string("Rest");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [string, ...infer Rest]
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_rest,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [string, number, boolean]
    let input_tuple = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Rest should be [number, boolean]
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_variadic_tuple_infer_first_element() {
    // Test: T extends [infer First, ...infer Rest] ? First : never
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

    let infer_first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: first_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer First, ...infer Rest]
    let extends_tuple = interner.tuple(vec![
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
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_first,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [number, string, boolean]
    let input_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // First should be number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_variadic_tuple_empty_rest() {
    // Test: [string] extends [string, ...infer R] ? R : never
    // Should produce empty tuple []
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [string, ...infer R]
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [string] - only one element
    let input_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // R should be empty tuple []
    let expected = interner.tuple(Vec::new());
    assert_eq!(result, expected);
}

// =========================================================================
// KeyOf and Indexed Access Type Tests - Additional Scenarios
// =========================================================================
// Tests for keyof and indexed access types in complex scenarios

#[test]
fn test_keyof_with_index_access_combination() {
    let interner = TypeInterner::new();

    // Pattern: { [K in keyof T]: T[K] } - identity mapped type
    // Object: { name: string, age: number }
    // keyof T = "name" | "age", T[K] produces the value types

    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");

    let obj = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);

    // Should produce "age" | "name" (order determined by interner)
    let expected = interner.union(vec![
        interner.literal_string("age"),
        interner.literal_string("name"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_with_keyof() {
    let interner = TypeInterner::new();

    // Pattern: T[keyof T] - get all value types from object
    // Object: { x: string, y: number }
    // T[keyof T] = string | number

    let x_prop = interner.intern_string("x");
    let y_prop = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::STRING),
        PropertyInfo::new(y_prop, TypeId::NUMBER),
    ]);

    // Access with "x" key
    let key_x = interner.literal_string("x");
    let result_x = evaluate_index_access(&interner, obj, key_x);
    assert_eq!(result_x, TypeId::STRING);

    // Access with "y" key
    let key_y = interner.literal_string("y");
    let result_y = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result_y, TypeId::NUMBER);
}

#[test]
fn test_index_access_nested_object() {
    let interner = TypeInterner::new();

    // Pattern: T["outer"]["inner"]
    // Object: { outer: { inner: string } }

    let inner_prop = interner.intern_string("inner");
    let inner_obj = interner.object(vec![PropertyInfo::new(inner_prop, TypeId::STRING)]);

    let outer_prop = interner.intern_string("outer");
    let outer_obj = interner.object(vec![PropertyInfo::new(outer_prop, inner_obj)]);

    // First access: T["outer"]
    let outer_key = interner.literal_string("outer");
    let first_result = evaluate_index_access(&interner, outer_obj, outer_key);

    // First result should be the inner object
    assert_eq!(first_result, inner_obj);

    // Second access: T["outer"]["inner"]
    let inner_key = interner.literal_string("inner");
    let final_result = evaluate_index_access(&interner, first_result, inner_key);

    // Final result should be string
    assert_eq!(final_result, TypeId::STRING);
}

// =============================================================================
// INDEXED ACCESS TYPE TESTS
// =============================================================================

/// Test basic indexed access with literal key.
///
/// { a: string, b: number }["a"] should be string.
#[test]
fn test_indexed_access_basic_literal_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);
    assert_eq!(result, TypeId::STRING);

    let key_b = interner.literal_string("b");
    let result_b = evaluate_index_access(&interner, obj, key_b);
    assert_eq!(result_b, TypeId::NUMBER);
}

/// Test indexed access with union key produces union type.
///
/// { a: string, b: number, c: boolean }["a" | "b"] should be string | number.
#[test]
fn test_indexed_access_union_key_produces_union() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

/// Test indexed access with triple union key.
///
/// { a: string, b: number, c: boolean }["a" | "b" | "c"] should be string | number | boolean.
#[test]
fn test_indexed_access_triple_union_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let key_union = interner.union(vec![key_a, key_b, key_c]);

    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

/// Test recursive indexed access for nested objects.
///
/// { outer: { middle: { inner: string } } }["outer"]["middle"]["inner"] should be string.
#[test]
fn test_indexed_access_recursive_three_levels() {
    let interner = TypeInterner::new();

    // Build innermost object: { inner: string }
    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);

    // Build middle object: { middle: { inner: string } }
    let middle_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("middle"),
        inner_obj,
    )]);

    // Build outer object: { outer: { middle: { inner: string } } }
    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outer"),
        middle_obj,
    )]);

    // Access T["outer"]
    let outer_key = interner.literal_string("outer");
    let first_result = evaluate_index_access(&interner, outer_obj, outer_key);
    assert_eq!(first_result, middle_obj);

    // Access T["outer"]["middle"]
    let middle_key = interner.literal_string("middle");
    let second_result = evaluate_index_access(&interner, first_result, middle_key);
    assert_eq!(second_result, inner_obj);

    // Access T["outer"]["middle"]["inner"]
    let inner_key = interner.literal_string("inner");
    let final_result = evaluate_index_access(&interner, second_result, inner_key);
    assert_eq!(final_result, TypeId::STRING);
}

/// Test indexed access on optional property includes undefined.
///
/// { a?: string }["a"] should be string | undefined.
#[test]
fn test_indexed_access_optional_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true, // optional property
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    // Optional property access should include undefined
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

/// Test indexed access with mix of required and optional properties.
///
/// { a: string, b?: number }["a" | "b"] should be string | number | undefined.
#[test]
fn test_indexed_access_mixed_optional_required() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // required
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // optional
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);

    // Union access includes all types + undefined from optional
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

/// Test indexed access on array type with number key.
///
/// string[][number] should be string.
#[test]
fn test_indexed_access_array_number_key() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

/// Test indexed access on tuple with literal index.
///
/// [string, number, boolean][1] should be number.
#[test]
fn test_indexed_access_tuple_literal_index() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let key_0 = interner.literal_number(0.0);
    let result_0 = evaluate_index_access(&interner, tuple, key_0);
    assert_eq!(result_0, TypeId::STRING);

    let key_1 = interner.literal_number(1.0);
    let result_1 = evaluate_index_access(&interner, tuple, key_1);
    assert_eq!(result_1, TypeId::NUMBER);

    let key_2 = interner.literal_number(2.0);
    let result_2 = evaluate_index_access(&interner, tuple, key_2);
    assert_eq!(result_2, TypeId::BOOLEAN);
}

/// Test indexed access with union of objects.
///
/// ({ a: string } | { a: number })["a"] should be string | number.
#[test]
fn test_indexed_access_union_object() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let union_obj = interner.union(vec![obj1, obj2]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, union_obj, key_a);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

/// Test indexed access with all optional properties.
///
/// { a?: string, b?: number }["a" | "b"] should be string | number | undefined.
#[test]
fn test_indexed_access_all_optional_properties() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

