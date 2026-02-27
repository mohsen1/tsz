use crate::intern::TypeInterner;
use crate::operations::binary_ops::BinaryOpEvaluator;
use crate::types::{TypeData, TypeId, TypeParamInfo};

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
