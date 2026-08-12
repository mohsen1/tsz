use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{
    FunctionShape, ParamInfo, TypeId, TypeParamInfo, TypePredicate, computation as c,
};

pub(crate) fn instantiate_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &c::TypeSubstitution,
) -> TypeId {
    c::instantiate_type_cached(db.as_type_database(), Some(db), type_id, substitution)
}

pub(crate) fn instantiate_generic(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    c::instantiate_generic_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        type_params,
        type_args,
    )
}

pub(crate) fn signature_domain_substitution(type_params: &[TypeParamInfo]) -> c::TypeSubstitution {
    c::TypeSubstitution::for_signature_domain(type_params)
}

pub(crate) fn substitution_domain_contains_type_parameter(
    substitution: &c::TypeSubstitution,
    info: &TypeParamInfo,
    fallback_names: &rustc_hash::FxHashSet<tsz_common::Atom>,
) -> bool {
    substitution.domain_contains_type_parameter(info, fallback_names)
}

pub(crate) fn empty_substitution_with_same_domain(
    substitution: &c::TypeSubstitution,
) -> c::TypeSubstitution {
    substitution.empty_with_same_domain()
}

pub(crate) fn type_contains_type_parameter_binder(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    type_param: TypeParamInfo,
) -> bool {
    tsz_solver::visitor::contains_type_parameter_binder(db, type_id, type_param)
}

/// Infer concrete bindings for a set of type parameters by structurally
/// matching each `(declared type, concrete type)` pair, reusing the solver's
/// call-resolution inference engine.
///
/// This is the boundary used to recover a type parameter's binding when it
/// sits nested inside a compound declared type (a generic alias or wrapper
/// such as `Array<T>`), where a direct parameter/type-parameter identity
/// check cannot recover it.
pub(crate) fn infer_type_arguments_from_param_args(
    db: &dyn QueryDatabase,
    type_params: &[TypeParamInfo],
    param_arg_pairs: &[(TypeId, TypeId)],
) -> Vec<(tsz_common::Atom, TypeId)> {
    tsz_solver::computation::infer_type_arguments_from_param_args(db, type_params, param_arg_pairs)
}

// ── FunctionShape transformation helpers ──

/// Apply a `TypeSubstitution` to every type component in a `FunctionShape`.
///
/// Replaces type parameter references in parameter types, return type, this-type,
/// and type predicate type. Clears `type_params` since they are now resolved.
pub(crate) fn instantiate_function_shape(
    db: &dyn QueryDatabase,
    func: &FunctionShape,
    substitution: &c::TypeSubstitution,
) -> FunctionShape {
    let instantiate = |type_id| {
        c::instantiate_type_cached(db.as_type_database(), Some(db), type_id, substitution)
    };
    FunctionShape {
        params: func
            .params
            .iter()
            .map(|param| ParamInfo {
                type_id: instantiate(param.type_id),
                ..*param
            })
            .collect(),
        return_type: instantiate(func.return_type),
        this_type: func.this_type.map(instantiate),
        type_params: vec![],
        type_predicate: func.type_predicate.as_ref().map(|predicate| TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target,
            type_id: predicate.type_id.map(instantiate),
            parameter_index: predicate.parameter_index,
        }),
        is_constructor: func.is_constructor,
        is_method: func.is_method,
    }
}

/// Instantiate a generic function shape by substituting type parameters with
/// their defaults or constraints. Used for return-context matching where we
/// need a concrete shape but have no argument-driven substitution.
///
/// Returns the shape unchanged if it has no type parameters or no
/// defaults/constraints to apply.
pub(crate) fn instantiate_shape_to_defaults(
    db: &dyn QueryDatabase,
    func: &FunctionShape,
) -> FunctionShape {
    if func.type_params.is_empty() {
        return func.clone();
    }

    let mut substitution = c::TypeSubstitution::for_signature_domain(&func.type_params);
    for tp in &func.type_params {
        let Some(replacement) = tp.default.or(tp.constraint) else {
            continue;
        };
        substitution.insert(tp.name, replacement);
    }

    if substitution.is_empty() {
        return func.clone();
    }

    instantiate_function_shape(db, func, &substitution)
}
