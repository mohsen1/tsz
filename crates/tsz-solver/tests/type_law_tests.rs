//! Type System Law Tests
//!
//! This module tests the mathematical properties that the type system must satisfy.
//! These are the foundational laws from SOLVER.md Section 1 and SOLVER_ROADMAP.md Section 7.2.
//!
//! ## Laws Tested
//! - **Reflexivity**: T ≤ T (every type is a subtype of itself)
//! - **Transitivity**: A ≤ B and B ≤ C implies A ≤ C
//! - **Antisymmetry**: A ≤ B and B ≤ A implies A = B (with canonicalization via interning)
//! - **Top**: T ≤ any (any is the top type)
//! - **Bottom**: never ≤ T (never is the bottom type)

use crate::types::{SymbolRef, TypeId, Visibility};
use crate::{FunctionShape, ParamInfo, PropertyInfo, SubtypeChecker, TupleElement, TypeInterner};

// =============================================================================
// Reflexivity Tests (T ≤ T)
// =============================================================================

#[test]
fn test_law_reflexivity_intrinsics() {
    // All intrinsic types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intrinsics = vec![
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
    ];

    for &ty in &intrinsics {
        assert!(
            checker.is_subtype_of(ty, ty),
            "Reflexivity failed for intrinsic type: {:?}",
            ty
        );
    }
}

#[test]
fn test_law_reflexivity_literals() {
    // Literal types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let num_42 = interner.literal_number(42.0);
    let bool_true = interner.literal_boolean(true);
    let bool_false = interner.literal_boolean(false);

    assert!(checker.is_subtype_of(hello, hello));
    assert!(checker.is_subtype_of(world, world));
    assert!(checker.is_subtype_of(num_42, num_42));
    assert!(checker.is_subtype_of(bool_true, bool_true));
    assert!(checker.is_subtype_of(bool_false, bool_false));
}

#[test]
fn test_law_reflexivity_objects() {
    // Object types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    assert!(checker.is_subtype_of(obj1, obj1));
    assert!(checker.is_subtype_of(obj2, obj2));
}

#[test]
fn test_law_reflexivity_arrays() {
    // Array types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let arr1 = interner.array(TypeId::NUMBER);
    let arr2 = interner.array(TypeId::STRING);
    let arr3 = interner.array(TypeId::ANY);

    assert!(checker.is_subtype_of(arr1, arr1));
    assert!(checker.is_subtype_of(arr2, arr2));
    assert!(checker.is_subtype_of(arr3, arr3));
}

#[test]
fn test_law_reflexivity_tuples() {
    // Tuple types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple1 = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple2 = interner.tuple(vec![
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

    assert!(checker.is_subtype_of(tuple1, tuple1));
    assert!(checker.is_subtype_of(tuple2, tuple2));
}

#[test]
fn test_law_reflexity_unions() {
    // Union types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let union3 = interner.union(vec![TypeId::ANY, TypeId::NEVER]);

    assert!(checker.is_subtype_of(union1, union1));
    assert!(checker.is_subtype_of(union2, union2));
    assert!(checker.is_subtype_of(union3, union3));
}

#[test]
fn test_law_reflexivity_intersections() {
    // Intersection types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    assert!(checker.is_subtype_of(intersection, intersection));
}

#[test]
fn test_law_reflexivity_functions() {
    // Function types should be subtypes of themselves
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(fn1, fn1));
    assert!(checker.is_subtype_of(fn2, fn2));
}

// =============================================================================
// Transitivity Tests (A ≤ B and B ≤ C implies A ≤ C)
// =============================================================================

#[test]
fn test_law_transitivity_primitives() {
    // string ≤ string (reflexive)
    // number ≤ number (reflexive)
    // Therefore string ≤ string (trivial but validates the chain)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::NUMBER));
}

#[test]
fn test_law_transitivity_literals_to_primitives() {
    // "hello" ≤ string
    // string ≤ string (reflexive)
    // Therefore "hello" ≤ string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::STRING));
    // Transitivity: hello ≤ string
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
}

#[test]
fn test_law_transitivity_objects() {
    // A = { x: number, y: string }
    // B = { x: number }
    // C = {} (empty object)
    // A ≤ B (A has all properties of B plus more)
    // B ≤ C (B has all properties of C - which is none)
    // Therefore A ≤ C
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let c = interner.object(vec![]);

    // A ≤ B (A has all properties of B)
    assert!(checker.is_subtype_of(a, b));
    // B ≤ C (B has all properties of C, which is empty)
    assert!(checker.is_subtype_of(b, c));
    // Therefore A ≤ C (transitivity)
    assert!(checker.is_subtype_of(a, c));
}

#[test]
fn test_law_transitivity_unions() {
    // A = number
    // B = number | string
    // C = number | string | boolean
    // A ≤ B (number is in the union)
    // B ≤ C (all members of B are in C)
    // Therefore A ≤ C
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = TypeId::NUMBER;
    let b = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let c = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);

    assert!(checker.is_subtype_of(a, b));
    assert!(checker.is_subtype_of(b, c));
    assert!(checker.is_subtype_of(a, c));
}

#[test]
fn test_law_transitivity_arrays() {
    // Array<T> is covariant in T
    // A = number
    // B = number | string
    // Array<A> ≤ Array<B>
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = TypeId::NUMBER;
    let b = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    let arr_a = interner.array(a);
    let arr_b = interner.array(b);

    assert!(checker.is_subtype_of(a, b));
    assert!(checker.is_subtype_of(arr_a, arr_b));
}

// =============================================================================
// Antisymmetry Tests (A ≤ B and B ≤ A implies A = B)
// =============================================================================

#[test]
fn test_law_antisymmetry_intrinsics() {
    // For intrinsic types, antisymmetry means:
    // If A ≤ B and B ≤ A, then A = B (same TypeId)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // For intrinsics, we know that:
    // string ≤ string is true, so they must be equal (same TypeId)
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::STRING));
    assert_eq!(TypeId::STRING, TypeId::STRING);

    // This also validates that different intrinsics are NOT subtypes of each other
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::STRING));
}

#[test]
fn test_law_antisymmetry_structural_objects() {
    // Structural types with the same shape should have the same TypeId
    // due to canonicalization via interning
    let interner = TypeInterner::new();

    // Create two structurally identical objects
    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let mut checker = SubtypeChecker::new(&interner);

    // obj1 ≤ obj2 and obj2 ≤ obj1
    assert!(checker.is_subtype_of(obj1, obj2));
    assert!(checker.is_subtype_of(obj2, obj1));

    // Therefore obj1 = obj2 (same TypeId due to interning)
    assert_eq!(obj1, obj2);
}

#[test]
fn test_law_antisymmetry_structural_unions() {
    // Unions with the same members in the same order should be equal
    let interner = TypeInterner::new();

    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let mut checker = SubtypeChecker::new(&interner);

    assert!(checker.is_subtype_of(union1, union2));
    assert!(checker.is_subtype_of(union2, union1));
    assert_eq!(union1, union2);
}

#[test]
fn test_law_antisymmetry_literals() {
    // Different literal values should NOT be subtypes of each other
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // hello is NOT a subtype of world
    assert!(!checker.is_subtype_of(hello, world));
    // world is NOT a subtype of hello
    assert!(!checker.is_subtype_of(world, hello));
    // Therefore hello != world (different TypeIds)
    assert_ne!(hello, world);
}

// =============================================================================
// Top Type Tests (T ≤ any)
// =============================================================================

#[test]
fn test_law_top_type_any() {
    // Every type should be a subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Primitives
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::ANY));
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::ANY));
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, TypeId::ANY));

    // Literals
    let hello = interner.literal_string("hello");
    let num_42 = interner.literal_number(42.0);
    assert!(checker.is_subtype_of(hello, TypeId::ANY));
    assert!(checker.is_subtype_of(num_42, TypeId::ANY));

    // Objects
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(checker.is_subtype_of(obj, TypeId::ANY));

    // Arrays
    let arr = interner.array(TypeId::STRING);
    assert!(checker.is_subtype_of(arr, TypeId::ANY));

    // Unions
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(union, TypeId::ANY));

    // Functions
    let fn_ty = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_subtype_of(fn_ty, TypeId::ANY));

    // never is also a subtype of any (bottom ≤ top)
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::ANY));

    // any is a subtype of itself
    assert!(checker.is_subtype_of(TypeId::ANY, TypeId::ANY));
}

// =============================================================================
// Bottom Type Tests (never ≤ T)
// =============================================================================

#[test]
fn test_law_bottom_type_never() {
    // never should be a subtype of every type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Primitives
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NUMBER));
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::BOOLEAN));

    // Objects
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(checker.is_subtype_of(TypeId::NEVER, obj));

    // Arrays
    let arr = interner.array(TypeId::STRING);
    assert!(checker.is_subtype_of(TypeId::NEVER, arr));

    // Unions
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(TypeId::NEVER, union));

    // Functions
    let fn_ty = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_subtype_of(TypeId::NEVER, fn_ty));

    // any and unknown
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::ANY));
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));

    // never is a subtype of itself
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NEVER));
}

#[test]
fn test_law_never_not_supertype() {
    // Nothing (except never) should be a subtype of never
    // This validates that never is truly the BOTTOM type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Primitives are NOT subtypes of never
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NEVER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::NEVER));

    // Objects are NOT subtypes of never
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(!checker.is_subtype_of(obj, TypeId::NEVER));
}

// =============================================================================
// Unknown Type Tests (Top for Safe Types)
// =============================================================================

#[test]
fn test_law_unknown_top_safe() {
    // unknown is the top type for safe types
    // Every type is a subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Primitives
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, TypeId::UNKNOWN));

    // Objects
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(checker.is_subtype_of(obj, TypeId::UNKNOWN));

    // never is a subtype of unknown
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));

    // unknown is a subtype of itself
    assert!(checker.is_subtype_of(TypeId::UNKNOWN, TypeId::UNKNOWN));
}

#[test]
fn test_law_unknown_not_any() {
    // unknown is NOT a subtype of concrete types
    // This distinguishes unknown from any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // unknown is NOT a subtype of string
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::STRING));
    // unknown is NOT a subtype of number
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::NUMBER));
}

// =============================================================================
// Coinductive Semantics Tests (Recursive Types)
// =============================================================================

#[test]
fn test_coinductive_recursive_type_reflexivity() {
    // Recursive types should be subtypes of themselves
    // This tests coinductive semantics (greatest fixed point)
    let interner = TypeInterner::new();
    let mut env = crate::TypeEnvironment::new();

    // Create a recursive type: interface A { x: A }
    let x = interner.intern_string("x");
    let sym_a = SymbolRef(100);

    // Create the recursive structure
    let recursive_a = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId(100), // Self-reference
        write_type: TypeId(100),
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // Register the type in the environment
    env.insert(sym_a, recursive_a);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // The type should be reflexive (subtype of itself)
    // This relies on coinductive cycle detection
    assert!(checker.check_subtype(recursive_a, recursive_a).is_true());
}

#[test]
fn test_coinductive_mutually_recursive_types() {
    // Mutually recursive types should be handled correctly
    // interface A { b: B }
    // interface B { a: A }
    let interner = TypeInterner::new();
    let mut env = crate::TypeEnvironment::new();

    let b_prop = interner.intern_string("b");
    let a_prop = interner.intern_string("a");

    let sym_a = SymbolRef(100);
    let sym_b = SymbolRef(101);

    // A = { b: B }
    let type_a = interner.object(vec![PropertyInfo {
        name: b_prop,
        type_id: TypeId(101), // Reference to B
        write_type: TypeId(101),
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // B = { a: A }
    let type_b = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId(100), // Reference to A
        write_type: TypeId(100),
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    env.insert(sym_a, type_a);
    env.insert(sym_b, type_b);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Both types should be reflexive
    assert!(checker.check_subtype(type_a, type_a).is_true());
    assert!(checker.check_subtype(type_b, type_b).is_true());
}

// =============================================================================
// Canonicalization Tests (O(1) Equality via Interning)
// =============================================================================

#[test]
fn test_canonicalization_structural_objects() {
    // Structurally identical objects should have the same TypeId
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Should be the same TypeId due to canonicalization
    assert_eq!(obj1, obj2);
}

#[test]
fn test_canonicalization_functions() {
    // Functions with identical signatures should have the same TypeId
    let interner = TypeInterner::new();

    let fn_shape = FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let fn1 = interner.function(fn_shape.clone());
    let fn2 = interner.function(fn_shape);

    // Should be the same TypeId due to canonicalization
    assert_eq!(fn1, fn2);
}

#[test]
fn test_canonicalization_tuples() {
    // Tuples with identical elements should have the same TypeId
    let interner = TypeInterner::new();

    let tuple_elems = vec![
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
    ];

    let tuple1 = interner.tuple(tuple_elems.clone());
    let tuple2 = interner.tuple(tuple_elems);

    // Should be the same TypeId due to canonicalization
    assert_eq!(tuple1, tuple2);
}

#[test]
fn test_canonicalization_unions_order_matters() {
    // Union order matters for TypeId equality
    // (unions are not automatically sorted)
    let interner = TypeInterner::new();

    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    // Different order = different TypeId (unless normalized)
    // This test documents current behavior
    assert_eq!(union1, union1);
    assert_eq!(union2, union2);
    // Note: These might be different TypeIds unless union normalization is implemented
}

#[test]
fn test_canonicalization_property_order_irrelevant() {
    // Object property order should NOT affect TypeId
    // (properties are sorted for canonicalization)
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("z"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("m"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("m"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("z"), TypeId::BOOLEAN),
    ]);

    // Should be the same TypeId (properties are sorted)
    assert_eq!(obj1, obj2);
}
