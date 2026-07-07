use tsz_binder::SymbolId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{CallSignature, FunctionShape, ObjectShape, TypeId};

pub(crate) use super::super::common::{
    application_info, intersection_members, lazy_def_id, union_members,
};
pub(crate) use tsz_solver::type_queries::PromiseTypeKind;

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn function_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    tsz_solver::type_queries::classify_promise_type(db, type_id)
}

pub(crate) fn promise_object_symbol_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolId> {
    let PromiseTypeKind::Object(shape_id) = classify_promise_type(db, type_id) else {
        return None;
    };
    db.object_shape(shape_id).symbol
}

pub(crate) fn type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

pub(crate) fn promise_application_type(
    db: &dyn TypeDatabase,
    promise_base: TypeId,
    type_arg: TypeId,
) -> TypeId {
    db.application(promise_base, vec![type_arg])
}

pub(crate) fn await_contextual_operand_type(
    db: &dyn TypeDatabase,
    contextual: TypeId,
    promise_like: TypeId,
    promise: Option<TypeId>,
) -> TypeId {
    let mut members = vec![contextual, promise_like];
    if let Some(promise) = promise {
        members.push(promise);
    }
    db.union(members)
}

pub(crate) fn awaited_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn awaited_assignability_array_with_mapped_element(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut map_element: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let element = super::super::common::array_element_type(db, type_id)?;
    let mapped = map_element(element);
    (mapped != element).then(|| db.array(mapped))
}

pub(crate) fn awaited_assignability_union_has_raw_awaited_distribution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut evaluate_for_assignability: impl FnMut(TypeId) -> TypeId,
) -> bool {
    super::super::common::union_members(db, type_id).is_some_and(|members| {
        members.iter().copied().any(|member| {
            raw_awaited_conditional_for_assignability(db, member, |ty| {
                evaluate_for_assignability(ty)
            })
            .is_some()
        })
    })
}

pub(crate) fn awaited_assignability_union_with_mapped_members_if_changed(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut map_member: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let members = super::super::common::union_members(db, type_id)?;
    let mut changed = false;
    let mapped_members: Vec<_> = members
        .into_iter()
        .map(|member| {
            let mapped = map_member(member);
            changed |= mapped != member;
            mapped
        })
        .collect();
    changed.then(|| db.union(mapped_members))
}

pub(crate) fn awaited_assignability_union_with_mapped_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    map_member: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let members = super::super::common::union_members(db, type_id)?;
    Some(db.union(members.into_iter().map(map_member).collect()))
}

pub(crate) fn awaited_assignability_tuple_with_mapped_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut map_element: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let elements = super::super::common::tuple_elements(db, type_id)?;
    let mut changed = false;
    let mapped_elements: Vec<_> = elements
        .into_iter()
        .map(|mut element| {
            let mapped = map_element(element.type_id);
            changed |= mapped != element.type_id;
            element.type_id = mapped;
            element
        })
        .collect();
    changed.then(|| db.tuple(mapped_elements))
}

pub(crate) fn awaited_assignability_application_with_mapped_args(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut map_arg: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let (base, args) = super::super::common::application_info(db, type_id)?;
    let mut changed = false;
    let mapped_args: Vec<_> = args
        .into_iter()
        .map(|arg| {
            let mapped = map_arg(arg);
            changed |= mapped != arg;
            mapped
        })
        .collect();
    changed.then(|| db.application(base, mapped_args))
}

pub(crate) fn awaited_assignability_object_with_mapped_slots(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut map_type: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let shape_id = super::super::common::object_shape_id(db, type_id)?;
    let shape = db.object_shape(shape_id);
    let mut changed = false;
    let evaluated_properties = shape
        .properties
        .iter()
        .map(|prop| {
            let evaluated_type = map_type(prop.type_id);
            let evaluated_write = map_type(prop.write_type);
            changed |= evaluated_type != prop.type_id || evaluated_write != prop.write_type;
            tsz_solver::PropertyInfo {
                type_id: evaluated_type,
                write_type: evaluated_write,
                ..*prop
            }
        })
        .collect();
    let evaluated_string_index = shape.string_index.map(|mut index| {
        let evaluated = map_type(index.value_type);
        changed |= evaluated != index.value_type;
        index.value_type = evaluated;
        index
    });
    let evaluated_number_index = shape.number_index.map(|mut index| {
        let evaluated = map_type(index.value_type);
        changed |= evaluated != index.value_type;
        index.value_type = evaluated;
        index
    });

    changed.then(|| {
        db.object_with_index(ObjectShape {
            properties: evaluated_properties,
            string_index: evaluated_string_index,
            number_index: evaluated_number_index,
            ..(*shape).clone()
        })
    })
}

pub(crate) struct RawAwaitedConditionalForAssignability {
    pub(crate) check_type: TypeId,
    pub(crate) false_type: TypeId,
}

pub(crate) fn raw_awaited_conditional_for_assignability(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut evaluate_for_assignability: impl FnMut(TypeId) -> TypeId,
) -> Option<RawAwaitedConditionalForAssignability> {
    let cond_id = super::super::common::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);
    if cond.false_type != cond.check_type {
        return None;
    }

    let extends_type = evaluate_for_assignability(cond.extends_type);
    awaited_assignability_type_has_then_property(db, extends_type).then_some(
        RawAwaitedConditionalForAssignability {
            check_type: cond.check_type,
            false_type: cond.false_type,
        },
    )
}

pub(crate) fn awaited_assignability_type_has_then_property(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    super::super::common::has_property_by_str(db, type_id, "then")
}

pub(crate) fn awaited_intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn thenable_callback_value_union(
    db: &dyn TypeDatabase,
    values: Vec<TypeId>,
) -> Option<TypeId> {
    match values.as_slice() {
        [] => None,
        [only] => Some(*only),
        _ => Some(db.union(values)),
    }
}

pub(crate) fn async_return_body_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}
