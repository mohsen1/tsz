use tsz_solver::construction::QueryDatabase;
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
                name: param.name,
                type_id: instantiate(param.type_id),
                optional: param.optional,
                rest: param.rest,
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

    let mut substitution = c::TypeSubstitution::new();
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
