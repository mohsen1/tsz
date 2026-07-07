//! Binding/destructuring pattern construction boundary.
//!
//! Binding-pattern callers own AST traversal, property-name extraction,
//! contextual typing policy, relation checks, and diagnostic anchors. This
//! module owns the solver shapes synthesized from those binding facts.

use tsz_binder::SymbolId;
use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{ObjectFlags, ObjectShape, PropertyInfo, TupleElement, TypeId};

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

pub(crate) fn binding_rest_omit_application(
    db: &dyn TypeDatabase,
    omit_type: TypeId,
    parent_type: TypeId,
    string_keys: &[String],
    computed_key_type_ids: &[TypeId],
) -> TypeId {
    let mut key_args: Vec<TypeId> = string_keys
        .iter()
        .map(|name| db.literal_string(name))
        .collect();
    key_args.extend_from_slice(computed_key_type_ids);
    let key_arg = if key_args.len() == 1 {
        key_args[0]
    } else {
        db.union(key_args)
    };
    db.application(omit_type, vec![parent_type, key_arg])
}

pub(crate) fn binding_rest_indexed_object_type(
    db: &dyn TypeDatabase,
    mut shape: ObjectShape,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    shape.properties = properties;
    shape.symbol = None;
    db.object_with_index(shape)
}

pub(crate) fn binding_rest_object_with_flags(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    flags: ObjectFlags,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, flags, symbol)
}

pub(crate) fn binding_rest_array_type(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.array(element_type)
}
