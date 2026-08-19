use tsz_common::Atom;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{
    CallSignature, CallableShape, FunctionShape, MappedModifier, ParamInfo, TupleElement, TypeId,
    TypeParamInfo, TypeParamOrigin, TypePredicate,
};

pub(crate) use super::common::PropertyAccessResult;
pub(crate) use super::common::intersection_members;
pub(crate) use super::common::raw_property_type;
pub(crate) use super::common::{
    array_element_type, callable_shape_for_type as callable_shape, is_string_type, unwrap_readonly,
};

pub(crate) use tsz_solver::type_queries::PropertyTraversalKind;

/// Classify `type_id` into a property-traversal shape — the object/callable
/// shape carrying the properties, or the member list to descend into.
///
/// Sibling of `common::classify_for_traversal` for callers that need each
/// property's `PropertyInfo` (name *and* `TypeId`) rather than a name-only
/// view, such as matching a declared symbol-keyed property by type identity.
pub(crate) fn classify_property_traversal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyTraversalKind {
    tsz_solver::type_queries::classify_property_traversal(db, type_id)
}

/// Resolve a named property on a type through the solver's property evaluator.
///
/// This is the canonical boundary for property access resolution. Checker code
/// must use this instead of directly instantiating `PropertyAccessEvaluator`.
pub(crate) fn resolve_property_access(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: Atom,
) -> PropertyAccessResult {
    resolve_property_access_with_options(db, obj_type, prop_name, false)
}

pub(crate) fn receiver_property_visibility(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    property_name: Atom,
) -> Option<tsz_solver::Visibility> {
    tsz_solver::type_queries::receiver_property_visibility_atom(db, object_type, property_name)
}

pub(crate) fn protected_intersection_owner_type(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    property_name: Atom,
) -> Option<TypeId> {
    let intersection_type = db
        .get_display_alias(object_type)
        .filter(|alias| intersection_members(db, *alias).is_some())
        .unwrap_or(object_type);
    let members = intersection_members(db, intersection_type)?;
    (members.len() >= 2
        && receiver_property_visibility(db, intersection_type, property_name)
            == Some(tsz_solver::Visibility::Protected))
    .then_some(intersection_type)
}

pub(crate) fn resolve_property_access_with_options(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: Atom,
    no_unchecked_indexed_access: bool,
) -> PropertyAccessResult {
    let mut evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.resolve_property_access_atom(obj_type, prop_name)
}

/// Resolve a property against `unknown`'s non-strict apparent surface (the
/// `Object.prototype` member table). See
/// `PropertyAccessEvaluator::resolve_unknown_non_strict_member`.
pub(crate) fn resolve_unknown_non_strict_property_access(
    db: &dyn QueryDatabase,
    prop_name: Atom,
) -> PropertyAccessResult {
    let evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.resolve_unknown_non_strict_member(prop_name)
}

pub(crate) fn resolve_private_identifier_property_access(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: Atom,
) -> PropertyAccessResult {
    let evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.set_allow_private_identifier_properties(true);
    evaluator.resolve_property_access_atom(obj_type, prop_name)
}

/// Like [`resolve_property_access`] but preserves raw `ThisType` in the result.
///
/// When `skip_this_binding` is set, the solver does not eagerly bind `this` to
/// the structural object shape. The caller can then substitute `this` with the
/// correct nominal receiver type (e.g., the class type instead of the flattened
/// intersection shape).
pub(crate) fn resolve_property_access_raw_this(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: Atom,
) -> PropertyAccessResult {
    resolve_property_access_raw_this_with_options(db, obj_type, prop_name, false)
}

pub(crate) fn resolve_property_access_raw_this_with_options(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: Atom,
    no_unchecked_indexed_access: bool,
) -> PropertyAccessResult {
    let mut evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.set_skip_this_binding(true);
    evaluator.resolve_property_access_atom(obj_type, prop_name)
}

pub(crate) fn resolve_property_access_with_resolver(
    db: &dyn QueryDatabase,
    resolver: &dyn tsz_solver::relations::subtype::TypeResolver,
    obj_type: TypeId,
    prop_name: Atom,
    no_unchecked_indexed_access: bool,
) -> PropertyAccessResult {
    let mut evaluator =
        tsz_solver::operations::property::PropertyAccessEvaluator::with_resolver(db, resolver);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.resolve_property_access_atom(obj_type, prop_name)
}

pub(crate) fn resolve_property_access_raw_this_with_resolver(
    db: &dyn QueryDatabase,
    resolver: &dyn tsz_solver::relations::subtype::TypeResolver,
    obj_type: TypeId,
    prop_name: Atom,
    no_unchecked_indexed_access: bool,
) -> PropertyAccessResult {
    let mut evaluator =
        tsz_solver::operations::property::PropertyAccessEvaluator::with_resolver(db, resolver);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.set_skip_this_binding(true);
    evaluator.resolve_property_access_atom(obj_type, prop_name)
}

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_function_type(db, type_id)
}

pub(crate) fn tuple_element_type_union(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_tuple_element_type_union(db, type_id)
}

pub(crate) fn mapped_property_read_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    optional_modifier: Option<MappedModifier>,
) -> TypeId {
    match optional_modifier {
        Some(MappedModifier::Add) => db.union2(type_id, TypeId::UNDEFINED),
        Some(MappedModifier::Remove) | None => type_id,
    }
}

pub(crate) fn union_property_access_success(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> Option<PropertyAccessResult> {
    (!members.is_empty())
        .then(|| PropertyAccessResult::simple(tsz_solver::utils::union_or_single(db, members)))
}

pub(crate) fn intersection_property_access_success(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
    from_index_signature: bool,
) -> Option<PropertyAccessResult> {
    (!members.is_empty()).then(|| {
        let type_id = tsz_solver::utils::intersection_or_single(db, members);
        if from_index_signature {
            PropertyAccessResult::from_index(type_id)
        } else {
            PropertyAccessResult::simple(type_id)
        }
    })
}

pub(crate) fn application_first_arg(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_application(db, type_id)?
        .args
        .first()
        .copied()
}

pub(crate) fn is_boolean_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_boolean_type(db, type_id)
}

pub(crate) fn is_number_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_number_type(db, type_id)
}

pub(crate) fn is_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_type(db, type_id)
}

pub(crate) fn is_bigint_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_bigint_type(db, type_id)
}

pub(crate) fn def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_def_id(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

pub(crate) fn type_parameter_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::type_queries::get_type_parameter_name(db, type_id)
}

pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_enum_def_id(db, type_id)
}

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) const fn strict_bind_call_apply_param_with_type(
    param: ParamInfo,
    type_id: TypeId,
) -> ParamInfo {
    ParamInfo { type_id, ..param }
}

pub(crate) const fn strict_bind_call_apply_type_param_with_constraint(
    type_param: TypeParamInfo,
    constraint: Option<TypeId>,
) -> TypeParamInfo {
    TypeParamInfo {
        constraint,
        ..type_param
    }
}

pub(crate) const fn strict_bind_call_apply_call_signature(
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_method,
        declaration_group: 0,
    }
}

pub(crate) fn strict_bind_call_apply_signature_from_function_shape(
    shape: &FunctionShape,
) -> CallSignature {
    strict_bind_call_apply_call_signature(
        shape.type_params.clone(),
        shape.params.clone(),
        shape.this_type,
        shape.return_type,
        shape.type_predicate,
        shape.is_method,
    )
}

pub(crate) fn strict_bind_call_apply_params_tuple_type(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
) -> TypeId {
    let tuple_elements: Vec<TupleElement> = params
        .iter()
        .map(|param| TupleElement {
            type_id: param.type_id,
            name: param.name,
            optional: param.optional && !param.rest,
            rest: param.rest,
        })
        .collect();
    db.tuple(tuple_elements)
}

pub(crate) fn strict_bind_call_apply_bound_return_type(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
    remaining_params: Vec<ParamInfo>,
    is_constructor: bool,
) -> TypeId {
    if is_constructor {
        return db.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures: vec![strict_bind_call_apply_call_signature(
                sig.type_params.clone(),
                remaining_params,
                None,
                sig.return_type,
                None,
                false,
            )],
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
    }

    db.function(FunctionShape {
        type_params: sig.type_params.clone(),
        params: remaining_params,
        this_type: None,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: false,
    })
}

pub(crate) fn strict_bind_call_apply_call_only_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) fn strict_bind_call_apply_this_arg_param(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ParamInfo {
    ParamInfo {
        name: Some(db.intern_string("thisArg")),
        type_id,
        optional: false,
        rest: false,
    }
}

pub(crate) fn strict_bind_call_apply_args_param(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ParamInfo {
    ParamInfo {
        name: Some(db.intern_string("args")),
        type_id,
        optional: true,
        rest: false,
    }
}

pub(crate) fn strict_bind_call_apply_generic_this_param(
    db: &dyn QueryDatabase,
    constraint: TypeId,
    sig: &CallSignature,
) -> (TypeParamInfo, TypeId) {
    let name_occurs_in = |type_id, name| {
        tsz_solver::query::contains_type_parameter_named(db, type_id, name) || {
            let evaluated = db.evaluate_type(type_id);
            evaluated != type_id
                && tsz_solver::query::contains_type_parameter_named(db, evaluated, name)
        }
    };
    let name_is_used = |name| {
        sig.type_params.iter().any(|param| {
            param.name == name
                || param
                    .constraint
                    .is_some_and(|constraint| name_occurs_in(constraint, name))
                || param
                    .default
                    .is_some_and(|default| name_occurs_in(default, name))
        }) || sig
            .params
            .iter()
            .any(|param| name_occurs_in(param.type_id, name))
            || sig
                .this_type
                .is_some_and(|this_type| name_occurs_in(this_type, name))
            || sig
                .type_predicate
                .and_then(|predicate| predicate.type_id)
                .is_some_and(|predicate_type| name_occurs_in(predicate_type, name))
            || name_occurs_in(sig.return_type, name)
    };
    let mut suffix = 0_u32;
    let name = loop {
        let candidate = if suffix == 0 {
            db.intern_string("TThis")
        } else {
            db.intern_string(&format!("TThis{suffix}"))
        };
        if !name_is_used(candidate) {
            break candidate;
        }
        suffix = suffix.saturating_add(1);
    };
    let info = TypeParamInfo {
        name,
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let type_id = db.type_param(info);
    (info, type_id)
}

/// Build the implicit receiver constraint for the zero-bound-argument
/// (`A = []`) specialization of the final generic `CallableFunction.bind`
/// overload.
///
/// A bare rest parameter owned by an enclosing signature is fixed to its
/// constraint before the `thisArg` is considered. For example, the `T` in
/// `(this: 1, ...args: T) => void`, where `T extends unknown[]` belongs to the
/// enclosing function, becomes `unknown[]`. Parameters declared by the
/// receiver signature itself remain generic. `NoInfer<T>` is the same bare
/// binder for this specialization, as is a transparent alias application such
/// as `Identity<T>`. Tuple/array and other structural wrappers remain intact.
pub(crate) fn strict_bind_call_apply_generic_bind_receiver_type<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    sig: &CallSignature,
    generic_this_type: TypeId,
) -> TypeId {
    let params = sig
        .params
        .iter()
        .map(|param| {
            if !param.rest {
                return *param;
            }
            let info =
                match tsz_solver::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                    db,
                    resolver,
                    param.type_id,
                ) {
                    tsz_solver::type_queries::RestBinderQuery::Complete(Some(info)) => info,
                    tsz_solver::type_queries::RestBinderQuery::Complete(None)
                    | tsz_solver::type_queries::RestBinderQuery::Incomplete => return *param,
                };
            if sig
                .type_params
                .iter()
                .any(|local| local.is_same_binder(info))
            {
                return *param;
            }
            ParamInfo {
                type_id: info
                    .constraint
                    .filter(|constraint| *constraint != TypeId::ERROR)
                    .unwrap_or(TypeId::UNKNOWN),
                ..*param
            }
        })
        .collect();

    db.function(FunctionShape {
        type_params: sig.type_params.clone(),
        params,
        this_type: Some(generic_this_type),
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: false,
    })
}

pub(crate) fn strict_bind_call_apply_method_type(
    db: &dyn TypeDatabase,
    method_signatures: Vec<CallSignature>,
) -> Option<TypeId> {
    match method_signatures.len() {
        0 => None,
        1 => {
            let sig = method_signatures.into_iter().next()?;
            Some(db.function(FunctionShape {
                type_params: sig.type_params,
                params: sig.params,
                this_type: None,
                return_type: sig.return_type,
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: false,
            }))
        }
        _ => Some(db.callable(CallableShape {
            call_signatures: method_signatures,
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        })),
    }
}

/// Check if a type has a named property accessible on all branches.
///
/// For unions, returns true only when ALL members have the property.
/// Used by TS2702/TS2713 diagnostic distinction.
pub(crate) fn type_has_property(db: &dyn TypeDatabase, type_id: TypeId, name: Atom) -> bool {
    tsz_solver::type_queries::type_has_property_atom(db, type_id, name)
}

/// Check if a type is the polymorphic `this` type.
///
/// Used during property access resolution to suppress TS2339 when `this`
/// comes from a `ThisType` marker (e.g., Vue 2 Options API pattern).
pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
}

/// Check if a type contains `never` (e.g. an intersection reduced to `never`).
///
/// Used to detect cases where property access should return `error` to suppress
/// cascading diagnostics (matching tsc behavior for `never` types).
pub(crate) fn contains_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_never_type_db(db, type_id)
}

/// Extract object and index types from an `IndexAccess` type (T[K]).
///
/// Returns `None` if `type_id` is not an `IndexAccess` type.
pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

/// Extract the [`MappedType`] shape from a type, if it is a mapped type.
///
/// Thin boundary wrapper around `tsz_solver::type_queries::get_mapped_type`.
pub(crate) fn get_mapped_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::types::MappedType>> {
    tsz_solver::type_queries::get_mapped_type(db, type_id)
}

/// Check whether a type transitively contains any type parameters.
///
/// Thin boundary wrapper around `tsz_solver::type_queries::contains_type_parameters_db`.
pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/property_access_boundaries.rs"]
mod tests;
