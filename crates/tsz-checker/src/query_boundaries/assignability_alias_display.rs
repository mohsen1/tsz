//! Structural facts for assignability alias-display diagnostics.

use super::common;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn source_preserves_declared_generic_alias_display(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> bool {
    common::is_intersection_type(db, source) || common::object_shape_id(db, source).is_some()
}

pub(crate) fn source_can_use_declared_generic_alias_annotation(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> bool {
    common::contains_conditional_type(db, source) || common::is_callable_type(db, source)
}

pub(crate) fn is_application_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::application_id(db, type_id).is_some()
}

pub(crate) fn is_object_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::object_shape_id(db, type_id).is_some()
}

pub(crate) fn contains_undefined_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::type_contains_undefined(db, type_id)
}

pub(crate) fn is_literal_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::is_literal_type(db, type_id)
}
