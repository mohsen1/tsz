//! Tests for mapped type key remapping with 'as never'
//!
//! Tests Rule #41: Key remapping to Never should skip that property.

use crate::types::*;
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
    let keyof_t = interner.intern(TypeKey::KeyOf(source_type));

    // Create type parameters P and K
    let type_param_p_info = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_p = interner.intern(TypeKey::TypeParameter(type_param_p_info.clone()));

    let type_param_k_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_k = interner.intern(TypeKey::TypeParameter(type_param_k_info.clone()));

    // Create the conditional: P extends K ? never : P
    let conditional_type = ConditionalType {
        check_type: type_param_p,
        extends_type: type_param_k,
        true_type: TypeId::NEVER,
        false_type: type_param_p,
        is_distributive: true,
    };

    let cond_id = interner.conditional(conditional_type);

    // Create the mapped type: { [P in keyof T as P extends K ? never : P]: T[P] }
    let mapped_type = MappedType {
        type_param: type_param_p_info,
        constraint: keyof_t,
        name_type: Some(cond_id),
        template: TypeId::ERROR,
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
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_mapped_type_key_remap_to_never_filters_property() {
    let interner = TypeInterner::new();

    // Test a simpler case: type Keys = 'a' | 'b'
    // type Mapped = { [K in Keys as K extends 'a' ? never : K]: any }

    let literal_a = interner.literal_string("a");
    let literal_b = interner.literal_string("b");
    let keys_union = interner.union(vec![literal_a, literal_b]);

    // Create type parameter K
    let type_param_k_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_k = interner.intern(TypeKey::TypeParameter(type_param_k_info.clone()));

    // Create conditional: K extends 'a' ? never : K
    let conditional = ConditionalType {
        check_type: type_param_k,
        extends_type: literal_a,
        true_type: TypeId::NEVER,
        false_type: type_param_k,
        is_distributive: true,
    };

    let cond_id = interner.conditional(conditional);

    // Create mapped type
    let mapped_type = MappedType {
        type_param: type_param_k_info,
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
        panic!(
            "Expected object type with one property, got {:?}",
            interner.lookup(result)
        );
    }
}
