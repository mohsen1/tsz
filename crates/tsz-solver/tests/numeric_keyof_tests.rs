//! Tests for numeric property key types in keyof and mapped types.
//!
//! TypeScript rule: `keyof { 0: T }` is `0` (number literal), not `"0"` (string literal).
//! A property declared as `{ "0": T }` with a quoted name has key type `"0"` (string literal).
//! These tests verify that tsz follows tsc's behaviour for all variants.

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::types::{
    LiteralValue, MappedModifier, MappedType, PropertyInfo, TypeData, TypeParamInfo,
};

// =============================================================================
// Helpers
// =============================================================================

/// Collects the numeric literal values from a union of number literals.
fn collect_number_literals(interner: &TypeInterner, type_id: TypeId) -> Vec<f64> {
    let mut result = Vec::new();
    match interner.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::Number(n))) => result.push(n.0),
        Some(TypeData::Union(list_id)) => {
            for &m in interner.type_list(list_id).iter() {
                if let Some(TypeData::Literal(LiteralValue::Number(n))) = interner.lookup(m) {
                    result.push(n.0);
                }
            }
        }
        _ => {}
    }
    result.sort_by(|a, b| a.partial_cmp(b).unwrap());
    result
}

/// Collects the string literal values from a union of string literals.
fn collect_string_literals(interner: &TypeInterner, type_id: TypeId) -> Vec<String> {
    let mut result = Vec::new();
    match interner.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            result.push(interner.resolve_atom(atom))
        }
        Some(TypeData::Union(list_id)) => {
            for &m in interner.type_list(list_id).iter() {
                if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(m) {
                    result.push(interner.resolve_atom(atom));
                }
            }
        }
        _ => {}
    }
    result.sort();
    result
}

/// Creates a `PropertyInfo` for a numeric-indexed property (e.g., `{ 0: T }`).
/// `is_string_named = false` (default) means the key type is numeric.
fn numeric_prop(interner: &TypeInterner, index: u32, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::new(interner.intern_string(&index.to_string()), type_id)
}

/// Creates a `PropertyInfo` for a string-named numeric-looking property (e.g., `{ "0": T }`).
/// `is_string_named = true` means the key type stays as a string literal.
fn string_named_numeric_prop(interner: &TypeInterner, index: u32, type_id: TypeId) -> PropertyInfo {
    PropertyInfo {
        name: interner.intern_string(&index.to_string()),
        type_id,
        write_type: type_id,
        is_string_named: true,
        ..PropertyInfo::new(interner.intern_string(&index.to_string()), type_id)
    }
}

// =============================================================================
// keyof — numeric properties produce number literal keys
// =============================================================================

#[test]
fn keyof_single_numeric_property_is_number_literal() {
    // tsc: keyof { 0: boolean } = 0
    let interner = TypeInterner::new();
    let obj = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let result = evaluate_type(&interner, interner.keyof(obj));
    let nums = collect_number_literals(&interner, result);
    assert_eq!(nums, vec![0.0], "keyof {{ 0: boolean }} should be 0");
    assert!(
        collect_string_literals(&interner, result).is_empty(),
        "should produce no string literals"
    );
}

#[test]
fn keyof_multiple_numeric_properties_are_number_literals() {
    // tsc: keyof { 0: boolean; 1: string } = 0 | 1
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        numeric_prop(&interner, 0, TypeId::BOOLEAN),
        numeric_prop(&interner, 1, TypeId::STRING),
    ]);
    let result = evaluate_type(&interner, interner.keyof(obj));
    let nums = collect_number_literals(&interner, result);
    assert_eq!(
        nums,
        vec![0.0, 1.0],
        "keyof {{ 0: boolean; 1: string }} should be 0 | 1"
    );
}

#[test]
fn keyof_mixed_numeric_and_string_properties() {
    // tsc: keyof { 0: boolean; a: string } = 0 | "a"
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        numeric_prop(&interner, 0, TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);
    let result = evaluate_type(&interner, interner.keyof(obj));
    let nums = collect_number_literals(&interner, result);
    let strs = collect_string_literals(&interner, result);
    assert_eq!(nums, vec![0.0], "should have numeric key 0");
    assert_eq!(strs, vec!["a".to_string()], "should have string key \"a\"");
}

#[test]
fn keyof_string_named_numeric_property_is_string_literal() {
    // tsc: keyof { "0": boolean } = "0"  (quoted name → string key)
    let interner = TypeInterner::new();
    let obj = interner.object(vec![string_named_numeric_prop(
        &interner,
        0,
        TypeId::BOOLEAN,
    )]);
    let result = evaluate_type(&interner, interner.keyof(obj));
    let strs = collect_string_literals(&interner, result);
    assert_eq!(
        strs,
        vec!["0".to_string()],
        "keyof {{ \"0\": boolean }} should be \"0\""
    );
    assert!(
        collect_number_literals(&interner, result).is_empty(),
        "should produce no number literals"
    );
}

#[test]
fn keyof_large_numeric_index() {
    // tsc: keyof { 404: boolean } = 404
    let interner = TypeInterner::new();
    let obj = interner.object(vec![numeric_prop(&interner, 404, TypeId::BOOLEAN)]);
    let result = evaluate_type(&interner, interner.keyof(obj));
    let nums = collect_number_literals(&interner, result);
    assert_eq!(nums, vec![404.0]);
}

// =============================================================================
// keyof assignability — numeric key type constraints
// =============================================================================

#[test]
fn numeric_key_is_assignable_to_keyof() {
    // const a: keyof { 0: boolean } = 0  → should compile (no TS2322)
    let interner = TypeInterner::new();
    let obj = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let keyof_obj = evaluate_type(&interner, interner.keyof(obj));
    let zero_literal = interner.literal_number(0.0);
    let mut checker = CompatChecker::new(&interner);
    assert!(
        checker.is_assignable(zero_literal, keyof_obj),
        "0 should be assignable to keyof {{ 0: boolean }}"
    );
}

#[test]
fn string_zero_not_assignable_to_numeric_keyof() {
    // const a: keyof { 0: boolean } = "0"  → should fail (TS2322)
    let interner = TypeInterner::new();
    let obj = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let keyof_obj = evaluate_type(&interner, interner.keyof(obj));
    let string_zero = interner.literal_string("0");
    let mut checker = CompatChecker::new(&interner);
    assert!(
        !checker.is_assignable(string_zero, keyof_obj),
        "\"0\" should NOT be assignable to keyof {{ 0: boolean }}"
    );
}

#[test]
fn string_key_assignable_to_string_named_keyof() {
    // const a: keyof { "0": boolean } = "0"  → should compile
    let interner = TypeInterner::new();
    let obj = interner.object(vec![string_named_numeric_prop(
        &interner,
        0,
        TypeId::BOOLEAN,
    )]);
    let keyof_obj = evaluate_type(&interner, interner.keyof(obj));
    let string_zero = interner.literal_string("0");
    let mut checker = CompatChecker::new(&interner);
    assert!(
        checker.is_assignable(string_zero, keyof_obj),
        "\"0\" should be assignable to keyof {{ \"0\": boolean }}"
    );
}

// =============================================================================
// Mapped types over numeric keyof
// =============================================================================

fn make_type_param(interner: &TypeInterner, name: &str) -> (TypeId, TypeParamInfo) {
    let info = TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let id = interner.intern(TypeData::TypeParameter(info));
    (id, info)
}

#[test]
fn mapped_type_over_numeric_keyof_produces_numeric_keyed_object() {
    // type T = { [K in keyof { 0: boolean }]: K }
    // tsc produces: { 0: 0 }
    let interner = TypeInterner::new();
    let src = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let keyof_src = interner.keyof(src);
    let (k_id, k_info) = make_type_param(&interner, "K");
    let mapped = MappedType {
        type_param: k_info,
        constraint: keyof_src,
        name_type: None,
        template: k_id,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));
    let Some(TypeData::Object(shape_id)) = interner.lookup(result) else {
        panic!("expected object, got {:?}", interner.lookup(result));
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties.len(), 1);
    let prop = &shape.properties[0];
    // Property name atom should be "0"
    assert_eq!(interner.resolve_atom(prop.name), "0");
    // Property value type (K substituted) should be Literal(Number(0.0))
    assert!(
        matches!(interner.lookup(prop.type_id), Some(TypeData::Literal(LiteralValue::Number(n))) if n.0 == 0.0),
        "expected Literal(Number(0)), got {:?}",
        interner.lookup(prop.type_id)
    );
}

#[test]
fn mapped_type_over_multiple_numeric_keys() {
    // type T = { [K in 0 | 1]: K }
    // tsc produces: { 0: 0; 1: 1 }
    let interner = TypeInterner::new();
    let zero = interner.literal_number(0.0);
    let one = interner.literal_number(1.0);
    let keys = interner.union(vec![zero, one]);
    let (k_id, k_info) = make_type_param(&interner, "K");
    let mapped = MappedType {
        type_param: k_info,
        constraint: keys,
        name_type: None,
        template: k_id,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));
    let Some(TypeData::Object(shape_id)) = interner.lookup(result) else {
        panic!("expected object, got {:?}", interner.lookup(result));
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties.len(), 2);
    // Collect (name_atom, value_type) pairs
    let mut pairs: Vec<(String, f64)> = shape
        .properties
        .iter()
        .filter_map(|p| {
            if let Some(TypeData::Literal(LiteralValue::Number(n))) = interner.lookup(p.type_id) {
                Some((interner.resolve_atom(p.name), n.0))
            } else {
                None
            }
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(pairs, vec![("0".to_string(), 0.0), ("1".to_string(), 1.0)]);
}

#[test]
fn mapped_type_variable_name_does_not_affect_numeric_key_behaviour() {
    // Same as mapped_type_over_numeric_keyof_produces_numeric_keyed_object but
    // using a different type parameter name to verify there's no hardcoded "K".
    let interner = TypeInterner::new();
    let src = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let keyof_src = interner.keyof(src);
    let (p_id, p_info) = make_type_param(&interner, "P"); // different name
    let mapped = MappedType {
        type_param: p_info,
        constraint: keyof_src,
        name_type: None,
        template: p_id,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));
    let Some(TypeData::Object(shape_id)) = interner.lookup(result) else {
        panic!("expected object, got {:?}", interner.lookup(result));
    };
    let prop = &interner.object_shape(shape_id).properties[0];
    assert!(
        matches!(interner.lookup(prop.type_id), Some(TypeData::Literal(LiteralValue::Number(n))) if n.0 == 0.0),
        "P should resolve to Literal(Number(0)), not a string literal"
    );
}

#[test]
fn mapped_type_string_named_numeric_prop_stays_string() {
    // type T = { [K in keyof { "0": boolean }]: K }
    // tsc produces: { "0": "0" }  (quoted property → string key)
    let interner = TypeInterner::new();
    let src = interner.object(vec![string_named_numeric_prop(
        &interner,
        0,
        TypeId::BOOLEAN,
    )]);
    let keyof_src = interner.keyof(src);
    let (k_id, k_info) = make_type_param(&interner, "K");
    let mapped = MappedType {
        type_param: k_info,
        constraint: keyof_src,
        name_type: None,
        template: k_id,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));
    let Some(TypeData::Object(shape_id)) = interner.lookup(result) else {
        panic!("expected object, got {:?}", interner.lookup(result));
    };
    let prop = &interner.object_shape(shape_id).properties[0];
    // K should resolve to Literal(String("0")), not a number literal
    assert!(
        matches!(
            interner.lookup(prop.type_id),
            Some(TypeData::Literal(LiteralValue::String(_)))
        ),
        "K for quoted \"0\" property should be string literal, got {:?}",
        interner.lookup(prop.type_id)
    );
}

#[test]
fn partial_over_numeric_keyed_object_preserves_types() {
    // type Partial<T> = { [K in keyof T]?: T[K] }
    // Partial<{ 0: boolean }> should produce { 0?: boolean }
    let interner = TypeInterner::new();

    // Build T = { 0: boolean }
    let t_name = interner.intern_string("T");
    let t_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_id = interner.intern(TypeData::TypeParameter(t_info));

    // Build keyof T
    let keyof_t = interner.keyof(t_id);

    // Build T[K] index access
    let k_name = interner.intern_string("K");
    let k_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_id = interner.intern(TypeData::TypeParameter(k_info));
    let t_k = interner.index_access(t_id, k_id);

    // Build mapped: { [K in keyof T]?: T[K] }
    let mapped = MappedType {
        type_param: k_info,
        constraint: keyof_t,
        name_type: None,
        template: t_k,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    };
    let partial_of_t = interner.mapped(mapped);

    // Instantiate Partial<{ 0: boolean }>
    let concrete = interner.object(vec![numeric_prop(&interner, 0, TypeId::BOOLEAN)]);
    let subst = crate::instantiation::instantiate::TypeSubstitution::single(t_name, concrete);
    let instantiated =
        crate::instantiation::instantiate::instantiate_type(&interner, partial_of_t, &subst);
    let result = evaluate_type(&interner, instantiated);

    let Some(TypeData::Object(shape_id)) = interner.lookup(result) else {
        panic!("expected object, got {:?}", interner.lookup(result));
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties.len(), 1);
    let prop = &shape.properties[0];
    assert_eq!(
        interner.resolve_atom(prop.name),
        "0",
        "property name should be '0'"
    );
    assert!(prop.optional, "property should be optional");
}
