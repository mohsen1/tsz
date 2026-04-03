use crate::PropertyInfo;
use crate::intern::TypeInterner;
use crate::operations::binary_ops::BinaryOpEvaluator;
use crate::types::{TypeData, TypeId, TypeParamInfo};

fn make_type_param(db: &TypeInterner, name: &str, constraint: Option<TypeId>) -> TypeId {
    db.intern(TypeData::TypeParameter(TypeParamInfo {
        name: db.intern_string(name),
        constraint,
        default: None,
        is_const: false,
    }))
}

fn object_with_ab_values(db: &TypeInterner, value: TypeId) -> TypeId {
    let a = PropertyInfo::new(db.intern_string("a"), value);
    let b = PropertyInfo::new(db.intern_string("b"), value);
    db.object(vec![a, b])
}

#[test]
fn computed_prop_name_valid_for_type_param_with_string_constraint() {
    let interner = TypeInterner::new();
    // K extends string
    let k = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(evaluator.is_valid_computed_property_name_type(k));
}

#[test]
fn computed_prop_name_valid_for_type_param_with_keyof_constraint() {
    let interner = TypeInterner::new();
    // keyof T
    let keyof_t = interner.intern(TypeData::KeyOf(TypeId::ANY));
    // K extends keyof T
    let k = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
    }));
    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(evaluator.is_valid_computed_property_name_type(k));
}

#[test]
fn computed_prop_name_invalid_for_unconstrained_type_param() {
    let interner = TypeInterner::new();
    // T (no constraint)
    let t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(!evaluator.is_valid_computed_property_name_type(t));
}

#[test]
fn computed_prop_name_invalid_for_type_param_with_object_constraint() {
    let interner = TypeInterner::new();
    // T extends object
    let t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    }));
    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(!evaluator.is_valid_computed_property_name_type(t));
}

#[test]
fn computed_prop_name_valid_for_keyof() {
    let interner = TypeInterner::new();
    let keyof = interner.intern(TypeData::KeyOf(TypeId::ANY));
    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(evaluator.is_valid_computed_property_name_type(keyof));
}

#[test]
fn mapped_key_type_accepts_index_access_with_constraint_superset_of_object_keys() {
    // When the index type parameter's constraint is a superset of the object's keys
    // (e.g., S extends "a" | "b" | "extra" indexing {a, b}), the mapped type key type
    // check conservatively accepts this in generic context. The interner-level
    // assignability check cannot fully evaluate KeyOf to reject the constraint mismatch,
    // so the deferred path treats the index access as valid.
    let interner = TypeInterner::new();
    let ab = object_with_ab_values(&interner, interner.literal_string("a"));
    let constraint = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
        interner.literal_string("extra"),
    ]);
    let s = make_type_param(&interner, "S", Some(constraint));
    let access = interner.index_access(ab, s);

    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(evaluator.is_valid_mapped_type_key_type(access));
}

#[test]
fn mapped_key_type_allows_index_access_indexed_by_keyof_object() {
    let interner = TypeInterner::new();
    let ab = object_with_ab_values(&interner, interner.literal_string("a"));
    let constraint = interner.keyof(ab);
    let k = make_type_param(&interner, "K", Some(constraint));
    let access = interner.index_access(ab, k);

    let evaluator = BinaryOpEvaluator::new(&interner);
    assert!(evaluator.is_valid_mapped_type_key_type(access));
}
