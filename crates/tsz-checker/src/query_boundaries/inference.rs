//! Query-boundary wrappers for generic inference helpers.

use crate::query_boundaries::common;
use crate::query_boundaries::common::QueryDatabase;
use tsz_common::interner::Atom;
use tsz_solver::{TypeId, TypeParamInfo, computation::TypeSubstitution};

struct ContextualTypeParamInstantiationRequest<'a> {
    type_id: TypeId,
    type_params: &'a [TypeParamInfo],
    current_substitution: &'a TypeSubstitution,
}

impl<'a> ContextualTypeParamInstantiationRequest<'a> {
    const fn new(
        type_id: TypeId,
        type_params: &'a [TypeParamInfo],
        current_substitution: &'a TypeSubstitution,
    ) -> Self {
        Self {
            type_id,
            type_params,
            current_substitution,
        }
    }
}

struct ContextualTypeParamInstantiationPlan {
    substitution: TypeSubstitution,
    infer_bindings: Vec<(Atom, TypeId)>,
}

impl ContextualTypeParamInstantiationPlan {
    const fn new(substitution: TypeSubstitution, infer_bindings: Vec<(Atom, TypeId)>) -> Self {
        Self {
            substitution,
            infer_bindings,
        }
    }
}

struct ContextualTypeParamInstantiationResult {
    type_id: TypeId,
}

impl ContextualTypeParamInstantiationResult {
    const fn unchanged(type_id: TypeId) -> Self {
        Self { type_id }
    }

    const fn type_id(self) -> TypeId {
        self.type_id
    }
}

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
    let request =
        ContextualTypeParamInstantiationRequest::new(type_id, type_params, current_substitution);
    instantiate_remaining_contextual_type_params_request(db, request).type_id()
}

fn instantiate_remaining_contextual_type_params_request(
    db: &dyn QueryDatabase,
    request: ContextualTypeParamInstantiationRequest<'_>,
) -> ContextualTypeParamInstantiationResult {
    if let Some(result) = instantiate_shadowed_contextual_type_param(db, &request) {
        return result;
    }

    let mut plan = contextual_type_param_instantiation_plan(db, &request);
    if request.type_params.is_empty() && plan.infer_bindings.is_empty() {
        return ContextualTypeParamInstantiationResult::unchanged(request.type_id);
    }

    complete_contextual_type_param_plan(db, &request, &mut plan);
    ContextualTypeParamInstantiationResult {
        type_id: instantiate_type_with_infer(db, request.type_id, &plan.substitution),
    }
}

fn instantiate_shadowed_contextual_type_param(
    db: &dyn QueryDatabase,
    request: &ContextualTypeParamInstantiationRequest<'_>,
) -> Option<ContextualTypeParamInstantiationResult> {
    for tp in request.type_params {
        if request.current_substitution.get(tp.name) != Some(request.type_id)
            || !common::contains_type_parameter_named(db, request.type_id, tp.name)
        {
            continue;
        }

        let declared_param = db.factory().type_param(*tp);
        let mut shadow_substitution = TypeSubstitution::new();
        for referenced in common::collect_referenced_types(db, request.type_id) {
            let Some(referenced_info) = common::type_param_info(db, referenced) else {
                continue;
            };
            if referenced_info.name != tp.name || referenced == declared_param {
                continue;
            }
            if let Some(replacement) = referenced_info.default.or(referenced_info.constraint) {
                shadow_substitution.insert(tp.name, replacement);
            } else {
                return Some(ContextualTypeParamInstantiationResult::unchanged(
                    request.type_id,
                ));
            }
        }

        if !shadow_substitution.is_empty() {
            return Some(ContextualTypeParamInstantiationResult {
                type_id: common::instantiate_type(db, request.type_id, &shadow_substitution),
            });
        }
    }

    None
}

fn contextual_type_param_instantiation_plan(
    db: &dyn QueryDatabase,
    request: &ContextualTypeParamInstantiationRequest<'_>,
) -> ContextualTypeParamInstantiationPlan {
    let mut infer_bindings = collect_infer_bindings(db, request.type_id);
    for referenced in common::collect_referenced_types(db, request.type_id) {
        let Some(info) = common::type_param_info(db, referenced) else {
            continue;
        };
        let name = db.resolve_atom(info.name);
        if name.starts_with("__infer_") || name.starts_with("__infer_src_") {
            infer_bindings.push((info.name, referenced));
        }
    }
    ContextualTypeParamInstantiationPlan::new(request.current_substitution.clone(), infer_bindings)
}

fn complete_contextual_type_param_plan(
    db: &dyn QueryDatabase,
    request: &ContextualTypeParamInstantiationRequest<'_>,
    plan: &mut ContextualTypeParamInstantiationPlan,
) {
    for tp in request.type_params {
        if plan.substitution.get(tp.name).is_some_and(|mapped| {
            !common::contains_type_parameters(db, mapped)
                && !common::contains_infer_types(db, mapped)
        }) {
            continue;
        }
        let replacement = tp.default.or(tp.constraint).unwrap_or(TypeId::UNKNOWN);
        let replacement = common::instantiate_type(db, replacement, &plan.substitution);
        let replacement = if common::contains_type_parameters(db, replacement)
            || common::contains_infer_types(db, replacement)
        {
            TypeId::UNKNOWN
        } else {
            replacement
        };
        plan.substitution.insert(tp.name, replacement);
    }

    for (name, infer_type) in plan.infer_bindings.iter().copied() {
        if plan.substitution.get(name).is_some_and(|mapped| {
            !common::contains_type_parameters(db, mapped)
                && !common::contains_infer_types(db, mapped)
        }) {
            continue;
        }
        let replacement = common::type_param_info(db, infer_type)
            .and_then(|info| info.default.or(info.constraint))
            .unwrap_or(TypeId::UNKNOWN);
        let replacement = common::instantiate_type(db, replacement, &plan.substitution);
        let replacement = if common::contains_type_parameters(db, replacement)
            || common::contains_infer_types(db, replacement)
        {
            TypeId::UNKNOWN
        } else {
            replacement
        };
        plan.substitution.insert(name, replacement);
    }
}
