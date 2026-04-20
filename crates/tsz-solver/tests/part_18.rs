use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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
