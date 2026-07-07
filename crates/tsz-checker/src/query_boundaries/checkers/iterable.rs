use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{FunctionShape, ObjectShapeId, TypeId};

pub(crate) use super::super::common::{
    call_signatures_for_type, is_string_type, is_this_type, union_members as union_members_for_type,
};
pub(crate) use tsz_solver::type_queries::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind,
};

pub(crate) enum IterableProtocolMethodStatus {
    Valid,
    Invalid,
    NeedsPropertyAccess,
}

pub(crate) enum NumericIndexSignatureFact {
    Present,
    Recurse(TypeId),
    Union(Vec<TypeId>),
    Absent,
}

pub(crate) enum IteratorReturnPropertyStatus {
    Absent,
    Valid,
    NeedsResolvedCallability(TypeId),
}

pub(crate) fn classify_full_iterable_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> FullIterableTypeKind {
    tsz_solver::type_queries::classify_full_iterable_type(db, type_id)
}

pub(crate) fn classify_async_iterable_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AsyncIterableTypeKind {
    tsz_solver::type_queries::classify_async_iterable_type(db, type_id)
}

pub(crate) fn async_iterable_protocol_lookup_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::async_iterable_protocol_lookup_type(db, type_id)
}

pub(crate) fn classify_for_of_element_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ForOfElementKind {
    tsz_solver::type_queries::classify_for_of_element_type(db, type_id)
}

pub(crate) fn function_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn iterator_method_status(
    db: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
) -> IterableProtocolMethodStatus {
    let shape = db.object_shape(shape_id);
    for prop in &shape.properties {
        let prop_name = db.resolve_atom_ref(prop.name);
        if prop_name.as_ref() != "[Symbol.iterator]" {
            continue;
        }
        if prop.optional {
            return IterableProtocolMethodStatus::Invalid;
        }
        if prop.is_method {
            return callable_accepts_no_required_args_status(db, prop.type_id);
        }
        if prop.type_id == TypeId::ANY || callable_accepts_no_required_args(db, prop.type_id) {
            return IterableProtocolMethodStatus::Valid;
        }
        return IterableProtocolMethodStatus::NeedsPropertyAccess;
    }

    IterableProtocolMethodStatus::NeedsPropertyAccess
}

pub(crate) fn async_iterator_method_status(
    db: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
) -> IterableProtocolMethodStatus {
    let shape = db.object_shape(shape_id);
    for prop in &shape.properties {
        let prop_name = db.resolve_atom_ref(prop.name);
        if prop_name.as_ref() != "[Symbol.asyncIterator]" {
            continue;
        }
        if prop.optional {
            return IterableProtocolMethodStatus::Invalid;
        }
        return callable_accepts_no_required_args_status(db, prop.type_id);
    }

    IterableProtocolMethodStatus::NeedsPropertyAccess
}

pub(crate) fn callable_accepts_no_required_args(
    db: &dyn TypeDatabase,
    callable_type: TypeId,
) -> bool {
    matches!(
        callable_accepts_no_required_args_status(db, callable_type),
        IterableProtocolMethodStatus::Valid
    )
}

fn callable_accepts_no_required_args_status(
    db: &dyn TypeDatabase,
    callable_type: TypeId,
) -> IterableProtocolMethodStatus {
    if callable_type == TypeId::ANY
        || callable_type == TypeId::UNKNOWN
        || callable_type == TypeId::ERROR
    {
        return IterableProtocolMethodStatus::Valid;
    }

    if let Some(sig) = function_shape_for_type(db, callable_type) {
        if sig.params.iter().all(|p| p.optional || p.rest) {
            return IterableProtocolMethodStatus::Valid;
        }
        return IterableProtocolMethodStatus::Invalid;
    }

    if let Some(call_signatures) = call_signatures_for_type(db, callable_type) {
        if call_signatures
            .iter()
            .any(|sig| sig.params.iter().all(|p| p.optional || p.rest))
        {
            return IterableProtocolMethodStatus::Valid;
        }
        return IterableProtocolMethodStatus::Invalid;
    }

    IterableProtocolMethodStatus::Invalid
}

pub(crate) fn callable_return_type(db: &dyn TypeDatabase, fn_type: TypeId) -> TypeId {
    if fn_type == TypeId::ANY {
        return TypeId::ANY;
    }
    if let Some(sig) = function_shape_for_type(db, fn_type) {
        return sig.return_type;
    }
    if let Some(call_signatures) = call_signatures_for_type(db, fn_type) {
        return call_signatures
            .first()
            .map_or(TypeId::ANY, |sig| sig.return_type);
    }
    TypeId::ANY
}

pub(crate) fn first_callable_param_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if let Some(sigs) = call_signatures_for_type(db, type_id) {
        return sigs.first()?.params.first().map(|p| p.type_id);
    }
    if let Some(shape) = function_shape_for_type(db, type_id) {
        return shape.params.first().map(|p| p.type_id);
    }
    let members = union_members_for_type(db, type_id)?;
    for member in &members {
        if let Some(sigs) = call_signatures_for_type(db, *member)
            && let Some(first) = sigs.first()
        {
            return first.params.first().map(|p| p.type_id);
        }
        if let Some(shape) = function_shape_for_type(db, *member) {
            return shape.params.first().map(|p| p.type_id);
        }
    }
    None
}

pub(crate) fn promise_like_awaited_type(db: &dyn QueryDatabase, type_id: TypeId) -> Option<TypeId> {
    let then_type = crate::query_boundaries::property_access::resolve_property_access(
        db,
        type_id,
        db.intern_string("then"),
    )
    .success_type()?;
    let sigs = call_signatures_for_type(db, then_type)?;
    let first_sig = sigs.first()?;
    let onfulfilled_type = first_sig.params.first().map(|p| p.type_id)?;
    first_callable_param_type(db, onfulfilled_type)
}

pub(crate) fn iterator_info_yield_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    is_async: bool,
) -> Option<TypeId> {
    tsz_solver::operations::get_iterator_info(db, type_id, is_async).map(|info| info.yield_type)
}

pub(crate) fn iterator_result_value_types(
    db: &dyn QueryDatabase,
    result_type: TypeId,
) -> (TypeId, TypeId) {
    tsz_solver::operations::extract_iterator_result_value_types(db, result_type)
}

pub(crate) fn evaluated_iterator_result_value_types(
    db: &dyn QueryDatabase,
    result_type: TypeId,
) -> (TypeId, TypeId) {
    iterator_result_value_types(db, db.evaluate_type(result_type))
}

pub(crate) fn evaluated_iterator_result_type(
    db: &dyn QueryDatabase,
    result_type: TypeId,
) -> TypeId {
    db.evaluate_type(result_type)
}

pub(crate) fn tuple_element_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::union_or_single(db, members)
}

pub(crate) fn union_element_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn intersection_element_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn type_has_next_method(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
        return true;
    }

    match classify_full_iterable_type(db, type_id) {
        FullIterableTypeKind::Object(shape_id) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                let name = db.resolve_atom_ref(prop.name);
                name.as_ref() == "next"
            })
        }
        FullIterableTypeKind::Union(members) => members
            .iter()
            .all(|&member| type_has_next_method(db, member)),
        FullIterableTypeKind::Intersection(members) => members
            .iter()
            .any(|&member| type_has_next_method(db, member)),
        FullIterableTypeKind::Readonly(inner) => type_has_next_method(db, inner),
        FullIterableTypeKind::Application { .. }
        | FullIterableTypeKind::TypeParameter { .. }
        | FullIterableTypeKind::ComplexType
        | FullIterableTypeKind::Array(_)
        | FullIterableTypeKind::Tuple(_)
        | FullIterableTypeKind::StringLiteral(_) => true,
        FullIterableTypeKind::FunctionOrCallable | FullIterableTypeKind::NotIterable => false,
    }
}

pub(crate) fn numeric_index_signature_fact(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> NumericIndexSignatureFact {
    match classify_full_iterable_type(db, type_id) {
        FullIterableTypeKind::Object(shape_id) => {
            let shape = db.object_shape(shape_id);
            if shape.number_index.is_some() {
                NumericIndexSignatureFact::Present
            } else {
                NumericIndexSignatureFact::Absent
            }
        }
        FullIterableTypeKind::Application { base } | FullIterableTypeKind::Readonly(base) => {
            NumericIndexSignatureFact::Recurse(base)
        }
        FullIterableTypeKind::Union(members) => NumericIndexSignatureFact::Union(members),
        _ => NumericIndexSignatureFact::Absent,
    }
}

pub(crate) fn iterator_return_property_status(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> IteratorReturnPropertyStatus {
    let FullIterableTypeKind::Object(shape_id) = classify_full_iterable_type(db, type_id) else {
        return IteratorReturnPropertyStatus::Absent;
    };

    let shape = db.object_shape(shape_id);
    for prop in &shape.properties {
        let name = db.resolve_atom_ref(prop.name);
        if name.as_ref() != "return" {
            continue;
        }
        if prop.is_method || prop.optional || callable_type_is_callable(db, prop.type_id) {
            return IteratorReturnPropertyStatus::Valid;
        }
        return IteratorReturnPropertyStatus::NeedsResolvedCallability(prop.type_id);
    }

    IteratorReturnPropertyStatus::Absent
}

pub(crate) fn callable_type_is_callable(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    function_shape_for_type(db, type_id).is_some()
        || call_signatures_for_type(db, type_id).is_some_and(|sigs| !sigs.is_empty())
}

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_type(db, type_id)
}

pub(crate) fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_tuple_type(db, type_id)
}

pub(crate) fn is_string_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        tsz_solver::type_queries::classify_for_literal_value(db, type_id),
        tsz_solver::type_queries::LiteralValueKind::String(_)
    )
}
