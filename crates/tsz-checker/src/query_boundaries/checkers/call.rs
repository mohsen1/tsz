use tsz_solver::operations::CallResult;
use tsz_solver::{
    AssignabilityChecker, FunctionShape, QueryDatabase, TypeDatabase, TypeId, TypeSubstitution,
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
) -> Option<FunctionShape> {
    let sigs = tsz_solver::type_queries::get_construct_signatures(db, type_id)?;
    // Prefer a generic signature (one with type parameters) over a non-generic one.
    // This ensures that overloaded constructors like Map's `new<K,V>(entries?)` trigger
    // proper contextual typing for array/object literal arguments.
    let sig = sigs
        .iter()
        .find(|s| !s.type_params.is_empty())
        .or_else(|| sigs.first())?;
    Some(FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate.clone(),
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
) -> (
    CallResult,
    Option<(tsz_solver::TypePredicate, Vec<tsz_solver::ParamInfo>)>,
) {
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
) -> CallResult {
    tsz_solver::operations::resolve_new_with_checker(
        db,
        checker,
        type_id,
        arg_types,
        force_bivariant_callbacks,
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
