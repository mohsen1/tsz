use super::*;
use crate::type_queries::data::is_homomorphic_mapped_type_context;
use crate::types::{MappedType, TypeData, TypeParamInfo};

/// Helper to create a type parameter T with given name and optional constraint.
fn make_type_param(db: &TypeInterner, name: &str, constraint: Option<TypeId>) -> TypeId {
    db.intern(TypeData::TypeParameter(TypeParamInfo {
        name: db.intern_string(name),
        constraint,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }))
}

#[test]
fn test_homomorphic_mapped_type_with_keyof_type_param() {
    // { [K in keyof T]: T[K] }
    let db = TypeInterner::new();
    let t = make_type_param(&db, "T", None);
    let k_name = db.intern_string("K");
    let constraint = db.keyof(t);
    let template = db.index_access(
        t,
        db.intern(TypeData::TypeParameter(TypeParamInfo {
            name: k_name,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        })),
    );
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert!(
        is_homomorphic_mapped_type_context(&db, mapped),
        "mapped type with keyof T should be recognized as homomorphic"
    );
}

#[test]
fn test_non_homomorphic_mapped_type() {
    // { [K in string]: number }
    let db = TypeInterner::new();
    let k_name = db.intern_string("K");
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_name,
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
        !is_homomorphic_mapped_type_context(&db, mapped),
        "mapped type with string constraint should NOT be homomorphic"
    );
}

#[test]
fn test_homomorphic_mapped_type_with_intersection_constraint() {
    // { [K in keyof T & keyof U]: ... } — has keyof T in intersection
    let db = TypeInterner::new();
    let t = make_type_param(&db, "T", None);
    let u = make_type_param(&db, "U", None);
    let keyof_t = db.keyof(t);
    let keyof_u = db.keyof(u);
    let constraint = db.intersection(vec![keyof_t, keyof_u]);
    let k_name = db.intern_string("K");
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert!(
        is_homomorphic_mapped_type_context(&db, mapped),
        "mapped type with intersection containing keyof T should be homomorphic"
    );
}

#[test]
fn test_non_mapped_type_not_homomorphic() {
    let db = TypeInterner::new();
    assert!(
        !is_homomorphic_mapped_type_context(&db, TypeId::STRING),
        "string should not be homomorphic"
    );
    assert!(
        !is_homomorphic_mapped_type_context(&db, TypeId::NUMBER),
        "number should not be homomorphic"
    );
    let arr = db.array(TypeId::STRING);
    assert!(
        !is_homomorphic_mapped_type_context(&db, arr),
        "string[] should not be homomorphic"
    );
}
