use super::*;
use crate::TypeInterner;
use crate::types::{
    LiteralValue, OrderedFloat, PropertyInfo, SymbolRef, TypeData, TypeParamInfo, Visibility,
};

#[test]
fn test_widen_string_literal() {
    let interner = TypeInterner::new();
    let string_lit = interner.intern(TypeData::Literal(LiteralValue::String(
        interner.intern_string("hello"),
    )));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, string_lit);
    assert_eq!(widened, TypeId::STRING);
}

#[test]
fn test_widen_number_literal() {
    let interner = TypeInterner::new();
    let number_lit = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(42.0))));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, number_lit);
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_boolean_literal() {
    let interner = TypeInterner::new();
    let bool_lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, bool_lit);
    assert_eq!(widened, TypeId::BOOLEAN);
}

#[test]
fn test_widen_union() {
    let interner = TypeInterner::new();
    let lit1 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0))));
    let lit2 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0))));
    let union = interner.union(vec![lit1, lit2]);

    let widened = widen_type(&interner as &dyn crate::TypeDatabase, union);
    // After widening, we get number | number which dedups to number
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_primitive_preserved() {
    let interner = TypeInterner::new();
    // Primitives should be preserved (already widened)
    let widened = widen_type(&interner, TypeId::STRING);
    assert_eq!(widened, TypeId::STRING);
}

#[test]
fn test_type_param_not_widened() {
    let interner = TypeInterner::new();
    // Type parameters are NOT widened
    let name = interner.intern_string("T");
    let info = TypeParamInfo {
        name,
        constraint: Some(
            interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
        ),
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(info));

    let widened = widen_type(&interner, type_param);
    // Should preserve the original type_param type
    assert_eq!(widened, type_param);
}

#[test]
fn test_widen_unique_symbol() {
    let interner = TypeInterner::new();
    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));
    let widened = widen_type(&interner, unique_sym);
    assert_eq!(widened, TypeId::SYMBOL);
}

#[test]
fn test_widen_object_properties() {
    let interner = TypeInterner::new();
    // Create object { x: 1 } where x is a literal number
    let props = vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
        write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];
    let obj_type = interner.object(props);

    let widened = widen_type(&interner, obj_type);

    // Check that the widened type has number, not the literal 1
    let widened_key = interner.lookup(widened);
    match widened_key {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
            assert_eq!(shape.properties[0].write_type, TypeId::NUMBER);
        }
        _ => panic!("Expected widened object type"),
    }
}

#[test]
fn test_widen_nested_object_properties() {
    let interner = TypeInterner::new();
    // Create nested object { a: { b: "hello" } }
    let inner_props = vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: interner.intern(TypeData::Literal(LiteralValue::String(
            interner.intern_string("hello"),
        ))),
        write_type: interner.intern(TypeData::Literal(LiteralValue::String(
            interner.intern_string("hello"),
        ))),
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];
    let inner_obj = interner.object(inner_props);

    let outer_props = vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: inner_obj,
        write_type: inner_obj,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];
    let outer_obj = interner.object(outer_props);

    let widened = widen_type(&interner, outer_obj);

    // Check that both inner and outer properties are widened
    let widened_key = interner.lookup(widened);
    match widened_key {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);

            // Outer property 'a' should be an object
            let inner_type = shape.properties[0].type_id;
            let inner_key = interner.lookup(inner_type);
            match inner_key {
                Some(
                    TypeData::Object(inner_shape_id) | TypeData::ObjectWithIndex(inner_shape_id),
                ) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert_eq!(inner_shape.properties.len(), 1);
                    // Inner property 'b' should be widened to string
                    assert_eq!(inner_shape.properties[0].type_id, TypeId::STRING);
                }
                _ => panic!("Expected inner object type"),
            }
        }
        _ => panic!("Expected widened object type"),
    }
}

#[test]
fn test_widen_readonly_property_preserved() {
    let interner = TypeInterner::new();
    // { a: 1, readonly b: 2 }
    let props = vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            optional: false,
            readonly: false, // Mutable -> Widens
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
            write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
            optional: false,
            readonly: true, // Readonly -> Preserved
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ];
    let obj_type = interner.object(props);
    let widened = widen_type(&interner, obj_type);

    // Verify 'a' is number, 'b' is literal 2
    let shape = match interner.lookup(widened).unwrap() {
        TypeData::Object(id) => interner.object_shape(id),
        _ => panic!("Expected object"),
    };

    let a = shape
        .properties
        .iter()
        .find(|p| interner.resolve_atom(p.name) == "a")
        .unwrap();
    let b = shape
        .properties
        .iter()
        .find(|p| interner.resolve_atom(p.name) == "b")
        .unwrap();

    assert_eq!(a.type_id, TypeId::NUMBER);
    assert!(matches!(
        interner.lookup(b.type_id),
        Some(TypeData::Literal(_))
    ));
}
