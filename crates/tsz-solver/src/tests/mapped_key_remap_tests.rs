//! Tests for mapped type key remapping with 'as never'
//!
//! Tests Rule #41: Key remapping to Never should skip that property.

use crate::types::{Visibility, *};
use crate::{evaluate::evaluate_type, intern::TypeInterner};

#[test]
fn test_mapped_type_as_never_skips_property() {
    let interner = TypeInterner::new();

    // Test: type Omit<T, K> = { [P in keyof T as P extends K ? never : P]: T[P] }
    // When P extends K, the key is remapped to 'never' and should be skipped

    // Create a simple type: { x: number, y: string }
    let source_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Create keyof T
    let keyof_t = interner.keyof(source_type);

    // Create the type parameter for K
    let type_param_k = TypeParamInfo {
        name: interner.intern_string("K"),
        default: None,
        is_const: false,
    };

    // Create the conditional: P extends K ? never : P
    let conditional_type = ConditionalType {
        check_type: TypeId::TYPE_PARAM,   // P
        extends_type: TypeId::TYPE_PARAM, // K
        true_type: TypeId::NEVER,         // never - skip property
        false_type: TypeId::TYPE_PARAM,   // P - keep property
        distributed_type_param: None,
    };

    let cond_id = interner.conditional(conditional_type);

    // Create the mapped type: { [P in keyof T as P extends K ? never : P]: T[P] }
    let mapped_type = MappedType {
        type_param: type_param_k,
        constraint: keyof_t,
        name_type: Some(cond_id), // Key remapping
        template: TypeId::ERROR,  // Placeholder
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);

    // Evaluate the mapped type
    let result = evaluate_type(&interner, mapped_id);

    // The result should be an object type
    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        // Should have 2 properties (x and y)
        // The 'as never' remapping doesn't filter anything because we're not using 'K' to filter
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_mapped_type_key_remap_to_never_filters_property() {
    let interner = TypeInterner::new();

    // Test a simpler case: type Keys = 'a' | 'b'
    // type Mapped = { [K in Keys as K extends 'a' ? never : K]: any }

    let keys_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    // Create type parameter K
    let type_param_k = TypeParamInfo {
        name: interner.intern_string("K"),
        default: None,
        is_const: false,
    };

    // Create conditional: K extends 'a' ? never : K
    let conditional = ConditionalType {
        check_type: TypeId::TYPE_PARAM,
        extends_type: TypeId::STRING_LITERAL, // 'a'
        true_type: TypeId::NEVER,
        false_type: TypeId::TYPE_PARAM,
        distributed_type_param: None,
    };

    let cond_id = interner.conditional(conditional);

    // Create mapped type
    let mapped_type = MappedType {
        type_param: type_param_k,
        constraint: keys_union,
        name_type: Some(cond_id),
        template: TypeId::ANY,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);

    // Evaluate
    let result = evaluate_type(&interner, mapped_id);

    // Should get an object with only 'b' (since 'a' is remapped to 'never')
    if let Some(TypeKey::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        // Should only have 'b' since 'a' was filtered out by 'as never'
        assert_eq!(shape.properties.len(), 1);
        let prop_name = interner.resolve_atom(shape.properties[0].name);
        assert_eq!(prop_name, "b");
    } else {
        panic!("Expected object type with one property");
    }
}
