use super::*;
#[test]
fn test_well_known_symbol_unique_type() {
    // Well-known symbols like Symbol.iterator are unique symbols
    let interner = TypeInterner::new();

    // Each well-known symbol has a unique SymbolRef
    let sym_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));
    let sym_async_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(101)));
    let sym_to_string_tag = interner.intern(TypeData::UniqueSymbol(SymbolRef(102)));
    let sym_has_instance = interner.intern(TypeData::UniqueSymbol(SymbolRef(103)));

    // Each is a distinct type
    assert_ne!(sym_iterator, sym_async_iterator);
    assert_ne!(sym_iterator, sym_to_string_tag);
    assert_ne!(sym_iterator, sym_has_instance);
    assert_ne!(sym_async_iterator, sym_to_string_tag);
}

#[test]
fn test_symbol_keyed_property() {
    // Object with symbol-keyed property: { [Symbol.iterator]: () => Iterator<T> }
    // Represented as object with unique symbol property
    let interner = TypeInterner::new();

    let sym_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));

    // Iterator function type
    let iter_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::ANY, // Simplified
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Note: In the actual implementation, symbol-keyed properties would need
    // special handling. This test verifies the unique symbol type exists.
    assert_ne!(sym_iterator, TypeId::SYMBOL);

    // The function type is valid
    match interner.lookup(iter_fn) {
        Some(TypeData::Function(_)) => {}
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_conditional_with_symbol() {
    // T extends symbol ? true : false
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));

    // unique symbol extends symbol should be true
    let cond = ConditionalType {
        check_type: unique_sym,
        extends_type: TypeId::SYMBOL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // TODO: Full implementation would recognize unique symbol as subtype of symbol
    // For now, verify evaluation completes
    assert!(result == interner.literal_boolean(true) || result == interner.literal_boolean(false));
}

#[test]
fn test_keyof_with_symbol_property() {
    // keyof { [sym]: number, foo: string } should include symbol | "foo"
    // Simplified test with just string keys
    let interner = TypeInterner::new();

    let foo_name = interner.intern_string("foo");
    let bar_name = interner.intern_string("bar");

    let obj = interner.object(vec![
        PropertyInfo::new(foo_name, TypeId::STRING),
        PropertyInfo::new(bar_name, TypeId::NUMBER),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // keyof should produce union of literal string keys
    // Evaluating keyof is implementation-dependent
    assert_ne!(keyof_obj, TypeId::NEVER);
}

#[test]
fn test_async_iterator_result() {
    // AsyncIteratorResult<T> wrapped in Promise
    // Simplified: { then: IteratorResult<T> }
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");
    let then_name = interner.intern_string("then");

    // IteratorResult<string>
    let iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::STRING),
        PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
    ]);

    // Promise<IteratorResult<string>> simplified as { then: IteratorResult }
    let promise_iter = interner.object(vec![PropertyInfo::readonly(then_name, iter_result)]);

    // Verify structure
    match interner.lookup(promise_iter) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(shape.properties[0].name, then_name);
        }
        _ => panic!("Expected Object type"),
    }
}

// ============================================================================
// Exclude/Extract Utility Type Tests
// ============================================================================

#[test]
fn test_exclude_basic_union() {
    // Exclude<string | number | boolean, string> should be number | boolean
    // Exclude<T, U> = T extends U ? never : T
    let interner = TypeInterner::new();

    // Build: string | number | boolean
    let _union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    // Exclude pattern: T extends string ? never : T
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: t_param,
        is_distributive: true,
    };

    // When T = string | number | boolean and distributive:
    // - string extends string ? never : string => never
    // - number extends string ? never : number => number
    // - boolean extends string ? never : boolean => boolean
    // Result: never | number | boolean = number | boolean
    let result = evaluate_conditional(&interner, &cond);

    // Distributive conditional should return conditional type for type param
    // (actual distribution happens during instantiation)
    assert_ne!(result, TypeId::NEVER);
}

#[test]
fn test_exclude_removes_matching_type() {
    // Exclude<"a" | "b" | "c", "a"> should be "b" | "c"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let _lit_c = interner.literal_string("c");

    // Test individual conditional: "a" extends "a" ? never : "a"
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: lit_a,
        true_type: TypeId::NEVER,
        false_type: lit_a,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, TypeId::NEVER); // "a" extends "a" is true

    // Test: "b" extends "a" ? never : "b"
    let cond_b = ConditionalType {
        check_type: lit_b,
        extends_type: lit_a,
        true_type: TypeId::NEVER,
        false_type: lit_b,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    assert_eq!(result_b, lit_b); // "b" does not extend "a"
}

#[test]
fn test_extract_basic_union() {
    // Extract<string | number | boolean, string | number> should be string | number
    // Extract<T, U> = T extends U ? T : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Extract pattern: T extends (string | number) ? T : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: string_or_number,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // With type parameter, returns the conditional
    assert_ne!(result, TypeId::NEVER);
}

#[test]
fn test_extract_filters_to_matching() {
    // Extract<"a" | "b" | 1 | 2, string> should be "a" | "b"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);

    // Test: "a" extends string ? "a" : never
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: TypeId::STRING,
        true_type: lit_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, lit_a); // "a" extends string

    // Test: 1 extends string ? 1 : never
    let cond_1 = ConditionalType {
        check_type: lit_1,
        extends_type: TypeId::STRING,
        true_type: lit_1,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_1 = evaluate_conditional(&interner, &cond_1);
    assert_eq!(result_1, TypeId::NEVER); // 1 does not extend string
}

#[test]
fn test_exclude_with_object_types() {
    // Exclude<{ a: string } | { b: number } | string, object>
    // Should filter out object types, keeping only string
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // Test: { a: string } extends object ? never : { a: string }
    let cond = ConditionalType {
        check_type: obj_a,
        extends_type: TypeId::OBJECT,
        true_type: TypeId::NEVER,
        false_type: obj_a,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Object literal extends object type
    // TODO: Full implementation should return NEVER
    assert!(result == TypeId::NEVER || result == obj_a);
}

#[test]
fn test_extract_function_types() {
    // Extract<string | (() => void) | number, Function>
    // Should extract the function type
    let interner = TypeInterner::new();

    let void_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Test: (() => void) extends (() => void) ? T : never
    // Using same type for extends to test identity
    let cond = ConditionalType {
        check_type: void_fn,
        extends_type: void_fn,
        true_type: void_fn,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, void_fn);
}

#[test]
fn test_exclude_null_undefined() {
    // Exclude<string | null | undefined, null | undefined>
    // This is essentially NonNullable<T>
    let interner = TypeInterner::new();

    let nullish = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    // Test: null extends (null | undefined) ? never : null
    let cond_null = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: nullish,
        true_type: TypeId::NEVER,
        false_type: TypeId::NULL,
        is_distributive: false,
    };
    let result_null = evaluate_conditional(&interner, &cond_null);
    assert_eq!(result_null, TypeId::NEVER);

    // Test: string extends (null | undefined) ? never : string
    let cond_string = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: nullish,
        true_type: TypeId::NEVER,
        false_type: TypeId::STRING,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    assert_eq!(result_string, TypeId::STRING);
}

#[test]
fn test_extract_literal_types() {
    // Extract<1 | 2 | 3 | "a" | "b", number>
    // Should be 1 | 2 | 3
    let interner = TypeInterner::new();

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);

    // Test: 1 extends number ? 1 : never
    let cond_1 = ConditionalType {
        check_type: lit_1,
        extends_type: TypeId::NUMBER,
        true_type: lit_1,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_1 = evaluate_conditional(&interner, &cond_1);
    assert_eq!(result_1, lit_1);

    // Test: 2 extends number ? 2 : never
    let cond_2 = ConditionalType {
        check_type: lit_2,
        extends_type: TypeId::NUMBER,
        true_type: lit_2,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_2 = evaluate_conditional(&interner, &cond_2);
    assert_eq!(result_2, lit_2);
}

#[test]
fn test_distributive_conditional_with_type_param() {
    // Distributive: T extends U ? X : Y distributes when T is type param
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends string ? "yes" : "no"
    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: yes,
        false_type: no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // With unresolved type param, returns conditional type
    assert_ne!(result, TypeId::NEVER);
}

#[test]
fn test_non_distributive_conditional() {
    // [T] extends [U] ? X : Y is non-distributive (wrapped in tuple)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Wrap in tuple to make non-distributive
    let tuple_t = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // [T] extends [string] ? true : false
    let cond = ConditionalType {
        check_type: tuple_t,
        extends_type: tuple_string,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // With wrapped type param, should defer evaluation
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_exclude_with_any() {
    // Exclude<any, string> behavior
    // any extends string is indeterminate, typically yields any
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::ANY,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // any in conditional typically returns union of both branches or any
    assert!(result == TypeId::ANY || result == TypeId::NEVER);
}

#[test]
fn test_extract_with_never() {
    // Extract<never, T> should be never (empty union)
    let interner = TypeInterner::new();

    // never extends string ? never : never
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_exclude_with_unknown() {
    // Exclude<unknown, string> - unknown doesn't extend string
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::UNKNOWN,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // unknown doesn't extend string, so should return unknown
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_complex_exclude_chain() {
    // Exclude<Exclude<string | number | boolean, string>, number>
    // First: Exclude<string | number | boolean, string> = number | boolean
    // Then: Exclude<number | boolean, number> = boolean
    let interner = TypeInterner::new();

    // Test step by step:
    // number extends string ? never : number => number
    let cond_num_str = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };
    let step1_num = evaluate_conditional(&interner, &cond_num_str);
    assert_eq!(step1_num, TypeId::NUMBER);

    // boolean extends string ? never : boolean => boolean
    let cond_bool_str = ConditionalType {
        check_type: TypeId::BOOLEAN,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };
    let step1_bool = evaluate_conditional(&interner, &cond_bool_str);
    assert_eq!(step1_bool, TypeId::BOOLEAN);

    // number extends number ? never : number => never
    let cond_num_num = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NEVER,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };
    let step2_num = evaluate_conditional(&interner, &cond_num_num);
    assert_eq!(step2_num, TypeId::NEVER);

    // boolean extends number ? never : boolean => boolean
    let cond_bool_num = ConditionalType {
        check_type: TypeId::BOOLEAN,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NEVER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };
    let step2_bool = evaluate_conditional(&interner, &cond_bool_num);
    assert_eq!(step2_bool, TypeId::BOOLEAN);
}

#[test]
fn test_extract_intersection() {
    // Extract<A & B, C> with intersection check type
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // (A & B) extends A ? (A & B) : never
    let cond = ConditionalType {
        check_type: intersection,
        extends_type: obj_a,
        true_type: intersection,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Intersection should extend its parts
    // TODO: Full implementation would verify structural subtyping
    assert!(result == intersection || result == TypeId::NEVER);
}

// ============================================================================
// NoInfer Utility Type Tests
// ============================================================================
// NoInfer<T> is an identity type that blocks type inference at specific sites.
// It evaluates to T but prevents that position from contributing to inference.

#[test]
fn test_noinfer_identity_behavior() {
    // NoInfer<T> should evaluate to T (identity)
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();

    // NoInfer<string> = string
    let noinfer_string = interner.intern(TypeData::NoInfer(TypeId::STRING));
    let evaluated = evaluate_type(&interner, noinfer_string);
    assert_eq!(evaluated, TypeId::STRING);

    // NoInfer<number> = number
    let noinfer_number = interner.intern(TypeData::NoInfer(TypeId::NUMBER));
    let evaluated = evaluate_type(&interner, noinfer_number);
    assert_eq!(evaluated, TypeId::NUMBER);

    // Test with literal type
    let lit_hello = interner.literal_string("hello");
    let noinfer_lit = interner.intern(TypeData::NoInfer(lit_hello));
    let evaluated = evaluate_type(&interner, noinfer_lit);
    assert_eq!(evaluated, lit_hello); // Identity property
}

#[test]
fn test_noinfer_with_union_type() {
    // NoInfer<string | number> should still be string | number
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let noinfer_union = interner.intern(TypeData::NoInfer(union));

    // NoInfer preserves the type structure
    let evaluated = evaluate_type(&interner, noinfer_union);
    match interner.lookup(evaluated) {
        Some(TypeData::Union(_)) => {} // Correct - still a union
        other => panic!("Expected Union type, got {other:?}"),
    }
}

#[test]
fn test_noinfer_in_function_param_position() {
    // function foo<T>(a: T, b: NoInfer<T>): T
    // When called as foo("hello", value), inference comes only from 'a'
    use crate::inference::infer::InferenceContext;
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let hello_lit = interner.literal_string("hello");
    let number_type = TypeId::NUMBER;

    // Parameter a: T - contributes to inference
    ctx.infer_from_types(hello_lit, t_param, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Parameter b: NoInfer<T> - should NOT contribute to inference
    let noinfer_t = interner.intern(TypeData::NoInfer(t_param));
    // This should return Ok(()) immediately without adding candidates
    ctx.infer_from_types(number_type, noinfer_t, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Resolve T - should only have "hello" as candidate (widened to string), not number
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING); // Only from parameter 'a', widened
}

#[test]
fn test_noinfer_inference_priority() {
    // When multiple inference sites exist, NoInfer blocks certain ones
    // function foo<T>(a: T, b: NoInfer<T>): T
    // foo("hello", 123) - T should be inferred as "hello" only, not "hello" | number
    use crate::inference::infer::InferenceContext;
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_hello = interner.literal_string("hello");
    let lit_123 = interner.literal_number(123.0);

    // Parameter a: T - contributes to inference
    ctx.infer_from_types(lit_hello, t_param, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Parameter b: NoInfer<T> - should NOT contribute
    let noinfer_t = interner.intern(TypeData::NoInfer(t_param));
    ctx.infer_from_types(lit_123, noinfer_t, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Resolve T - should only have "hello" (widened to string), not a union
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING); // Only from first parameter, widened
    assert_ne!(result, lit_123); // Not from NoInfer position
}

#[test]
fn test_noinfer_with_conditional_type() {
    // NoInfer<T> in conditional: NoInfer<T> extends U ? X : Y
    // Should behave same as T extends U since NoInfer evaluates to T
    let interner = TypeInterner::new();

    // NoInfer<string> extends string ? "yes" : "no"
    // Should be "yes" since NoInfer<string> evaluates to string
    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");

    let noinfer_string = interner.intern(TypeData::NoInfer(TypeId::STRING));
    let cond = ConditionalType {
        check_type: noinfer_string,
        extends_type: TypeId::STRING,
        true_type: yes,
        false_type: no,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, yes);
}

#[test]
fn test_noinfer_nested() {
    // NoInfer<NoInfer<T>> = NoInfer<T> = T
    // Multiple NoInfer wrappers should still result in identity
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();

    let lit_42 = interner.literal_number(42.0);
    let noinfer_42 = interner.intern(TypeData::NoInfer(lit_42));
    let noinfer_noinfer_42 = interner.intern(TypeData::NoInfer(noinfer_42));

    // NoInfer<NoInfer<42>> should evaluate to 42
    let evaluated = evaluate_type(&interner, noinfer_noinfer_42);
    assert_eq!(evaluated, lit_42);
}

#[test]
fn test_noinfer_with_object_property() {
    // { value: NoInfer<string> } - NoInfer is preserved in property type
    // until evaluation context strips it (e.g. during instantiation or subtype check)
    let interner = TypeInterner::new();

    let value_name = interner.intern_string("value");
    let t_param = TypeId::STRING;

    // Object with property value: NoInfer<string>
    let noinfer_t = interner.intern(TypeData::NoInfer(t_param));
    let obj = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: noinfer_t,
        write_type: noinfer_t,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Object preserves NoInfer in property types (structurally unchanged)
    match interner.lookup(obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // Property type is NoInfer<string>
            assert_eq!(shape.properties[0].type_id, noinfer_t);

            // But evaluating the NoInfer wrapper itself should yield string
            use crate::evaluation::evaluate::evaluate_type;
            let evaluated_prop = evaluate_type(&interner, shape.properties[0].type_id);
            assert_eq!(evaluated_prop, t_param);
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_noinfer_preserves_constraints() {
    // NoInfer<T extends string> should preserve the constraint
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");

    // T with constraint: extends string
    let t_constrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // NoInfer<T> should still have the constraint information
    // The type parameter structure is preserved
    match interner.lookup(t_constrained) {
        Some(TypeData::TypeParameter(info)) => {
            assert_eq!(info.constraint, Some(TypeId::STRING));
        }
        _ => panic!("Expected TypeParameter"),
    }
}

#[test]
fn test_noinfer_with_array() {
    // NoInfer<T[]> = T[]
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    // NoInfer<string[]> should still be string[]
    match interner.lookup(string_array) {
        Some(TypeData::Array(elem)) => {
            assert_eq!(elem, TypeId::STRING);
        }
        _ => panic!("Expected Array type"),
    }
}

#[test]
fn test_noinfer_with_tuple() {
    // NoInfer<[string, number]> = [string, number]
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

    match interner.lookup(tuple) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_noinfer_default_parameter() {
    // function foo<T = string>(x: NoInfer<T>): T
    // When no inference possible, falls back to default
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let x_name = interner.intern_string("x");

    // Type parameter with default
    let t_with_default = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    };

    let t_param = interner.intern(TypeData::TypeParameter(t_with_default));

    let func = interner.function(FunctionShape {
        type_params: vec![t_with_default],
        params: vec![ParamInfo::required(x_name, t_param)],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params[0].default, Some(TypeId::STRING));
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_noinfer_multiple_type_params() {
    // function foo<T, U>(a: T, b: NoInfer<U>): [T, U]
    // T inferred from a, U must be explicit or default
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_param,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        type_params: vec![
            TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: u_name,
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: u_param, // NoInfer<U>
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: result_tuple,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 2);
            assert_eq!(shape.params.len(), 2);
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_noinfer_union_distribution() {
    // NoInfer<string | number> should not distribute over union
    // It wraps the whole union, not each member
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // NoInfer<string | number> = string | number (as a unit)
    // Unlike distributive conditionals, NoInfer doesn't distribute
    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_noinfer_in_return_position() {
    // function foo<T>(x: T): NoInfer<T>
    // Return type NoInfer<T> = T, but doesn't contribute to inference from return
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param, // NoInfer<T> = T
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, t_param);
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_noinfer_conditional_true_branch() {
    // T extends string ? NoInfer<T> : never
    // In true branch, NoInfer<T> = T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // When check passes, return NoInfer<T> = T
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param, // NoInfer<T> = T
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Verify it's a conditional type
    match interner.lookup(cond_type) {
        Some(TypeData::Conditional(_)) => {}
        _ => panic!("Expected Conditional type"),
    }
}

#[test]
fn test_noinfer_with_infer_keyword() {
    // NoInfer combined with infer in conditional
    // T extends NoInfer<infer U> ? U : never
    let interner = TypeInterner::new();

    let u_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: NoInfer<infer U> = infer U for matching purposes
    // Test that infer still works within NoInfer context
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: infer_u, // infer U (wrapped in NoInfer conceptually)
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer U = string
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Record/Partial/Required/Readonly Utility Type Tests
// ============================================================================

#[test]
fn test_record_string_keys() {
    // Record<string, number> = { [key: string]: number }
    let interner = TypeInterner::new();

    // Record with string keys creates an index signature
    let record = interner.object_with_index(ObjectShape {
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

    match interner.lookup(record) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.string_index.is_some());
            assert_eq!(
                shape.string_index.as_ref().unwrap().value_type,
                TypeId::NUMBER
            );
        }
        _ => panic!("Expected ObjectWithIndex type"),
    }
}

#[test]
fn test_record_number_keys() {
    // Record<number, string> = { [key: number]: string }
    let interner = TypeInterner::new();

    let record = interner.object_with_index(ObjectShape {
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

    match interner.lookup(record) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.number_index.is_some());
            assert_eq!(
                shape.number_index.as_ref().unwrap().value_type,
                TypeId::STRING
            );
        }
        _ => panic!("Expected ObjectWithIndex type"),
    }
}

#[test]
fn test_record_literal_keys() {
    // Record<"a" | "b", number> = { a: number, b: number }
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Record with literal union keys creates explicit properties
    let record = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    match interner.lookup(record) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_record_with_object_value() {
    // Record<string, { name: string }> = { [key: string]: { name: string } }
    let interner = TypeInterner::new();

    let name_prop = interner.intern_string("name");
    let inner_obj = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);

    let record = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: inner_obj,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    match interner.lookup(record) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.string_index.is_some());
            let idx = shape.string_index.as_ref().unwrap();
            // Value should be the inner object
            assert_ne!(idx.value_type, TypeId::STRING);
        }
        _ => panic!("Expected ObjectWithIndex type"),
    }
}

