//! Assignability-owned solver construction helpers.
//!
//! Callers in the checker decide which transient comparison surface is needed;
//! this module owns the raw solver construction used by those relation-adjacent
//! preparation paths.

use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{FunctionShape, PropertyInfo, TupleElement, TypeId};

pub(crate) const fn assignability_namespace_export_property(
    name: Atom,
    type_id: TypeId,
) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) const fn assignability_contextual_pattern_property(
    name: Atom,
    type_id: TypeId,
) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) const fn assignability_tuple_element(type_id: TypeId, rest: bool) -> TupleElement {
    TupleElement {
        type_id,
        optional: false,
        rest,
        name: None,
    }
}

pub(crate) const fn assignability_resolved_tuple_element(
    element: &TupleElement,
    type_id: TypeId,
) -> TupleElement {
    TupleElement {
        type_id,
        name: element.name,
        optional: element.optional,
        rest: element.rest,
    }
}

pub(crate) fn assignability_resolved_property(
    property: &PropertyInfo,
    type_id: TypeId,
    write_type: TypeId,
) -> PropertyInfo {
    PropertyInfo {
        type_id,
        write_type,
        ..property.clone()
    }
}

pub(crate) fn assignability_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn assignability_empty_object_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn assignability_readonly_type(db: &dyn TypeDatabase, inner: TypeId) -> TypeId {
    db.readonly_type(inner)
}

pub(crate) fn assignability_noinfer_type(db: &dyn TypeDatabase, inner: TypeId) -> TypeId {
    db.no_infer(inner)
}

pub(crate) fn assignability_array_type(db: &dyn TypeDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) fn assignability_tuple_type(
    db: &dyn TypeDatabase,
    elements: Vec<TupleElement>,
) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn assignability_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn assignability_intersection_type(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.intersection(members)
}

pub(crate) fn assignability_union_preserve_members(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.union_preserve_members(members)
}

pub(crate) fn assignability_function_with_return_type(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    return_type: TypeId,
) -> TypeId {
    db.function(FunctionShape {
        return_type,
        ..shape.clone()
    })
}

pub(crate) fn assignability_index_access_type(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    db.index_access(object_type, index_type)
}
