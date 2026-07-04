//! Array-literal surface construction boundary.
//!
//! Array-literal computation owns AST traversal, contextual typing, spread
//! policy, tuple forcing, excess-property checks, and diagnostics. This module
//! owns the solver tuple/array/union surfaces those decisions produce.

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{TupleElement, TypeId};

pub(crate) const fn tuple_element(type_id: TypeId, optional: bool, rest: bool) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional,
        rest,
    }
}

pub(crate) fn tuple_type(db: &dyn TypeDatabase, elements: Vec<TupleElement>) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn tuple_from_element_types(db: &dyn TypeDatabase, element_types: &[TypeId]) -> TypeId {
    let elements = element_types
        .iter()
        .copied()
        .map(|type_id| tuple_element(type_id, false, false))
        .collect();
    db.tuple(elements)
}

pub(crate) fn empty_tuple_type(db: &dyn TypeDatabase) -> TypeId {
    db.tuple(Vec::new())
}

pub(crate) fn array_type(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.array(element_type)
}

pub(crate) fn any_array_type(db: &dyn TypeDatabase) -> TypeId {
    db.array(TypeId::ANY)
}

pub(crate) fn never_array_type(db: &dyn TypeDatabase) -> TypeId {
    db.array(TypeId::NEVER)
}

pub(crate) fn error_array_type(db: &dyn TypeDatabase) -> TypeId {
    db.array(TypeId::ERROR)
}

pub(crate) fn element_union(db: &dyn TypeDatabase, element_types: Vec<TypeId>) -> TypeId {
    db.union(element_types)
}
