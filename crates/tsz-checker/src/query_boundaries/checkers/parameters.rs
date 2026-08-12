//! Parameter and contextual-callable construction boundary.
//!
//! Parameter checking and contextual typing own AST position, diagnostics, and
//! fallback policy. This module owns the solver records and synthesized types
//! those paths need: optional-parameter `undefined` unions, rest-array targets,
//! contextual rest tuples, merged contextual callables, and normalized
//! callable re-interning.

use std::sync::Arc;

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{FunctionShape, ParamInfo, TupleElement, TypeId};

pub(crate) fn optional_parameter_type_with_undefined(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    db.union2(type_id, TypeId::UNDEFINED)
}

pub(crate) fn readonly_any_array_type(db: &dyn TypeDatabase) -> TypeId {
    let any_array = db.array(TypeId::ANY);
    db.readonly_type(any_array)
}

pub(crate) fn tuple_type_from_elements(
    db: &dyn TypeDatabase,
    elements: Vec<TupleElement>,
) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn tuple_type_from_element_slice(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
) -> TypeId {
    tuple_type_from_elements(db, elements.to_vec())
}

pub(crate) fn contextual_rest_tuple_from_signature_tail(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    index: usize,
    rest_start: usize,
    rest_param: &ParamInfo,
) -> TypeId {
    let mut elements = params[index..rest_start]
        .iter()
        .map(param_tuple_element)
        .collect::<Vec<_>>();
    elements.push(TupleElement {
        type_id: rest_param.type_id,
        name: rest_param.name,
        optional: false,
        rest: true,
    });
    tuple_type_from_elements(db, elements)
}

pub(crate) fn union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn union_preserve_members_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union_preserve_members(members)
}

pub(crate) fn intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn function_type_from_shape(db: &dyn TypeDatabase, shape: FunctionShape) -> TypeId {
    crate::query_boundaries::construct_signatures::function_type_from_shape(db, shape)
}

pub(crate) fn merge_callable_contextual_types(
    db: &dyn TypeDatabase,
    types: &[TypeId],
) -> Option<TypeId> {
    let mut shapes: Vec<Arc<FunctionShape>> = Vec::new();
    for &ty in types {
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, ty) {
            shapes.push(shape);
            continue;
        }

        let members = crate::query_boundaries::common::union_members(db, ty)?;
        let mut found_any = false;
        for &member in &members {
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(db, member)
            {
                shapes.push(shape);
                found_any = true;
            }
        }
        if !found_any {
            return None;
        }
    }

    if shapes.len() < 2 {
        return None;
    }

    let first_non_rest_count = shapes[0].params.iter().filter(|p| !p.rest).count();
    if !shapes
        .iter()
        .all(|s| s.params.iter().filter(|p| !p.rest).count() == first_non_rest_count)
    {
        return None;
    }

    let param_count = shapes[0].params.len();
    if !shapes.iter().all(|s| s.params.len() == param_count) {
        return None;
    }

    let mut combined_params = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let param_types = shapes.iter().map(|s| s.params[i].type_id).collect();
        combined_params.push(ParamInfo {
            suppress_display_optional: false,
            name: shapes[0].params[i].name,
            type_id: union_type(db, param_types),
            optional: shapes.iter().all(|s| s.params[i].optional),
            rest: shapes[0].params[i].rest,
        });
    }

    let return_types = shapes.iter().map(|s| s.return_type).collect();
    Some(function_type_from_shape(
        db,
        FunctionShape {
            params: combined_params,
            return_type: union_type(db, return_types),
            this_type: None,
            type_params: vec![],
            type_predicate: None,
            is_constructor: false,
            is_method: shapes[0].is_method,
        },
    ))
}

const fn param_tuple_element(param: &ParamInfo) -> TupleElement {
    TupleElement {
        type_id: param.type_id,
        name: param.name,
        optional: param.optional,
        rest: false,
    }
}
