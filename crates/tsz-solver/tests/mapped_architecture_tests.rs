//! Tests for mapped-type architecture helpers.
//!
//! Verifies the solver-side helpers that centralize mapped-type expansion policy:
//! - `classify_mapped_source`: structural classification for array/tuple preservation
//! - `compute_mapped_modifiers`: centralized modifier computation
//! - `is_identity_name_mapping`: identity `as` clause detection
//! - `collect_homomorphic_source_properties`: source property extraction
//! - `expand_mapped_type_to_properties`: full expansion with modifier application

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::type_queries::{
    MappedSourceKind, classify_mapped_source, collect_homomorphic_source_properties,
    compute_mapped_modifiers, expand_mapped_type_to_properties, is_identity_name_mapping,
};
use crate::types::{
    MappedModifier, MappedType, PropertyInfo, TupleElement, TypeData, TypeParamInfo, Visibility,
};
use rustc_hash::FxHashMap;

// =============================================================================
// classify_mapped_source tests
// =============================================================================

#[test]
fn classify_source_array() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    assert_eq!(
        classify_mapped_source(&interner, arr),
        MappedSourceKind::Array(TypeId::NUMBER)
    );
}

#[test]
fn classify_source_tuple() {
    let interner = TypeInterner::new();
    let tup = interner.tuple(vec![
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
    match classify_mapped_source(&interner, tup) {
        MappedSourceKind::Tuple(_) => {} // expected
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn classify_source_plain_object() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
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
    }]);
    assert_eq!(
        classify_mapped_source(&interner, obj),
        MappedSourceKind::Object
    );
}

#[test]
fn classify_source_type_param_with_array_constraint() {
    let interner = TypeInterner::new();
    let arr_constraint = interner.array(TypeId::UNKNOWN);
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(arr_constraint),
        default: None,
        is_const: false,
    });
    match classify_mapped_source(&interner, tp) {
        MappedSourceKind::TypeParamWithArrayConstraint(_) => {} // expected
        other => panic!("expected TypeParamWithArrayConstraint, got {other:?}"),
    }
}

#[test]
fn classify_source_type_param_with_object_constraint() {
    let interner = TypeInterner::new();
    let obj_constraint = interner.object(vec![]);
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(obj_constraint),
        default: None,
        is_const: false,
    });
    assert_eq!(
        classify_mapped_source(&interner, tp),
        MappedSourceKind::Object
    );
}

// =============================================================================
// compute_mapped_modifiers tests
// =============================================================================

#[test]
fn modifiers_add_optional_and_readonly() {
    let interner = TypeInterner::new();
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: Some(MappedModifier::Add),
    };
    let (opt, ro) = compute_mapped_modifiers(&mapped, false, false, false);
    assert!(opt, "should be optional with +?");
    assert!(ro, "should be readonly with +readonly");
}

#[test]
fn modifiers_remove_optional_and_readonly() {
    let interner = TypeInterner::new();
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: Some(MappedModifier::Remove),
        readonly_modifier: Some(MappedModifier::Remove),
    };
    let (opt, ro) = compute_mapped_modifiers(&mapped, true, true, true);
    assert!(!opt, "should not be optional with -?");
    assert!(!ro, "should not be readonly with -readonly");
}

#[test]
fn modifiers_homomorphic_preserves_source() {
    let interner = TypeInterner::new();
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    // Homomorphic: should preserve source modifiers
    let (opt, ro) = compute_mapped_modifiers(&mapped, true, true, true);
    assert!(opt, "homomorphic should preserve source optional");
    assert!(ro, "homomorphic should preserve source readonly");

    // Non-homomorphic: should default to false
    let (opt, ro) = compute_mapped_modifiers(&mapped, false, true, true);
    assert!(!opt, "non-homomorphic should default optional to false");
    assert!(!ro, "non-homomorphic should default readonly to false");
}

// =============================================================================
// is_identity_name_mapping tests
// =============================================================================

#[test]
fn identity_no_name_type() {
    let interner = TypeInterner::new();
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&interner, &mapped));
}

#[test]
fn identity_name_type_same_param() {
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");
    let k_param = interner.type_param(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: Some(k_param),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&interner, &mapped));
}

#[test]
fn non_identity_name_type_different_param() {
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");
    let other_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: Some(other_param),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(!is_identity_name_mapping(&interner, &mapped));
}

// =============================================================================
// collect_homomorphic_source_properties tests
// =============================================================================

#[test]
fn collect_source_props_from_object() {
    let interner = TypeInterner::new();
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let obj = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);
    let props = collect_homomorphic_source_properties(&interner, obj);
    assert_eq!(props.len(), 2);
    assert_eq!(props[&a_name], (true, false, TypeId::STRING));
    assert_eq!(props[&b_name], (false, true, TypeId::NUMBER));
}

// =============================================================================
// expand_mapped_type_to_properties tests
// =============================================================================

#[test]
fn expand_simple_mapped_type() {
    let interner = TypeInterner::new();
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let k_name = interner.intern_string("K");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let source_props = FxHashMap::default();
    let keys = vec![a_name, b_name];
    let props = expand_mapped_type_to_properties(&interner, &mapped, &keys, &source_props, false);

    assert_eq!(props.len(), 2);
    assert_eq!(props[0].name, a_name);
    assert_eq!(props[0].type_id, TypeId::NUMBER);
    assert!(!props[0].optional);
    assert!(!props[0].readonly);
    assert_eq!(props[1].name, b_name);
    assert_eq!(props[1].type_id, TypeId::NUMBER);
}

#[test]
fn expand_mapped_with_add_optional() {
    let interner = TypeInterner::new();
    let a_name = interner.intern_string("a");
    let k_name = interner.intern_string("K");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: Some(MappedModifier::Add),
    };

    let source_props = FxHashMap::default();
    let keys = vec![a_name];
    let props = expand_mapped_type_to_properties(&interner, &mapped, &keys, &source_props, false);

    assert_eq!(props.len(), 1);
    assert!(props[0].optional, "should be optional with +?");
    assert!(props[0].readonly, "should be readonly with +readonly");
}

#[test]
fn expand_homomorphic_with_remove_optional() {
    let interner = TypeInterner::new();
    let a_name = interner.intern_string("a");
    let k_name = interner.intern_string("K");

    // Simulating Required<T> where T has optional props
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::STRING, // simplified; real case is T[K]
        optional_modifier: Some(MappedModifier::Remove),
        readonly_modifier: None,
    };

    let mut source_props = FxHashMap::default();
    source_props.insert(a_name, (true, false, TypeId::STRING));

    let keys = vec![a_name];
    let props = expand_mapped_type_to_properties(&interner, &mapped, &keys, &source_props, true);

    assert_eq!(props.len(), 1);
    assert!(!props[0].optional, "should not be optional with -?");
    // With -? and homomorphic, the declared type should be used
    assert_eq!(props[0].type_id, TypeId::STRING);
}

// =============================================================================
// Mapped types over tuples — verify solver evaluator preserves structure
// =============================================================================

#[test]
fn mapped_type_over_tuple_preserves_structure() {
    // Verify that `{ [K in keyof [number, string]]: T }` evaluates to a tuple
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");

    let source_tuple = interner.tuple(vec![
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

    let keyof_tuple = interner.keyof(source_tuple);

    // Template: boolean (simplified — maps all elements to boolean)
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_tuple,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);
    let result = evaluate_type(&interner, mapped_id);

    // Should produce a tuple, not an object
    match interner.lookup(result) {
        Some(TypeData::Tuple(tuple_id)) => {
            let elements = interner.tuple_list(tuple_id);
            assert_eq!(elements.len(), 2, "tuple should have 2 elements");
            for elem in elements.iter() {
                assert_eq!(elem.type_id, TypeId::BOOLEAN);
            }
        }
        other => panic!("expected Tuple from mapped type over tuple source, got {other:?}"),
    }
}

#[test]
fn mapped_type_over_array_preserves_structure() {
    // Verify that `{ [K in keyof number[]]: T }` evaluates to an array
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");

    let source_array = interner.array(TypeId::NUMBER);
    let keyof_array = interner.keyof(source_array);

    // Template: boolean (maps element to boolean)
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_array,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);
    let result = evaluate_type(&interner, mapped_id);

    // Should produce an array type, not an object
    match interner.lookup(result) {
        Some(TypeData::Array(_)) => {} // correct
        other => panic!("expected Array from mapped type over array source, got {other:?}"),
    }
}

// =============================================================================
// Mapped types with rest elements in tuples
// =============================================================================

#[test]
fn mapped_type_over_tuple_with_rest() {
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");

    let rest_array = interner.array(TypeId::NUMBER);
    let source_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let keyof_tuple = interner.keyof(source_tuple);

    // Partial-like: { [K in keyof T]+?: T[K] }
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_tuple,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);
    let result = evaluate_type(&interner, mapped_id);

    // Should still produce a tuple with the rest element preserved
    match interner.lookup(result) {
        Some(TypeData::Tuple(tuple_id)) => {
            let elements = interner.tuple_list(tuple_id);
            assert!(elements.len() >= 2, "tuple should have at least 2 elements");
            assert!(elements[0].optional, "first element should be optional");
            assert!(elements.last().unwrap().rest, "last element should be rest");
        }
        other => panic!("expected Tuple from mapped type over tuple-with-rest, got {other:?}"),
    }
}

// =============================================================================
// Mapped types with type parameter constrained to array
// =============================================================================

#[test]
fn mapped_type_over_type_param_with_array_constraint() {
    let interner = TypeInterner::new();
    let k_name = interner.intern_string("K");

    // T extends unknown[]
    let arr_constraint = interner.array(TypeId::UNKNOWN);
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(arr_constraint),
        default: None,
        is_const: false,
    });

    let keyof_tp = interner.keyof(tp);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_tp,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);
    let result = evaluate_type(&interner, mapped_id);

    // For type param constrained to array, the solver should produce an array type.
    // Also acceptable: deferred mapped type (since T is a type parameter).
    // The key thing is it should NOT produce a plain Object.
    if let Some(TypeData::Object(_)) = interner.lookup(result) {
        panic!("Expected array or deferred mapped type, got plain Object");
    }
}
