//! Type construction boundary helpers.
//!
//! Provides mediated access to solver type construction facilities.
//! Production checker code should prefer purpose-specific helpers here
//! over direct `TypeInterner` access. Test code may use the re-exported
//! `TypeInterner` type for scaffolding.

use tsz_binder::SymbolId;
use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
#[cfg(test)]
pub(crate) use tsz_solver::construction::TypeInterner;
use tsz_solver::{
    IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, StringIntrinsicKind, TypeId,
};

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

fn enum_namespace_flags(is_const_enum: bool) -> ObjectFlags {
    let mut flags = ObjectFlags::ENUM_NAMESPACE;
    if is_const_enum {
        flags |= ObjectFlags::CONST_ENUM;
    }
    flags
}

/// Intern a `typeof Enum` namespace object without exposing raw solver flags to
/// checker call sites.
pub(crate) fn enum_namespace_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
    is_const_enum: bool,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, enum_namespace_flags(is_const_enum), symbol)
}

/// Intern a numeric/mixed enum namespace object with the implicit reverse-map
/// number index used by value-side element access.
pub(crate) fn enum_namespace_object_with_number_reverse_map(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
    index_param_name: Option<Atom>,
    index_readonly: bool,
    is_const_enum: bool,
) -> TypeId {
    db.object_with_index(ObjectShape {
        flags: enum_namespace_flags(is_const_enum),
        properties,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: index_readonly,
            param_name: index_param_name,
        }),
        symbol,
        ..ObjectShape::default()
    })
}

/// Intern the indexed object produced by an inline type literal.
pub(crate) fn type_literal_object_with_index(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    has_late_bound_members: bool,
) -> TypeId {
    let mut shape = ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    };
    if has_late_bound_members {
        shape.mark_has_late_bound_members();
    }
    db.object_with_index(shape)
}

/// Intern the extra number-index object intersected into a type literal when
/// multiple number index signatures must be preserved.
pub(crate) fn type_literal_extra_number_index_object(
    db: &dyn TypeDatabase,
    number_index: IndexSignature,
) -> TypeId {
    db.object_with_index(ObjectShape {
        number_index: Some(number_index),
        ..ObjectShape::default()
    })
}

/// Intern the plain object produced by an inline type literal.
pub(crate) fn type_literal_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    has_late_bound_members: bool,
) -> TypeId {
    let flags = if has_late_bound_members {
        ObjectFlags::HAS_LATE_BOUND_MEMBERS
    } else {
        ObjectFlags::empty()
    };
    db.object_with_flags_and_symbol(properties, flags, None)
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
