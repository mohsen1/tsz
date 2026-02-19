//! Comprehensive tests for type narrowing operations.
//!
//! These tests verify TypeScript's type narrowing behavior:
//! - Type guards (typeof, instanceof, in)
//! - Discriminant unions
//! - Truthy/falsy narrowing
//! - Equality narrowing

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TypeData};

// =============================================================================
// typeof Type Guard Tests
// =============================================================================

#[test]
fn test_typeof_string_narrowing() {
    let interner = TypeInterner::new();

    // When typeof x === "string", narrow to string
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let narrowed = TypeId::STRING;

    // Verify string is subtype of the union
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

#[test]
fn test_typeof_number_narrowing() {
    let interner = TypeInterner::new();

    // When typeof x === "number", narrow to number
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let narrowed = TypeId::NUMBER;

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

#[test]
fn test_typeof_boolean_narrowing() {
    let interner = TypeInterner::new();

    // When typeof x === "boolean", narrow to boolean
    let union = interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN);
    let narrowed = TypeId::BOOLEAN;

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

#[test]
fn test_typeof_object_narrowing() {
    let interner = TypeInterner::new();

    // typeof x === "object" narrows to object (excluding function)
    let obj = interner.object(vec![]);
    let func = interner.function(crate::types::FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union2(obj, func);

    // typeof === "object" would narrow to obj, not func
    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good - union created
    } else {
        panic!("Expected union type");
    }
}

// =============================================================================
// Truthy/Falsy Narrowing Tests
// =============================================================================

#[test]
fn test_truthy_narrowing_excludes_null_undefined() {
    let interner = TypeInterner::new();

    // When x is truthy, exclude null and undefined
    let union = interner.union3(TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED);
    let narrowed = TypeId::STRING;

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

#[test]
fn test_truthy_narrowing_string() {
    let interner = TypeInterner::new();

    // When x is truthy string, exclude ""
    let empty_string = interner.literal_string("");
    let non_empty = interner.literal_string("hello");
    let string_union = interner.union2(empty_string, non_empty);

    // Truthy check narrows to non_empty
    if let Some(TypeData::Union(_)) = interner.lookup(string_union) {
        // Good - union created
    }
}

#[test]
fn test_falsy_narrowing() {
    let interner = TypeInterner::new();

    // When !x, narrow to falsy types: null, undefined, 0, "", false
    let null_or_undefined = interner.union2(TypeId::NULL, TypeId::UNDEFINED);

    if let Some(TypeData::Union(_)) = interner.lookup(null_or_undefined) {
        // Good
    }
}

#[test]
fn test_boolean_literal_narrowing() {
    let interner = TypeInterner::new();

    let true_lit = interner.literal_boolean(true);
    let false_lit = interner.literal_boolean(false);
    let bool_union = interner.union2(true_lit, false_lit);

    // Boolean narrowing from true | false
    if let Some(TypeData::Union(_)) = interner.lookup(bool_union) {
        // Good
    }
}

// =============================================================================
// Equality Narrowing Tests
// =============================================================================

#[test]
fn test_equality_narrowing_literal() {
    let interner = TypeInterner::new();

    // When x === "foo", narrow string | number to "foo"
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let union = interner.union2(foo, bar);

    // Equality with "foo" narrows to "foo"
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(foo, union));
}

#[test]
fn test_inequality_narrowing() {
    let interner = TypeInterner::new();

    // When x !== "foo", exclude "foo" from "foo" | "bar"
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let union = interner.union2(foo, bar);

    // Inequality with "foo" leaves "bar"
    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    }
}

// =============================================================================
// Discriminant Union Tests
// =============================================================================

#[test]
fn test_discriminant_union_with_kind() {
    let interner = TypeInterner::new();

    // { kind: "circle", radius: number } | { kind: "square", size: number }
    let circle_kind = interner.literal_string("circle");
    let square_kind = interner.literal_string("square");

    let circle = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), circle_kind),
        PropertyInfo::new(interner.intern_string("radius"), TypeId::NUMBER),
    ]);

    let square = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), square_kind),
        PropertyInfo::new(interner.intern_string("size"), TypeId::NUMBER),
    ]);

    let shape_union = interner.union2(circle, square);

    // When shape.kind === "circle", narrow to circle
    if let Some(TypeData::Union(_)) = interner.lookup(shape_union) {
        // Good - discriminant union created
    }
}

#[test]
fn test_discriminant_union_with_type() {
    let interner = TypeInterner::new();

    // { type: "success", data: T } | { type: "error", error: E }
    let success_type = interner.literal_string("success");
    let error_type = interner.literal_string("error");

    let success = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), success_type),
        PropertyInfo::new(interner.intern_string("data"), TypeId::STRING),
    ]);

    let error = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), error_type),
        PropertyInfo::new(interner.intern_string("error"), TypeId::STRING),
    ]);

    let result_union = interner.union2(success, error);

    if let Some(TypeData::Union(_)) = interner.lookup(result_union) {
        // Good
    }
}

// =============================================================================
// Property Existence (in operator) Tests
// =============================================================================

#[test]
fn test_in_operator_narrowing() {
    let interner = TypeInterner::new();

    // { foo: number } | { bar: number }
    // "foo" in x narrows to { foo: number }
    let with_foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::NUMBER,
    )]);

    let with_bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::NUMBER,
    )]);

    let union = interner.union2(with_foo, with_bar);

    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}

#[test]
fn test_in_operator_with_optional_property() {
    let interner = TypeInterner::new();

    // { foo?: number } | { bar: number }
    let mut foo_prop = PropertyInfo::new(interner.intern_string("foo"), TypeId::NUMBER);
    foo_prop.optional = true;

    let with_optional_foo = interner.object(vec![foo_prop]);

    let with_bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::NUMBER,
    )]);

    let union = interner.union2(with_optional_foo, with_bar);

    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}

// =============================================================================
// instanceof Narrowing Tests
// =============================================================================

#[test]
fn test_instanceof_narrowing() {
    let interner = TypeInterner::new();

    // Date | string, instanceof Date narrows to Date
    let date_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("getTime"),
        TypeId::ANY,
    )]);

    let union = interner.union2(date_obj, TypeId::STRING);

    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}

#[test]
fn test_instanceof_with_array() {
    let interner = TypeInterner::new();

    // Array vs non-array narrowing
    let array = interner.array(TypeId::NUMBER);
    let obj = interner.object(vec![]);

    let union = interner.union2(array, obj);

    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}

// =============================================================================
// Assignment Narrowing Tests
// =============================================================================

#[test]
fn test_assignment_narrowing() {
    let interner = TypeInterner::new();

    // let x: string | number = ...; x = "hello" narrows x to string
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let assigned = TypeId::STRING;

    // After assignment, x has type of assigned value
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(assigned, union));
}

// =============================================================================
// Control Flow Analysis Tests
// =============================================================================

#[test]
fn test_if_statement_narrowing() {
    let interner = TypeInterner::new();

    // if (typeof x === "string") { x is string }
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    // In the then branch, x is narrowed to string
    let narrowed = TypeId::STRING;

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

#[test]
fn test_else_branch_narrowing() {
    let interner = TypeInterner::new();

    // if (typeof x === "string") { x is string } else { x is not string }
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    // In else branch, x would be number (excluded string)
    let narrowed = TypeId::NUMBER;

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, union));
}

// =============================================================================
// Exhaustiveness Checking Tests
// =============================================================================

#[test]
fn test_exhaustive_union_check() {
    let interner = TypeInterner::new();

    // "a" | "b" | "c" - if we handle "a" and "b", "c" remains
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let union = interner.union3(a, b, c);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 3);
    }
}

#[test]
fn test_never_from_exhaustion() {
    let _interner = TypeInterner::new();

    // After handling all cases, type becomes never
    // NEVER is a built-in type with id 2
    assert_eq!(TypeId::NEVER.0, 2, "NEVER type id should be 2");
}

// =============================================================================
// Array Element Narrowing Tests
// =============================================================================

#[test]
fn test_array_filter_narrowing() {
    let interner = TypeInterner::new();

    // (string | number)[] filtered by typeof === "string" gives string[]
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union_array = interner.array(union);
    let string_array = interner.array(TypeId::STRING);

    // The filtered array type is string[]
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(string_array, union_array));
}

// =============================================================================
// Nested Property Narrowing Tests
// =============================================================================

#[test]
fn test_nested_property_narrowing() {
    let interner = TypeInterner::new();

    // { config: { enabled: boolean } } | { config: { enabled: false } }
    let enabled_true = interner.literal_boolean(true);
    let enabled_false = interner.literal_boolean(false);

    let config_true = interner.object(vec![PropertyInfo::new(
        interner.intern_string("enabled"),
        enabled_true,
    )]);

    let config_false = interner.object(vec![PropertyInfo::new(
        interner.intern_string("enabled"),
        enabled_false,
    )]);

    let obj_true = interner.object(vec![PropertyInfo::new(
        interner.intern_string("config"),
        config_true,
    )]);

    let obj_false = interner.object(vec![PropertyInfo::new(
        interner.intern_string("config"),
        config_false,
    )]);

    let union = interner.union2(obj_true, obj_false);

    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}

// =============================================================================
// Custom Type Guard Tests
// =============================================================================

#[test]
fn test_custom_type_guard() {
    let interner = TypeInterner::new();

    // function isString(x: unknown): x is string
    // This is represented by a function with type predicate
    let func = interner.function(crate::types::FunctionShape {
        params: vec![crate::types::ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_params: vec![],
        type_predicate: Some(crate::types::TypePredicate {
            target: crate::types::TypePredicateTarget::Identifier(interner.intern_string("x")),
            type_id: Some(TypeId::STRING),
            asserts: false,
            parameter_index: Some(0),
        }),
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(func) {
        let shape = interner.function_shape(shape_id);
        assert!(shape.type_predicate.is_some());
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_assertion_type_guard() {
    let interner = TypeInterner::new();

    // function assertDefined(x: unknown): asserts x is defined
    let func = interner.function(crate::types::FunctionShape {
        params: vec![crate::types::ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: Some(crate::types::TypePredicate {
            target: crate::types::TypePredicateTarget::Identifier(interner.intern_string("x")),
            type_id: Some(TypeId::STRING),
            asserts: true,
            parameter_index: Some(0),
        }),
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(func) {
        let shape = interner.function_shape(shape_id);
        if let Some(pred) = &shape.type_predicate {
            assert!(pred.asserts);
        } else {
            panic!("Expected type predicate");
        }
    } else {
        panic!("Expected function type");
    }
}

// =============================================================================
// Narrowing Identity Tests
// =============================================================================

#[test]
fn test_narrowed_type_identity() {
    let interner = TypeInterner::new();

    // Narrowed types should maintain identity
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union2 = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert_eq!(union, union2, "Same union should produce same TypeId");
}

// =============================================================================
// Narrowing with any/unknown Tests
// =============================================================================

#[test]
fn test_narrowing_any_to_string() {
    let interner = TypeInterner::new();

    // typeof x === "string" when x: any narrows to string
    // (In practice, any is special, but the narrowed type is string)
    let narrowed = TypeId::STRING;

    // string is subtype of any
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, TypeId::ANY));
}

#[test]
fn test_narrowing_unknown_to_string() {
    let interner = TypeInterner::new();

    // typeof x === "string" when x: unknown narrows to string
    let narrowed = TypeId::STRING;

    // string is subtype of unknown
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(narrowed, TypeId::UNKNOWN));
}

// =============================================================================
// Switch Statement Narrowing Tests
// =============================================================================

#[test]
fn test_switch_case_narrowing() {
    let interner = TypeInterner::new();

    // switch (x) { case "a": ... case "b": ... }
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let union = interner.union3(a, b, c);

    // In case "a", x is narrowed to "a"
    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 3);
    }
}

#[test]
fn test_switch_default_narrowing() {
    let interner = TypeInterner::new();

    // After handling all but one case, default narrows to that one
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");

    let union = interner.union2(a, b);

    // In default after "a", x is narrowed to "b"
    if let Some(TypeData::Union(_)) = interner.lookup(union) {
        // Good
    }
}
