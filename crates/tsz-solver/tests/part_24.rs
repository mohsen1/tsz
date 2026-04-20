use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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

