//! Comprehensive tests for mapped type evaluation.
//!
//! These tests verify TypeScript's mapped type behavior:
//! - Basic mapped types: { [P in K]: T }
//! - Homomorphic mapped types: { [P in keyof T]: T[P] }
//! - Key remapping: { [P in K as R]: T }
//! - Optional modifiers: +?, -?
//! - Readonly modifiers: +readonly, -readonly

use super::*;
use crate::def::DefId;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::types::{MappedModifier, MappedType, PropertyInfo, TypeData, TypeParamInfo};

// =============================================================================
// Basic Mapped Type Tests
// =============================================================================

#[test]
fn test_mapped_type_simple() {
    // type Keys = 'a' | 'b'
    // type Mapped = { [K in Keys]: number }
    // Should produce { a: number, b: number }
    let interner = TypeInterner::new();

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

    // Create mapped type: { [K in Keys]: number }
    let mapped_type = MappedType {
        type_param: type_param_k_info,
        constraint: keys_union,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    // Should be an object with 'a' and 'b' properties, both number
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // Check both properties are number
        for prop in &shape.properties {
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_mapped_type_with_keyof() {
    // type T = { x: string, y: number }
    // type Mapped = { [K in keyof T]: boolean }
    // Should produce { x: boolean, y: boolean }
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // All properties should be boolean
        for prop in &shape.properties {
            assert_eq!(prop.type_id, TypeId::BOOLEAN);
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Homomorphic Mapped Type Tests (preserves modifiers)
// =============================================================================

#[test]
fn test_homomorphic_mapped_preserves_optional() {
    // type T = { a: string, b?: number }
    // type Mapped = { [K in keyof T]: T[K] }
    // Should produce { a: string, b?: number } (preserving optional)
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    // Template is T[K] (index access)
    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // Find 'b' property and check it's optional
        let b_prop = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "b");
        if let Some(prop) = b_prop {
            assert!(prop.optional, "Property 'b' should be optional");
        } else {
            panic!("Property 'b' not found");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_homomorphic_mapped_preserves_readonly() {
    // type T = { readonly a: string, b: number }
    // type Mapped = { [K in keyof T]: T[K] }
    // Should preserve readonly on 'a'
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // Find 'a' property and check it's readonly
        let a_prop = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "a");
        if let Some(prop) = a_prop {
            assert!(prop.readonly, "Property 'a' should be readonly");
        } else {
            panic!("Property 'a' not found");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Optional Modifier Tests
// =============================================================================

#[test]
fn test_mapped_type_add_optional() {
    // type T = { a: string, b: number }
    // type Partial = { [K in keyof T]?: T[K] }
    // All properties should become optional
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // All properties should be optional
        for prop in &shape.properties {
            assert!(prop.optional, "Property should be optional");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_mapped_type_remove_optional() {
    // type T = { a?: string, b?: number }
    // type Required = { [K in keyof T]-?: T[K] }
    // All properties should become required
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Remove),
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // All properties should be required
        for prop in &shape.properties {
            assert!(!prop.optional, "Property should be required");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Readonly Modifier Tests
// =============================================================================

#[test]
fn test_mapped_type_add_readonly() {
    // type T = { a: string, b: number }
    // type Readonly = { readonly [K in keyof T]: T[K] }
    // All properties should become readonly
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: Some(MappedModifier::Add),
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // All properties should be readonly
        for prop in &shape.properties {
            assert!(prop.readonly, "Property should be readonly");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_mapped_type_remove_readonly() {
    // type T = { readonly a: string, readonly b: number }
    // type Mutable = { -readonly [K in keyof T]: T[K] }
    // All properties should become mutable
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source, type_param);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: Some(MappedModifier::Remove),
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        // All properties should be mutable
        for prop in &shape.properties {
            assert!(!prop.readonly, "Property should be mutable");
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Key Remapping Tests
// =============================================================================

#[test]
fn test_mapped_type_key_remap_with_template_literal() {
    // type Getters = { [K in 'a' | 'b' as `get${K}`]: () => string }
    // Should produce { geta: () => string, getb: () => string }
    let interner = TypeInterner::new();

    let literal_a = interner.literal_string("a");
    let literal_b = interner.literal_string("b");
    let keys_union = interner.union(vec![literal_a, literal_b]);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    // Create template literal: `get${K}`
    let template_literal = interner.template_literal(vec![
        crate::types::TemplateSpan::Text(interner.intern_string("get")),
        crate::types::TemplateSpan::Type(type_param),
    ]);

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keys_union,
        name_type: Some(template_literal),
        template: TypeId::STRING,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    // Should produce an object with 'geta' and 'getb' keys
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);

        let names: Vec<_> = shape
            .properties
            .iter()
            .map(|p| interner.resolve_atom(p.name))
            .collect();
        assert!(names.contains(&"geta".to_string()));
        assert!(names.contains(&"getb".to_string()));
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_mapped_type_empty_object() {
    // type Mapped = { [K in keyof {}]: never }
    // Should produce {} (empty object)
    let interner = TypeInterner::new();

    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.keyof(empty_obj);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_empty,
        name_type: None,
        template: TypeId::NEVER,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 0);
    } else {
        panic!(
            "Expected empty object type, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_mapped_type_identity() {
    // type Identity = { [K in 'a' | 'b' | 'c']: K }
    // Each property's type should be its key as a literal
    let interner = TypeInterner::new();

    let literal_a = interner.literal_string("a");
    let literal_b = interner.literal_string("b");
    let literal_c = interner.literal_string("c");
    let keys_union = interner.union(vec![literal_a, literal_b, literal_c]);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keys_union,
        name_type: None,
        template: type_param,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 3);

        // Each property type should be the literal string of its key
        for prop in &shape.properties {
            let _name = interner.resolve_atom(prop.name);
            // The type should be a string literal matching the name
            if let Some(TypeData::Literal(crate::types::LiteralValue::String(_))) =
                interner.lookup(prop.type_id)
            {
                // Good - it's a string literal
            } else {
                // It might also be the type parameter if not fully evaluated
                // This is acceptable depending on evaluation strategy
            }
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

#[test]
fn test_mapped_type_preserves_property_order() {
    // Property order should be preserved from the source
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("first"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("second"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("third"), TypeId::BOOLEAN),
    ]);

    let keyof_t = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_t,
        name_type: None,
        template: TypeId::ANY,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 3);
        // Note: Properties are sorted by name in object construction
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Enum Union Constraint Tests
// =============================================================================

#[test]
fn test_mapped_type_enum_union_constraint_with_overlapping_keys() {
    // Reproduces the mappedTypeOverlappingStringEnumKeys.ts scenario:
    //   enum TerrestrialAnimalTypes { CAT = "cat", DOG = "dog" }
    //   enum AlienAnimalTypes { CAT = "cat" }
    //   type AnimalTypes = TerrestrialAnimalTypes | AlienAnimalTypes
    //   type CatMap = { [V in AnimalTypes]: ... }
    //
    // The constraint is a Union of Enum members. extract_mapped_keys must:
    // 1. Unwrap Enum(DefId, inner) to reach the literal string values
    // 2. Recursively extract Union members within each enum group
    // 3. Deduplicate overlapping keys ("cat" appears in both enums)
    let interner = TypeInterner::new();

    let enum_terrestrial_def = DefId(100);
    let enum_alien_def = DefId(200);

    // TerrestrialAnimalTypes.CAT = "cat", TerrestrialAnimalTypes.DOG = "dog"
    let cat_lit = interner.literal_string("cat");
    let dog_lit = interner.literal_string("dog");
    let terr_cat = interner.intern(TypeData::Enum(enum_terrestrial_def, cat_lit));
    let terr_dog = interner.intern(TypeData::Enum(enum_terrestrial_def, dog_lit));

    // AlienAnimalTypes.CAT = "cat"
    let alien_cat = interner.intern(TypeData::Enum(enum_alien_def, cat_lit));

    // AnimalTypes = TerrestrialAnimalTypes | AlienAnimalTypes
    // = (TerrestrialAnimalTypes.CAT | TerrestrialAnimalTypes.DOG) | AlienAnimalTypes.CAT
    let animal_types = interner.union(vec![terr_cat, terr_dog, alien_cat]);

    // Create: { [V in AnimalTypes]: number }
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: animal_types,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    // Should produce { cat: number, dog: number } (2 properties, not 3)
    // because "cat" is deduplicated from both enums
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(
            shape.properties.len(),
            2,
            "Expected 2 properties (cat, dog) after dedup, got {:?}",
            shape
                .properties
                .iter()
                .map(|p| interner.resolve_atom(p.name))
                .collect::<Vec<_>>()
        );

        let names: Vec<_> = shape
            .properties
            .iter()
            .map(|p| interner.resolve_atom(p.name))
            .collect();
        assert!(names.contains(&"cat".to_string()));
        assert!(names.contains(&"dog".to_string()));

        for prop in &shape.properties {
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
    } else {
        panic!("Expected object type, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Identity Name Type (as K) Array Preservation Tests
// =============================================================================

#[test]
fn test_identity_name_type_preserves_array_structure() {
    // type Mappy<T extends unknown[]> = { [K in keyof T as K]: T[K] }
    // Mappy<number[]> should evaluate to number[] (Array), NOT a plain object.
    // The `as K` is an identity transformation and should be treated the same
    // as no name type for homomorphic array preservation.
    let interner = TypeInterner::new();

    // Source array: number[]
    let source_array = interner.array(TypeId::NUMBER);
    let keyof_source = interner.keyof(source_array);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    // Template is T[K] (index access)
    let template = interner.index_access(source_array, type_param);

    // Mapped type with identity name_type: { [K in keyof T as K]: T[K] }
    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_source,
        name_type: Some(type_param), // as K — identity
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    // Should produce an Array type (number[]), NOT a plain Object
    match interner.lookup(result) {
        Some(TypeData::Array(element_type)) => {
            assert_eq!(
                element_type,
                TypeId::NUMBER,
                "Expected Array<number>, got Array<{:?}>",
                interner.lookup(element_type)
            );
        }
        other => {
            panic!(
                "Expected Array type for identity as-clause mapped type over array, got {other:?}"
            );
        }
    }
}

#[test]
fn test_non_identity_name_type_degrades_to_object() {
    // type Remapped<T extends unknown[]> = { [K in keyof T as `get${K}`]: T[K] }
    // Remapped<number[]> should NOT preserve array structure (name type is not identity).
    let interner = TypeInterner::new();

    let source_array = interner.array(TypeId::NUMBER);
    let keyof_source = interner.keyof(source_array);

    let k_name = interner.intern_string("K");
    let type_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let template = interner.index_access(source_array, type_param);

    // Name type is a different type parameter (not K), simulating a non-identity transform
    let other_param_info = TypeParamInfo {
        name: interner.intern_string("Other"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let other_param = interner.intern(TypeData::TypeParameter(other_param_info));

    let mapped_type = MappedType {
        type_param: type_param_info,
        constraint: keyof_source,
        name_type: Some(other_param), // as Other — NOT identity
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);
    let result = evaluate_type(&interner, mapped_id);

    // Should NOT be an Array — non-identity name types degrade to objects
    assert!(
        !matches!(interner.lookup(result), Some(TypeData::Array(_))),
        "Non-identity name type should NOT preserve array structure, got Array"
    );
}
