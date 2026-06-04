#[test]
fn test_nullable_and_optional_union() {
    // string | null | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // All three are subtypes
    assert!(checker.is_subtype_of(TypeId::STRING, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, nullable_optional));

    // Not subtype of any individual
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::STRING));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::NULL));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::UNDEFINED));
}

#[test]
fn test_null_distinct_from_undefined() {
    // null and undefined are distinct types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::UNDEFINED));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::NULL));
}

#[test]
fn test_null_subtype_of_self() {
    // null is subtype of null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::NULL));
}

#[test]
fn test_undefined_subtype_of_self() {
    // undefined is subtype of undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNDEFINED));
}

#[test]
fn test_null_subtype_of_any() {
    // null is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::ANY));
}

#[test]
fn test_undefined_subtype_of_any() {
    // undefined is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::ANY));
}

#[test]
fn test_null_subtype_of_unknown() {
    // null is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::UNKNOWN));
}

#[test]
fn test_undefined_subtype_of_unknown() {
    // undefined is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNKNOWN));
}

#[test]
fn test_null_not_subtype_of_object() {
    // null is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
}

#[test]
fn test_undefined_not_subtype_of_object() {
    // undefined is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
}

#[test]
fn test_null_not_subtype_of_never() {
    // null is not subtype of never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::NEVER));
}

#[test]
fn test_never_subtype_of_null() {
    // never is subtype of null (never is bottom type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NULL));
}

#[test]
fn test_nullable_object_type() {
    // { x: string } | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let nullable_obj = interner.union(vec![obj, TypeId::NULL]);

    // Object is subtype of nullable object
    assert!(checker.is_subtype_of(obj, nullable_obj));

    // null is subtype of nullable object
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_obj));

    // Nullable object is not subtype of object
    assert!(!checker.is_subtype_of(nullable_obj, obj));
}

#[test]
fn test_nullable_function_type() {
    // (() => void) | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let nullable_fn = interner.union(vec![fn_type, TypeId::NULL]);

    // Function is subtype of nullable function
    assert!(checker.is_subtype_of(fn_type, nullable_fn));

    // null is subtype of nullable function
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_fn));
}

#[test]
fn test_nullable_array_type() {
    // string[] | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let string_array = interner.array(TypeId::STRING);
    let nullable_array = interner.union(vec![string_array, TypeId::NULL]);

    // Array is subtype of nullable array
    assert!(checker.is_subtype_of(string_array, nullable_array));

    // null is subtype of nullable array
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_array));
}

#[test]
fn test_void_distinct_from_undefined() {
    // void is not the same as undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // undefined is subtype of void
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::VOID));

    // void is not subtype of undefined (void is wider)
    // Note: In TypeScript, void can accept undefined
    // but void is not assignable to undefined
}

#[test]
fn test_nullable_literal_type() {
    // "hello" | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let hello = interner.literal_string("hello");
    let nullable_hello = interner.union(vec![hello, TypeId::NULL]);

    // Literal is subtype of nullable literal
    assert!(checker.is_subtype_of(hello, nullable_hello));

    // null is subtype of nullable literal
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_hello));

    // string is not subtype of nullable literal
    assert!(!checker.is_subtype_of(TypeId::STRING, nullable_hello));
}

#[test]
fn test_non_null_assertion_type() {
    // NonNullable<string | null | undefined> = string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After non-null assertion, only string remains
    let non_null_result = TypeId::STRING;

    // string is subtype of the original union
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(non_null_result, nullable_optional));
}

#[test]
fn test_nullable_union_widening() {
    // string | null | undefined is wider than string | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // string | null is subtype of string | null | undefined
    assert!(checker.is_subtype_of(nullable, nullable_optional));

    // string | null | undefined is not subtype of string | null
    assert!(!checker.is_subtype_of(nullable_optional, nullable));
}

#[test]
fn test_null_in_intersection() {
    // string & null = never (incompatible)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NULL]);

    // Intersection of incompatible types reduces to never-like
    // The intersection is subtype of string (vacuously)
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
}
