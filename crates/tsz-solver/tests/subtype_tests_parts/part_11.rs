#[test]
fn test_optional_property_accepts_undefined() {
    // { x?: string } - x can be string | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // Optional property type
    let optional_value = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // undefined is valid
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_value));

    // string is valid
    assert!(checker.is_subtype_of(TypeId::STRING, optional_value));

    // null is not valid for optional property (unless explicitly added)
    assert!(!checker.is_subtype_of(TypeId::NULL, optional_value));
}

#[test]
fn test_nullish_coalescing_result_type() {
    // (string | null) ?? "default" -> string
    // The result excludes null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After ?? operation, null is excluded
    let result = TypeId::STRING;

    // Result is subtype of original nullable
    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    assert!(checker.is_subtype_of(result, nullable));
}

#[test]
fn test_null_union_with_literal_numbers() {
    // 1 | 2 | 3 | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    let nullable_nums = interner.union(vec![lit_1, lit_2, lit_3, TypeId::NULL]);

    // Each literal is subtype
    assert!(checker.is_subtype_of(lit_1, nullable_nums));
    assert!(checker.is_subtype_of(lit_2, nullable_nums));
    assert!(checker.is_subtype_of(lit_3, nullable_nums));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_nums));

    // Number itself is not subtype
    assert!(!checker.is_subtype_of(TypeId::NUMBER, nullable_nums));
}

#[test]
fn test_undefined_union_with_boolean() {
    // boolean | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let optional_bool = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);

    assert!(checker.is_subtype_of(TypeId::BOOLEAN, optional_bool));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_bool));

    // true/false literals are subtypes too
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    assert!(checker.is_subtype_of(lit_true, optional_bool));
    assert!(checker.is_subtype_of(lit_false, optional_bool));
}

// =============================================================================
// Intersection Type Tests - Object and Primitive Intersections
// =============================================================================
// Additional tests for intersection type behavior

#[test]
fn test_primitive_intersection_string_number_is_never() {
    // string & number should reduce to never (disjoint primitives)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_and_number = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    // Should be never (or equivalent to never)
    assert!(checker.is_subtype_of(string_and_number, TypeId::NEVER));
}

#[test]
fn test_primitive_intersection_boolean_string_is_never() {
    // boolean & string should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let bool_and_string = interner.intersection(vec![TypeId::BOOLEAN, TypeId::STRING]);

    assert!(checker.is_subtype_of(bool_and_string, TypeId::NEVER));
}

#[test]
fn test_primitive_intersection_number_bigint_is_never() {
    // number & bigint should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num_and_bigint = interner.intersection(vec![TypeId::NUMBER, TypeId::BIGINT]);

    assert!(checker.is_subtype_of(num_and_bigint, TypeId::NEVER));
}

#[test]
fn test_literal_intersection_same_type() {
    // "hello" & string should be "hello"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let hello_and_string = interner.intersection(vec![hello, TypeId::STRING]);

    // "hello" & string is just "hello"
    assert!(checker.is_subtype_of(hello_and_string, hello));
    assert!(checker.is_subtype_of(hello, hello_and_string));
}

#[test]
fn test_literal_intersection_different_literals_is_never() {
    // "hello" & "world" should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let hello_and_world = interner.intersection(vec![hello, world]);

    assert!(checker.is_subtype_of(hello_and_world, TypeId::NEVER));
}

#[test]
fn test_number_literal_intersection_different_values() {
    // 1 & 2 should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_and_two = interner.intersection(vec![one, two]);

    assert!(checker.is_subtype_of(one_and_two, TypeId::NEVER));
}

#[test]
fn test_boolean_literal_intersection() {
    // true & false should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let true_and_false = interner.intersection(vec![lit_true, lit_false]);

    assert!(checker.is_subtype_of(true_and_false, TypeId::NEVER));
}

#[test]
fn test_object_intersection_disjoint_properties() {
    // { a: string } & { b: number } = { a: string, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Should be subtype of both components
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}

#[test]
fn test_object_intersection_same_property_compatible() {
    // { x: string } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let obj1 = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj2 = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    // Should be equivalent to the original
    assert!(checker.is_subtype_of(intersection, obj1));
    assert!(checker.is_subtype_of(obj1, intersection));
}

#[test]
fn test_object_intersection_property_narrowing() {
    // { x: string | number } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo::new(x_name, string_or_number)]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of the narrow version
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}

#[test]
fn test_intersection_with_any() {
    // T & any = any (any absorbs in intersection for assignability)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj_and_any = interner.intersection(vec![obj, TypeId::ANY]);

    // any is assignable to/from most things
    assert!(checker.is_subtype_of(TypeId::ANY, obj_and_any));
}

#[test]
fn test_intersection_with_unknown() {
    // T & unknown = T (unknown is identity for intersection)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj_and_unknown = interner.intersection(vec![obj, TypeId::UNKNOWN]);

    // Should be equivalent to obj
    assert!(checker.is_subtype_of(obj_and_unknown, obj));
    assert!(checker.is_subtype_of(obj, obj_and_unknown));
}

#[test]
fn test_function_intersection_creates_overload() {
    // ((x: string) => number) & ((x: number) => string)
    // Creates an overloaded function type
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let fn_str_to_num = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_num_to_str = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let intersection = interner.intersection(vec![fn_str_to_num, fn_num_to_str]);

    // Intersection should be valid (creates overloaded type)
    assert!(intersection != TypeId::ERROR);
    assert!(intersection != TypeId::NEVER);
}

#[test]
fn test_intersection_brand_pattern() {
    // Branded type: string & { __brand: "UserId" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let user_id_lit = interner.literal_string("UserId");

    let brand_obj = interner.object(vec![PropertyInfo::new(brand_name, user_id_lit)]);

    let branded_string = interner.intersection(vec![TypeId::STRING, brand_obj]);

    // Branded string should NOT be assignable to plain string
    // (intersection is more specific)
    assert!(!checker.is_subtype_of(TypeId::STRING, branded_string));

    // Branded string IS a subtype of string
    assert!(checker.is_subtype_of(branded_string, TypeId::STRING));
}

#[test]
fn test_intersection_different_brands_is_never() {
    // (string & {__brand: "A"}) & (string & {__brand: "B"}) = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let lit_a = interner.literal_string("A");
    let lit_b = interner.literal_string("B");

    let brand_a = interner.object(vec![PropertyInfo::new(brand_name, lit_a)]);

    let brand_b = interner.object(vec![PropertyInfo::new(brand_name, lit_b)]);

    let branded_a = interner.intersection(vec![TypeId::STRING, brand_a]);
    let branded_b = interner.intersection(vec![TypeId::STRING, brand_b]);
    let both = interner.intersection(vec![branded_a, branded_b]);

    // Two different brands intersected should be never
    assert!(checker.is_subtype_of(both, TypeId::NEVER));
}

#[test]
fn test_intersection_readonly_property() {
    // { readonly x: string } & { x: string }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let readonly_obj = interner.object(vec![PropertyInfo::readonly(x_name, TypeId::STRING)]);

    let mutable_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![readonly_obj, mutable_obj]);

    // Should be a valid intersection
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_intersection_optional_and_required() {
    // { x?: string } & { x: string } = { x: string } (required wins)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let optional_obj = interner.object(vec![PropertyInfo::opt(x_name, TypeId::STRING)]);

    let required_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![optional_obj, required_obj]);

    // Intersection should be subtype of required
    assert!(checker.is_subtype_of(intersection, required_obj));
}

#[test]
fn test_intersection_index_signature_with_properties() {
    // { [key: string]: number } & { x: number }
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let index_sig = interner.object_with_index(ObjectShape {
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

    let prop_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![index_sig, prop_obj]);

    // Should be valid
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_intersection_two_index_signatures() {
    // { [key: string]: number } & { [key: string]: 1 | 2 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_or_two = interner.union(vec![one, two]);

    let index_number = interner.object_with_index(ObjectShape {
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

    let index_literal = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: one_or_two,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let intersection = interner.intersection(vec![index_number, index_literal]);

    // Intersection should be subtype of the more specific one
    assert!(checker.is_subtype_of(intersection, index_literal));
}

#[test]
fn test_array_intersection() {
    // string[] & number[] — tsc does NOT eagerly reduce this to never.
    // The intersection remains a valid (albeit uninhabitable) type.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    let intersection = interner.intersection(vec![string_array, number_array]);

    // tsc does not reduce array intersections with incompatible elements to never
    assert!(!checker.is_subtype_of(intersection, TypeId::NEVER));
}

#[test]
fn test_tuple_intersection_compatible() {
    // [string, number] & [string, number] = [string, number]
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

    let intersection = interner.intersection(vec![tuple, tuple]);

    // Should be equivalent to the tuple itself
    assert!(checker.is_subtype_of(intersection, tuple));
    assert!(checker.is_subtype_of(tuple, intersection));
}

#[test]
fn test_tuple_intersection_incompatible() {
    // [string, number] & [number, string] — tsc does NOT reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple1 = interner.tuple(vec![
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

    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let intersection = interner.intersection(vec![tuple1, tuple2]);

    // tsc does not eagerly reduce tuple intersections with incompatible elements to never
    assert!(!checker.is_subtype_of(intersection, TypeId::NEVER));
}

#[test]
fn test_intersection_union_distribution() {
    // (A | B) & C = (A & C) | (B & C) in terms of assignability
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let a_or_b = interner.union(vec![obj_a, obj_b]);
    let union_and_c = interner.intersection(vec![a_or_b, obj_c]);

    let a_and_c = interner.intersection(vec![obj_a, obj_c]);
    let b_and_c = interner.intersection(vec![obj_b, obj_c]);
    let distributed = interner.union(vec![a_and_c, b_and_c]);

    // Both should be mutually subtype (equivalent)
    assert!(checker.is_subtype_of(union_and_c, distributed));
    assert!(checker.is_subtype_of(distributed, union_and_c));
}

#[test]
fn test_intersection_null_with_object_is_never() {
    // null & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let null_and_obj = interner.intersection(vec![TypeId::NULL, obj]);

    assert!(checker.is_subtype_of(null_and_obj, TypeId::NEVER));
}

#[test]
fn test_intersection_undefined_with_object_is_never() {
    // undefined & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let undefined_and_obj = interner.intersection(vec![TypeId::UNDEFINED, obj]);

    assert!(checker.is_subtype_of(undefined_and_obj, TypeId::NEVER));
}

#[test]
fn test_intersection_method_signatures() {
    // { foo(): void } & { bar(): void } = { foo(): void, bar(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let bar_name = interner.intern_string("bar");

    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo = interner.object(vec![PropertyInfo::method(foo_name, fn_void)]);

    let obj_bar = interner.object(vec![PropertyInfo::method(bar_name, fn_void)]);

    let intersection = interner.intersection(vec![obj_foo, obj_bar]);

    // Should be subtype of both
    assert!(checker.is_subtype_of(intersection, obj_foo));
    assert!(checker.is_subtype_of(intersection, obj_bar));
}

#[test]
fn test_intersection_same_method_different_returns() {
    // { foo(): string } & { foo(): number } - conflicting method returns
    let interner = TypeInterner::new();

    let foo_name = interner.intern_string("foo");

    let fn_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo_string = interner.object(vec![PropertyInfo::method(foo_name, fn_string)]);

    let obj_foo_number = interner.object(vec![PropertyInfo::method(foo_name, fn_number)]);

    let intersection = interner.intersection(vec![obj_foo_string, obj_foo_number]);

    // Should produce valid intersection (methods become overloaded or intersection)
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_intersection_three_objects() {
    // { a: string } & { b: number } & { c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Should be subtype of all three
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}

#[test]
fn test_intersection_symbol_with_primitive_is_never() {
    // symbol & string = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let symbol_and_string = interner.intersection(vec![TypeId::SYMBOL, TypeId::STRING]);

    assert!(checker.is_subtype_of(symbol_and_string, TypeId::NEVER));
}

#[test]
fn test_intersection_object_intrinsic_with_object() {
    // object & { x: string } - object intrinsic with concrete object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let object_and_obj = interner.intersection(vec![TypeId::OBJECT, obj]);

    // { x: string } is an object, so intersection should be equivalent to { x: string }
    assert!(checker.is_subtype_of(object_and_obj, obj));
}

#[test]
fn test_intersection_never_identity() {
    // never & T = never (never absorbs everything)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let never_and_obj = interner.intersection(vec![TypeId::NEVER, obj]);
    let obj_and_never = interner.intersection(vec![obj, TypeId::NEVER]);

    assert!(checker.is_subtype_of(never_and_obj, TypeId::NEVER));
    assert!(checker.is_subtype_of(obj_and_never, TypeId::NEVER));
}

// =============================================================================
// KeyOf Type Operator Tests
// =============================================================================
// Tests for keyof type operator and property key relationships

#[test]
fn test_keyof_single_property_is_literal() {
    // keyof { x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // keyof { x } should be subtype of "x" (they're equivalent)
    assert!(checker.is_subtype_of(keyof_obj, lit_x));
}

#[test]
fn test_keyof_multiple_properties_is_union() {
    // keyof { a, b, c } = "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let expected = interner.union(vec![lit_a, lit_b, lit_c]);

    // Each literal key should be subtype of keyof
    assert!(checker.is_subtype_of(lit_a, keyof_obj));
    assert!(checker.is_subtype_of(lit_b, keyof_obj));
    assert!(checker.is_subtype_of(lit_c, keyof_obj));

    // keyof should be subtype of the union of keys
    assert!(checker.is_subtype_of(keyof_obj, expected));
}

#[test]
fn test_keyof_empty_object_is_never() {
    // keyof {} = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.intern(TypeData::KeyOf(empty_obj));

    // keyof {} should be subtype of never (they're equivalent)
    assert!(checker.is_subtype_of(keyof_empty, TypeId::NEVER));
}

#[test]
fn test_keyof_with_optional_property() {
    // keyof { x?: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::opt(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Optional property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}

#[test]
fn test_keyof_with_readonly_property() {
    // keyof { readonly x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::readonly(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Readonly property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}

#[test]
fn test_keyof_with_method() {
    // keyof { foo(): void } = "foo"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(foo_name, fn_void)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_foo = interner.literal_string("foo");

    assert!(checker.is_subtype_of(lit_foo, keyof_obj));
}

#[test]
fn test_keyof_subtype_of_string() {
    // keyof { x: number } <: string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // keyof object with string keys is subtype of string
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}

#[test]
fn test_keyof_not_equal_to_string() {
    // string is NOT a subtype of keyof { x: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // string is wider than keyof { x }
    assert!(!checker.is_subtype_of(TypeId::STRING, keyof_obj));
}

#[test]
fn test_keyof_wider_object_has_more_keys() {
    // keyof { a, b } has more keys than keyof { a }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let keyof_a = interner.intern(TypeData::KeyOf(obj_a));
    let keyof_ab = interner.intern(TypeData::KeyOf(obj_ab));

    // keyof { a } <: keyof { a, b } (fewer keys is narrower)
    assert!(checker.is_subtype_of(keyof_a, keyof_ab));
    // keyof { a, b } is NOT subtype of keyof { a }
    assert!(!checker.is_subtype_of(keyof_ab, keyof_a));
}

#[test]
fn test_keyof_union_is_intersection_of_keys() {
    // keyof (A | B) = (keyof A) & (keyof B) - only common keys
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let obj_bc = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_ab, obj_bc]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));
    let lit_b = interner.literal_string("b");

    // Only "b" is common to both - should be subtype of keyof union
    assert!(checker.is_subtype_of(lit_b, keyof_union));
}

#[test]
fn test_keyof_intersection_is_union_of_keys() {
    // keyof (A & B) = (keyof A) | (keyof B) - all keys from both
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Both "a" and "b" should be subtypes of keyof intersection
    assert!(checker.is_subtype_of(lit_a, keyof_intersection));
    assert!(checker.is_subtype_of(lit_b, keyof_intersection));
}

#[test]
fn test_keyof_any_is_string_number_symbol() {
    // keyof any = string | number | symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_any = interner.intern(TypeData::KeyOf(TypeId::ANY));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    // keyof any should be equivalent to PropertyKey
    assert!(checker.is_subtype_of(keyof_any, property_key));
}

#[test]
fn test_keyof_unknown_is_never() {
    // keyof unknown = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_unknown = interner.intern(TypeData::KeyOf(TypeId::UNKNOWN));

    assert!(checker.is_subtype_of(keyof_unknown, TypeId::NEVER));
}

#[test]
fn test_keyof_never_is_string_number_symbol() {
    // keyof never = string | number | symbol (vacuously true)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_never = interner.intern(TypeData::KeyOf(TypeId::NEVER));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    assert!(checker.is_subtype_of(keyof_never, property_key));
}

#[test]
fn test_keyof_string_has_string_methods() {
    // keyof string includes string method names
    let interner = TypeInterner::new();

    let keyof_string = interner.intern(TypeData::KeyOf(TypeId::STRING));

    // Should be valid type
    assert!(keyof_string != TypeId::ERROR);
}

#[test]
fn test_keyof_number_has_number_methods() {
    // keyof number includes number method names
    let interner = TypeInterner::new();

    let keyof_number = interner.intern(TypeData::KeyOf(TypeId::NUMBER));

    // Should be valid type
    assert!(keyof_number != TypeId::ERROR);
}

#[test]
fn test_keyof_array_type() {
    // keyof string[] includes array methods and number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let keyof_array = interner.intern(TypeData::KeyOf(string_array));

    // number should be subtype of keyof array (for index access)
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_array));
}

#[test]
fn test_keyof_tuple_type() {
    // keyof [string, number] includes "0" | "1" | array methods
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

    let keyof_tuple = interner.intern(TypeData::KeyOf(tuple));
    let lit_0 = interner.literal_string("0");
    let lit_1 = interner.literal_string("1");

    // "0" and "1" should be subtypes of keyof tuple
    assert!(checker.is_subtype_of(lit_0, keyof_tuple));
    assert!(checker.is_subtype_of(lit_1, keyof_tuple));
}

#[test]
fn test_keyof_with_index_signature_includes_string() {
    // keyof { [key: string]: number } includes string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let keyof_indexed = interner.intern(TypeData::KeyOf(indexed_obj));

    // string should be subtype of keyof { [key: string]: number }
    assert!(checker.is_subtype_of(TypeId::STRING, keyof_indexed));
}

#[test]
fn test_keyof_with_number_index_signature() {
    // keyof { [key: number]: string } includes number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let keyof_indexed = interner.intern(TypeData::KeyOf(indexed_obj));

    // number should be subtype of keyof { [key: number]: string }
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_indexed));
}

#[test]
fn test_keyof_nested_object() {
    // keyof { x: { y: number } } = "x" (not "x" | "y")
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let y_name = interner.intern_string("y");
    let inner_obj = interner.object(vec![PropertyInfo::new(y_name, TypeId::NUMBER)]);

    let x_name = interner.intern_string("x");
    let outer_obj = interner.object(vec![PropertyInfo::new(x_name, inner_obj)]);

    let keyof_outer = interner.intern(TypeData::KeyOf(outer_obj));
    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");

    // "x" is a key of outer
    assert!(checker.is_subtype_of(lit_x, keyof_outer));
    // "y" is NOT a key of outer (it's a key of the nested object)
    assert!(!checker.is_subtype_of(lit_y, keyof_outer));
}

#[test]
fn test_keyof_generic_constraint() {
    // <K extends keyof T> constraint pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_name = interner.literal_string("name");
    let lit_age = interner.literal_string("age");
    let lit_invalid = interner.literal_string("invalid");

    // Valid keys satisfy the constraint
    assert!(checker.is_subtype_of(lit_name, keyof_obj));
    assert!(checker.is_subtype_of(lit_age, keyof_obj));
    // Invalid key doesn't satisfy
    assert!(!checker.is_subtype_of(lit_invalid, keyof_obj));
}

#[test]
fn test_keyof_mapped_type_source() {
    // keyof used as constraint in mapped type: { [K in keyof T]: ... }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // keyof should produce valid keys for iteration
    assert!(keyof_obj != TypeId::ERROR);
    assert!(keyof_obj != TypeId::NEVER);

    // Should be subtype of string (for string-keyed objects)
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}

#[test]
fn test_keyof_reflexive() {
    // keyof T <: keyof T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    assert!(checker.is_subtype_of(keyof_obj, keyof_obj));
}

#[test]
fn test_keyof_null_is_never() {
    // keyof null = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_null = interner.intern(TypeData::KeyOf(TypeId::NULL));

    assert!(checker.is_subtype_of(keyof_null, TypeId::NEVER));
}

#[test]
fn test_keyof_undefined_is_never() {
    // keyof undefined = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_undefined = interner.intern(TypeData::KeyOf(TypeId::UNDEFINED));

    assert!(checker.is_subtype_of(keyof_undefined, TypeId::NEVER));
}

#[test]
fn test_keyof_void_is_never() {
    // keyof void = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_void = interner.intern(TypeData::KeyOf(TypeId::VOID));

    assert!(checker.is_subtype_of(keyof_void, TypeId::NEVER));
}

#[test]
fn test_keyof_object_intrinsic() {
    // keyof object includes all possible property keys
    let interner = TypeInterner::new();

    let keyof_object = interner.intern(TypeData::KeyOf(TypeId::OBJECT));

    // Should be valid
    assert!(keyof_object != TypeId::ERROR);
}

#[test]
fn test_keyof_symbol_keyed_object() {
    // Objects with symbol keys in keyof result
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // Simulated: { [Symbol.iterator]: () => Iterator }
    let sym_iterator = interner.intern_string("Symbol.iterator");
    let fn_iterator = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(sym_iterator, fn_iterator)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // Should include the symbol key
    assert!(keyof_obj != TypeId::NEVER);
}

// =============================================================================
// Constructor Type Tests
// =============================================================================
// Tests for new signatures, abstract constructors, and constructor types

#[test]
fn test_constructor_basic_new_signature() {
    // new () => T
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor type should be valid
    assert!(constructor != TypeId::ERROR);
    assert!(constructor != TypeId::NEVER);
}

#[test]
fn test_constructor_with_parameters() {
    // new (x: string, y: number) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let constructor = interner.function(FunctionShape {
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
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(constructor != TypeId::ERROR);
}

#[test]
fn test_constructor_vs_regular_function() {
    // Constructor and regular function are different types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let regular_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Constructor and function with same signature are not assignable
    assert!(!checker.is_subtype_of(constructor, regular_fn));
    assert!(!checker.is_subtype_of(regular_fn, constructor));
}

#[test]
fn test_constructor_callable_with_construct_signature() {
    // interface C { new (): T }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let callable_with_new = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable_with_new != TypeId::ERROR);
}

#[test]
fn test_constructor_with_call_and_construct() {
    // interface F { (): string; new (): T }
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let callable_both = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable_both != TypeId::ERROR);
}

#[test]
fn test_constructor_subtype_by_return_type() {
    // new () => Derived <: new () => Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let derived = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let ctor_base = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: base,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: derived,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor returning derived is subtype of constructor returning base
    assert!(checker.is_subtype_of(ctor_derived, ctor_base));
    // Reverse is not true
    assert!(!checker.is_subtype_of(ctor_base, ctor_derived));
}

#[test]
fn test_constructor_contravariant_parameters() {
    // new (x: Base) => T <: new (x: Derived) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("result"),
        TypeId::BOOLEAN,
    )]);

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let ctor_wide_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: string_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_narrow_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor with wider param type is subtype (contravariance)
    assert!(checker.is_subtype_of(ctor_wide_param, ctor_narrow_param));
}

#[test]
fn test_constructor_optional_parameter() {
    // new (x?: string) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![]);

    let ctor_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Optional param constructor is wider (accepts more call patterns)
    assert!(checker.is_subtype_of(ctor_optional, ctor_required));
}

#[test]
fn test_constructor_rest_parameter() {
    // new (...args: string[]) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![]);
    let string_array = interner.array(TypeId::STRING);

    let ctor_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(ctor_rest != TypeId::ERROR);
}

#[test]
fn test_constructor_overload_signatures() {
    // interface C { new (): A; new (x: string): B }
    let interner = TypeInterner::new();

    let instance_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let instance_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let overloaded_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance_a,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: instance_b,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(overloaded_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_generic_type_param() {
    // new <T>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let generic_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(generic_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_generic_with_constraint() {
    // new <T extends object>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let constrained_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(constrained_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_abstract_pattern() {
    // abstract new () => T (abstract constructor)
    // Represented as a construct signature that can't be directly called
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Abstract constructor (conceptually - just a construct signature)
    let abstract_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Concrete constructor
    let concrete_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Both should be valid
    assert!(abstract_ctor != TypeId::ERROR);
    assert!(concrete_ctor != TypeId::ERROR);
}

#[test]
fn test_constructor_with_static_properties() {
    // Constructor function with static members
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let ctor_with_static = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![PropertyInfo {
            name: interner.intern_string("create"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }],
        string_index: None,
        number_index: None,
    });

    assert!(ctor_with_static != TypeId::ERROR);
}

