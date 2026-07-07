//! Import attribute/options construction boundary.
//!
//! Import declaration and dynamic-import checkers own AST traversal, grammar
//! diagnostics, and relation anchors. This module owns the synthetic solver
//! object shapes those syntax facts become for `ImportAttributes` and
//! `ImportCallOptions` checks.

use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{PropertyInfo, TypeId};

pub(crate) fn import_attribute_literal_string_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    db.literal_string(value)
}

pub(crate) const fn import_attribute_property(name: Atom, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) const fn optional_import_option_property(name: Atom, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::opt(name, type_id)
}

pub(crate) fn import_attribute_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn import_call_options_type(
    db: &dyn TypeDatabase,
    with_name: Atom,
    assert_name: Atom,
    import_attributes_type: TypeId,
) -> TypeId {
    import_attribute_object_type(
        db,
        vec![
            optional_import_option_property(with_name, import_attributes_type),
            optional_import_option_property(assert_name, import_attributes_type),
        ],
    )
}
