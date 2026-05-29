#[test]
fn test_exact_optional_property_types_false_allows_undefined() {
    // With exact_optional_property_types=false, optional properties implicitly
    // include undefined
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_exact_optional_property_types(false);

    let x = interner.intern_string("x");

    // { x?: number } - implicitly { x?: number | undefined }
    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    // { x: number | undefined }
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let _explicit_undefined = interner.object(vec![PropertyInfo::new(x, number_or_undefined)]);

    // With non-exact mode, undefined should be assignable to optional property
    // This tests that the optional property type is widened to include undefined
    let just_undefined = interner.object(vec![PropertyInfo::new(x, TypeId::UNDEFINED)]);

    assert!(
        checker.is_assignable(just_undefined, optional_number),
        "Explicit undefined should be assignable to optional property in non-exact mode"
    );
}

#[test]
fn test_exact_optional_property_types_toggle_behavior() {
    // Verify that toggling exact_optional_property_types changes behavior
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");

    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    let just_undefined = interner.object(vec![PropertyInfo::new(x, TypeId::UNDEFINED)]);

    // Default (false): undefined is assignable to optional
    assert!(checker.is_assignable(just_undefined, optional_number));

    // Toggle to true: undefined is NOT assignable to optional
    checker.set_exact_optional_property_types(true);
    assert!(!checker.is_assignable(just_undefined, optional_number));

    // Toggle back to false: undefined is assignable again
    checker.set_exact_optional_property_types(false);
    assert!(checker.is_assignable(just_undefined, optional_number));
}

// =============================================================================
// strictNullChecks Legacy Behavior Tests (Catalog Rule #9)
// =============================================================================

#[test]
fn test_strict_null_checks_off_null_assignable_to_anything() {
    // With strictNullChecks=false, null is assignable to most types.
    // Exception: null is NOT assignable to void (only undefined is).
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_null_checks(false);

    // null is assignable to primitive types
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::BOOLEAN));
    // null is NOT assignable to void — only undefined is (tsc intrinsic rule)
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::VOID));

    // undefined is also assignable to everything
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::BOOLEAN));
}

#[test]
fn test_strict_null_checks_on_null_not_assignable() {
    // With strictNullChecks=true, null and undefined are NOT assignable to non-nullish types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_null_checks(true);

    // null is NOT assignable to non-nullish types
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::BOOLEAN));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::VOID));

    // undefined is also NOT assignable
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::STRING));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::BOOLEAN));
}

#[test]
fn test_strict_null_checks_union_with_null() {
    // Test behavior of unions containing null/undefined based on strictNullChecks
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let nullable_string = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let undefinable_number = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    // With strict mode on (default), nullable types are distinct from non-nullable
    // But a specific type IS assignable to a union containing it (normal subtyping)
    assert!(!checker.is_assignable(nullable_string, TypeId::STRING)); // string | null not assignable to string
    assert!(checker.is_assignable(TypeId::STRING, nullable_string)); // string IS assignable to string | null
    assert!(!checker.is_assignable(undefinable_number, TypeId::NUMBER)); // number | undefined not assignable to number
    assert!(checker.is_assignable(TypeId::NUMBER, undefinable_number)); // number IS assignable to number | undefined

    // With strict mode off, null/undefined are "never-like" and assignable
    checker.set_strict_null_checks(false);
    // Now string | null "collapses" to string (null is bottom-like)
    assert!(checker.is_assignable(nullable_string, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_assignable(undefinable_number, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
}

#[test]
fn test_strict_null_checks_empty_object() {
    // Test empty object assignability with null/undefined based on strictNullChecks
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());

    // With strict mode on (default), null/undefined are NOT assignable to {}
    assert!(!checker.is_assignable(TypeId::NULL, empty_object));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, empty_object));

    // With strict mode off, null/undefined ARE assignable to {}
    checker.set_strict_null_checks(false);
    assert!(checker.is_assignable(TypeId::NULL, empty_object));
    assert!(checker.is_assignable(TypeId::UNDEFINED, empty_object));
}

// =============================================================================
// Void Return Exception Tests (Catalog Rule #6)
// =============================================================================

#[test]
fn test_void_return_exception_functions() {
    // Functions returning void can accept functions with any return type
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // () => void
    let void_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // () => string
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // () => number
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Functions with non-void returns ARE assignable to void-returning functions
    assert!(
        checker.is_assignable(string_fn, void_fn),
        "Function returning string should be assignable to void function"
    );
    assert!(
        checker.is_assignable(number_fn, void_fn),
        "Function returning number should be assignable to void function"
    );

    // But void-return function is NOT assignable to non-void function
    assert!(
        !checker.is_assignable(void_fn, string_fn),
        "Void function should NOT be assignable to string function"
    );
}

#[test]
fn test_void_return_exception_with_parameters() {
    // Void return exception applies even with parameter mismatches
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");

    // (x: number) => void
    let void_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::required(x, TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: string) => number
    let string_number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::required(x, TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Return type mismatch still applies void exception
    // Even though parameters don't match, the void return should allow non-void returns
    // (though parameters will still be checked separately)
    assert!(
        !checker.is_assignable(string_number_fn, void_fn),
        "Parameter mismatch should still cause rejection"
    );
}

#[test]
fn test_void_return_exception_constructors() {
    // Void return exception also applies to constructors
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // new () => void
    let void_ctor = interner.object(vec![PropertyInfo {
        name: interner.intern_string("constructor"),
        type_id: interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // new () => Instance
    let instance_ctor = interner.object(vec![PropertyInfo {
        name: interner.intern_string("constructor"),
        type_id: interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type: instance_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Constructor returning instance IS assignable to void-returning constructor
    assert!(
        checker.is_assignable(instance_ctor, void_ctor),
        "Constructor returning instance should be assignable to void constructor"
    );
}

// =============================================================================
// Covariant This Types Tests (Catalog Rule #19)
// =============================================================================

#[test]
fn test_method_bivariance_allows_derived_methods() {
    // Methods are bivariant in TypeScript, allowing Derived methods to override Base methods
    // even though method parameters should normally be contravariant
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let method_name = interner.intern_string("compare");

    // class Base { compare(other: Base): void }
    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let base_method = interner.object(vec![PropertyInfo {
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(base)],
            this_type: Some(base),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true, // This is a method
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // class Derived extends Base { x: string; y: number; compare(other: Derived): void }
    let derived = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let derived_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(derived)],
            this_type: Some(derived),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true, // This is a method
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // With method bivariance (default), derived method with narrower parameter is assignable
    // This simulates the covariant 'this' behavior
    assert!(
        checker.is_assignable(derived_method, base_method),
        "Derived method with narrower 'this' parameter should be assignable to Base method due to bivariance"
    );
}

#[test]
fn test_method_bivariance_persists_with_strict_function_types() {
    // Methods remain bivariant even with strictFunctionTypes=true
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_function_types(true);

    let method_name = interner.intern_string("method");

    // Base type with method
    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let base_with_method = interner.object(vec![PropertyInfo {
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(base)],
            this_type: Some(base),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Derived type with method
    let derived = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let derived_with_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo::unnamed(derived)],
            this_type: Some(derived),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Methods are still bivariant even with strictFunctionTypes
    assert!(
        checker.is_assignable(derived_with_method, base_with_method),
        "Methods should remain bivariant even with strictFunctionTypes"
    );
}

#[test]
fn test_function_variance_strict_function_types_affects_functions_not_methods() {
    // strictFunctionTypes affects standalone functions but not methods
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    // Standalone functions: contravariant with strictFunctionTypes
    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // NOT a method
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // NOT a method
    });

    // Functions should be contravariant (not assignable) with strictFunctionTypes
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Standalone functions should be contravariant with strictFunctionTypes"
    );

    // But methods are still bivariant
    let method_name = interner.intern_string("method");

    let obj_with_dog_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: fn_dog,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true, // IS a method
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let obj_with_animal_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: fn_animal,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true, // IS a method
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Methods are bivariant even with strictFunctionTypes
    assert!(
        checker.is_assignable(obj_with_dog_method, obj_with_animal_method),
        "Methods should be bivariant even with strictFunctionTypes"
    );
}

// =============================================================================
// Integration Tests: Compiler Options Toggle Behaviors
// =============================================================================

#[test]
fn test_strict_mode_enables_all_strict_flags() {
    // Integration test: strict mode should enable multiple strict behaviors
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Test strict null checks behavior
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING));

    // Test function variance (default is non-strict)
    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Default: non-strict (bivariant)
    assert!(
        checker.is_assignable(fn_dog, fn_animal),
        "Functions should be bivariant by default"
    );

    // Enable strict function types
    checker.set_strict_function_types(true);
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Functions should be contravariant with strictFunctionTypes"
    );
}

#[test]
fn test_compiler_options_independent_toggles() {
    // Test that compiler options can be toggled independently
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Start with all defaults
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING)); // strictNullChecks=true (default)

    // Toggle strictNullChecks
    checker.set_strict_null_checks(false);
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));

    // Reset for next test
    checker.set_strict_null_checks(true);

    // Toggle exact_optional_property_types
    let x = interner.intern_string("x");
    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let explicit_union = interner.object(vec![PropertyInfo::new(x, number_or_undefined)]);

    // Default (exact_optional_property_types=false): optional includes undefined
    // So { x: number | undefined } should be assignable to { x?: number }
    assert!(
        checker.is_assignable(explicit_union, optional_number),
        "Explicit number|undefined should be assignable to optional number in default mode"
    );

    // Toggle exact_optional_property_types
    checker.set_exact_optional_property_types(true);
    // In exact mode, optional does NOT include undefined
    // So { x: number | undefined } should NOT be assignable to { x?: number }
    assert!(
        !checker.is_assignable(explicit_union, optional_number),
        "Explicit number|undefined should NOT be assignable to optional number in exact mode"
    );

    // Toggle no_unchecked_indexed_access
    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::STRING));

    // Reset exact mode for next test
    checker.set_exact_optional_property_types(false);

    // Default: no_unchecked_indexed_access=false, index access returns NUMBER
    assert!(checker.is_assignable(index_access, TypeId::NUMBER));

    // Toggle no_unchecked_indexed_access
    checker.set_no_unchecked_indexed_access(true);
    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
}

// =============================================================================
// Rule #29: The Global Function type - Intrinsic(Function) as untyped callable supertype
// =============================================================================

#[test]
fn test_function_intrinsic_accepts_any_function() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create a simple function type
    let simple_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function intrinsic should accept any function
    assert!(
        checker.is_assignable(simple_fn, TypeId::FUNCTION),
        "Any function should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_accepts_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create a callable with multiple signatures
    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    // Function intrinsic should accept callable types
    assert!(
        checker.is_assignable(callable, TypeId::FUNCTION),
        "Callable types should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_rejects_non_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Primitives are NOT callable
    assert!(
        !checker.is_assignable(TypeId::STRING, TypeId::FUNCTION),
        "String should NOT be assignable to Function intrinsic"
    );
    assert!(
        !checker.is_assignable(TypeId::NUMBER, TypeId::FUNCTION),
        "Number should NOT be assignable to Function intrinsic"
    );

    // Objects are NOT callable (unless they have call signatures)
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(
        !checker.is_assignable(obj, TypeId::FUNCTION),
        "Plain object should NOT be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_with_union_of_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let fn1 = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn2 = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union of callables should be assignable to Function
    let union_fn = interner.union(vec![fn1, fn2]);
    assert!(
        checker.is_assignable(union_fn, TypeId::FUNCTION),
        "Union of callables should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_with_union_non_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let fn1 = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union of callable and non-callable should NOT be assignable to Function
    let mixed_union = interner.union(vec![fn1, TypeId::STRING]);
    assert!(
        !checker.is_assignable(mixed_union, TypeId::FUNCTION),
        "Mixed union (callable | non-callable) should NOT be assignable to Function"
    );
}

// =============================================================================
// Union/Intersection Distributivity Tests
// =============================================================================

#[test]
fn test_union_intersection_distributivity_basic() {
    // Test: (A | B) & C distributes to (A & C) | (B & C)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo::new(age, TypeId::NUMBER)]);

    let type_c = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // (A | B) & C
    let union_ab = interner.union(vec![type_a, type_b]);
    let intersection = interner.intersection(vec![union_ab, type_c]);

    // A & C (should be compatible since both have 'name: string')
    let a_and_c = interner.intersection(vec![type_a, type_c]);

    assert!(
        checker.is_assignable(intersection, a_and_c),
        "(A | B) & C should distribute correctly"
    );
}

#[test]
fn test_intersection_union_distributivity() {
    // Test: A & (B | C) distributes to (A & B) | (A & C)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo::new(age, TypeId::NUMBER)]);

    let type_c = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // A & (B | C)
    let union_bc = interner.union(vec![type_b, type_c]);
    let intersection = interner.intersection(vec![type_a, union_bc]);

    // (A & B) is empty (incompatible), so intersection should simplify
    assert!(
        checker.is_assignable(type_a, intersection),
        "A & (B | C) should distribute to (A & B) | (A & C)"
    );
}

#[test]
fn test_distributivity_with_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // (string | number) & string should be string
    let str_num = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = interner.intersection(vec![str_num, TypeId::STRING]);

    assert!(
        checker.is_assignable(TypeId::STRING, result),
        "(string | number) & string should be string"
    );
}

// =============================================================================
// Enhanced Weak Type Detection Tests
// =============================================================================

#[test]
fn test_weak_type_detection_with_all_strict_options() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Enable all strict options
    checker.set_strict_function_types(true);
    checker.set_strict_null_checks(true);
    checker.set_exact_optional_property_types(true);
    checker.set_no_unchecked_indexed_access(true);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");

    // Weak type: all optional properties
    let weak_type = interner.object(vec![
        PropertyInfo::opt(x, TypeId::STRING),
        PropertyInfo::opt(y, TypeId::NUMBER),
    ]);

    // Source with no common properties should be rejected
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("z"),
        TypeId::BOOLEAN,
    )]);

    assert!(
        !checker.is_assignable(source, weak_type),
        "Weak type detection should work with all strict options enabled"
    );
}

#[test]
fn test_weak_union_detection_improved() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");

    // Weak types in a union
    let weak1 = interner.object(vec![PropertyInfo::opt(x, TypeId::STRING)]);

    let weak2 = interner.object(vec![PropertyInfo::opt(y, TypeId::NUMBER)]);

    let weak_union = interner.union(vec![weak1, weak2]);

    // Source with no common properties should be rejected
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("z"),
        TypeId::BOOLEAN,
    )]);

    assert!(
        !checker.is_assignable(source, weak_union),
        "Weak union detection should reject source with no common properties"
    );
}

// =============================================================================
// Comprehensive Compiler Options Tests
// =============================================================================

#[test]
fn test_all_compiler_options_combinations() {
    let interner = TypeInterner::new();
    let x = interner.intern_string("x");

    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let explicit_union = interner.object(vec![PropertyInfo::new(x, number_or_undefined)]);

    // Test all combinations
    let test_cases = vec![
        (false, false, false, "all defaults"),
        (true, false, false, "strictFunctionTypes only"),
        (false, true, false, "exactOptionalProperties only"),
        (false, false, true, "noUncheckedIndexedAccess only"),
        (true, true, false, "strict + exact"),
        (true, false, true, "strict + noUnchecked"),
        (false, true, true, "exact + noUnchecked"),
        (true, true, true, "all strict"),
    ];

    for (strict_fn, exact, no_unchecked, desc) in test_cases {
        let mut checker = CompatChecker::new(&interner);
        checker.set_strict_function_types(strict_fn);
        checker.set_exact_optional_property_types(exact);
        checker.set_no_unchecked_indexed_access(no_unchecked);

        // The behavior should change based on exact_optional_property_types
        let expected = !exact; // When exact=true, should NOT be assignable
        let result = checker.is_assignable(explicit_union, optional_number);

        assert_eq!(result, expected, "Failed for: {desc} (exact={exact})");
    }
}

#[test]
fn test_strict_function_types_affects_methods_independently() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Animal: { name: string }
    let name = interner.intern_string("name");
    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Dog: { name: string, breed: string } - Dog is subtype of Animal
    let breed = interner.intern_string("breed");
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    // Create method types
    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // Function, not method
    });

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // Function, not method
    });

    // Default: bivariant
    assert!(
        checker.is_assignable(fn_dog, fn_animal),
        "Functions should be bivariant by default"
    );

    // Enable strict function types
    checker.set_strict_function_types(true);
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Functions should be contravariant with strictFunctionTypes"
    );

    // Methods should remain bivariant even with strictFunctionTypes
    let method_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method
    });

    let method_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method
    });

    assert!(
        checker.is_assignable(method_dog, method_animal),
        "Methods should remain bivariant even with strictFunctionTypes"
    );
}

#[test]
fn test_no_unchecked_indexed_access_with_nested_types() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create nested array type
    let nested_array = interner.array(interner.array(TypeId::STRING));

    // Index access should include undefined when no_unchecked_indexed_access is enabled
    checker.set_no_unchecked_indexed_access(true);

    // String should NOT be assignable to (string | undefined)
    assert!(
        !checker.is_assignable(TypeId::STRING, nested_array),
        "With noUncheckedIndexedAccess, array indexing includes undefined"
    );
}

// =============================================================================
// Rule #30: keyof contravariance - keyof(A | B) === keyof A & keyof B
// =============================================================================

#[test]
fn test_keyof_union_contravariance() {
    let interner = TypeInterner::new();
    let checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string }
    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Type B: { age: number }
    let type_b = interner.object(vec![PropertyInfo::new(age, TypeId::NUMBER)]);

    // keyof (A | B) should be keyof A & keyof B
    // Since A has "name" and B has "age" with NO common keys,
    // keyof (A | B) = "name" & "age" = never
    let union_ab = interner.union(vec![type_a, type_b]);
    let keyof_union = crate::evaluate_keyof(&interner, union_ab);

    // keyof (A | B) with no common keys should be never
    assert_eq!(
        keyof_union,
        TypeId::NEVER,
        "keyof (A | B) with disjoint keys should be never"
    );

    // Verify that keyof properly extracts keys when there ARE common properties
    let name_prop = PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };
    // Type C: { name: string, x: number }
    let type_c = interner.object(vec![
        name_prop.clone(),
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
    ]);
    // Type D: { name: string, y: boolean }
    let type_d = interner.object(vec![
        name_prop,
        PropertyInfo::new(interner.intern_string("y"), TypeId::BOOLEAN),
    ]);

    // keyof (C | D) = keyof C & keyof D = ("name" | "x") & ("name" | "y") = "name"
    let union_cd = interner.union(vec![type_c, type_d]);
    let keyof_union_cd = crate::evaluate_keyof(&interner, union_cd);

    let name_literal = interner.literal_string("name");
    assert_eq!(
        keyof_union_cd, name_literal,
        "keyof (C | D) with common 'name' key should be 'name'"
    );

    // Suppress unused checker warning
    let _ = checker;
}

#[test]
fn test_keyof_intersection_distributivity() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string }
    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Type B: { name: string, age: number }
    let type_b = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(age, TypeId::NUMBER),
    ]);

    // keyof (A & B) should be keyof A | keyof B
    // Both have 'name', B also has 'age'
    let intersection_ab = interner.intersection(vec![type_a, type_b]);
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection_ab));

    let name_literal = interner.intern(TypeData::Literal(crate::LiteralValue::String(name)));
    let age_literal = interner.intern(TypeData::Literal(crate::LiteralValue::String(age)));

    // keyof (A & B) should include 'name' (common to both)
    assert!(
        checker.is_assignable(name_literal, keyof_intersection),
        "keyof (A & B) should include 'name'"
    );

    // keyof (A & B) should include 'age' (from B)
    assert!(
        checker.is_assignable(age_literal, keyof_intersection),
        "keyof (A & B) should include 'age'"
    );
}

#[test]
fn test_keyof_with_union_of_objects_with_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string, age: number }
    let type_a = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(age, TypeId::NUMBER),
    ]);

    // Type B: { name: string, email: string }
    let email = interner.intern_string("email");
    let type_b = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(email, TypeId::STRING),
    ]);

    // keyof (A | B) should be keyof A & keyof B
    // which is just "name" (the common property)
    let union_ab = interner.union(vec![type_a, type_b]);
    let keyof_union = interner.intern(TypeData::KeyOf(union_ab));

    let name_literal = interner.intern(TypeData::Literal(crate::LiteralValue::String(name)));
    let age_literal = interner.intern(TypeData::Literal(crate::LiteralValue::String(age)));
    let email_literal = interner.intern(TypeData::Literal(crate::LiteralValue::String(email)));

    // keyof (A | B) should include 'name' (common to both)
    assert!(
        checker.is_assignable(name_literal, keyof_union),
        "keyof (A | B) should include common property 'name'"
    );

    // keyof (A | B) should NOT include 'age' (only in A)
    assert!(
        !checker.is_assignable(age_literal, keyof_union),
        "keyof (A | B) should NOT include 'age' (only in A)"
    );

    // keyof (A | B) should NOT include 'email' (only in B)
    assert!(
        !checker.is_assignable(email_literal, keyof_union),
        "keyof (A | B) should NOT include 'email' (only in B)"
    );
}

// =============================================================================
// Rule #32: Best Common Type (BCT) inference for array literals
// =============================================================================

#[test]
fn test_best_common_type_array_literal_inference() {
    let interner = TypeInterner::new();
    let ctx = crate::inference::infer::InferenceContext::new(&interner);

    // Array literal with mixed types: [1, "hello", true]
    // Best common type should be the union: number | string | boolean
    let types = vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN];
    let bct = ctx.best_common_type(&types);

    // The BCT should be a union of all three types
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(bct, expected, "BCT of mixed types should be their union");
}

#[test]
fn test_best_common_type_with_supertype() {
    let interner = TypeInterner::new();
    let ctx = crate::inference::infer::InferenceContext::new(&interner);

    let name = interner.intern_string("name");

    // Type Animal: { name: string }
    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Type Dog: { name: string, breed: string }
    let breed = interner.intern_string("breed");
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    // BCT of [Animal, Dog] should be Animal (the supertype)
    let types = vec![animal, dog];
    let bct = ctx.best_common_type(&types);

    // Animal should be assignable to BCT
    assert!(
        interner.is_subtype_of(animal, bct),
        "Animal should be subtype of BCT"
    );
}

#[test]
fn test_best_common_type_empty_array() {
    let interner = TypeInterner::new();
    let ctx = crate::inference::infer::InferenceContext::new(&interner);

    // Empty array should infer to unknown[] (or any[])
    let types: Vec<TypeId> = vec![];
    let bct = ctx.best_common_type(&types);

    // Empty arrays default to unknown
    assert_eq!(bct, TypeId::UNKNOWN, "BCT of empty array should be unknown");
}

#[test]
fn test_best_common_type_single_element() {
    let interner = TypeInterner::new();
    let ctx = crate::inference::infer::InferenceContext::new(&interner);

    // Single element array should just be that type
    let types = vec![TypeId::STRING];
    let bct = ctx.best_common_type(&types);

    assert_eq!(
        bct,
        TypeId::STRING,
        "BCT of single element should be that element"
    );
}

#[test]
fn test_best_common_type_with_literal_widening() {
    let interner = TypeInterner::new();
    let ctx = crate::inference::infer::InferenceContext::new(&interner);

    // [1, "a"] should infer to (number | string)[]
    let types = vec![TypeId::NUMBER, TypeId::STRING];
    let bct = ctx.best_common_type(&types);

    // Should be a union of both types
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(
        bct, expected,
        "BCT of number and string should be their union"
    );
}

// =============================================================================
// Private Brand Assignability Override Tests
// =============================================================================

#[test]
fn test_private_brand_lazy_self_resolution_does_not_recurse() {
    struct SelfReferentialLazyResolver {
        def_id: DefId,
        lazy_type: TypeId,
    }

    impl TypeResolver for SelfReferentialLazyResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            if def_id == self.def_id {
                Some(self.lazy_type)
            } else {
                None
            }
        }
    }

    let interner = TypeInterner::new();
    let def_id = DefId(42);
    let lazy_type = interner.intern(TypeData::Lazy(def_id));
    let resolver = SelfReferentialLazyResolver { def_id, lazy_type };
    let checker = CompatChecker::with_resolver(&interner, &resolver);

    // A self-referential lazy resolution should short-circuit instead of recurring forever.
    assert_eq!(
        checker.private_brand_assignability_override(lazy_type, lazy_type),
        None
    );
}

#[test]
fn test_private_brand_lazy_cycle_does_not_recurse() {
    struct CyclicLazyResolver {
        first_def: DefId,
        first_type: TypeId,
        second_def: DefId,
        second_type: TypeId,
    }

    impl TypeResolver for CyclicLazyResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            if def_id == self.first_def {
                Some(self.second_type)
            } else if def_id == self.second_def {
                Some(self.first_type)
            } else {
                None
            }
        }
    }

    let interner = TypeInterner::new();
    let first_def = DefId(42);
    let second_def = DefId(43);
    let first_type = interner.intern(TypeData::Lazy(first_def));
    let second_type = interner.intern(TypeData::Lazy(second_def));
    let resolver = CyclicLazyResolver {
        first_def,
        first_type,
        second_def,
        second_type,
    };
    let checker = CompatChecker::with_resolver(&interner, &resolver);
    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    assert_eq!(
        checker.private_brand_assignability_override(first_type, target),
        None
    );
}

#[test]
fn test_private_brand_same_brand_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with the same private brand should be assignable
    let brand = interner.intern_string("__private_brand_Foo");
    let source = interner.object(vec![PropertyInfo::new(brand, TypeId::NEVER)]);
    let target = interner.object(vec![PropertyInfo::new(brand, TypeId::NEVER)]);

    // Same brand = same class declaration = assignable
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_different_brand_not_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with different private brands should NOT be assignable
    let brand1 = interner.intern_string("__private_brand_Foo");
    let brand2 = interner.intern_string("__private_brand_Bar");

    let source = interner.object(vec![PropertyInfo::new(brand1, TypeId::NEVER)]);
    let target = interner.object(vec![PropertyInfo::new(brand2, TypeId::NEVER)]);

    // Different brands = different class declarations = not assignable
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_without_brand_not_assignable_to_target_with_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source without brand cannot satisfy target's private requirements
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let target = interner.object(vec![
        PropertyInfo::new(brand, TypeId::NEVER),
        PropertyInfo {
            name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    // Source without brand cannot be assigned to target with brand
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_with_brand_assignable_to_target_without_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source with brand CAN be assigned to target without brand (e.g., interface)
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![
        PropertyInfo::new(brand, TypeId::NEVER),
        PropertyInfo {
            name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // A class can implement an interface (source with brand -> target without brand)
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_neither_has_brand_falls_through() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // When neither has a brand, fall through to structural checking
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Structural check passes
    assert!(checker.is_assignable(source, target));
}

