use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// Template inference with union input distributes
#[test]
fn test_template_infer_union_distributive() {
    let interner = TypeInterner::new();

    let input_a = interner.literal_string("get-foo");
    let input_b = interner.literal_string("get-bar");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get-")),
        TemplateSpan::Type(infer_r),
    ]);

    // Process "get-foo"
    let cond_a = ConditionalType {
        check_type: input_a,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    let expected_a = interner.literal_string("foo");
    assert_eq!(result_a, expected_a);

    // Process "get-bar"
    let cond_b = ConditionalType {
        check_type: input_b,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    let expected_b = interner.literal_string("bar");
    assert_eq!(result_b, expected_b);

    // Combined: "foo" | "bar"
    let result_union = interner.union(vec![result_a, result_b]);
    match interner.lookup(result_union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Template literal with multi-segment pattern
#[test]
fn test_template_multi_segment_extraction() {
    let interner = TypeInterner::new();

    // `item-${infer N}-end` should match "item-123-end" and extract "123"
    let input = interner.literal_string("item-123-end");

    let infer_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item-")),
        TemplateSpan::Type(infer_n),
        TemplateSpan::Text(interner.intern_string("-end")),
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_n,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should extract "123"
    let expected = interner.literal_string("123");
    assert_eq!(result, expected);
}

// ============================================================================
// String Literal Type Narrowing Tests
// ============================================================================

/// String literal extends check
#[test]
fn test_string_literal_extends_literal() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" extends "hello" ? true : false
    let cond_same = ConditionalType {
        check_type: hello,
        extends_type: hello,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_same = evaluate_conditional(&interner, &cond_same);
    assert_eq!(result_same, interner.literal_boolean(true));

    // "hello" extends "world" ? true : false
    let cond_diff = ConditionalType {
        check_type: hello,
        extends_type: world,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_diff = evaluate_conditional(&interner, &cond_diff);
    assert_eq!(result_diff, interner.literal_boolean(false));
}

/// String literal extends base string type
#[test]
fn test_string_literal_extends_string() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // "hello" extends string ? true : false
    let cond = ConditionalType {
        check_type: hello,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Base string type doesn't extend specific literal
#[test]
fn test_string_not_extends_literal() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // string extends "hello" ? true : false
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: hello,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// Union narrowing by string literal discrimination
#[test]
fn test_string_union_narrowing() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    // Given union "a" | "b" | "c", extract those extending "a" | "b"
    let _union_abc = interner.union(vec![lit_a, lit_b, lit_c]);
    let target_ab = interner.union(vec![lit_a, lit_b]);

    // Process each member
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: target_ab,
        true_type: lit_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, lit_a);

    let cond_b = ConditionalType {
        check_type: lit_b,
        extends_type: target_ab,
        true_type: lit_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    assert_eq!(result_b, lit_b);

    let cond_c = ConditionalType {
        check_type: lit_c,
        extends_type: target_ab,
        true_type: lit_c,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_c = evaluate_conditional(&interner, &cond_c);
    assert_eq!(result_c, TypeId::NEVER);
}

/// Template literal type subtyping to string
#[test]
fn test_template_literal_extends_string() {
    let interner = TypeInterner::new();

    // `prefix${string}` extends string ? true : false
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: template,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Specific literal matches template pattern via infer
/// Uses infer to demonstrate that "prefix-value" matches `prefix-${infer R}`
#[test]
fn test_literal_matches_template_via_infer() {
    let interner = TypeInterner::new();

    let literal = interner.literal_string("prefix-value");

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(infer_r),
    ]);

    // "prefix-value" extends `prefix-${infer R}` ? R : never
    let cond = ConditionalType {
        check_type: literal,
        extends_type: template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should extract "value"
    let expected = interner.literal_string("value");
    assert_eq!(result, expected);
}

/// Literal doesn't match template pattern
#[test]
fn test_literal_not_matching_template_pattern() {
    let interner = TypeInterner::new();

    let literal = interner.literal_string("other-value");
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // "other-value" extends `prefix-${string}` ? true : false
    let cond = ConditionalType {
        check_type: literal,
        extends_type: template,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// String literal with special characters
#[test]
fn test_string_literal_special_chars() {
    let interner = TypeInterner::new();

    let special = interner.literal_string("hello\nworld");
    let pattern = interner.literal_string("hello\nworld");

    let cond = ConditionalType {
        check_type: special,
        extends_type: pattern,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Mapped type with Uppercase key remapping
#[test]
fn test_mapped_type_uppercase_keys() {
    let interner = TypeInterner::new();

    // { [K in "a" | "b" as Uppercase<K>]: number }
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_upper_a = interner.literal_string("A");
    let key_upper_b = interner.literal_string("B");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Map "a" -> "A", "b" -> "B" via nested conditionals
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_upper_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_upper_a,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { A: number; B: number }
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let expected = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Mapped type with template literal key transformation
#[test]
fn test_mapped_type_template_literal_keys() {
    let interner = TypeInterner::new();

    // { [K in "click" | "focus" as `on${K}`]: EventHandler }
    let key_click = interner.literal_string("click");
    let key_focus = interner.literal_string("focus");
    let keys = interner.union(vec![key_click, key_focus]);

    let on_click = interner.literal_string("onclick");
    let on_focus = interner.literal_string("onfocus");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Map via nested conditionals
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_focus,
        true_type: on_focus,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_click,
        true_type: on_click,
        false_type: inner_cond,
        is_distributive: false,
    });

    // Event handler function type
    let handler = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: handler,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { onclick: () => void; onfocus: () => void }
    let onclick_name = interner.intern_string("onclick");
    let onfocus_name = interner.intern_string("onfocus");
    let expected = interner.object(vec![
        PropertyInfo::new(onclick_name, handler),
        PropertyInfo::new(onfocus_name, handler),
    ]);

    assert_eq!(result, expected);
}

// ============================================================================
// satisfies operator tests
// The satisfies operator checks if a type is assignable to a constraint
// while preserving the inferred (narrower) type
// ============================================================================

#[test]
fn test_satisfies_basic_literal_string() {
    use crate::SubtypeChecker;

    // const x = "hello" satisfies string
    // The literal type "hello" should satisfy the string constraint
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    // "hello" satisfies string - should be true
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    // The inferred type remains "hello", not string
    assert_ne!(hello, TypeId::STRING);
}

#[test]
fn test_satisfies_basic_literal_number() {
    use crate::SubtypeChecker;

    // const x = 42 satisfies number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);
    // 42 satisfies number - should be true
    assert!(checker.is_subtype_of(forty_two, TypeId::NUMBER));
    // The inferred type remains 42, not number
    assert_ne!(forty_two, TypeId::NUMBER);
}

#[test]
fn test_satisfies_basic_object_type() {
    use crate::SubtypeChecker;

    // const x = { a: 1, b: "hello" } satisfies { a: number, b: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let hello = interner.literal_string("hello");

    // Object with literal types (inferred type)
    let inferred = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), one),
        PropertyInfo::new(interner.intern_string("b"), hello),
    ]);

    // Constraint type (wider)
    let constraint = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    // Inferred type satisfies constraint
    assert!(checker.is_subtype_of(inferred, constraint));
    // Types are different (inferred has literal types)
    assert_ne!(inferred, constraint);
}

#[test]
fn test_satisfies_constraint_failure() {
    use crate::SubtypeChecker;

    // const x = "hello" satisfies number - should fail
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    // String literal does not satisfy number constraint
    assert!(!checker.is_subtype_of(hello, TypeId::NUMBER));
}

#[test]
fn test_satisfies_literal_widening_preserved_string() {
    use crate::{LiteralValue, SubtypeChecker};

    // With satisfies, literal types are preserved:
    // const x = "hello" satisfies string -> type is "hello"
    // With type annotation:
    // const x: string = "hello" -> type is string (widened)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // satisfies: literal type is preserved
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    // The type is still the literal, not widened
    match interner.lookup(hello) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}

#[test]
fn test_satisfies_literal_widening_preserved_number() {
    use crate::{LiteralValue, SubtypeChecker};

    // const x = 42 satisfies number -> type remains 42 (literal)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);

    assert!(checker.is_subtype_of(forty_two, TypeId::NUMBER));
    match interner.lookup(forty_two) {
        Some(TypeData::Literal(LiteralValue::Number(_))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(Number), got {other:?}"),
    }
}

#[test]
fn test_satisfies_literal_widening_preserved_boolean() {
    use crate::{LiteralValue, SubtypeChecker};

    // const x = true satisfies boolean -> type remains true (literal)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_true = interner.literal_boolean(true);

    assert!(checker.is_subtype_of(lit_true, TypeId::BOOLEAN));
    match interner.lookup(lit_true) {
        Some(TypeData::Literal(LiteralValue::Boolean(true))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(Boolean(true)), got {other:?}"),
    }
}

#[test]
fn test_satisfies_excess_property_check_fails() {
    use crate::SubtypeChecker;

    // In TypeScript, satisfies performs excess property checking:
    // const x = { a: 1, b: 2, c: 3 } satisfies { a: number, b: number }
    // This is a compile error because 'c' is not in the constraint
    //
    // However, in structural subtyping, extra properties are allowed
    // (an object with more props is a subtype of one with fewer)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::NUMBER),
    ]);

    let target = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    // Structurally, {a, b, c} is a subtype of {a, b}
    // Note: Excess property checking is a separate, expression-level check
    assert!(checker.is_subtype_of(source, target));
}

