#![allow(dead_code)]
//! Tests for type predicate compatibility in function subtyping.
//!
//! Type predicates (`x is T` and `asserts x is T`) make functions more specific.
//! A function with a type predicate can only be assigned to a function with
//! a compatible (or more general) predicate.

use super::*;

// =============================================================================
// Helper Functions
// =============================================================================

/// Create a type predicate: `paramName is Type`
fn type_predicate(interner: &TypeInterner, param_name: &str, type_id: TypeId) -> TypePredicate {
    TypePredicate {
        asserts: false,
        target: TypePredicateTarget::Identifier(interner.intern_string(param_name)),
        type_id: Some(type_id),
    }
}

/// Create an assertion predicate: `asserts paramName is Type`
fn asserts_predicate(interner: &TypeInterner, param_name: &str, type_id: TypeId) -> TypePredicate {
    TypePredicate {
        asserts: true,
        target: TypePredicateTarget::Identifier(interner.intern_string(param_name)),
        type_id: Some(type_id),
    }
}

/// Create a bare assertion: `asserts paramName`
fn bare_asserts(interner: &TypeInterner, param_name: &str) -> TypePredicate {
    TypePredicate {
        asserts: true,
        target: TypePredicateTarget::Identifier(interner.intern_string(param_name)),
        type_id: None,
    }
}

/// Create a function shape with the given params, return type, and type predicate
fn fn_with_predicate(
    interner: &TypeInterner,
    param_name: &str,
    param_type: TypeId,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
) -> FunctionShape {
    let param_name_atom = interner.intern_string(param_name);
    FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_name_atom),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type,
        type_predicate,
        is_constructor: false,
        is_method: false,
    }
}

// =============================================================================
// Type Guard Tests (`x is T`)
// =============================================================================

#[test]
fn test_type_guard_more_specific_than_no_predicate() {
    // A function with a type guard is MORE specific than one without
    // (x: string) => x is string cannot be assigned to (x: string) => boolean
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_param = interner.intern_string("x");

    // Source: (x: string) => x is string
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(string_param),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(string_param),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: string) => boolean (no predicate)
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(string_param),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Source (with predicate) should NOT be subtype of target (without predicate)
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Function with type guard should NOT be assignable to function without guard"
    );
}

#[test]
fn test_no_predicate_not_compatible_with_type_guard() {
    // A function WITHOUT a type guard is NOT compatible with one that has a guard
    // if return types don't match
    // (x: string) => boolean CANNOT be assigned to (x: string) => x is string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_param = interner.intern_string("x");

    // Source: (x: string) => boolean (no predicate)
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(string_param),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: string) => x is string
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(string_param),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(string_param),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (without predicate, returns boolean) should NOT be subtype of target (with predicate, returns string)
    // because boolean is not a subtype of string
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Function returning boolean should NOT be assignable to function with type guard returning string"
    );
}

#[test]
fn test_no_predicate_compatible_with_type_guard_matching_return() {
    // A function WITHOUT a type guard IS compatible if return types match
    // (x: unknown) => string can be assigned to (x: unknown) => x is string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_x = interner.intern_string("x");

    // Source: (x: unknown) => string (no predicate)
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: unknown) => x is string
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (without predicate, returns string) should be subtype of target (with predicate, returns string)
    assert!(
        checker.is_subtype_of(source_fn, target_fn),
        "Function returning string should be assignable to function with type guard returning string"
    );
}

#[test]
fn test_type_guard_narrowing_is_compatible() {
    // (x: Animal) => x is Dog can be assigned to (x: Animal) => x is Animal
    // where Dog extends Animal (Dog <: Animal)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Create Animal and Dog types (Dog <: Animal)
    let animal = interner.object(vec![]);
    let dog = interner.object(vec![]);

    let param_x = interner.intern_string("x");

    // Source: (x: Animal) => x is Dog (narrower type guard)
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: dog,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(dog),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: Animal) => x is Animal (wider type guard)
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: animal,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(animal),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (narrower guard) should be subtype of target (wider guard)
    // Note: This tests the predicate narrowing, though with identical object types
    // they will be equal. In real usage with distinct types, Dog <: Animal would allow this.
    assert!(
        checker.is_subtype_of(source_fn, target_fn),
        "Narrower type guard should be assignable to wider type guard"
    );
}

#[test]
fn test_type_guard_different_parameters_incompatible() {
    // (x: string) => x is string cannot be assigned to (y: string) => y is string
    // The predicates target different parameters
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_x = interner.intern_string("x");
    let param_y = interner.intern_string("y");

    // Source: (x: string) => x is string
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (y: string) => y is string
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_y),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_y),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source should NOT be subtype of target (different predicate targets)
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Type guards on different parameters should be incompatible"
    );
}

// =============================================================================
// Assertion Tests (`asserts x is T`)
// =============================================================================

#[test]
fn test_asserts_more_specific_than_type_guard() {
    // (asserts x is T) is MORE specific than (x is T) due to assertion semantics
    // (x: unknown) => asserts x is string cannot be assigned to (x: unknown) => x is string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_x = interner.intern_string("x");

    // Source: (x: unknown) => asserts x is string
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: true,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: unknown) => x is string
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (asserts) should NOT be subtype of target (type guard)
    // because assertions and type guards are incompatible
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Assertion should NOT be assignable to type guard (incompatible kinds)"
    );
}

#[test]
fn test_type_guard_not_compatible_with_asserts() {
    // (x is T) is NOT compatible with (asserts x is T) due to assertion semantics
    // (x: unknown) => x is string cannot be assigned to (x: unknown) => asserts x is string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_x = interner.intern_string("x");

    // Source: (x: unknown) => x is string
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: unknown) => asserts x is string
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: true,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (type guard) should NOT be subtype of target (asserts)
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Type guard should NOT be assignable to assertion (incompatible kinds)"
    );
}

#[test]
fn test_bare_asserts_compatibility() {
    // `asserts x` (bare assertion) is less specific than `asserts x is T`
    // Bare assertion cannot be assigned to typed assertion
    // Typed assertion can be assigned to bare assertion
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_x = interner.intern_string("x");

    // Source: (x: unknown) => asserts x (bare assertion)
    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: Some(TypePredicate {
            asserts: true,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: None, // Bare assertion, no type
        }),
        is_constructor: false,
        is_method: false,
    });

    // Target: (x: unknown) => asserts x is string (typed assertion)
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_x),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: true,
            target: TypePredicateTarget::Identifier(param_x),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Source (bare asserts) should NOT be subtype of target (asserts with type)
    // because bare assertion (no type) is less specific than typed assertion
    assert!(
        !checker.is_subtype_of(source_fn, target_fn),
        "Bare assertion should NOT be assignable to typed assertion"
    );

    // Note: The reverse direction (typed assertion to bare assertion) would also
    // fail in this specific test case due to return type mismatch (STRING vs VOID),
    // not because of the predicate logic. With matching return types, typed assertion
    // would be assignable to bare assertion since typed is more specific.
}

// =============================================================================
// Integration Tests
// =============================================================================

#[test]
#[ignore = "Type guard in overloads not fully implemented"]
fn test_type_guard_in_overloads() {
    // Test type predicate behavior with callable overloads
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_value = interner.intern_string("value");

    // Create a function with type guard
    let type_guard_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_value),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_value),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    });

    // Create a function without type guard
    let regular_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(param_value),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Type guard function is NOT assignable to regular function
    assert!(
        !checker.is_subtype_of(type_guard_fn, regular_fn),
        "Type guard function should NOT be assignable to regular function"
    );

    // Regular function IS assignable to type guard function
    assert!(
        checker.is_subtype_of(regular_fn, type_guard_fn),
        "Regular function should be assignable to type guard function"
    );
}
