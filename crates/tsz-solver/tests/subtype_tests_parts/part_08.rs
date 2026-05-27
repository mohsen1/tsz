#[test]
fn test_interface_merge_global_augmentation() {
    // Simulating global augmentation:
    // interface Window { myProp: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let document = interner.intern_string("document");
    let my_prop = interner.intern_string("myProp");

    // Original Window
    let window_original = interner.object(vec![PropertyInfo::new(document, TypeId::STRING)]);

    // Augmented Window
    let window_augmented = interner.object(vec![
        PropertyInfo::new(document, TypeId::STRING),
        PropertyInfo::new(my_prop, TypeId::STRING),
    ]);

    // Augmented is subtype of original
    assert!(checker.is_subtype_of(window_augmented, window_original));
}

#[test]
fn test_interface_merge_namespace_merge() {
    // interface + namespace merge (modeled as object with call signature + properties)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("prop");

    // Interface part
    let interface_part = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Another object with same structure
    let same_structure = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Same structure - mutual subtypes
    assert!(checker.is_subtype_of(interface_part, same_structure));
    assert!(checker.is_subtype_of(same_structure, interface_part));
}

#[test]
fn test_interface_merge_multiple_files() {
    // Simulating interface merged from multiple files
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let file1_prop = interner.intern_string("fromFile1");
    let file2_prop = interner.intern_string("fromFile2");

    // What file1 sees
    let file1_view = interner.object(vec![PropertyInfo::new(file1_prop, TypeId::STRING)]);

    // Fully merged
    let merged = interner.object(vec![
        PropertyInfo::new(file1_prop, TypeId::STRING),
        PropertyInfo::new(file2_prop, TypeId::NUMBER),
    ]);

    // Merged is subtype of partial view
    assert!(checker.is_subtype_of(merged, file1_view));
}

#[test]
fn test_interface_merge_empty_interface() {
    // interface A {}
    // interface A { prop: string }
    // Merged: { prop: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("prop");

    let empty = interner.object(vec![]);

    let with_prop = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Both subtype of empty
    assert!(checker.is_subtype_of(with_prop, empty));
    assert!(checker.is_subtype_of(empty, empty));
}

// =============================================================================
// INTERFACE VS TYPE ALIAS COMPATIBILITY TESTS
// =============================================================================

#[test]
fn test_interface_vs_type_alias_same_structure() {
    // interface I { a: string }
    // type T = { a: string }
    // Both should be compatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");

    // Interface
    let interface_i = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Type alias (same structure)
    let type_t = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}

#[test]
fn test_interface_vs_type_alias_with_methods() {
    // interface I { method(): void }
    // type T = { method(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface_i = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    let type_t = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}

#[test]
fn test_interface_vs_intersection_type() {
    // interface I { a: string; b: number }
    // type T = { a: string } & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_i = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let type_intersection = interner.intersection(vec![obj_a, obj_b]);

    // Interface should be subtype of intersection (has all properties)
    assert!(checker.is_subtype_of(interface_i, type_intersection));
}

#[test]
fn test_interface_vs_type_alias_optional() {
    // interface I { value?: string }
    // type T = { value?: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_i = interner.object(vec![PropertyInfo::opt(value, TypeId::STRING)]);

    let type_t = interner.object(vec![PropertyInfo::opt(value, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}

#[test]
fn test_interface_vs_type_alias_readonly() {
    // interface I { readonly value: string }
    // type T = { readonly value: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_i = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    let type_t = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}

#[test]
fn test_interface_vs_type_alias_index_signature() {
    // interface I { [key: string]: number }
    // type T = { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface_i = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let type_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Same structure
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}

#[test]
fn test_interface_extends_type_alias() {
    // type Base = { a: string }
    // interface Derived extends Base { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let type_base = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_derived = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // Interface extends type alias
    assert!(checker.is_subtype_of(interface_derived, type_base));
}

#[test]
fn test_type_alias_intersection_with_interface() {
    // interface I { a: string }
    // type T = I & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_i = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let extra = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let type_t = interner.intersection(vec![interface_i, extra]);

    // T is subtype of I (intersection contains interface)
    assert!(checker.is_subtype_of(type_t, interface_i));
}

// =============================================================================
// NEVER AS BOTTOM TYPE TESTS
// =============================================================================

#[test]
fn test_never_is_bottom_type_for_primitives() {
    // never is subtype of all primitive types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // never <: string
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::STRING));
    // never <: number
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NUMBER));
    // never <: boolean
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::BOOLEAN));
    // never <: symbol
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::SYMBOL));
    // never <: bigint
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::BIGINT));

    // But primitives are NOT subtypes of never
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NEVER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::NEVER));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, TypeId::NEVER));
}

#[test]
fn test_never_is_bottom_type_for_object_types() {
    // never is subtype of object types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
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

    // never <: { name: string }
    assert!(checker.is_subtype_of(TypeId::NEVER, obj));
    // { name: string } is NOT subtype of never
    assert!(!checker.is_subtype_of(obj, TypeId::NEVER));
}

#[test]
fn test_never_is_bottom_type_for_function_types() {
    // never is subtype of function types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // never <: (x: string) => number
    assert!(checker.is_subtype_of(TypeId::NEVER, fn_type));
    // (x: string) => number is NOT subtype of never
    assert!(!checker.is_subtype_of(fn_type, TypeId::NEVER));
}

#[test]
fn test_never_is_bottom_type_for_tuple_types() {
    // never is subtype of tuple types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    // never <: [string, number]
    assert!(checker.is_subtype_of(TypeId::NEVER, tuple));
    // [string, number] is NOT subtype of never
    assert!(!checker.is_subtype_of(tuple, TypeId::NEVER));
}

#[test]
fn test_never_is_bottom_type_for_union_types() {
    // never is subtype of union types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // never <: string | number
    assert!(checker.is_subtype_of(TypeId::NEVER, union));
    // string | number is NOT subtype of never
    assert!(!checker.is_subtype_of(union, TypeId::NEVER));
}

// =============================================================================
// UNKNOWN AS TOP TYPE TESTS
// =============================================================================

#[test]
fn test_unknown_is_top_type_for_primitives() {
    // All primitive types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string <: unknown
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    // number <: unknown
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::UNKNOWN));
    // boolean <: unknown
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, TypeId::UNKNOWN));
    // symbol <: unknown
    assert!(checker.is_subtype_of(TypeId::SYMBOL, TypeId::UNKNOWN));
    // bigint <: unknown
    assert!(checker.is_subtype_of(TypeId::BIGINT, TypeId::UNKNOWN));

    // But unknown is NOT subtype of primitives
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::BOOLEAN));
}

#[test]
fn test_unknown_is_top_type_for_object_types() {
    // Object types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
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

    // { name: string } <: unknown
    assert!(checker.is_subtype_of(obj, TypeId::UNKNOWN));
    // unknown is NOT subtype of { name: string }
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, obj));
}

#[test]
fn test_unknown_is_top_type_for_function_types() {
    // Function types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: number) => string <: unknown
    assert!(checker.is_subtype_of(fn_type, TypeId::UNKNOWN));
    // unknown is NOT subtype of (x: number) => string
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, fn_type));
}

#[test]
fn test_unknown_is_top_type_for_tuple_types() {
    // Tuple types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // [boolean, string] <: unknown
    assert!(checker.is_subtype_of(tuple, TypeId::UNKNOWN));
    // unknown is NOT subtype of [boolean, string]
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, tuple));
}

#[test]
fn test_unknown_is_top_type_for_never() {
    // never is subtype of unknown (bottom <: top)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // never <: unknown
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));
    // unknown is NOT subtype of never
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::NEVER));
}

// =============================================================================
// UNION WITH NEVER SIMPLIFICATION TESTS
// =============================================================================

#[test]
fn test_union_never_with_primitive_simplifies() {
    // T | never simplifies to T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string | never should behave like string
    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NEVER]);

    // string | never <: string (via simplification)
    assert!(checker.is_subtype_of(union_with_never, TypeId::STRING));
    // string <: string | never
    assert!(checker.is_subtype_of(TypeId::STRING, union_with_never));
}

#[test]
fn test_union_never_with_multiple_types_simplifies() {
    // (A | B | never) should behave like (A | B)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NEVER]);
    let union_without_never = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // (string | number | never) <: (string | number)
    assert!(checker.is_subtype_of(union_with_never, union_without_never));
    // (string | number) <: (string | number | never)
    assert!(checker.is_subtype_of(union_without_never, union_with_never));
}

#[test]
fn test_union_never_with_object_simplifies() {
    // { x: T } | never should behave like { x: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let union_with_never = interner.union(vec![obj, TypeId::NEVER]);

    // { x: number } | never <: { x: number }
    assert!(checker.is_subtype_of(union_with_never, obj));
    // { x: number } <: { x: number } | never
    assert!(checker.is_subtype_of(obj, union_with_never));
}

#[test]
fn test_union_only_never_remains_never() {
    // never | never should still be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_of_nevers = interner.union(vec![TypeId::NEVER, TypeId::NEVER]);

    // never | never <: never
    assert!(checker.is_subtype_of(union_of_nevers, TypeId::NEVER));
    // never <: never | never
    assert!(checker.is_subtype_of(TypeId::NEVER, union_of_nevers));
}

#[test]
fn test_union_never_first_position_simplifies() {
    // never | T should behave like T (never in first position)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_never_first = interner.union(vec![TypeId::NEVER, TypeId::BOOLEAN]);

    // never | boolean <: boolean
    assert!(checker.is_subtype_of(union_never_first, TypeId::BOOLEAN));
    // boolean <: never | boolean
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, union_never_first));
}

// =============================================================================
// INTERSECTION WITH UNKNOWN SIMPLIFICATION TESTS
// =============================================================================

#[test]
fn test_intersection_unknown_with_primitive_simplifies() {
    // T & unknown simplifies to T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::UNKNOWN]);

    // string & unknown <: string
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
    // string <: string & unknown
    assert!(checker.is_subtype_of(TypeId::STRING, intersection));
}

#[test]
fn test_intersection_unknown_with_object_simplifies() {
    // { x: T } & unknown should behave like { x: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj, TypeId::UNKNOWN]);

    // { x: string } & unknown <: { x: string }
    assert!(checker.is_subtype_of(intersection, obj));
    // { x: string } <: { x: string } & unknown
    assert!(checker.is_subtype_of(obj, intersection));
}

#[test]
fn test_intersection_unknown_with_function_simplifies() {
    // ((x: T) => U) & unknown should behave like (x: T) => U
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    let intersection = interner.intersection(vec![fn_type, TypeId::UNKNOWN]);

    // ((x: string) => boolean) & unknown <: (x: string) => boolean
    assert!(checker.is_subtype_of(intersection, fn_type));
    // (x: string) => boolean <: ((x: string) => boolean) & unknown
    assert!(checker.is_subtype_of(fn_type, intersection));
}

#[test]
fn test_intersection_unknown_first_position_simplifies() {
    // unknown & T should behave like T (unknown in first position)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::UNKNOWN, TypeId::NUMBER]);

    // unknown & number <: number
    assert!(checker.is_subtype_of(intersection, TypeId::NUMBER));
    // number <: unknown & number
    assert!(checker.is_subtype_of(TypeId::NUMBER, intersection));
}

#[test]
fn test_intersection_multiple_unknowns_simplifies() {
    // unknown & unknown & T should behave like T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection =
        interner.intersection(vec![TypeId::UNKNOWN, TypeId::STRING, TypeId::UNKNOWN]);

    // unknown & string & unknown <: string
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
    // string <: unknown & string & unknown
    assert!(checker.is_subtype_of(TypeId::STRING, intersection));
}

// =============================================================================
// NUMERIC ENUM ASSIGNABILITY TESTS
// =============================================================================

#[test]
fn test_numeric_enum_member_to_number() {
    // enum E { A = 0, B = 1 }
    // E.A (literal 0) is subtype of number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);

    // Numeric enum members are subtypes of number
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(enum_b, TypeId::NUMBER));
}

#[test]
fn test_numeric_enum_union() {
    // enum E { A = 0, B = 1, C = 2 }
    // E is union of 0 | 1 | 2
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_c = interner.literal_number(2.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Enum type is subtype of number
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));

    // Individual members are subtypes of enum type
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));
}

#[test]
fn test_numeric_enum_same_values_equal() {
    // enum E1 { A = 0 }
    // enum E2 { A = 0 }
    // Same literal values are equal structurally
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e2_a = interner.literal_number(0.0);

    // Same literal values are equal
    assert!(checker.is_subtype_of(e1_a, e2_a));
    assert!(checker.is_subtype_of(e2_a, e1_a));
}

#[test]
fn test_numeric_enum_computed_values() {
    // enum E { A = 1, B = 2, C = A + B } // C = 3
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(1.0);
    let enum_b = interner.literal_number(2.0);
    let enum_c = interner.literal_number(3.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // All computed values are part of enum
    assert!(checker.is_subtype_of(enum_c, enum_type));
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));
}

#[test]
fn test_numeric_enum_negative_values() {
    // enum E { A = -1, B = 0, C = 1 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(-1.0);
    let enum_b = interner.literal_number(0.0);
    let enum_c = interner.literal_number(1.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Negative values work correctly
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(enum_a, enum_type));
}

#[test]
fn test_number_not_subtype_of_numeric_enum() {
    // number is not subtype of enum (enum is more specific)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_type = interner.union(vec![enum_a, enum_b]);

    // number is not subtype of specific enum union
    assert!(!checker.is_subtype_of(TypeId::NUMBER, enum_type));
}

#[test]
fn test_numeric_enum_single_member() {
    // enum E { Only = 42 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let only = interner.literal_number(42.0);

    // Single member enum
    assert!(checker.is_subtype_of(only, TypeId::NUMBER));

    // Other number literals are not the enum value
    let other = interner.literal_number(43.0);
    assert!(!checker.is_subtype_of(other, only));
}

// =============================================================================
// STRING ENUM ASSIGNABILITY TESTS
// =============================================================================

#[test]
fn test_string_enum_member_to_string() {
    // enum E { A = "a", B = "b" }
    // E.A (literal "a") is subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_string("a");
    let enum_b = interner.literal_string("b");

    // String enum members are subtypes of string
    assert!(checker.is_subtype_of(enum_a, TypeId::STRING));
    assert!(checker.is_subtype_of(enum_b, TypeId::STRING));
}

#[test]
fn test_string_enum_union() {
    // enum Direction { Up = "UP", Down = "DOWN", Left = "LEFT", Right = "RIGHT" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let up = interner.literal_string("UP");
    let down = interner.literal_string("DOWN");
    let left = interner.literal_string("LEFT");
    let right = interner.literal_string("RIGHT");

    let direction = interner.union(vec![up, down, left, right]);

    // Enum type is subtype of string
    assert!(checker.is_subtype_of(direction, TypeId::STRING));

    // Individual members are subtypes of enum type
    assert!(checker.is_subtype_of(up, direction));
    assert!(checker.is_subtype_of(down, direction));
}

#[test]
fn test_string_not_subtype_of_string_enum() {
    // string is not subtype of string enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let enum_type = interner.union(vec![a, b]);

    // string is not subtype of specific string enum
    assert!(!checker.is_subtype_of(TypeId::STRING, enum_type));
}

#[test]
fn test_string_enum_non_member_literal() {
    // Non-member string literal is not subtype of enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let enum_type = interner.union(vec![a, b]);

    let c = interner.literal_string("c");

    // "c" is not a member of the enum
    assert!(!checker.is_subtype_of(c, enum_type));
}

#[test]
fn test_string_enum_case_sensitive() {
    // String enums are case-sensitive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let upper = interner.literal_string("UP");
    let lower = interner.literal_string("up");

    // Different cases are different values
    assert!(!checker.is_subtype_of(upper, lower));
    assert!(!checker.is_subtype_of(lower, upper));
}

#[test]
fn test_string_enum_empty_string() {
    // enum E { Empty = "" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty = interner.literal_string("");

    assert!(checker.is_subtype_of(empty, TypeId::STRING));
}

#[test]
fn test_string_enum_with_special_chars() {
    // enum E { Special = "hello-world_123" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let special = interner.literal_string("hello-world_123");

    assert!(checker.is_subtype_of(special, TypeId::STRING));
}

// =============================================================================
// CONST ENUM HANDLING TESTS
// =============================================================================

#[test]
fn test_const_enum_numeric_values() {
    // const enum E { A = 0, B = 1, C = 2 }
    // Const enums are inlined - same as regular numeric enum for type checking
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_number(0.0);
    let b = interner.literal_number(1.0);
    let c = interner.literal_number(2.0);

    let const_enum = interner.union(vec![a, b, c]);

    // Same behavior as regular enum
    assert!(checker.is_subtype_of(const_enum, TypeId::NUMBER));
    assert!(checker.is_subtype_of(a, const_enum));
}

#[test]
fn test_const_enum_string_values() {
    // const enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");

    let const_enum = interner.union(vec![a, b]);

    assert!(checker.is_subtype_of(const_enum, TypeId::STRING));
    assert!(checker.is_subtype_of(a, const_enum));
}

#[test]
fn test_const_enum_computed_member() {
    // const enum E { A = 1 << 0, B = 1 << 1, C = 1 << 2 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_number(1.0); // 1 << 0
    let b = interner.literal_number(2.0); // 1 << 1
    let c = interner.literal_number(4.0); // 1 << 2

    let flags_enum = interner.union(vec![a, b, c]);

    assert!(checker.is_subtype_of(flags_enum, TypeId::NUMBER));
}

#[test]
fn test_const_enum_single_value() {
    // const enum E { Only = 42 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let only = interner.literal_number(42.0);

    // Single value const enum
    assert!(checker.is_subtype_of(only, TypeId::NUMBER));
}

#[test]
fn test_const_enum_mixed_types() {
    // Testing union behavior for hypothetical mixed enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num = interner.literal_number(0.0);
    let str = interner.literal_string("b");

    let mixed = interner.union(vec![num, str]);

    // Mixed enum is subtype of string | number
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(mixed, string_or_number));

    // But not just string or just number
    assert!(!checker.is_subtype_of(mixed, TypeId::STRING));
    assert!(!checker.is_subtype_of(mixed, TypeId::NUMBER));
}

#[test]
fn test_const_enum_preserves_literal_types() {
    // Const enum values should preserve their literal types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let val = interner.literal_number(42.0);
    let other = interner.literal_number(42.0);

    // Same literal values are equal
    assert!(checker.is_subtype_of(val, other));
    assert!(checker.is_subtype_of(other, val));
}

#[test]
fn test_const_enum_bitwise_flags() {
    // const enum Flags { None = 0, Read = 1, Write = 2, Execute = 4, All = 7 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let none = interner.literal_number(0.0);
    let read = interner.literal_number(1.0);
    let write = interner.literal_number(2.0);
    let execute = interner.literal_number(4.0);
    let all = interner.literal_number(7.0);

    let flags = interner.union(vec![none, read, write, execute, all]);

    assert!(checker.is_subtype_of(flags, TypeId::NUMBER));
    assert!(checker.is_subtype_of(all, flags));
}

// =============================================================================
// ENUM MEMBER ACCESS TESTS
// =============================================================================

#[test]
fn test_enum_member_access_numeric() {
    // enum E { A = 0, B = 1 }
    // typeof E.A is literal type 0
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e_a = interner.literal_number(0.0);
    let e_b = interner.literal_number(1.0);

    // E.A is distinct from E.B
    assert!(!checker.is_subtype_of(e_a, e_b));
    assert!(!checker.is_subtype_of(e_b, e_a));

    // But both are numbers
    assert!(checker.is_subtype_of(e_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(e_b, TypeId::NUMBER));
}

#[test]
fn test_literal_enum_members_with_same_def_id_are_distinct_subtypes() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_def = DefId(42);
    let enum_a = interner.intern(TypeData::Enum(enum_def, interner.literal_number(0.0)));
    let enum_b = interner.intern(TypeData::Enum(enum_def, interner.literal_number(1.0)));

    assert!(checker.is_subtype_of(enum_a, enum_a));
    assert!(checker.is_subtype_of(enum_b, enum_b));
    assert!(!checker.is_subtype_of(enum_a, enum_b));
    assert!(!checker.is_subtype_of(enum_b, enum_a));
}

#[test]
fn test_enum_member_access_string() {
    // enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e_a = interner.literal_string("a");
    let e_b = interner.literal_string("b");

    // E.A is distinct from E.B
    assert!(!checker.is_subtype_of(e_a, e_b));

    // Both are strings
    assert!(checker.is_subtype_of(e_a, TypeId::STRING));
    assert!(checker.is_subtype_of(e_b, TypeId::STRING));
}

#[test]
fn test_enum_member_in_object_property() {
    // interface I { status: Status.Active }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let status_prop = interner.intern_string("status");
    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");

    let interface_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_inactive = interner.object(vec![PropertyInfo::new(status_prop, inactive)]);

    // Object with matching status is subtype
    assert!(checker.is_subtype_of(obj_active, interface_active));

    // Object with different status is not
    assert!(!checker.is_subtype_of(obj_inactive, interface_active));
}

#[test]
fn test_enum_member_union_in_property() {
    // interface I { status: Status.Active | Status.Pending }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let status_prop = interner.intern_string("status");
    let active = interner.literal_string("ACTIVE");
    let pending = interner.literal_string("PENDING");
    let completed = interner.literal_string("COMPLETED");

    let active_or_pending = interner.union(vec![active, pending]);

    let interface_type = interner.object(vec![PropertyInfo::new(status_prop, active_or_pending)]);

    let obj_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_completed = interner.object(vec![PropertyInfo::new(status_prop, completed)]);

    // Active matches union
    assert!(checker.is_subtype_of(obj_active, interface_type));

    // Completed does not match union
    assert!(!checker.is_subtype_of(obj_completed, interface_type));
}

#[test]
fn test_enum_member_as_function_param() {
    // function f(status: Status.Active): void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");

    let fn_active_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("status")),
            type_id: active,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_inactive_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("status")),
            type_id: inactive,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Functions with different enum member params are not subtypes
    assert!(!checker.is_subtype_of(fn_active_param, fn_inactive_param));
}

#[test]
fn test_enum_member_as_return_type() {
    // function f(): Status.Active
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");

    let fn_returns_active = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: active,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_returns_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function returning enum member is subtype of function returning string
    assert!(checker.is_subtype_of(fn_returns_active, fn_returns_string));
}

#[test]
fn test_enum_member_narrowing() {
    // Testing narrowing: if status === Status.Active, type is Status.Active
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");
    let pending = interner.literal_string("PENDING");

    let status_enum = interner.union(vec![active, inactive, pending]);

    // After narrowing, active is subtype of the full enum
    assert!(checker.is_subtype_of(active, status_enum));

    // And the narrowed type is more specific
    assert!(!checker.is_subtype_of(status_enum, active));
}

#[test]
fn test_enum_reverse_mapping_numeric() {
    // Numeric enums have reverse mappings: E[0] === "A"
    // This is runtime behavior, but the type would be the key type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // The reverse mapped value is a string (the enum key name)
    let key_name = interner.literal_string("A");

    assert!(checker.is_subtype_of(key_name, TypeId::STRING));
}

#[test]
fn test_enum_reverse_mapping_multiple_keys() {
    // enum E { A = 0, B = 1, C = 2 }
    // E[0] === "A", E[1] === "B", E[2] === "C"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("A");
    let key_b = interner.literal_string("B");
    let key_c = interner.literal_string("C");

    // All reverse mapped keys are strings
    let key_union = interner.union(vec![key_a, key_b, key_c]);

    assert!(checker.is_subtype_of(key_union, TypeId::STRING));
    assert!(checker.is_subtype_of(key_a, key_union));
}

#[test]
fn test_string_enum_no_reverse_mapping() {
    // String enums do NOT have reverse mappings
    // enum E { A = "a" } - E["a"] is undefined, not "A"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_value = interner.literal_string("a");
    let enum_key = interner.literal_string("A");

    // The key and value are distinct types
    assert!(!checker.is_subtype_of(enum_value, enum_key));
    assert!(!checker.is_subtype_of(enum_key, enum_value));
}

#[test]
fn test_heterogeneous_enum_mixed_types() {
    // enum E { A = 0, B = "b", C = 1 }
    // Heterogeneous enum: mix of string and number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_string("b");
    let enum_c = interner.literal_number(1.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Each member is subtype of enum
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));

    // Enum is subtype of string | number
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(enum_type, string_or_number));

    // But not just string or just number
    assert!(!checker.is_subtype_of(enum_type, TypeId::STRING));
    assert!(!checker.is_subtype_of(enum_type, TypeId::NUMBER));
}

#[test]
fn test_const_enum_inlined_literal() {
    // const enum E { A = 1, B = 2 }
    // At type level, behaves like literals
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_a = interner.literal_number(1.0);
    let _const_b = interner.literal_number(2.0);

    // Const enum members maintain literal types
    assert!(checker.is_subtype_of(const_a, TypeId::NUMBER));

    // And are compatible with same literal
    let same_literal = interner.literal_number(1.0);
    assert!(checker.is_subtype_of(const_a, same_literal));
    assert!(checker.is_subtype_of(same_literal, const_a));
}

#[test]
fn test_const_enum_string_inlined() {
    // const enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_a = interner.literal_string("a");
    let const_b = interner.literal_string("b");
    let const_enum = interner.union(vec![const_a, const_b]);

    // Inlined const enum values are literal types
    assert!(checker.is_subtype_of(const_a, TypeId::STRING));
    assert!(checker.is_subtype_of(const_enum, TypeId::STRING));
}

#[test]
fn test_enum_cross_compatibility_same_shape() {
    // enum E1 { A = 0, B = 1 }
    // enum E2 { X = 0, Y = 1 }
    // Structurally equivalent but nominally different
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e1_b = interner.literal_number(1.0);
    let e1_type = interner.union(vec![e1_a, e1_b]);

    let e2_x = interner.literal_number(0.0);
    let e2_y = interner.literal_number(1.0);
    let e2_type = interner.union(vec![e2_x, e2_y]);

    // Same structure = compatible in structural type system
    assert!(checker.is_subtype_of(e1_type, e2_type));
    assert!(checker.is_subtype_of(e2_type, e1_type));

    // Individual members also compatible
    assert!(checker.is_subtype_of(e1_a, e2_x));
}

#[test]
fn test_enum_partial_overlap() {
    // enum E1 { A = 0, B = 1, C = 2 }
    // enum E2 { X = 0, Y = 1 }
    // E2 is subset of E1
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e1_b = interner.literal_number(1.0);
    let e1_c = interner.literal_number(2.0);
    let e1_type = interner.union(vec![e1_a, e1_b, e1_c]);

    let e2_x = interner.literal_number(0.0);
    let e2_y = interner.literal_number(1.0);
    let e2_type = interner.union(vec![e2_x, e2_y]);

    // E2 <: E1 (E2 is subset)
    assert!(checker.is_subtype_of(e2_type, e1_type));

    // E1 </: E2 (E1 has extra member)
    assert!(!checker.is_subtype_of(e1_type, e2_type));
}

#[test]
fn test_enum_with_auto_increment() {
    // enum E { A, B, C } // A = 0, B = 1, C = 2 (auto-incremented)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_c = interner.literal_number(2.0);

    // Auto-incremented values form sequential literals
    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));
}

#[test]
fn test_enum_with_explicit_and_auto() {
    // enum E { A = 10, B, C } // A = 10, B = 11, C = 12
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(10.0);
    let enum_b = interner.literal_number(11.0);
    let enum_c = interner.literal_number(12.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // All are part of enum
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));
}

#[test]
fn test_enum_member_in_conditional() {
    // Using enum member as conditional type extends target
    // E.A extends number ? true : false
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);

    // Enum member extends number
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));

    // Enum member extends same literal
    let literal_zero = interner.literal_number(0.0);
    assert!(checker.is_subtype_of(enum_a, literal_zero));
}

#[test]
fn test_const_enum_as_type_parameter_constraint() {
    // type OnlyZeroOrOne<T extends 0 | 1> = T
    // Can use const enum values as constraints
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let constraint = interner.union(vec![lit_0, lit_1]);

    let lit_2 = interner.literal_number(2.0);

    // 0 and 1 satisfy constraint
    assert!(checker.is_subtype_of(lit_0, constraint));
    assert!(checker.is_subtype_of(lit_1, constraint));

    // 2 does not satisfy constraint
    assert!(!checker.is_subtype_of(lit_2, constraint));
}

#[test]
fn test_enum_keyof() {
    // keyof typeof E for numeric enum
    // enum E { A = 0, B = 1 } -> keyof typeof E = "A" | "B"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("A");
    let key_b = interner.literal_string("B");
    let keyof_enum = interner.union(vec![key_a, key_b]);

    // Keys are strings
    assert!(checker.is_subtype_of(keyof_enum, TypeId::STRING));

    // Individual keys are part of keyof
    assert!(checker.is_subtype_of(key_a, keyof_enum));
}

#[test]
fn test_enum_value_type() {
    // typeof E[keyof typeof E] for enum E { A = 0, B = 1 }
    // = 0 | 1
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let val_a = interner.literal_number(0.0);
    let val_b = interner.literal_number(1.0);
    let value_type = interner.union(vec![val_a, val_b]);

    // Value type is union of literals
    assert!(checker.is_subtype_of(value_type, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, value_type));
}

#[test]
fn test_enum_with_bigint_like_value() {
    // enum E { BIG = 9007199254740991 } // MAX_SAFE_INTEGER
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let big_val = interner.literal_number(9007199254740991.0);

    // Large numbers still work
    assert!(checker.is_subtype_of(big_val, TypeId::NUMBER));
}

#[test]
fn test_enum_preserves_literal_identity() {
    // enum E { A = 1 }
    // const x: 1 = E.A; // Should be assignable
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(1.0);
    let literal_one = interner.literal_number(1.0);

    // Enum member is same as literal
    assert!(checker.is_subtype_of(enum_a, literal_one));
    assert!(checker.is_subtype_of(literal_one, enum_a));
}

#[test]
fn test_string_enum_unicode() {
    // enum E { EMOJI = "🎉", SYMBOL = "→" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let emoji = interner.literal_string("🎉");
    let symbol = interner.literal_string("→");
    let enum_type = interner.union(vec![emoji, symbol]);

    // Unicode strings work
    assert!(checker.is_subtype_of(emoji, TypeId::STRING));
    assert!(checker.is_subtype_of(symbol, enum_type));
}

#[test]
fn test_enum_in_mapped_type_context() {
    // { [K in E]: K } where E = "a" | "b"
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Result object has properties "a" and "b"
    let result = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), lit_a),
        PropertyInfo::new(interner.intern_string("b"), lit_b),
    ]);

    assert!(result != TypeId::ERROR);
}

// =============================================================================
// Index Signature Tests - String/Number Keys and Intersections
// =============================================================================
// These tests cover index signature behavior including string/number keys,
// intersection of index signatures, and edge cases.

#[test]
fn test_index_signature_string_to_string() {
    // { [key: string]: number } is subtype of { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}

#[test]
fn test_index_signature_number_to_number() {
    // { [key: number]: string } is subtype of { [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}

#[test]
fn test_index_signature_covariant_value_type() {
    // { [key: string]: "a" | "b" } is subtype of { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let literal_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let obj_specific = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: literal_union,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_general = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(checker.is_subtype_of(obj_specific, obj_general));
    assert!(!checker.is_subtype_of(obj_general, obj_specific));
}

#[test]
fn test_index_signature_both_string_and_number() {
    // { [key: string]: any, [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_both = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let obj_string_only = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with both is subtype of object with just string
    assert!(checker.is_subtype_of(obj_both, obj_string_only));
}

#[test]
fn test_index_signature_number_subtype_of_string() {
    // Number index signature value must be subtype of string index signature value
    // { [key: string]: any, [key: number]: string } - string is subtype of any
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    // This should be valid - string is subtype of any
    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_index_signature_intersection_combines() {
    // { [key: string]: A } & { [key: string]: B } = { [key: string]: A & B }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Intersection should be assignable to either
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}

