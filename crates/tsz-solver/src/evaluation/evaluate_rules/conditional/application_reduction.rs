use super::super::super::evaluate::TypeEvaluator;
use crate::evaluation::evaluate_rules::infer_pattern::InferPatternVisited;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashMap;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Cheap pre-check before `reduce_alias_body_to_application_form`: only
    /// candidate types can be usefully reduced. Avoids the per-conditional
    /// hot-path cost of entering the reducer just to bail on the first
    /// step for intrinsics, type parameters, etc.
    pub(super) fn is_alias_reducible_candidate(
        interner: &dyn crate::construction::TypeDatabase,
        ty: TypeId,
    ) -> bool {
        if crate::type_queries::is_generic_type(interner, ty) {
            return true;
        }
        // Parametric structural instantiations record a back-reference from
        // their evaluated structural form to the original `Application` via
        // the display-alias map; the reducer can recover that form.
        interner
            .get_display_alias(ty)
            .is_some_and(|alias| matches!(interner.lookup(alias), Some(TypeData::Application(_))))
    }

    /// Whether `ty` is an `Application` whose base resolves to a *self-recursive
    /// conditional alias* — a def whose resolved body is a `Conditional` that
    /// re-references its own `DefId` (e.g.
    /// `Deep<K> = K extends Promise<infer U> ? Deep<U> : K`).
    ///
    /// Such an application cannot serve as a structural reduction step: peeling
    /// it ([`alias_application_substituted_body`](Self::alias_application_substituted_body))
    /// yields the conditional body, and simulating that conditional re-enters
    /// the same recursion, so
    /// [`reduce_alias_body_to_application_form`](Self::reduce_alias_body_to_application_form)
    /// must not follow a diagnostic-only `display_alias` back-reference to one.
    ///
    /// The body must be a `Conditional` specifically: a self-referential
    /// *interface* body (e.g. `Promise`, whose `then`/`catch` return
    /// `Promise<…>`) or a recursive *structural* alias body (object/union/
    /// intersection) is not a hazard — peeling it produces a non-`Application`,
    /// non-`Conditional` shape that terminates the reduction loop. Gating on the
    /// conditional body (rather than the broad self-reference test used by
    /// [`result_has_residual_recursive_alias`](Self::result_has_residual_recursive_alias)
    /// for cache poisoning) keeps the legitimate `Promise<…>` recovery working.
    pub(super) fn application_is_recursive_alias(&self, ty: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner().lookup(ty) else {
            return false;
        };
        let base = self.interner().type_application(app_id).base;
        let Some(def_id) = (match self.interner().lookup(base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::TypeQuery(sym_ref)) => self.resolver().symbol_to_def_id(sym_ref),
            _ => None,
        }) else {
            return false;
        };
        self.resolver()
            .resolve_lazy(def_id, self.interner())
            .is_some_and(|body| {
                matches!(self.interner().lookup(body), Some(TypeData::Conditional(_)))
                    && crate::visitor::contains_lazy_def_id(self.interner(), body, def_id)
            })
    }

    /// Reduce `ty` to its underlying `Application(...)` form by walking one
    /// alias step (Application body) or simulating one infer-match step
    /// (Conditional body with `infer` in `extends`). When `ty` isn't itself
    /// an `Application`, falls back to the display-alias back-reference
    /// `evaluate_application` records for parametric structural
    /// instantiations. Returns `None` on no-op or fixed point.
    pub(in crate::evaluation) fn reduce_alias_body_to_application_form(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let mut current = ty;
        for _ in 0..Self::MAX_ALIAS_REDUCTION_STEPS {
            // A `display_alias` back-reference whose application base is a
            // *self-recursive conditional alias* (its resolved body is a
            // `Conditional` that re-references its own `DefId`) is a
            // diagnostic-only label, not a structural reduction handle:
            // peeling/simulating it re-enters the very recursion that produced
            // `current`, never reaching an `Application(interface)` form to read
            // the infer slot from. For example a self-recursive
            // `Deep<K> = K extends Promise<infer U> ? Deep<U> : K` records
            // `{ id: 0 } -> Deep<AB<{ id: 0 }>>` on its reduced result; following
            // that back into `Deep` spins forever (#14123/#14417). Recoveries to
            // a non-recursive alias — including the structural `Promise` body
            // back to `Promise<…>`, the legitimate reduction — are still
            // followed.
            if let Some(alias) = self.try_recover_application_from_display_alias(current)
                && !self.application_is_recursive_alias(alias)
            {
                current = alias;
            }

            let Some(substituted) = self.alias_application_substituted_body(current) else {
                break;
            };
            let next = match self.interner().lookup(substituted)? {
                TypeData::Application(_) => substituted,
                TypeData::Conditional(cond_id) => {
                    let Some(next) = self.with_optional_meta_rereduce_recursion_identity(
                        current,
                        current,
                        |evaluator| {
                            let cond = evaluator.interner().get_conditional(cond_id);
                            if !evaluator.type_contains_infer(cond.extends_type) {
                                return None;
                            }
                            let cond_extends = cond.extends_type;
                            let cond_true = cond.true_type;
                            let check_eval = evaluator.evaluate(cond.check_type);
                            let mut checker = evaluator.conditional_subtype_checker();
                            checker.allow_bivariant_rest = true;
                            let mut bindings = FxHashMap::default();
                            let mut visited = InferPatternVisited::default();
                            if !evaluator.match_infer_pattern(
                                check_eval,
                                cond_extends,
                                &mut bindings,
                                &mut visited,
                                &mut checker,
                            ) {
                                return None;
                            }
                            let result = evaluator.substitute_infer(cond_true, &bindings);
                            let result = evaluator.evaluate(result);
                            Some(
                                evaluator
                                    .try_recover_application_from_display_alias(result)
                                    .filter(|&recovered| {
                                        !evaluator.application_is_recursive_alias(recovered)
                                    })
                                    .unwrap_or(result),
                            )
                        },
                    ) else {
                        break;
                    };
                    if next == current {
                        break;
                    }
                    next
                }
                TypeData::Intersection(_)
                    if self.is_concrete_application_led_intersection(substituted) =>
                {
                    let Some(TypeData::Intersection(members)) = self.interner().lookup(substituted)
                    else {
                        break;
                    };
                    let members = self.interner().type_list(members);
                    members.first().copied()?
                }
                _ => break,
            };
            current = next;
        }
        (current != ty).then_some(current)
    }
}
