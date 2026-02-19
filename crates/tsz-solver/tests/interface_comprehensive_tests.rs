//! Comprehensive tests for interface type operations.
//!
//! These tests verify TypeScript's interface type behavior:
//! - Interface assignability
//! - Interface inheritance
//! - Interface with optional properties
//! - Interface with readonly properties
//! - Interface with index signatures
//! - Interface vs object type literal

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeData};

// =============================================================================
// Basic Interface Construction Tests
// =============================================================================

#[test]
fn test_interface_construction() {
    let interner = TypeInterner::new();

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_empty_interface() {
    let interner = TypeInterner::new();

    let empty_interface = interner.object(vec![]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(empty_interface) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 0);
    } else {
        panic!("Expected empty object type");
    }
}

// =============================================================================
// Interface Subtype Tests
// =============================================================================

#[test]
fn test_interface_same_type_is_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    assert!(
        checker.is_subtype_of(interface, interface),
        "Interface should be subtype of itself"
    );
}

#[test]
fn test_interface_subproperty_is_subtype() {
    // { name: string, age: number } <: { name: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let extended = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    assert!(
        checker.is_subtype_of(extended, base),
        "Extended interface should be subtype of base interface"
    );
}

#[test]
fn test_interface_not_subtype_missing_property() {
    // { name: string } is NOT <: { name: string, age: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let extended = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    assert!(
        !checker.is_subtype_of(base, extended),
        "Base interface should not be subtype of extended interface"
    );
}

#[test]
fn test_interface_not_subtype_wrong_type() {
    // { name: string } is NOT <: { name: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_name = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let number_name = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::NUMBER,
    )]);

    assert!(
        !checker.is_subtype_of(string_name, number_name),
        "String property should not be subtype of number property"
    );
}

// =============================================================================
// Interface with Optional Properties
// =============================================================================

#[test]
fn test_interface_with_optional_property() {
    let interner = TypeInterner::new();

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        let required = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "required");
        assert!(required.unwrap().optional == false);

        let optional = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "optional");
        assert!(optional.unwrap().optional);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_optional_property_assignability() {
    // { required: string } <: { required: string, optional?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let without_optional = interner.object(vec![PropertyInfo::new(
        interner.intern_string("required"),
        TypeId::STRING,
    )]);

    let with_optional = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);

    assert!(
        checker.is_subtype_of(without_optional, with_optional),
        "Type without optional property should be subtype of type with optional property"
    );
}

// =============================================================================
// Interface with Readonly Properties
// =============================================================================

#[test]
fn test_interface_with_readonly_property() {
    let interner = TypeInterner::new();

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("mutable"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("readonly"), TypeId::NUMBER),
    ]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        let mutable = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "mutable");
        assert!(mutable.unwrap().readonly == false);

        let readonly = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "readonly");
        assert!(readonly.unwrap().readonly);
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Interface with Index Signatures
// =============================================================================

#[test]
fn test_interface_with_string_index() {
    let interner = TypeInterner::new();

    let interface = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("known"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    if let Some(TypeData::ObjectWithIndex(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert!(shape.string_index.is_some());
    } else {
        panic!("Expected object with index signature");
    }
}

#[test]
fn test_interface_with_number_index() {
    let interner = TypeInterner::new();

    let interface = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    if let Some(TypeData::ObjectWithIndex(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert!(shape.number_index.is_some());
    } else {
        panic!("Expected object with number index signature");
    }
}

// =============================================================================
// Interface Assignability with any
// =============================================================================

#[test]
fn test_interface_assignable_to_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    assert!(
        checker.is_subtype_of(interface, TypeId::ANY),
        "Interface should be subtype of any"
    );
}

#[test]
fn test_any_assignable_to_interface() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    assert!(
        checker.is_subtype_of(TypeId::ANY, interface),
        "any should be subtype of interface"
    );
}

// =============================================================================
// Interface with never
// =============================================================================

#[test]
fn test_never_assignable_to_interface() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    assert!(
        checker.is_subtype_of(TypeId::NEVER, interface),
        "never should be subtype of interface"
    );
}

#[test]
fn test_interface_not_assignable_to_never() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    assert!(
        !checker.is_subtype_of(interface, TypeId::NEVER),
        "Interface should not be subtype of never"
    );
}

// =============================================================================
// Interface with unknown
// =============================================================================

#[test]
fn test_interface_assignable_to_unknown() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    assert!(
        checker.is_subtype_of(interface, TypeId::UNKNOWN),
        "Interface should be subtype of unknown"
    );
}

#[test]
fn test_unknown_not_assignable_to_interface() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    assert!(
        !checker.is_subtype_of(TypeId::UNKNOWN, interface),
        "unknown should not be subtype of interface"
    );
}

// =============================================================================
// Interface Property Order Independence
// =============================================================================

#[test]
fn test_interface_property_order_independence() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let interface2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    // Both should be subtypes of each other
    assert!(
        checker.is_subtype_of(interface1, interface2),
        "Property order should not affect subtyping"
    );
    assert!(
        checker.is_subtype_of(interface2, interface1),
        "Property order should not affect subtyping"
    );
}

// =============================================================================
// Interface with Function Properties
// =============================================================================

#[test]
fn test_interface_with_function_property() {
    let interner = TypeInterner::new();

    let func = interner.function(crate::types::FunctionShape {
        params: vec![crate::types::ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("method"), func),
    ]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(interface) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Interface with Nested Objects
// =============================================================================

#[test]
fn test_interface_with_nested_object() {
    let interner = TypeInterner::new();

    let nested = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let outer = interner.object(vec![
        PropertyInfo::new(interner.intern_string("position"), nested),
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
    ]);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(outer) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Interface with Union Property Types
// =============================================================================

#[test]
fn test_interface_with_union_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        string_or_number,
    )]);

    // { value: string } <: { value: string | number }
    let string_only = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    assert!(
        checker.is_subtype_of(string_only, interface),
        "String property should be subtype of string | number"
    );
}

// =============================================================================
// Interface Identity Tests
// =============================================================================

#[test]
fn test_interface_identity_stability() {
    let interner = TypeInterner::new();

    let props = vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ];

    let interface1 = interner.object(props.clone());
    let interface2 = interner.object(props);

    assert_eq!(
        interface1, interface2,
        "Same interface construction should produce same TypeId"
    );
}
