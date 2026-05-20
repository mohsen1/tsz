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

/// `true` when `type_id` still has free type parameters or unresolved `infer` placeholders.
fn has_inference_holes(db: &dyn QueryDatabase, type_id: TypeId) -> bool {
    common::contains_type_parameters(db, type_id) || common::contains_infer_types(db, type_id)
}

/// Instantiate `raw` with `substitution`, replacing the result with `TypeId::UNKNOWN`
/// when inference holes survive — they cannot be meaningfully used as contextual types.
fn resolve_replacement(
    db: &dyn QueryDatabase,
    raw: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    let replacement = common::instantiate_type(db, raw, substitution);
    if has_inference_holes(db, replacement) {
        TypeId::UNKNOWN
    } else {
        replacement
    }
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
    let referenced_types = common::collect_referenced_types(db, type_id);

    for tp in type_params {
        // Catches the case where a return-context substitution bound the callee's param
        // to a callback that transitively references the same-named outer param.
        if current_substitution.get(tp.name) != Some(type_id)
            || !common::contains_type_parameter_named(db, type_id, tp.name)
        {
            continue;
        }

        // Distinguishes the callee's declaration from any identically-named outer param.
        let declared_param = db.factory().type_param(*tp);
        let mut shadow_substitution = TypeSubstitution::new();
        for &referenced in &referenced_types {
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
    for &referenced in &referenced_types {
        let Some(info) = common::type_param_info(db, referenced) else {
            continue;
        };
        // `__infer_` and `__infer_src_` are internal names the solver assigns to
        // `infer T` placeholders; they must be treated the same as explicit infer bindings.
        let name = db.resolve_atom_ref(info.name);
        if name.starts_with("__infer_") || name.starts_with("__infer_src_") {
            infer_bindings.push((info.name, referenced));
        }
    }
    if type_params.is_empty() && infer_bindings.is_empty() {
        return type_id;
    }

    let mut substitution = current_substitution.clone();
    for tp in type_params {
        if substitution
            .get(tp.name)
            .is_some_and(|mapped| !has_inference_holes(db, mapped))
        {
            continue;
        }
        let raw = tp.default.or(tp.constraint).unwrap_or(TypeId::UNKNOWN);
        substitution.insert(tp.name, resolve_replacement(db, raw, &substitution));
    }
    for (name, infer_type) in infer_bindings {
        if substitution
            .get(name)
            .is_some_and(|mapped| !has_inference_holes(db, mapped))
        {
            continue;
        }
        let raw = common::type_param_info(db, infer_type)
            .and_then(|info| info.default.or(info.constraint))
            .unwrap_or(TypeId::UNKNOWN);
        substitution.insert(name, resolve_replacement(db, raw, &substitution));
    }

    instantiate_type_with_infer(db, type_id, &substitution)
}
