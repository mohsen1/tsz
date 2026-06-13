//! Type construction boundary helpers.
//!
//! Provides mediated access to solver type construction facilities.
//! Production checker code should prefer purpose-specific helpers here
//! over direct `TypeInterner` access. Test code may use the re-exported
//! `TypeInterner` type for scaffolding.

use tsz_solver::construction::TypeDatabase;
#[cfg(test)]
pub(crate) use tsz_solver::construction::TypeInterner;
use tsz_solver::{IndexSignature, ObjectShape, StringIntrinsicKind, TypeId};

/// Intern an object type carrying only a string index signature
/// (`{ [key: string]: V }`).
pub(crate) fn object_with_string_index_value(
    db: &dyn TypeDatabase,
    value_type: TypeId,
    readonly: bool,
) -> TypeId {
    db.object_with_index(ObjectShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type,
            readonly,
            param_name: None,
        }),
        ..ObjectShape::default()
    })
}

/// Create a string intrinsic type from a validated lib intrinsic name.
pub(crate) fn string_intrinsic_by_name(
    db: &dyn TypeDatabase,
    name: &str,
    type_arg: TypeId,
) -> TypeId {
    match name {
        "Uppercase" => db.string_intrinsic(StringIntrinsicKind::Uppercase, type_arg),
        "Lowercase" => db.string_intrinsic(StringIntrinsicKind::Lowercase, type_arg),
        "Capitalize" => db.string_intrinsic(StringIntrinsicKind::Capitalize, type_arg),
        "Uncapitalize" => db.string_intrinsic(StringIntrinsicKind::Uncapitalize, type_arg),
        _ => TypeId::ERROR,
    }
}
