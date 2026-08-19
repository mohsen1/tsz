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
        if info.is_infer_placeholder() {
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
        // A round-1 candidate may legitimately mention a type parameter of an
        // *enclosing* signature that happens to share this parameter's name:
        //
        // ```ts
        // declare function each<T>(v: T, run: (v: T) => void): void;
        // function outer<T>(v: Box<T>) { each(v, sub => { /* sub: Box<T> */ }); }
        // ```
        //
        // Here the callee's `T` is fixed to `Box<T_outer>`, which is concrete
        // enough for `tsc` -- the free `T_outer` belongs to `outer`, not to
        // `each`. Substitutions are name-keyed, so at this point the two
        // occurrences are indistinguishable and defaulting the callee's `T`
        // would rewrite the enclosing `T` as well, yielding `Box<unknown>` and
        // a spurious `TS2322`/`TS2345` on the callback body.
        //
        // Drop the binding instead of defaulting it: leaving the candidate's
        // own free parameter intact is what `tsc` reports, and it matches the
        // choice `instantiate_shadowed_contextual_type_param` already makes for
        // the narrower "candidate is the whole contextual type" case.
        //
        // A *self-referential constraint* mentions its own parameter name for a
        // completely different reason and must not be caught here:
        //
        // ```ts
        // <O extends NoExcessProperties<RepeatOptions<A>, O>, A>(options: O): ...
        // ```
        //
        // `O`'s candidate mentions `O` because `O`'s own constraint does, not
        // because an enclosing signature declares a second `O`. Dropping that
        // binding would strip the contextual type off `options`' nested callback
        // parameters and report a spurious `TS7006` under `noImplicitAny`.
        // Distinguish by where the self-mention comes from: the callee's own
        // declaration, or an inferred argument. Binder identity cannot make this
        // call -- two unconstrained same-named parameters intern to one
        // `TypeId`, which is the very collision this guard exists for.
        let constraint_is_self_referential = tp.constraint.is_some_and(|constraint| {
            common::contains_type_parameter_named(db, constraint, tp.name)
        });
        if !constraint_is_self_referential
            && plan
                .substitution
                .get(tp.name)
                .is_some_and(|mapped| common::contains_type_parameter_named(db, mapped, tp.name))
        {
            plan.substitution.remove(tp.name);
            continue;
        }
        // Name-keyed capture guard: the contextual type handed to this pass was
        // already instantiated once with the round-1 candidates, so an
        // occurrence of `tp.name` inside it may belong to an *enclosing*
        // signature's parameter that a different callee parameter's candidate
        // legitimately introduced (`map<F, R, A, B>(self: Kind<F, R, A>, f:
        // (a: A) => B)` called inside `use<F, R, A, B>` infers `A := B_outer`;
        // the callee's own `B` is still unfixed). Substitutions are name-keyed,
        // so defaulting the unfixed `B` here would rewrite the foreign
        // `B_outer` occurrence to `unknown` as well — a tsz-only false
        // positive; `tsc` keeps the outer parameter. When any *other*
        // parameter's round-1 candidate mentions this name, leave the name
        // unbound instead of capturing it.
        let name_introduced_by_other_candidate = request.type_params.iter().any(|other| {
            other.name != tp.name
                && request
                    .current_substitution
                    .get(other.name)
                    .is_some_and(|candidate| {
                        common::contains_type_parameter_named(db, candidate, tp.name)
                    })
        });
        if name_introduced_by_other_candidate {
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
