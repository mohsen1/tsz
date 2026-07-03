//! Binding/destructuring pattern construction boundary.
//!
//! Binding-pattern callers own AST traversal, property-name extraction,
//! contextual typing policy, relation checks, and diagnostic anchors. This
//! module owns the solver shapes synthesized from those binding facts.

use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{PropertyInfo, TupleElement, TypeId};

pub(crate) fn binding_pattern_initializer_union_type(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    initializer_type: TypeId,
) -> TypeId {
    db.union2(element_type, initializer_type)
}

pub(crate) fn binding_pattern_member_union_type(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.union(members)
}

pub(crate) const fn binding_pattern_property(
    name: Atom,
    type_id: TypeId,
    optional: bool,
) -> PropertyInfo {
    let mut property = PropertyInfo::new(name, type_id);
    property.optional = optional;
    property
}

pub(crate) const fn binding_pattern_tuple_element(
    type_id: TypeId,
    optional: bool,
    rest: bool,
) -> TupleElement {
    TupleElement {
        type_id,
        optional,
        rest,
        name: None,
    }
}

pub(crate) fn binding_pattern_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn binding_pattern_tuple_type(
    db: &dyn TypeDatabase,
    elements: Vec<TupleElement>,
) -> TypeId {
    db.tuple(elements)
}
