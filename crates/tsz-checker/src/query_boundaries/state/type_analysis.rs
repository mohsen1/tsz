pub(crate) use super::super::common::{call_signatures_for_type, callable_shape_for_type};

use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

pub(crate) const fn namespace_export_property(
    name: Atom,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn namespace_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn namespace_export_equals_intersection(
    db: &dyn TypeDatabase,
    export_equals_type: TypeId,
    namespace_type: TypeId,
) -> TypeId {
    db.intersection2(export_equals_type, namespace_type)
}

#[cfg(test)]
#[path = "../../../tests/state_type_analysis.rs"]
mod tests;
