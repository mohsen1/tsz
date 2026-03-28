use tsz_solver::operations::CallResult;
use tsz_solver::{
    AssignabilityChecker, ContextualTypeContext, FunctionShape, QueryDatabase, TypeDatabase,
    TypeEnvironment, TypeId, TypeResolver, TypeSubstitution,
};

pub(crate) use super::super::common::array_element_type as array_element_type_for_type;
pub(crate) use super::super::common::is_type_parameter_like as is_type_parameter_type;
pub(crate) use super::super::common::lazy_def_id as lazy_def_id_for_type;
pub(crate) use super::super::common::tuple_elements as tuple_elements_for_type;

pub(crate) fn get_contextual_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    tsz_solver::get_contextual_signature_with_compat_checker(db, type_id)
}

pub(crate) fn get_contextual_signature_for_arity(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    tsz_solver::get_contextual_signature_for_arity_with_compat_checker(db, type_id, arg_count)
}

/// Get the call signature of a type, preferring a generic one.
///
/// Used by the checker's two-pass call path when overloaded callables mix
/// generic and non-generic signatures. `get_contextual_signature_for_arity`
/// intentionally returns `None` for that case to avoid unsafe contextual typing
/// of callbacks, but we still need to know whether there is an arity-compatible
/// generic signature so generic inference/sanitization can run.
pub(crate) fn get_call_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    let sigs = tsz_solver::type_queries::get_call_signatures(db, type_id)?;
    let signature_accepts_arg_count = |params: &[tsz_solver::ParamInfo], count: usize| {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            count >= required_count
        } else {
            count >= required_count && count <= params.len()
        }
    };
    let applicable: Vec<_> = sigs
        .iter()
        .filter(|s| signature_accepts_arg_count(&s.params, arg_count))
        .collect();

    let sig = if !applicable.is_empty() {
        applicable
            .iter()
            .find(|s| !s.type_params.is_empty())
            .copied()
            .or_else(|| applicable.first().copied())?
    } else {
        sigs.iter()
            .find(|s| !s.type_params.is_empty())
            .or_else(|| sigs.first())?
    };

    Some(FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: sig.is_method,
    })
}

pub(crate) fn get_function_parameter_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    tsz_solver::type_queries::get_function_parameter_types(db, type_id)
}

pub(crate) fn stable_call_recovery_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id) {
        return Some(shape.return_type);
    }

    if let Some(shape) = tsz_solver::type_queries::get_callable_shape(db, type_id) {
        let first = shape.call_signatures.first()?.return_type;
        return shape
            .call_signatures
            .iter()
            .all(|sig| sig.return_type == first)
            .then_some(first);
    }

    let members = tsz_solver::type_queries::get_intersection_members(db, type_id)?;
    let mut candidate = None;
    for member in members {
        let Some(return_type) = stable_call_recovery_return_type(db, member) else {
            continue;
        };
        if let Some(existing) = candidate {
            if existing != return_type {
                return None;
            }
        } else {
            candidate = Some(return_type);
        }
    }
    candidate
}

/// Get the construct signature of a type, preferring a generic one.
/// Used for two-pass inference in `new` expressions where the construct
/// signature may have type parameters that need to be inferred.
///
/// For overloaded constructors (e.g. `Map` with `new()` and `new<K,V>(entries?)`),
/// we prefer the generic signature so that `is_generic_new` is set correctly
/// and proper contextual types are provided to array/object literal arguments.
pub(crate) fn get_construct_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    let sigs = tsz_solver::type_queries::get_construct_signatures(db, type_id)?;
    let signature_accepts_arg_count = |params: &[tsz_solver::ParamInfo], count: usize| {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            count >= required_count
        } else {
            count >= required_count && count <= params.len()
        }
    };
    let applicable: Vec<_> = sigs
        .iter()
        .filter(|s| signature_accepts_arg_count(&s.params, arg_count))
        .collect();

    // Prefer generic signatures among arity-compatible candidates.
    let sig = if !applicable.is_empty() {
        applicable
            .iter()
            .find(|s| !s.type_params.is_empty())
            .copied()
            .or_else(|| applicable.first().copied())?
    } else {
        // Fallback to previous behavior when no signature matches arity.
        sigs.iter()
            .find(|s| !s.type_params.is_empty())
            .or_else(|| sigs.first())?
    };
    Some(FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: true,
        is_method: false,
    })
}

pub(crate) fn resolve_call<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
) -> tsz_solver::operations::CallWithCheckerResult {
    tsz_solver::operations::resolve_call_with_checker(
        db,
        checker,
        func_type,
        arg_types,
        force_bivariant_callbacks,
        contextual_type,
        actual_this_type,
    )
}

pub(crate) fn resolve_new<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
) -> CallResult {
    tsz_solver::operations::resolve_new_with_checker(
        db,
        checker,
        type_id,
        arg_types,
        force_bivariant_callbacks,
        contextual_type,
    )
}

pub(crate) fn compute_contextual_types_with_context(
    db: &dyn QueryDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    env: &tsz_solver::TypeEnvironment,
    shape: &tsz_solver::FunctionShape,
    arg_types: &[TypeId],
    contextual_type: Option<TypeId>,
) -> TypeSubstitution {
    tsz_solver::operations::compute_contextual_types_with_compat_checker(
        db,
        env,
        shape,
        arg_types,
        contextual_type,
        |checker| ctx.configure_compat_checker(checker),
    )
}

pub(crate) fn expanded_this_type_from_application(
    db: &dyn TypeDatabase,
    env: &TypeEnvironment,
    type_id: TypeId,
    no_implicit_any: bool,
) -> Option<TypeId> {
    let app = tsz_solver::type_queries::get_type_application(db, type_id)?;
    let def_id = tsz_solver::type_queries::get_lazy_def_id(db, app.base)?;
    let body = env.resolve_lazy(def_id, db)?;
    let type_params = env.get_lazy_type_params(def_id).unwrap_or_default();
    let expanded = tsz_solver::instantiate_generic(db, body, &type_params, &app.args);
    let expanded_ctx =
        ContextualTypeContext::with_expected_and_options(db, expanded, no_implicit_any);
    expanded_ctx.get_this_type_from_marker()
}

pub(crate) fn get_overload_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::data::get_overload_call_signatures(db, type_id)
}

pub(crate) fn is_valid_union_predicate(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::flow::is_valid_union_predicate(db, type_id)
}

pub(crate) fn extract_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::flow::ExtractedPredicateSignature> {
    tsz_solver::type_queries::flow::extract_predicate_signature(db, type_id)
}
