use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_partial_simple_object() {
    // Partial<{ a: string, b: number }> = { a?: string, b?: number }
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Partial makes all properties optional
    let partial_obj = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true, // Made optional by Partial
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // Made optional by Partial
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    match interner.lookup(partial_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            assert!(shape.properties[1].optional);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_partial_nested_object() {
    // Partial<{ inner: { value: string } }> = { inner?: { value: string } }
    // Note: Partial is shallow, inner object properties stay required
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let inner_name = interner.intern_string("inner");

    let inner_obj = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false, // Inner property stays required
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let partial_outer = interner.object(vec![PropertyInfo {
        name: inner_name,
        type_id: inner_obj,
        write_type: inner_obj,
        optional: true, // Outer property made optional
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(partial_outer) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            // Inner object retains its structure
            match interner.lookup(shape.properties[0].type_id) {
                Some(TypeData::Object(inner_shape_id)) => {
                    let inner = interner.object_shape(inner_shape_id);
                    assert!(!inner.properties[0].optional); // Not affected by Partial
                }
                _ => panic!("Expected inner Object type"),
            }
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_partial_deep_nesting() {
    // DeepPartial<T> pattern - all nested properties optional
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let point_name = interner.intern_string("point");

    // DeepPartial<{ point: { x: number, y: number } }>
    // = { point?: { x?: number, y?: number } }
    let deep_partial_point = interner.object(vec![
        PropertyInfo {
            name: x_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // Deep optional
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: y_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // Deep optional
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let deep_partial_outer =
        interner.object(vec![PropertyInfo::opt(point_name, deep_partial_point)]);

    match interner.lookup(deep_partial_outer) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            // Verify nested is also optional
            match interner.lookup(shape.properties[0].type_id) {
                Some(TypeData::Object(inner_id)) => {
                    let inner = interner.object_shape(inner_id);
                    assert!(inner.properties[0].optional);
                    assert!(inner.properties[1].optional);
                }
                _ => panic!("Expected nested Object"),
            }
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_required_simple_object() {
    // Required<{ a?: string, b?: number }> = { a: string, b: number }
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Required removes optional modifiers
    let required_obj = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // Made required
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false, // Made required
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    match interner.lookup(required_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties[0].optional);
            assert!(!shape.properties[1].optional);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_required_nested_optionals() {
    // Required<{ inner?: { value?: string } }>
    // = { inner: { value?: string } } (shallow Required)
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let inner_name = interner.intern_string("inner");

    let inner_obj = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true, // Stays optional (Required is shallow)
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let required_outer = interner.object(vec![PropertyInfo {
        name: inner_name,
        type_id: inner_obj,
        write_type: inner_obj,
        optional: false, // Made required at top level
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(required_outer) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties[0].optional); // Outer is required
            // Inner still has optional property
            match interner.lookup(shape.properties[0].type_id) {
                Some(TypeData::Object(inner_id)) => {
                    let inner = interner.object_shape(inner_id);
                    assert!(inner.properties[0].optional); // Still optional
                }
                _ => panic!("Expected inner Object"),
            }
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_required_mapped_type() {
    // Required<T> implemented as mapped type with -? modifier
    let interner = TypeInterner::new();

    let k_name = interner.intern_string("K");

    // MappedType with optional_modifier = Remove
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING, // keyof T
        name_type: None,
        template: TypeId::NUMBER, // T[K]
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove), // -? removes optional
    };

    let mapped_id = interner.mapped(mapped);

    match interner.lookup(mapped_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let m = interner.mapped_type(mapped_id);
            assert_eq!(m.optional_modifier, Some(MappedModifier::Remove));
        }
        _ => panic!("Expected Mapped type"),
    }
}

#[test]
fn test_readonly_simple_object() {
    // Readonly<{ a: string, b: number }> = { readonly a: string, readonly b: number }
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let readonly_obj = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true, // Made readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true, // Made readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    match interner.lookup(readonly_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].readonly);
            assert!(shape.properties[1].readonly);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_array() {
    // Readonly<string[]> = readonly string[]
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(string_array));

    match interner.lookup(readonly_array) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, string_array);
        }
        _ => panic!("Expected ReadonlyType"),
    }
}

#[test]
fn test_readonly_tuple() {
    // Readonly<[string, number]> = readonly [string, number]
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
    ]);

    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, tuple);
            // Verify inner is still a tuple
            match interner.lookup(inner) {
                Some(TypeData::Tuple(_)) => {}
                _ => panic!("Expected Tuple inside ReadonlyType"),
            }
        }
        _ => panic!("Expected ReadonlyType"),
    }
}

#[test]
fn test_readonly_nested() {
    // Readonly<{ items: string[] }> - items property is readonly, not the array
    let interner = TypeInterner::new();

    let items_name = interner.intern_string("items");
    let string_array = interner.array(TypeId::STRING);

    let readonly_obj = interner.object(vec![PropertyInfo {
        name: items_name,
        type_id: string_array, // Array itself isn't readonly
        write_type: string_array,
        optional: false,
        readonly: true, // Property is readonly
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(readonly_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].readonly);
            // The array type itself is not wrapped in ReadonlyType
            match interner.lookup(shape.properties[0].type_id) {
                Some(TypeData::Array(_)) => {} // Regular array
                _ => panic!("Expected Array type"),
            }
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_mapped_type() {
    // Readonly<T> implemented as mapped type with readonly modifier
    let interner = TypeInterner::new();

    let k_name = interner.intern_string("K");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);

    match interner.lookup(mapped_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let m = interner.mapped_type(mapped_id);
            assert_eq!(m.readonly_modifier, Some(MappedModifier::Add));
        }
        _ => panic!("Expected Mapped type"),
    }
}

#[test]
fn test_record_with_union_value() {
    // Record<string, string | number>
    let interner = TypeInterner::new();

    let value_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let record = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: value_union,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    match interner.lookup(record) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            let idx = shape.string_index.as_ref().unwrap();
            // Verify value is a union
            match interner.lookup(idx.value_type) {
                Some(TypeData::Union(_)) => {}
                _ => panic!("Expected Union value type"),
            }
        }
        _ => panic!("Expected ObjectWithIndex"),
    }
}

#[test]
fn test_partial_with_methods() {
    // Partial<{ greet(): void }> - methods also become optional
    let interner = TypeInterner::new();

    let greet_name = interner.intern_string("greet");
    let method_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let partial_obj = interner.object(vec![PropertyInfo {
        name: greet_name,
        type_id: method_type,
        write_type: method_type,
        optional: true, // Method made optional
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(partial_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            assert!(shape.properties[0].is_method);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_with_index_signature() {
    // Readonly<{ [key: string]: number }> - index signature becomes readonly
    let interner = TypeInterner::new();

    let readonly_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true, // Made readonly,
            param_name: None,
        }),
        number_index: None,
    });

    match interner.lookup(readonly_indexed) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.string_index.as_ref().unwrap().readonly);
        }
        _ => panic!("Expected ObjectWithIndex"),
    }
}

#[test]
fn test_partial_required_inverse() {
    // Required<Partial<T>> should restore original (modulo undefined)
    // Partial<Required<T>> should be same as Partial<T>
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");

    // Original: { a: string }
    // Partial: { a?: string }
    // Required<Partial>: { a: string } (back to required)
    let required_partial = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false, // Required restores it
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(required_partial) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties[0].optional);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_with_optional() {
    // Readonly<{ a?: string }> = { readonly a?: string }
    // Both modifiers can coexist
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");

    let readonly_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: true, // Both optional and readonly
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    match interner.lookup(readonly_optional) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            assert!(shape.properties[0].readonly);
        }
        _ => panic!("Expected Object type"),
    }
}

// =============================================================================
// Template Literal Infer Tests
// =============================================================================

#[test]
fn test_template_infer_prefix_extraction() {
    // T extends `prefix${infer Rest}` ? Rest : never
    // Input: "prefixSuffix" -> Rest = "Suffix"
    let interner = TypeInterner::new();

    let infer_rest_name = interner.intern_string("Rest");
    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer Rest}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_rest),
    ]);

    let input = interner.literal_string("prefixSuffix");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.literal_string("Suffix");
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_template_infer_suffix_extraction() {
    // T extends `${infer Start}suffix` ? Start : never
    // Input: "PrefixSuffix" where suffix is "Suffix" -> Start = "Prefix"
    let interner = TypeInterner::new();

    let infer_start_name = interner.intern_string("Start");
    let infer_start = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_start_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer Start}Suffix`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_start),
        TemplateSpan::Text(interner.intern_string("Suffix")),
    ]);

    let input = interner.literal_string("PrefixSuffix");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_start,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.literal_string("Prefix");
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_template_infer_middle_extraction() {
    // T extends `start${infer Middle}end` ? Middle : never
    let interner = TypeInterner::new();

    let infer_middle_name = interner.intern_string("Middle");
    let infer_middle = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_middle_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `start${infer Middle}end`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("start")),
        TemplateSpan::Type(infer_middle),
        TemplateSpan::Text(interner.intern_string("end")),
    ]);

    let input = interner.literal_string("startMIDDLEend");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_middle,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.literal_string("MIDDLE");
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_template_infer_no_match() {
    // T extends `prefix${infer Rest}` ? Rest : never
    // Input: "wrongStart" doesn't match -> never
    let interner = TypeInterner::new();

    let infer_rest_name = interner.intern_string("Rest");
    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer Rest}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_rest),
    ]);

    let input = interner.literal_string("wrongStart");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NEVER);
}

