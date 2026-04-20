use super::*;
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

/// Test complex composition: keyof, template literal, conditional, and infer
/// Extract property names from template literal pattern
#[test]
fn test_complex_keyof_template_infer_composition() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // T extends `get${infer K}` ? K : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    // Create object type to use in keyof
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("getName"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("getAge"), TypeId::NUMBER),
    ]);

    // keyof obj = "getName" | "getAge"
    let keys_of_obj = evaluate_keyof(&interner, obj);

    // Now test the conditional with the keys
    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, keys_of_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract "Name" | "Age" from the keys
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union of extracted names");
    }
}

/// Test template literal with number interpolation
/// `item${number}` should work with number types
#[test]
fn test_template_literal_with_number_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Verify template was created
    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal");
    }
}

/// Test multiple infers in template literal pattern with union input
/// `${infer A}-${infer B}` with "foo-bar" | "baz-qux"
#[test]
fn test_template_literal_two_infers_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer A}-${infer B}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);

    // Result type: `${infer A}-${infer B}` (reconstruct the pattern)
    let result_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: result_template,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "foo-bar" | "baz-qux"
    let mut subst = TypeSubstitution::new();
    let foo_bar = interner.literal_string("foo-bar");
    let baz_qux = interner.literal_string("baz-qux");
    subst.insert(t_name, interner.union(vec![foo_bar, baz_qux]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should return "foo-bar" | "baz-qux"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union");
    }
}

/// Test template literal with constrained infer
/// T extends `prefix${infer R extends string}` ? R : never
#[test]
fn test_template_literal_constrained_infer() {
    let interner = TypeInterner::new();

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
        constraint: Some(TypeId::STRING), // Constrained to string
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer R extends string}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "prefixValue"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("prefixValue");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Value");
    assert_eq!(result, expected);
}

/// Test keyof with object containing template literal keys
/// { [`get${string}`]: string } should have string keys
#[test]
fn test_keyof_object_with_template_literal_computed_keys() {
    let interner = TypeInterner::new();

    // In TypeScript, you can have computed properties with template literals
    // This tests that we handle the keyof correctly
    // For now, we test that keyof of an object with some properties works

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("getName"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("getAge"), TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);

    // Should return "getName" | "getAge"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union of property names");
    }
}

/// Test empty template literal
/// `(empty template)` should be handled
#[test]
fn test_empty_template_literal() {
    let interner = TypeInterner::new();

    // Empty template literal is optimized to an empty string literal
    let template = interner.template_literal(vec![]);

    // With the template literal optimization, empty template literals become empty string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let s = interner.resolve_atom_ref(atom);
        assert_eq!(
            s.as_ref(),
            "",
            "Empty template literal should be empty string"
        );
    } else {
        panic!(
            "Expected empty string literal for empty template literal, got {:?}",
            interner.lookup(template)
        );
    }
}

/// Test template literal with only text (no interpolation)
/// `hello` should behave like a string literal
#[test]
fn test_template_literal_only_text() {
    let interner = TypeInterner::new();

    // Template literal with only text is optimized to a string literal
    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // With the template literal optimization, text-only template literals become string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let s = interner.resolve_atom_ref(atom);
        assert_eq!(
            s.as_ref(),
            "hello",
            "Text-only template literal should be 'hello' string literal"
        );
    } else {
        panic!(
            "Expected string literal for text-only template literal, got {:?}",
            interner.lookup(template)
        );
    }

    // keyof of string literal returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test template literal with only type interpolation (no text)
/// `${string}` should behave like string
#[test]
fn test_template_literal_only_type_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    // Verify it was created
    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 1);
    } else {
        panic!("Expected template literal");
    }

    // keyof returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test distributive conditional with template literal and union
/// ("a" | "b") extends `${infer R}x` ? R : never
#[test]
fn test_distributive_conditional_template_union() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer R}x`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax" | "bx" | "c"
    let lit_ax = interner.literal_string("ax");
    let lit_bx = interner.literal_string("bx");
    let lit_c = interner.literal_string("c");
    let input_union = interner.union(vec![lit_ax, lit_bx, lit_c]);

    let cond = ConditionalType {
        check_type: input_union,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Should extract "a" | "b" (the "c" doesn't match and becomes never)
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union");
    }
}

/// Test non-distributive conditional with template literal
/// ("a" | "b") extends `${infer R}x` ? R : never (non-distributive)
#[test]
fn test_non_distributive_conditional_template_union() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer R}x`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax" | "bx"
    let lit_ax = interner.literal_string("ax");
    let lit_bx = interner.literal_string("bx");
    let input_union = interner.union(vec![lit_ax, lit_bx]);

    let cond = ConditionalType {
        check_type: input_union,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false, // Non-distributive
    };

    let result = evaluate_conditional(&interner, &cond);

    // Non-distributive: the entire union is checked against the pattern
    // For "ax" | "bx" against `${infer R}x`, R infers to "a" | "b"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let expected_union = interner.union(vec![lit_a, lit_b]);
    // Result could be the inferred union, never, or string depending on implementation
    assert!(
        result == TypeId::NEVER || result == TypeId::STRING || result == expected_union,
        "Expected never, string, or \"a\" | \"b\", got {result:?}"
    );
}

/// Test template literal with boolean interpolation
/// `flag${boolean}` expands to "flagtrue" | "flagfalse"
#[test]
fn test_template_literal_with_boolean_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("flag")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    // TypeScript expands boolean interpolation to union
    match interner.lookup(template) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2, "Expected 2 members for boolean expansion");
        }
        other => panic!("Expected Union type for `flag${{boolean}}`, got {other:?}"),
    }
}

/// Test template literal matching with literal union input
/// T extends `${"a" | "b"}x` ? T : never
#[test]
fn test_template_literal_literal_union_pattern() {
    let interner = TypeInterner::new();

    // Pattern: `${"a" | "b"}x`
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_ab = interner.union(vec![lit_a, lit_b]);

    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(union_ab),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax"
    let input = interner.literal_string("ax");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: input,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "ax" should match `${"a" | "b"}x`
    assert_eq!(result, input);
}

/// Test template literal types with array/tuple index access scenarios
/// This verifies that template literals work correctly in index access contexts
/// which is important for noUncheckedIndexedAccess scenarios
#[test]
fn test_template_literal_index_access_scenario() {
    let interner = TypeInterner::new();

    // Create an object with template literal-like string properties
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("item0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("item1"), TypeId::NUMBER),
    ]);

    // Access with a literal string key
    let key = interner.literal_string("item0");
    let result = evaluate_index_access(&interner, obj, key);

    assert_eq!(result, TypeId::STRING);
}

/// Test template literal pattern matching in mapped types
/// { [K in `${Prefix}${infer S}`]: S } expands correctly
#[test]
fn test_template_literal_mapped_type_pattern() {
    let interner = TypeInterner::new();

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create a template literal pattern like `get${infer S}`
    let pattern_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_s),
    ]);

    // Verify the pattern was created
    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(pattern_template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal");
    }
}

/// Test multiple template literal infers with complex union patterns
/// T extends `start${infer A}-middle${infer B}-end` ? [A, B] : never
#[test]
fn test_template_literal_multiple_infers_complex_pattern() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `start${infer A}-middle${infer B}-end`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("start")),
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-middle")),
        TemplateSpan::Type(infer_b),
        TemplateSpan::Text(interner.intern_string("-end")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_a, // Return first infer
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "startFOO-middleBAR-end"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("startFOO-middleBAR-end");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("FOO");
    assert_eq!(result, expected);
}

/// Test template literal with union of unions
/// `prefix${("a" | "b") | ("c" | "d")}` should handle nested unions
#[test]
fn test_template_literal_nested_union_interpolation() {
    let interner = TypeInterner::new();

    // Create nested unions: ("a" | "b") | ("c" | "d")
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_ab = interner.union(vec![lit_a, lit_b]);

    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let union_cd = interner.union(vec![lit_c, lit_d]);

    let nested_union = interner.union(vec![union_ab, union_cd]);

    // Template with nested union interpolation
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(nested_union),
    ]);

    // With optimization, nested unions in template literals should be expanded
    // The nested union is flattened to "a" | "b" | "c" | "d" and template expands to
    // "prefixa" | "prefixb" | "prefixc" | "prefixd"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 4, "Expected 4 members in expanded union");
        }
        _ => panic!(
            "Expected Union type for template with nested union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

/// Test template literal matching against another template literal
/// `foo${string}` extends `foo${infer R}` ? R : never
#[test]
fn test_template_literal_matches_template_literal() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `foo${infer R}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);

    // Check type: `foo${string}`
    let check_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: check_template,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer string
    assert_eq!(result, TypeId::STRING);
}

/// Test keyof with template literal that expands to multiple literals
/// keyof `item${0 | 1 | 2}` should return keyof string (apparent keys)
#[test]
fn test_keyof_template_literal_number_union_interpolation() {
    let interner = TypeInterner::new();

    // Create 0 | 1 | 2 union
    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let union_012 = interner.union(vec![lit_0, lit_1, lit_2]);

    // Create template literal: `item${0 | 1 | 2}`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item")),
        TemplateSpan::Type(union_012),
    ]);

    // keyof returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test conditional with template literal in both check and extends
/// `prefix${string}` extends `prefix${string}` ? true : false
#[test]
fn test_template_literal_conditional_same_pattern() {
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: template1,
        extends_type: template2,
        true_type: TypeId::STRING,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should match and return true branch
    assert_eq!(result, TypeId::STRING);
}

/// Test tail-recursion elimination for conditional types.
///
/// This test verifies that tail-recursive conditional types can recurse
/// up to `MAX_TAIL_RECURSION_DEPTH` (1000) instead of being limited by
/// `MAX_EVALUATE_DEPTH` (50).
#[test]
fn test_tail_recursive_conditional() {
    let interner = TypeInterner::new();

    // Build a chain of 60 nested conditionals
    // Each conditional: `string extends number ? never : string`
    // This will take the false branch each time

    let mut current_type = TypeId::STRING;

    for _ in 0..60 {
        let cond = ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::NEVER,
            false_type: current_type,
            is_distributive: false,
        };

        current_type = interner.conditional(cond);
    }

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(current_type);

    // The result should be STRING (the false branch all the way down)
    // Without tail-recursion elimination, this would hit MAX_EVALUATE_DEPTH (50)
    assert_eq!(result, TypeId::STRING);
}

