//! Comprehensive tests for intersection and union type normalization and edge cases.

use super::*;
use crate::intern::TypeInterner;

// =============================================================================
// Intersection Type Tests - Primitive to Never
// =============================================================================

#[test]
fn test_intersection_string_number_is_never() {
    let interner = TypeInterner::new();

    // string & number = never
    let result = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(result, TypeId::NEVER, "string & number should be never");
}

#[test]
fn test_intersection_string_boolean_is_never() {
    let interner = TypeInterner::new();

    // string & boolean = never
    let result = interner.intersection2(TypeId::STRING, TypeId::BOOLEAN);
    assert_eq!(result, TypeId::NEVER, "string & boolean should be never");
}

#[test]
fn test_intersection_number_boolean_is_never() {
    let interner = TypeInterner::new();

    // number & boolean = never
    let result = interner.intersection2(TypeId::NUMBER, TypeId::BOOLEAN);
    assert_eq!(result, TypeId::NEVER, "number & boolean should be never");
}

#[test]
fn test_intersection_string_bigint_is_never() {
    let interner = TypeInterner::new();

    // string & bigint = never
    let result = interner.intersection2(TypeId::STRING, TypeId::BIGINT);
    assert_eq!(result, TypeId::NEVER, "string & bigint should be never");
}

#[test]
fn test_intersection_symbol_string_is_never() {
    let interner = TypeInterner::new();

    // symbol & string = never
    let result = interner.intersection2(TypeId::SYMBOL, TypeId::STRING);
    assert_eq!(result, TypeId::NEVER, "symbol & string should be never");
}

#[test]
fn test_intersection_null_undefined_is_never() {
    let interner = TypeInterner::new();

    // null & undefined = never
    let result = interner.intersection2(TypeId::NULL, TypeId::UNDEFINED);
    assert_eq!(result, TypeId::NEVER, "null & undefined should be never");
}

#[test]
fn test_intersection_literal_of_different_types_is_never() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let one = interner.literal_number(1.0);

    // "hello" & 1 = never
    let result = interner.intersection2(hello, one);
    assert_eq!(result, TypeId::NEVER, "\"hello\" & 1 should be never");
}

#[test]
fn test_intersection_same_primitive_is_itself() {
    let interner = TypeInterner::new();

    // string & string = string
    let result = interner.intersection2(TypeId::STRING, TypeId::STRING);
    assert_eq!(result, TypeId::STRING, "string & string should be string");
}

#[test]
fn test_intersection_different_string_literals_is_never() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" & "world" = never
    let result = interner.intersection2(hello, world);
    assert_eq!(
        result,
        TypeId::NEVER,
        "\"hello\" & \"world\" should be never"
    );
}

#[test]
fn test_intersection_different_number_literals_is_never() {
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    // 1 & 2 = never
    let result = interner.intersection2(one, two);
    assert_eq!(result, TypeId::NEVER, "1 & 2 should be never");
}

#[test]
fn test_intersection_literal_with_primitive_is_literal() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // "hello" & string = "hello"
    let result = interner.intersection2(hello, TypeId::STRING);
    // The intersection should narrow to the literal type since "hello" is a subtype of string
    // But in TypeScript's type system, intersection of a literal with its primitive type
    // results in the literal type
    assert_eq!(result, hello, "\"hello\" & string should be \"hello\"");
}

// =============================================================================
// Intersection Type Tests - Object Property Merging
// =============================================================================

#[test]
fn test_intersection_object_merge_properties() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // A & B should merge properties
    let result = interner.intersection2(obj_a, obj_b);

    // Result should have both properties
    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2, "Should have both properties");

        // Check property "a"
        let prop_a = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("a"));
        assert!(prop_a.is_some(), "Should have property 'a'");
        assert_eq!(prop_a.unwrap().type_id, TypeId::STRING);

        // Check property "b"
        let prop_b = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("b"));
        assert!(prop_b.is_some(), "Should have property 'b'");
        assert_eq!(prop_b.unwrap().type_id, TypeId::NUMBER);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_intersection_object_same_property_intersect_types() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // A & B should have property x: string & number = never
    let result = interner.intersection2(obj_a, obj_b);

    // The intersection creates an object with a property of type never
    // This is different from the whole intersection being never
    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        let prop_x = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("x"));
        assert!(prop_x.is_some());
        assert_eq!(
            prop_x.unwrap().type_id,
            TypeId::NEVER,
            "Property type should be never"
        );
    } else {
        panic!("Expected object type with never property");
    }
}

#[test]
fn test_intersection_required_wins_over_optional() {
    let interner = TypeInterner::new();

    let obj_optional = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_required = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // optional & required = required (required wins)
    let result = interner.intersection2(obj_optional, obj_required);

    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        let prop_x = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("x"));
        assert!(prop_x.is_some());
        assert!(!prop_x.unwrap().optional, "Property should be required");
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_intersection_readonly_is_cumulative() {
    let interner = TypeInterner::new();

    let obj_readonly = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_mutable = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // readonly & mutable = readonly (readonly is cumulative)
    let result = interner.intersection2(obj_readonly, obj_mutable);

    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        let prop_x = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("x"));
        assert!(prop_x.is_some());
        assert!(prop_x.unwrap().readonly, "Property should be readonly");
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_intersection_both_optional_stays_optional() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // optional & optional = optional
    let result = interner.intersection2(obj_a, obj_b);

    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        let prop_x = shape
            .properties
            .iter()
            .find(|p| p.name == interner.intern_string("x"));
        assert!(prop_x.is_some());
        assert!(prop_x.unwrap().optional, "Property should be optional");
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Intersection Type Tests - Function Overloading
// =============================================================================

#[test]
fn test_intersection_function_overloads() {
    let interner = TypeInterner::new();

    // Create first function signature: (x: string) => number
    let func1 = interner.function(FunctionShape {
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

    // Create second function signature: (x: number) => string
    let func2 = interner.function(FunctionShape {
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

    // Intersection should create a callable with both signatures
    let result = interner.intersection2(func1, func2);

    if let Some(TypeKey::Callable(shape_id)) = interner.lookup(result) {
        let shape = interner.callable_shape(shape_id);
        assert_eq!(
            shape.call_signatures.len(),
            2,
            "Should have both call signatures"
        );
    } else {
        panic!("Expected callable type with overloaded signatures");
    }
}

// =============================================================================
// Union Type Tests - Literal Absorption
// =============================================================================

#[test]
fn test_union_literal_absorbed_into_primitive() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" | "world" | string should normalize to just string
    let result = interner.union3(hello, world, TypeId::STRING);

    assert_eq!(
        result,
        TypeId::STRING,
        "Literals should be absorbed into primitive"
    );
}

#[test]
fn test_union_number_literals_absorbed_into_number() {
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    // 1 | 2 | 3 | number should normalize to just number
    let result = interner.union(vec![one, two, three, TypeId::NUMBER]);

    assert_eq!(
        result,
        TypeId::NUMBER,
        "Number literals should be absorbed into number"
    );
}

#[test]
fn test_union_boolean_literals_absorbed_into_boolean() {
    let interner = TypeInterner::new();

    // true | false | boolean should normalize to just boolean
    let result = interner.union3(TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN);

    assert_eq!(
        result,
        TypeId::BOOLEAN,
        "Boolean literals should be absorbed into boolean"
    );
}

#[test]
fn test_union_bigint_literals_absorbed_into_bigint() {
    let interner = TypeInterner::new();

    let bigint1 = interner.literal_bigint("1");
    let bigint2 = interner.literal_bigint("2");

    // 1n | 2n | bigint should normalize to just bigint
    let result = interner.union3(bigint1, bigint2, TypeId::BIGINT);

    assert_eq!(
        result,
        TypeId::BIGINT,
        "Bigint literals should be absorbed into bigint"
    );
}

#[test]
fn test_union_literals_without_primitive_stay_as_union() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" | "world" should stay as a union (no primitive present)
    let result = interner.union2(hello, world);

    if let Some(TypeKey::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Should have both literals");
    } else {
        panic!("Expected union type");
    }
}

// =============================================================================
// Union Type Tests - Any/Unknown Handling
// =============================================================================

#[test]
fn test_union_any_dominates() {
    let interner = TypeInterner::new();

    // any | string = any
    let result = interner.union2(TypeId::ANY, TypeId::STRING);
    assert_eq!(result, TypeId::ANY, "any should dominate union");

    // string | any = any
    let result = interner.union2(TypeId::STRING, TypeId::ANY);
    assert_eq!(result, TypeId::ANY, "any should dominate union");
}

#[test]
fn test_union_unknown_dominates() {
    let interner = TypeInterner::new();

    // unknown | string = unknown
    let result = interner.union2(TypeId::UNKNOWN, TypeId::STRING);
    assert_eq!(result, TypeId::UNKNOWN, "unknown should dominate union");
}

#[test]
fn test_union_any_dominates_unknown() {
    let interner = TypeInterner::new();

    // any | unknown = any
    let result = interner.union2(TypeId::ANY, TypeId::UNKNOWN);
    assert_eq!(result, TypeId::ANY, "any should dominate unknown");
}

// =============================================================================
// Union Type Tests - Simplification (Removing Never, Deduplicating, Sorting)
// =============================================================================

#[test]
fn test_union_remove_never() {
    let interner = TypeInterner::new();

    // string | never should normalize to string
    let result = interner.union2(TypeId::STRING, TypeId::NEVER);
    assert_eq!(result, TypeId::STRING, "never should be removed from union");
}

#[test]
fn test_union_multiple_never_removed() {
    let interner = TypeInterner::new();

    // string | never | number | never should normalize to string | number
    let result = interner.union(vec![
        TypeId::STRING,
        TypeId::NEVER,
        TypeId::NUMBER,
        TypeId::NEVER,
    ]);

    if let Some(TypeKey::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(
            members.len(),
            2,
            "Should have 2 members after removing never"
        );
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_union_only_never_is_never() {
    let interner = TypeInterner::new();

    // never | never = never
    let result = interner.union2(TypeId::NEVER, TypeId::NEVER);
    assert_eq!(result, TypeId::NEVER, "Union of only never should be never");
}

#[test]
fn test_union_deduplicates() {
    let interner = TypeInterner::new();

    // string | string | number should normalize to string | number
    let result = interner.union(vec![TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);

    if let Some(TypeKey::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Should deduplicate string");
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_union_sorts_consistently() {
    let interner = TypeInterner::new();

    // Create union in one order
    let result1 = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    // Create union in different order
    let result2 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Should be the same (sorted for consistent hashing)
    assert_eq!(result1, result2, "Unions should be sorted consistently");
}

// =============================================================================
// Intersection Type Tests - Simplification
// =============================================================================

#[test]
fn test_intersection_remove_unknown() {
    let interner = TypeInterner::new();

    // string & unknown should normalize to string
    let result = interner.intersection2(TypeId::STRING, TypeId::UNKNOWN);
    assert_eq!(
        result,
        TypeId::STRING,
        "unknown should be removed from intersection"
    );
}

#[test]
fn test_intersection_any_is_identity() {
    let interner = TypeInterner::new();

    // string & any = string (any is identity for intersection in practice)
    let result = interner.intersection2(TypeId::STRING, TypeId::ANY);
    assert_eq!(
        result,
        TypeId::ANY,
        "any in intersection should result in any"
    );
}

#[test]
fn test_intersection_flattens_nested() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let inner = interner.intersection2(obj_a, obj_b);
    let outer = interner.intersection2(inner, obj_a);

    // Should flatten and deduplicate
    assert_eq!(inner, outer, "Nested intersections should be flattened");
}

// =============================================================================
// Distributive Conditional Types Over Unions
// =============================================================================

#[test]
fn test_distributive_conditional_over_union() {
    let interner = TypeInterner::new();

    // (string | number) extends string ? true : false
    // Should distribute to: (string extends string ? true : false) | (number extends string ? true : false)
    // = true | false = boolean

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let conditional = ConditionalType {
        check_type: union,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: true,
    };

    let result = interner.conditional(conditional);

    // The result should be a conditional type with is_distributive flag set
    // Note: The actual distribution happens during type evaluation in the evaluator
    if let Some(TypeKey::Conditional(cond_id)) = interner.lookup(result) {
        let cond = interner.conditional_type(cond_id);
        assert!(cond.is_distributive, "Should be marked as distributive");
        assert_eq!(cond.check_type, union);
        assert_eq!(cond.extends_type, TypeId::STRING);
    } else {
        panic!("Expected conditional type");
    }
}

// =============================================================================
// Edge Cases - Empty and Single Member
// =============================================================================

#[test]
fn test_intersection_empty_is_unknown() {
    let interner = TypeInterner::new();

    // Empty intersection should be unknown (identity element)
    let result = interner.intersection(vec![]);
    assert_eq!(
        result,
        TypeId::UNKNOWN,
        "Empty intersection should be unknown"
    );
}

#[test]
fn test_intersection_single_member_is_itself() {
    let interner = TypeInterner::new();

    // Single-member intersection should be that member
    let result = interner.intersection(vec![TypeId::STRING]);
    assert_eq!(
        result,
        TypeId::STRING,
        "Single-member intersection should be that member"
    );
}

#[test]
fn test_union_empty_is_never() {
    let interner = TypeInterner::new();

    // Empty union should be never (identity element)
    let result = interner.union(vec![]);
    assert_eq!(result, TypeId::NEVER, "Empty union should be never");
}

#[test]
fn test_union_single_member_is_itself() {
    let interner = TypeInterner::new();

    // Single-member union should be that member
    let result = interner.union(vec![TypeId::STRING]);
    assert_eq!(
        result,
        TypeId::STRING,
        "Single-member union should be that member"
    );
}
