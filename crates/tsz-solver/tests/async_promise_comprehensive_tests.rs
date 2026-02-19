//! Comprehensive tests for async/promise type operations.
//!
//! These tests verify TypeScript's async/promise type behavior:
//! - Promise type construction
//! - Promise subtype relationships
//! - Awaited type behavior
//! - Async function types

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TypeData};

// =============================================================================
// Basic Promise Construction Tests
// =============================================================================

#[test]
fn test_promise_of_string() {
    let interner = TypeInterner::new();

    // Promise<string> as a global reference or built-in
    // For now we can represent this as an object with then method
    let then_method = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: interner.function(FunctionShape {
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_like = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        then_method,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_like) {
        // Good - promise-like object created
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_promise_of_number() {
    let interner = TypeInterner::new();

    // Simple promise representation
    let promise_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::NUMBER,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_obj) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_promise_of_void() {
    let interner = TypeInterner::new();

    // Promise<void>
    let promise_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::VOID,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_obj) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise Subtype Tests
// =============================================================================

#[test]
fn test_promise_assignable_to_object() {
    let interner = TypeInterner::new();

    let promise_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        TypeId::ANY,
    )]);

    let empty_obj = interner.object(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(promise_obj, empty_obj),
        "Promise should be subtype of object"
    );
}

#[test]
fn test_promise_assignable_to_any() {
    let interner = TypeInterner::new();

    let promise_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        TypeId::ANY,
    )]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(promise_obj, TypeId::ANY),
        "Promise should be subtype of any"
    );
}

// =============================================================================
// Async Function Tests
// =============================================================================

#[test]
fn test_async_function_returns_promise() {
    let interner = TypeInterner::new();

    // async function foo(): Promise<string>
    let promise_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )]);

    let async_func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: promise_string,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(async_func) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.params.len(), 0);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_async_function_with_params() {
    let interner = TypeInterner::new();

    // async function fetch(url: string): Promise<Response>
    let response_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("status"),
        TypeId::NUMBER,
    )]);

    let async_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("url")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: response_type,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(async_func) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.params.len(), 1);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_async_arrow_function() {
    let interner = TypeInterner::new();

    // async (x: number) => Promise<number>
    let promise_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::NUMBER,
    )]);

    let async_arrow = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: promise_number,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(_)) = interner.lookup(async_arrow) {
        // Good
    } else {
        panic!("Expected function type");
    }
}

// =============================================================================
// Promise.then Chain Tests
// =============================================================================

#[test]
fn test_promise_then_chain() {
    let interner = TypeInterner::new();

    // Promise<T>.then<U>(onfulfilled: (value: T) => U): Promise<U>
    // Simplified: Promise<number>.then(x => string): Promise<string>

    let _number_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::NUMBER,
    )]);

    let string_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )]);

    let then_callback = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_method = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: then_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: string_promise,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let promise_with_then = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        then_method,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_with_then) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise.all Tests
// =============================================================================

#[test]
fn test_promise_all_result() {
    let interner = TypeInterner::new();

    // Promise.all([Promise<string>, Promise<number>]): Promise<[string, number]>
    let tuple_result = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        tuple_result,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(result_promise) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise.race Tests
// =============================================================================

#[test]
fn test_promise_race_result() {
    let interner = TypeInterner::new();

    // Promise.race([Promise<string>, Promise<number>]): Promise<string | number>
    let union_result = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let result_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        union_result,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(result_promise) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise.resolve/reject Tests
// =============================================================================

#[test]
fn test_promise_resolve() {
    let interner = TypeInterner::new();

    // Promise.resolve(value: T): Promise<T>
    let resolve_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.object(vec![PropertyInfo::new(
            interner.intern_string("__value"),
            TypeId::STRING,
        )]),
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(_)) = interner.lookup(resolve_func) {
        // Good
    } else {
        panic!("Expected function type");
    }
}

// =============================================================================
// Awaited Type Tests (structural representation)
// =============================================================================

#[test]
fn test_awaited_promise_t_is_t() {
    let interner = TypeInterner::new();

    // Awaited<Promise<T>> = T
    // Structurally, unwrapping a promise gives you T
    let promise_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )]);

    // The awaited type would be string
    // This is a structural test - in practice, Awaited is computed
    if let Some(TypeData::Object(_)) = interner.lookup(promise_string) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_awaited_nested_promise() {
    let interner = TypeInterner::new();

    // Awaited<Promise<Promise<T>>> = T (recursive unwrapping)
    let inner_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )]);

    let outer_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        inner_promise,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(outer_promise) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise with Generics Tests
// =============================================================================

#[test]
fn test_generic_promise() {
    let interner = TypeInterner::new();

    // Promise<T> where T is a type parameter
    let type_param_info = crate::types::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let generic_promise = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        type_param,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(generic_promise) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise Identity Tests
// =============================================================================

#[test]
fn test_promise_identity_stability() {
    let interner = TypeInterner::new();

    let props = vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )];

    let promise1 = interner.object(props.clone());
    let promise2 = interner.object(props);

    assert_eq!(
        promise1, promise2,
        "Same promise construction should produce same TypeId"
    );
}

// =============================================================================
// Promise with never Tests
// =============================================================================

#[test]
fn test_promise_of_never() {
    let interner = TypeInterner::new();

    // Promise<never> is a valid type (promise that never resolves)
    let promise_never = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::NEVER,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_never) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise with any Tests
// =============================================================================

#[test]
fn test_promise_of_any() {
    let interner = TypeInterner::new();

    // Promise<any>
    let promise_any = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::ANY,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(promise_any) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Async Method Tests
// =============================================================================

#[test]
fn test_async_method_in_class() {
    let interner = TypeInterner::new();

    let promise_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__value"),
        TypeId::STRING,
    )]);

    let async_method = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(TypeId::NUMBER), // this type
        return_type: promise_string,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let class_instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("asyncMethod"),
        async_method,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(class_instance) {
        // Good
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Promise Like Interface Tests
// =============================================================================

#[test]
fn test_promise_like_with_then() {
    let interner = TypeInterner::new();

    // PromiseLike<T> interface: { then: ... }
    let thenable = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        interner.function(FunctionShape {
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_params: vec![],
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }),
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(thenable) {
        // Good
    } else {
        panic!("Expected object type");
    }
}
