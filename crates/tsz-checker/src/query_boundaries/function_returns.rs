//! Solver query helpers used by return-type inference.
//!
//! This module keeps return-type inference callers away from the broad
//! `common` quarantine while #8225 splits that surface into narrower request
//! boundaries.

use super::common::{self, TypeDatabase};
use tsz_solver::{DefId, TypeId};

pub(crate) use super::common::{
    application_info, array_element_type, contains_free_type_parameters, contains_infer_types,
    contains_type_parameters, index_access_types, lazy_def_id, mapped_type_info, type_application,
    type_param_info, union_members,
};

pub(crate) fn function_return_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

/// Combine the `TNext`s an unannotated generator's `yield*` delegations
/// declared. `tsc` uses `getIntersectionType` here, not a union: a value sent
/// into the outer generator can be forwarded to any of its delegates, so it
/// must satisfy all of them.
pub(crate) fn function_return_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    common::intersection_or_single(db, members)
}

pub(crate) fn function_return_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn function_return_application(
    db: &dyn TypeDatabase,
    base: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(base, args)
}

pub(crate) fn array_literal_return_context_has_usable_tuple_slots(
    db: &dyn TypeDatabase,
    return_context: TypeId,
) -> bool {
    common::tuple_elements(db, return_context).is_some_and(|elements| {
        !elements.is_empty()
            && elements.iter().all(|element| {
                contextual_tuple_slot_has_no_unresolved_params(db, element.type_id, 0)
            })
    })
}

fn contextual_tuple_slot_has_no_unresolved_params(
    db: &dyn TypeDatabase,
    mut type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 16 {
        return false;
    }

    while let Some(inner) = common::unwrap_readonly_or_noinfer(db, type_id) {
        type_id = inner;
    }

    if common::is_type_parameter_like(db, type_id) || common::is_this_type(db, type_id) {
        return false;
    }

    if let Some(elements) = common::tuple_elements(db, type_id) {
        return elements.iter().all(|element| {
            contextual_tuple_slot_has_no_unresolved_params(db, element.type_id, depth + 1)
        });
    }

    if let Some(element_type) = common::array_element_type(db, type_id) {
        return contextual_tuple_slot_has_no_unresolved_params(db, element_type, depth + 1);
    }

    if let Some(members) = common::union_members(db, type_id) {
        return members
            .iter()
            .all(|member| contextual_tuple_slot_has_no_unresolved_params(db, *member, depth + 1));
    }

    if let Some(members) = common::intersection_members(db, type_id) {
        return members
            .iter()
            .all(|member| contextual_tuple_slot_has_no_unresolved_params(db, *member, depth + 1));
    }

    if let Some((_base, args)) = common::application_info(db, type_id) {
        return args
            .iter()
            .all(|&arg| contextual_tuple_slot_has_no_unresolved_params(db, arg, depth + 1));
    }

    if let Some(shape) = common::object_shape_for_type(db, type_id)
        && shape.symbol.is_none()
    {
        let properties_clear = shape.properties.iter().all(|prop| {
            contextual_tuple_slot_has_no_unresolved_params(db, prop.type_id, depth + 1)
                && (prop.write_type == TypeId::NONE
                    || prop.write_type == prop.type_id
                    || contextual_tuple_slot_has_no_unresolved_params(
                        db,
                        prop.write_type,
                        depth + 1,
                    ))
        });
        if !properties_clear {
            return false;
        }

        let index_types_clear = [
            &shape.string_index,
            &shape.number_index,
            &shape.symbol_index,
        ]
        .into_iter()
        .flatten()
        .all(|index| {
            contextual_tuple_slot_has_no_unresolved_params(db, index.value_type, depth + 1)
        });
        if !index_types_clear {
            return false;
        }
    }

    true
}
