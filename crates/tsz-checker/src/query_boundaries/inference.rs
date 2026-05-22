//! Query-boundary wrappers for generic inference helpers.

use crate::query_boundaries::common;
use crate::query_boundaries::common::QueryDatabase;
use tsz_common::interner::Atom;
use tsz_solver::{TypeId, TypeParamInfo, computation::TypeSubstitution};

pub(crate) fn instantiate_type_with_infer(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    tsz_solver::computation::instantiate_type_with_infer_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        substitution,
    )
}

pub(crate) fn collect_infer_bindings(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Vec<(Atom, TypeId)> {
    tsz_solver::collect_infer_bindings(db, type_id)
}

/// Apply default or constraint substitutions for any remaining unbound contextual
/// type parameters in `type_id`.
///
/// When a return-context substitution binds a callee type parameter to a
/// contextual callback that mentions an outer type parameter with the same name,
/// the outer parameter must not be defaulted to the callee's constraint — use
/// the outer parameter's own bound instead.  After handling that case, any
/// remaining `infer`-introduced names and ordinary unbound type params are
/// filled with their declared defaults or constraints (falling back to
/// `unknown`).
pub(crate) fn instantiate_remaining_contextual_type_params(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    current_substitution: &TypeSubstitution,
) -> TypeId {
    for tp in type_params {
        if current_substitution.get(tp.name) != Some(type_id)
            || !common::contains_type_parameter_named(db, type_id, tp.name)
        {
            continue;
        }

        let declared_param = db.factory().type_param(*tp);
        let mut shadow_substitution = TypeSubstitution::new();
        for referenced in common::collect_referenced_types(db, type_id) {
            let Some(referenced_info) = common::type_param_info(db, referenced) else {
                continue;
            };
            if referenced_info.name != tp.name || referenced == declared_param {
                continue;
            }
            if let Some(replacement) = referenced_info.default.or(referenced_info.constraint) {
                shadow_substitution.insert(tp.name, replacement);
            } else {
                return type_id;
            }
        }

        if !shadow_substitution.is_empty() {
            return common::instantiate_type(db, type_id, &shadow_substitution);
        }
    }

    let mut infer_bindings = collect_infer_bindings(db, type_id);
    for referenced in common::collect_referenced_types(db, type_id) {
        let Some(info) = common::type_param_info(db, referenced) else {
            continue;
        };
        let name = db.resolve_atom(info.name);
        if name.starts_with("__infer_") || name.starts_with("__infer_src_") {
            infer_bindings.push((info.name, referenced));
        }
    }
    if type_params.is_empty() && infer_bindings.is_empty() {
        return type_id;
    }

    let mut substitution = current_substitution.clone();
    for tp in type_params {
        if substitution.get(tp.name).is_some_and(|mapped| {
            !common::contains_type_parameters(db, mapped)
                && !common::contains_infer_types(db, mapped)
        }) {
            continue;
        }
        let replacement = tp.default.or(tp.constraint).unwrap_or(TypeId::UNKNOWN);
        let replacement = common::instantiate_type(db, replacement, &substitution);
        let replacement = if common::contains_type_parameters(db, replacement)
            || common::contains_infer_types(db, replacement)
        {
            TypeId::UNKNOWN
        } else {
            replacement
        };
        substitution.insert(tp.name, replacement);
    }
    for (name, infer_type) in infer_bindings {
        if substitution.get(name).is_some_and(|mapped| {
            !common::contains_type_parameters(db, mapped)
                && !common::contains_infer_types(db, mapped)
        }) {
            continue;
        }
        let replacement = common::type_param_info(db, infer_type)
            .and_then(|info| info.default.or(info.constraint))
            .unwrap_or(TypeId::UNKNOWN);
        let replacement = common::instantiate_type(db, replacement, &substitution);
        let replacement = if common::contains_type_parameters(db, replacement)
            || common::contains_infer_types(db, replacement)
        {
            TypeId::UNKNOWN
        } else {
            replacement
        };
        substitution.insert(name, replacement);
    }

    instantiate_type_with_infer(db, type_id, &substitution)
}
