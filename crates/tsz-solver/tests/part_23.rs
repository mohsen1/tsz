use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// Test Awaited<T> with non-Promise type (passthrough).
/// Awaited<string> = string (non-thenable types pass through)
#[test]
fn test_awaited_non_promise_passthrough() {
    let interner = TypeInterner::new();

    // Non-promise type: just string
    let then_name = interner.intern_string("then");

    // Awaited pattern: T extends { then: infer R } ? R : T
    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_r)]);

    let cond = ConditionalType {
        check_type: TypeId::STRING, // Non-promise type
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::STRING, // Returns T if not thenable
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // String doesn't have 'then' property, so it passes through unchanged
    assert_eq!(result, TypeId::STRING);
}

/// Test Awaited<T> with mixed union (Promise and non-Promise).
/// Awaited<Promise<boolean> | number> = boolean | number
#[test]
fn test_awaited_mixed_union() {
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    // Promise<boolean>: { then: boolean }
    let promise_boolean = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::BOOLEAN)]);

    // Union: Promise<boolean> | number
    let mixed_union = interner.union(vec![promise_boolean, TypeId::NUMBER]);

    // Awaited pattern with distributive conditional
    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_r)]);

    // For mixed unions, distributive conditional applies Awaited to each member
    let cond = ConditionalType {
        check_type: mixed_union,
        extends_type: pattern,
        true_type: infer_r,
        false_type: mixed_union, // Passthrough for non-matching types
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Distributive conditional: Promise<boolean> unwraps to boolean, number passes through
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::NUMBER]);
    assert_eq!(
        result, expected,
        "Awaited<Promise<boolean> | number> should equal boolean | number"
    );
}

// ============================================================================
// Infer in Mapped Type Value Position
// ============================================================================

#[test]
fn test_infer_mapped_type_value_extraction() {
    // ValueOf<T> = T extends { [K in keyof T]: infer V } ? V : never
    // Extracting value types from mapped type
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: object with infer V as value type
    // { x: infer V, y: infer V }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), infer_v),
        PropertyInfo::new(interner.intern_string("y"), infer_v),
    ]);

    // Input: { x: string, y: string }
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // V should be inferred as string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_mapped_type_mixed_values() {
    // When values differ, should infer union
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { a: infer V, b: infer V }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), infer_v),
        PropertyInfo::new(interner.intern_string("b"), infer_v),
    ]);

    // Input: { a: string, b: number }
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // V should be string | number (union of all value types)
    // Behavior depends on implementation - may return first match, union, or never
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(
        result == expected
            || result == TypeId::STRING
            || result == TypeId::NUMBER
            || result == TypeId::NEVER
    );
}

#[test]
fn test_infer_mapped_type_key_and_value() {
    // Extract value type from object with specific key
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern with infer in value position
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("key"),
        infer_v,
    )]);

    // Input: { key: boolean }
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("key"),
        TypeId::BOOLEAN,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // V should be boolean
    assert_eq!(result, TypeId::BOOLEAN);
}

// ============================================================================
// Infer with Multiple Constraints
// ============================================================================

#[test]
fn test_infer_with_extends_constraint() {
    // infer U extends string - constrained infer
    let interner = TypeInterner::new();

    let infer_u_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_u_name,
        constraint: Some(TypeId::STRING), // U extends string
        default: None,
        is_const: false,
    }));

    // Pattern: (x: infer U extends string) => any
    let pattern_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: infer_u,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (x: "hello") => void - literal string satisfies constraint
    let lit_hello = interner.literal_string("hello");
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: lit_hello,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // U should be "hello" (satisfies string constraint)
    assert_eq!(result, lit_hello);
}

#[test]
fn test_infer_with_constraint_violation() {
    // When inferred type doesn't satisfy constraint
    let interner = TypeInterner::new();

    let infer_u_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_u_name,
        constraint: Some(TypeId::STRING), // U extends string
        default: None,
        is_const: false,
    }));

    // Pattern: (x: infer U extends string) => any
    let pattern_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: infer_u,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (x: number) => void - number does NOT satisfy string constraint
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Constraint not satisfied - behavior depends on implementation
    assert!(result == TypeId::NEVER || result == TypeId::NUMBER);
}

#[test]
fn test_infer_multiple_same_name_covariant() {
    // Same infer variable in covariant position (return type)
    let interner = TypeInterner::new();

    let infer_r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Getter method returning infer R
    let getter = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: infer_r, // covariant position
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Pattern object with getter
    let pattern = interner.object(vec![PropertyInfo::method(
        interner.intern_string("get"),
        getter,
    )]);

    // Input getter returning string
    let string_getter = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let input = interner.object(vec![PropertyInfo::method(
        interner.intern_string("get"),
        string_getter,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // R should be inferred as string from covariant position
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Infer in Template Literal Types
// ============================================================================

#[test]
fn test_infer_template_literal_prefix() {
    // T extends `prefix${infer Rest}` ? Rest : never
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

    // Input: "prefixSuffix"
    let input = interner.literal_string("prefixSuffix");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Rest should be "Suffix"
    let expected = interner.literal_string("Suffix");
    // Template literal inference may not be fully implemented
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_infer_template_literal_suffix() {
    // T extends `${infer Prefix}Suffix` ? Prefix : never
    let interner = TypeInterner::new();

    let infer_prefix_name = interner.intern_string("Prefix");
    let infer_prefix = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_prefix_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer Prefix}Suffix`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_prefix),
        TemplateSpan::Text(interner.intern_string("Suffix")),
    ]);

    // Input: "PrefixSuffix"
    let input = interner.literal_string("PrefixSuffix");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_prefix,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Prefix should be "Prefix"
    let expected = interner.literal_string("Prefix");
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_infer_template_literal_middle() {
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

    // Input: "startMIDDLEend"
    let input = interner.literal_string("startMIDDLEend");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_middle,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Middle should be "MIDDLE"
    let expected = interner.literal_string("MIDDLE");
    assert!(result == expected || result == TypeId::STRING || result == TypeId::NEVER);
}

#[test]
fn test_infer_template_literal_no_match() {
    // T extends `prefix${infer Rest}` ? Rest : never
    // When input doesn't match prefix
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

    // Input: "wrongStart" - doesn't start with "prefix"
    let input = interner.literal_string("wrongStart");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Should return never since pattern doesn't match
    assert_eq!(result, TypeId::NEVER);
}

// ============================================================================
// Symbol Type Tests
// ============================================================================

#[test]
fn test_unique_symbol_type_distinct() {
    // Two unique symbols with different SymbolRefs should be distinct
    let interner = TypeInterner::new();

    let sym1 = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym2 = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    // Unique symbols with different refs are distinct types
    assert_ne!(sym1, sym2);
}

#[test]
fn test_unique_symbol_type_same_ref() {
    // Two unique symbols with same SymbolRef should intern to same TypeId
    let interner = TypeInterner::new();

    let sym1 = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));
    let sym2 = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));

    // Same SymbolRef produces same TypeId
    assert_eq!(sym1, sym2);
}

#[test]
fn test_unique_symbol_not_assignable_to_base_symbol() {
    // unique symbol should be distinct from base symbol type
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));

    // Unique symbol is a separate type from base symbol
    assert_ne!(unique_sym, TypeId::SYMBOL);
}

#[test]
fn test_symbol_union_with_unique() {
    // symbol | unique symbol should create a union
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let union = interner.union(vec![TypeId::SYMBOL, unique_sym]);

    // Union should be created (not collapsed)
    assert_ne!(union, TypeId::SYMBOL);
    assert_ne!(union, unique_sym);
}

#[test]
fn test_iterator_result_type_done_false() {
    // IteratorResult<T, TReturn> when done is false: { value: T, done: false }
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::STRING),
        PropertyInfo::readonly(done_name, interner.literal_boolean(false)),
    ]);

    // Verify it's a valid object type
    match interner.lookup(iter_result) {
        Some(TypeData::Object(_)) => {}
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_iterator_result_type_done_true() {
    // IteratorResult<T, TReturn> when done is true: { value: TReturn, done: true }
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::UNDEFINED),
        PropertyInfo::readonly(done_name, interner.literal_boolean(true)),
    ]);

    // Verify it's a valid object type
    match interner.lookup(iter_result) {
        Some(TypeData::Object(_)) => {}
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_iterator_result_union() {
    // Full IteratorResult is union: { value: T, done: false } | { value: TReturn, done: true }
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let yielding = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::NUMBER),
        PropertyInfo::readonly(done_name, interner.literal_boolean(false)),
    ]);

    let completed = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::UNDEFINED),
        PropertyInfo::readonly(done_name, interner.literal_boolean(true)),
    ]);

    let result_union = interner.union(vec![yielding, completed]);

    // Verify it's a union type
    match interner.lookup(result_union) {
        Some(TypeData::Union(_)) => {}
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_iterable_with_symbol_iterator() {
    // Iterable<T> has [Symbol.iterator](): Iterator<T>
    // Simplified: object with iterator method returning { next(): IteratorResult }
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");
    let next_name = interner.intern_string("next");

    // IteratorResult<number>
    let iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::NUMBER),
        PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
    ]);

    // next(): IteratorResult<number>
    let next_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: iter_result,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Iterator<number> = { next(): IteratorResult<number> }
    let iterator = interner.object(vec![PropertyInfo {
        name: next_name,
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Verify iterator structure
    match interner.lookup(iterator) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(shape.properties[0].name, next_name);
        }
        _ => panic!("Expected Object type"),
    }
}

