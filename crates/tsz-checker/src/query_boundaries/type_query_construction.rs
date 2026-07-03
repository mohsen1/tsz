//! Const value/type-query construction boundary.
//!
//! Type-query callers own syntax guards, symbol lookup, declaration checks,
//! name matching, and fallback policy. This module owns the solver surfaces
//! synthesized from accepted const literal facts.

use tsz_common::{Atom, Visibility};
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{ObjectShape, PropertyInfo, TupleElement, TypeId};

pub(crate) fn const_query_literal_string_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    db.literal_string(value)
}

pub(crate) fn const_query_literal_number_type(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.literal_number(value)
}

pub(crate) fn const_query_literal_boolean_type(db: &dyn TypeDatabase, value: bool) -> TypeId {
    db.literal_boolean(value)
}

pub(crate) const fn const_query_readonly_property(
    name: Atom,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: true,
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

pub(crate) fn const_query_object_literal_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties,
        ..ObjectShape::default()
    })
}

pub(crate) fn const_query_array_to_enum_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) const fn const_query_tuple_element(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

pub(crate) fn const_query_tuple_type(db: &dyn TypeDatabase, elements: Vec<TupleElement>) -> TypeId {
    db.tuple(elements)
}
