use crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id;
use tsz_binder::SymbolId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{CallSignature, FunctionShape, ObjectShape, TypeId};

pub(crate) use super::super::common::{
    application_info, intersection_members, lazy_def_id, union_members,
};
use tsz_solver::type_queries::PromiseTypeKind;

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

fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    tsz_solver::type_queries::classify_promise_type(db, type_id)
}

pub(crate) fn promise_object_symbol_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolId> {
    let PromiseTypeKind::Object(shape_id) = classify_promise_type(db, type_id) else {
        return None;
    };
    db.object_shape(shape_id).symbol
}

pub(crate) struct PromiseApplicationParts {
    base: TypeId,
    args: Vec<TypeId>,
}

impl PromiseApplicationParts {
    pub(crate) const fn base(&self) -> TypeId {
        self.base
    }

    pub(crate) fn args(&self) -> &[TypeId] {
        &self.args
    }

    pub(crate) fn first_arg(&self) -> Option<TypeId> {
        self.args.first().copied()
    }

    pub(crate) fn first_arg_or_unknown(&self) -> TypeId {
        self.first_arg().unwrap_or(TypeId::UNKNOWN)
    }
}

pub(crate) fn promise_application_parts(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<PromiseApplicationParts> {
    let PromiseTypeKind::Application { base, args, .. } = classify_promise_type(db, type_id) else {
        return None;
    };
    Some(PromiseApplicationParts { base, args })
}

pub(crate) fn promise_application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    promise_application_parts(db, type_id).map(|parts| parts.base)
}

pub(crate) fn promise_application_base_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::DefId> {
    let base = promise_application_base(db, type_id)?;
    promise_base_lazy_def_id(db, base)
}

pub(crate) fn promise_base_symbol_id(
    db: &dyn TypeDatabase,
    base: TypeId,
    mut def_to_symbol_id: impl FnMut(tsz_solver::DefId) -> Option<SymbolId>,
) -> Option<SymbolId> {
    match classify_promise_type(db, base) {
        PromiseTypeKind::Lazy(def_id) => def_to_symbol_id(def_id),
        PromiseTypeKind::TypeQuery(sym_ref) => Some(symbol_ref_to_symbol_id(sym_ref)),
        _ => None,
    }
}

pub(crate) fn promise_base_lazy_def_id(
    db: &dyn TypeDatabase,
    base: TypeId,
) -> Option<tsz_solver::DefId> {
    match classify_promise_type(db, base) {
        PromiseTypeKind::Lazy(def_id) => Some(def_id),
        _ => None,
    }
}

pub(crate) fn promise_base_matches(
    db: &dyn TypeDatabase,
    base: TypeId,
    lazy_matches: impl FnMut(tsz_solver::DefId) -> bool,
    symbol_matches: impl FnMut(SymbolId) -> bool,
) -> bool {
    promise_reference_matches(db, base, lazy_matches, symbol_matches)
}

pub(crate) fn promise_reference_matches(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut lazy_matches: impl FnMut(tsz_solver::DefId) -> bool,
    mut symbol_matches: impl FnMut(SymbolId) -> bool,
) -> bool {
    promise_reference_matches_inner(db, type_id, &mut lazy_matches, &mut symbol_matches)
}

fn promise_reference_matches_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    lazy_matches: &mut impl FnMut(tsz_solver::DefId) -> bool,
    symbol_matches: &mut impl FnMut(SymbolId) -> bool,
) -> bool {
    match classify_promise_type(db, type_id) {
        PromiseTypeKind::Lazy(def_id) => lazy_matches(def_id),
        PromiseTypeKind::TypeQuery(sym_ref) => symbol_matches(symbol_ref_to_symbol_id(sym_ref)),
        _ => false,
    }
}

pub(crate) fn promise_type_matches_through_applications(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut lazy_matches: impl FnMut(tsz_solver::DefId) -> bool,
    mut symbol_matches: impl FnMut(SymbolId) -> bool,
    object_matches: bool,
) -> bool {
    fn matches_inner(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        lazy_matches: &mut impl FnMut(tsz_solver::DefId) -> bool,
        symbol_matches: &mut impl FnMut(SymbolId) -> bool,
        object_matches: bool,
    ) -> bool {
        match classify_promise_type(db, type_id) {
            PromiseTypeKind::Application { base, .. } => {
                matches_inner(db, base, lazy_matches, symbol_matches, object_matches)
            }
            PromiseTypeKind::Object(_) => object_matches,
            PromiseTypeKind::Union(_) | PromiseTypeKind::NotPromise => false,
            PromiseTypeKind::Lazy(_) | PromiseTypeKind::TypeQuery(_) => {
                promise_reference_matches_inner(db, type_id, lazy_matches, symbol_matches)
            }
        }
    }

    matches_inner(
        db,
        type_id,
        &mut lazy_matches,
        &mut symbol_matches,
        object_matches,
    )
}

pub(crate) fn promise_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::DefId> {
    match classify_promise_type(db, type_id) {
        PromiseTypeKind::Lazy(def_id) => Some(def_id),
        _ => None,
    }
}

pub(crate) fn promise_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    match classify_promise_type(db, type_id) {
        PromiseTypeKind::Union(members) => Some(members),
        _ => None,
    }
}

pub(crate) fn promise_type_is_object(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_promise_type(db, type_id),
        PromiseTypeKind::Object(_)
    )
}

pub(crate) struct ThenableSignatureSurface {
    this_type: Option<TypeId>,
    onfulfilled_type: Option<TypeId>,
}

impl ThenableSignatureSurface {
    pub(crate) const fn this_type(&self) -> Option<TypeId> {
        self.this_type
    }

    pub(crate) const fn onfulfilled_type(&self) -> Option<TypeId> {
        self.onfulfilled_type
    }
}

pub(crate) fn thenable_property_type(
    db: &dyn QueryDatabase,
    receiver_type: TypeId,
) -> Option<TypeId> {
    crate::query_boundaries::property_access::resolve_property_access(
        db,
        receiver_type,
        db.intern_string("then"),
    )
    .success_type()
}

pub(crate) fn thenable_signature_surfaces(
    db: &dyn TypeDatabase,
    then_type: TypeId,
) -> Vec<ThenableSignatureSurface> {
    let mut sigs = call_signatures_for_type(db, then_type).unwrap_or_default();
    if sigs.is_empty()
        && let Some(shape) = function_shape_for_type(db, then_type)
    {
        sigs.push(CallSignature {
            type_params: shape.type_params.clone(),
            params: shape.params.clone(),
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_method: shape.is_method,
            declaration_group: 0,
        });
    }

    sigs.into_iter()
        .map(|sig| ThenableSignatureSurface {
            this_type: sig.this_type,
            onfulfilled_type: sig.params.first().map(|param| param.type_id),
        })
        .collect()
}

/// Whether every constituent of a type is a primitive or `never`, mirroring
/// `tsc`'s `allTypesAssignableToKind(getBaseConstraintOrType(t), Primitive | Never)`
/// guard in `isThenableType`/`getPromisedTypeOfPromiseEx`.
///
/// A primitive never adopts a `then` member, so `string & { then(...) }` is not
/// a thenable however the intersection's property lookup resolves — `tsc`
/// reaches that verdict through `isTypeAssignableTo(source, stringType)`, which
/// the structural intersection arm below stands in for.
pub(crate) fn type_is_primitive_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    fn is_primitive_leaf(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        type_id == TypeId::NEVER || tsz_solver::visitor::is_primitive_type(db, type_id)
    }

    fn walk(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if let Some(members) = super::super::common::union_members(db, type_id) {
            return members.iter().all(|&member| walk(db, member));
        }
        if let Some(members) = intersection_members(db, type_id) {
            return members.iter().any(|&member| walk(db, member));
        }
        is_primitive_leaf(db, type_id)
    }

    // `getBaseConstraintOrType` first: a type parameter constrained to a
    // primitive is in the primitive domain regardless of any `then` its
    // constraint also carries, so `T extends number & { then(): void }` is not
    // a thenable even though `then` resolves through the constraint.
    walk(
        db,
        tsz_solver::type_queries::get_base_constraint_of_type(db, type_id),
    ) || walk(db, type_id)
}

/// Strip `null`/`undefined` from a type, mirroring `tsc`'s
/// `getTypeWithFacts(t, TypeFacts.NEUndefinedOrNull)`.
pub(crate) fn non_nullish_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::narrowing::remove_nullish(db, type_id)
}

/// Whether a `then` signature's `onfulfilled` parameter is itself callable.
///
/// `tsc`'s `getPromisedTypeOfPromiseEx` asks
/// `getSignaturesOfType(onfulfilledParameterType, Call)` and treats an empty
/// result as "not a valid promise" — independently of whether a *payload* can
/// be recovered from that callback, because a zero-parameter callback still
/// resolves (to `never`, via `getTypeOfFirstParameterOfSignature`'s fallback).
/// `thenable_callback_value_type` cannot answer this: it returns `None` both
/// for "not callable" and for "callable but declares no parameters".
///
/// The member scan mirrors `thenable_callback_value_type`'s own union walk so
/// the two queries agree on which surfaces count as a callback.
pub(crate) fn thenable_callback_is_callable(db: &dyn TypeDatabase, callback_type: TypeId) -> bool {
    fn is_callable_surface(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        call_signatures_for_type(db, type_id).is_some_and(|sigs| !sigs.is_empty())
            || function_shape_for_type(db, type_id).is_some()
    }

    if is_callable_surface(db, callback_type) {
        return true;
    }
    super::super::common::union_members(db, callback_type).is_some_and(|members| {
        members
            .iter()
            .any(|&member| is_callable_surface(db, member))
    })
}

pub(crate) fn thenable_callback_value_type(
    db: &dyn TypeDatabase,
    callback_type: TypeId,
) -> Option<TypeId> {
    if let Some(sigs) = call_signatures_for_type(db, callback_type) {
        return sigs.first()?.params.first().map(|param| param.type_id);
    }
    if let Some(shape) = function_shape_for_type(db, callback_type) {
        return shape.params.first().map(|param| param.type_id);
    }
    let members = super::super::common::union_members(db, callback_type)?;
    for member in members {
        if let Some(sigs) = call_signatures_for_type(db, member)
            && let Some(first) = sigs.first()
        {
            return first.params.first().map(|param| param.type_id);
        }
        if let Some(shape) = function_shape_for_type(db, member) {
            return shape.params.first().map(|param| param.type_id);
        }
    }
    None
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

pub(crate) fn awaited_application_arg_from_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut is_awaited_base: impl FnMut(TypeId) -> bool,
) -> Option<TypeId> {
    let base = super::super::common::get_application_base(db, type_id)?;
    if !is_awaited_base(base) {
        return None;
    }
    let (_, args) = super::super::common::application_info(db, type_id)?;
    args.first().copied()
}

pub(crate) fn for_each_awaited_application_container_child(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut visit: impl FnMut(TypeId),
) {
    if let Some(element) = super::super::common::array_element_type(db, type_id) {
        visit(element);
    }
    if let Some(members) = super::super::common::union_members(db, type_id) {
        for member in members {
            visit(member);
        }
    }
    if let Some(elements) = super::super::common::tuple_elements(db, type_id) {
        for element in elements {
            visit(element.type_id);
        }
    }
}

pub(crate) fn awaited_variance_application_with_mapped_args(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    map_arg: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    awaited_assignability_application_with_mapped_args(db, type_id, map_arg)
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
