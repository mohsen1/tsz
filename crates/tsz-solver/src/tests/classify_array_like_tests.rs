use crate::intern::TypeInterner;
use crate::type_queries::extended::{ArrayLikeKind, classify_array_like};
use crate::types::{MappedType, TypeData, TypeId, TypeParamInfo};

#[test]
fn classify_array_like_plain_array() {
    let interner = TypeInterner::new();
    let arr = interner.intern(TypeData::Array(TypeId::STRING));
    assert!(matches!(
        classify_array_like(&interner, arr),
        ArrayLikeKind::Array(_)
    ));
}

#[test]
fn classify_array_like_type_param_with_array_constraint() {
    let interner = TypeInterner::new();
    let any_array = interner.intern(TypeData::Array(TypeId::ANY));
    let t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    assert!(matches!(
        classify_array_like(&interner, t),
        ArrayLikeKind::Array(_)
    ));
}

#[test]
fn classify_array_like_unconstrained_type_param() {
    let interner = TypeInterner::new();
    let t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    assert!(matches!(
        classify_array_like(&interner, t),
        ArrayLikeKind::Other
    ));
}

#[test]
fn classify_array_like_mapped_over_array_type_param() {
    let interner = TypeInterner::new();
    // T extends readonly unknown[]
    let readonly_unknown_arr = interner.intern(TypeData::ReadonlyType(
        interner.intern(TypeData::Array(TypeId::UNKNOWN)),
    ));
    let t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(readonly_unknown_arr),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    // keyof T
    let keyof_t = interner.intern(TypeData::KeyOf(t));
    // { [K in keyof T]: T[K] } — a homomorphic mapped type over array T
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(keyof_t),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint: keyof_t,
        name_type: None,
        template: TypeId::ANY, // simplified
        readonly_modifier: None,
        optional_modifier: None,
    });
    // The mapped type should be classified as array-like (readonly array)
    // because keyof T where T extends readonly unknown[] preserves array structure
    let result = classify_array_like(&interner, mapped);
    // Should follow the chain: Mapped -> KeyOf(T) -> TypeParameter(T) -> ReadonlyType(Array)
    assert!(
        matches!(result, ArrayLikeKind::Readonly(_)),
        "Mapped type over keyof array-constrained type param should be array-like, got {result:?}"
    );
}

#[test]
fn classify_array_like_mapped_non_keyof_constraint() {
    let interner = TypeInterner::new();
    // { [K in string]: number } — NOT a homomorphic mapped type
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    assert!(
        matches!(classify_array_like(&interner, mapped), ArrayLikeKind::Other),
        "Non-keyof mapped type should not be classified as array-like"
    );
}
