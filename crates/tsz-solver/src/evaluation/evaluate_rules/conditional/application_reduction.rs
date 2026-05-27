use super::super::super::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

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
            if let Some(alias) = self.try_recover_application_from_display_alias(current) {
                current = alias;
            }

            let Some(substituted) = self.alias_application_substituted_body(current) else {
                break;
            };
            let next = match self.interner().lookup(substituted)? {
                TypeData::Application(_) => substituted,
                TypeData::Conditional(cond_id) => {
                    let cond = self.interner().get_conditional(cond_id);
                    if !self.type_contains_infer(cond.extends_type) {
                        break;
                    }
                    let cond_extends = cond.extends_type;
                    let cond_true = cond.true_type;
                    let check_eval = self.evaluate(cond.check_type);
                    let mut checker = self.conditional_subtype_checker();
                    checker.allow_bivariant_rest = true;
                    let mut bindings = FxHashMap::default();
                    let mut visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        check_eval,
                        cond_extends,
                        &mut bindings,
                        &mut visited,
                        &mut checker,
                    ) {
                        break;
                    }
                    let result = self.substitute_infer(cond_true, &bindings);
                    let result = self
                        .try_recover_application_from_display_alias(result)
                        .unwrap_or(result);
                    if result == current {
                        break;
                    }
                    result
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
