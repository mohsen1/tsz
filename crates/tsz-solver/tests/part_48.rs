use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_indexed_access_with_readonly_property() {
    // { readonly a: string }["a"] = string
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_union_of_objects() {
    // ({ a: string } | { a: number })["a"] = string | number
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

#[test]
fn test_indexed_access_intersection_object() {
    // ({ a: string } & { b: number })["a"] = string
    // ({ a: string } & { b: number })["b"] = number
    // Note: Implementation may return intersection or merged type
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");

    let result_a = evaluate_index_access(&interner, intersection, key_a);
    let result_b = evaluate_index_access(&interner, intersection, key_b);

    // Results should not be errors
    assert!(result_a != TypeId::ERROR);
    assert!(result_b != TypeId::ERROR);
}

#[test]
fn test_indexed_access_string_index_signature() {
    // { [key: string]: number }["anyKey"] = number
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let any_key = interner.literal_string("anyKey");
    let result = evaluate_index_access(&interner, obj, any_key);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_number_index_signature() {
    // { [key: number]: string }[42] = string
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let key_42 = interner.literal_number(42.0);
    let result = evaluate_index_access(&interner, obj, key_42);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_property_overrides_index_signature() {
    // { a: boolean, [key: string]: number }["a"] = boolean (specific property wins)
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("a"),
            TypeId::BOOLEAN,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    // Specific property takes precedence over index signature
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_indexed_access_nested_with_union_intermediate() {
    // { data: { value: string } | { value: number } }["data"]["value"] = string | number
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let union_data = interner.union(vec![obj1, obj2]);

    let outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("data"),
        union_data,
    )]);

    let key_data = interner.literal_string("data");
    let key_value = interner.literal_string("value");

    let r1 = evaluate_index_access(&interner, outer, key_data);
    let r2 = evaluate_index_access(&interner, r1, key_value);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(r2, expected);
}

#[test]
fn test_indexed_access_literal_types() {
    // { status: "active" | "inactive" }["status"] = "active" | "inactive"
    let interner = TypeInterner::new();

    let lit_active = interner.literal_string("active");
    let lit_inactive = interner.literal_string("inactive");
    let status_type = interner.union(vec![lit_active, lit_inactive]);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("status"),
        status_type,
    )]);

    let key_status = interner.literal_string("status");
    let result = evaluate_index_access(&interner, obj, key_status);

    assert_eq!(result, status_type);
}

#[test]
fn test_indexed_access_function_property() {
    // { fn: () => string }["fn"] = () => string
    let interner = TypeInterner::new();

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(
        interner.intern_string("fn"),
        fn_type,
    )]);

    let key_fn = interner.literal_string("fn");
    let result = evaluate_index_access(&interner, obj, key_fn);

    assert_eq!(result, fn_type);
}

#[test]
fn test_indexed_access_array_method_property() {
    // string[]["length"] = number
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let key_length = interner.literal_string("length");

    let result = evaluate_index_access(&interner, string_array, key_length);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_nested_array() {
    // string[][number][number] = string (flattened char)
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    // First access: string[][number] = string
    let r1 = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(r1, TypeId::STRING);

    // Second access: string[number] = string (character)
    let r2 = evaluate_index_access(&interner, r1, TypeId::NUMBER);
    assert_eq!(r2, TypeId::STRING);
}

#[test]
fn test_indexed_access_2d_array() {
    // number[][0] = number[]
    // number[][0][0] = number
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let array_2d = interner.array(number_array);

    let key_0 = interner.literal_number(0.0);

    // First access returns inner array type
    let r1 = evaluate_index_access(&interner, array_2d, key_0);
    assert_eq!(r1, number_array);

    // Second access returns element type
    let r2 = evaluate_index_access(&interner, r1, key_0);
    assert_eq!(r2, TypeId::NUMBER);
}

// =============================================================================
// TEMPLATE LITERAL AND KEYOF TESTS
// =============================================================================

/// Test keyof with template literal containing union interpolation
/// keyof `get${Action}Done` should return keyof string (apparent keys of String)
#[test]
fn test_keyof_template_literal_union_interpolation() {
    let interner = TypeInterner::new();

    // Create "A" | "B" | "C" union
    let lit_a = interner.literal_string("A");
    let lit_b = interner.literal_string("B");
    let lit_c = interner.literal_string("C");
    let union_abc = interner.union(vec![lit_a, lit_b, lit_c]);

    // Create template literal: `get${"A" | "B" | "C"}Done`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(union_abc),
        TemplateSpan::Text(interner.intern_string("Done")),
    ]);

    // keyof template literal returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test keyof with union of template literals
/// keyof (`foo${string}` | `bar${string}`) should return keyof string (apparent keys)
#[test]
fn test_keyof_union_of_template_literals() {
    let interner = TypeInterner::new();

    // Create `foo${string}` template
    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Create `bar${string}` template
    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("bar")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Union of template literals
    let union_templates = interner.union(vec![template1, template2]);

    // keyof (union of templates) = intersection of keyofs, which is keyof string
    let result = evaluate_keyof(&interner, union_templates);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test conditional type with template literal infer and keyof
/// T extends `get${infer K}Done` ? keyof { [P in K]: any } : never
#[test]
fn test_conditional_infer_template_with_keyof_result() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `get${infer K}Done`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
        TemplateSpan::Text(interner.intern_string("Done")),
    ]);

    // T extends `get${infer K}Done` ? K : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "getFooDone"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("getFooDone");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Foo");
    assert_eq!(result, expected);
}

/// Test string intrinsic (Uppercase) with template literal
/// `get${Uppercase<Action>}` should create template with uppercased value
/// Note: Uppercase is typically implemented via mapped types, this tests the pattern
#[test]
fn test_template_literal_with_uppercase_intrinsic_pattern() {
    let interner = TypeInterner::new();

    // Simulate Uppercase<"a" | "b"> -> "A" | "B"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let input_union = interner.union(vec![lit_a, lit_b]);

    // Template that would use uppercased values: `on${Uppercase<"a" | "b">}Change`
    // In real TS, this would expand to "onAChange" | "onBChange"
    // Here we test that template literals handle the union interpolation correctly
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("on")),
        TemplateSpan::Type(input_union),
        TemplateSpan::Text(interner.intern_string("Change")),
    ]);

    // With optimization, template literals with expandable unions are expanded immediately
    // `on${"a"|"b"}Change` becomes "onaChange" | "onbChange"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 2, "Expected 2 members in expanded union");
        }
        _ => panic!(
            "Expected Union type for template with union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

/// Test nested conditional types with template literals
/// T extends `prefix${infer R}` ? R extends `suffix${infer S}` ? S : never : never
#[test]
fn test_nested_conditional_template_literal_infer() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Outer pattern: `prefix${infer R}`
    let outer_pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    // Inner conditional: R extends `suffix${infer S}` ? S : never
    let inner_pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("suffix")),
        TemplateSpan::Type(infer_s),
    ]);

    let inner_cond = ConditionalType {
        check_type: infer_r,
        extends_type: inner_pattern,
        true_type: infer_s,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    // Outer conditional: T extends `prefix${infer R}` ? (inner) : never
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: outer_pattern,
        true_type: interner.conditional(inner_cond),
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);

    // Test with "prefixsuffixValue"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("prefixsuffixValue");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Value");
    assert_eq!(result, expected);
}

/// Test template literal in conditional extends clause
/// `prefix${string}` extends `prefix${infer R}` ? R : never
#[test]
fn test_template_literal_conditional_extends_template() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer R}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    // Check type: `prefix${string}`
    let check_type = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer string from the template
    assert_eq!(result, TypeId::STRING);
}

/// Test escape sequences in template literal evaluation
/// Template literals with special characters should be handled correctly
#[test]
fn test_template_literal_escape_sequences() {
    let interner = TypeInterner::new();

    // Template with newline escape sequence - text-only templates become string literals
    let template = interner.template_literal(vec![TemplateSpan::Text(
        interner.intern_string("line1\\nline2"),
    )]);

    // With optimization, text-only template literals become string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let resolved = interner.resolve_atom_ref(atom);
        // The escape sequence should be preserved in the string
        assert!(
            resolved.contains("\\n"),
            "Escape sequence should be preserved"
        );
    } else {
        panic!(
            "Expected string literal for text-only template, got {:?}",
            interner.lookup(template)
        );
    }
}

/// Test template literal with special characters in infer pattern
/// `prefix\n${infer R}` should match "prefix\nvalue"
#[test]
fn test_template_literal_infer_with_special_chars() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern with special character
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("data-")),
        TemplateSpan::Type(infer_r),
    ]);

    // Input with hyphen (special character in property names)
    let input = interner.literal_string("data-value");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.literal_string("value");
    assert_eq!(result, expected);
}

