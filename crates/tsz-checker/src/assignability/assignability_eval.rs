use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_lazy_alias_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)?;
        if !self
            .ctx
            .definition_store
            .get(def_id)
            .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        {
            return None;
        }
        let body = self.ctx.get_semantic_def_body(def_id)?;
        if body == TypeId::ERROR || body == TypeId::ANY || body == type_id {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != TypeId::ERROR && evaluated != TypeId::ANY && evaluated != type_id {
            Some(evaluated)
        } else {
            Some(self.evaluate_type_for_assignability_inner(body))
        }
    }
}

impl<'a> CheckerState<'a> {
    /// Normalize a *concrete* deferred `keyof` relation target to its
    /// evaluation before check + report.
    ///
    /// tsc's `getIndexType` reduces `keyof` over a fully concrete operand at
    /// type creation, so a concrete `keyof` never reaches
    /// `checkTypeAssignableTo`: both the relation and the diagnostic pair
    /// carry the reduced key union. tsz can still hold the deferred `KeyOf`
    /// here (a type alias body like `keyof typeof E` is lowered before the
    /// operand's members materialize), which leaves the reported target
    /// rendered as `keyof ...` and makes the display layer widen a literal
    /// source to its primitive. The relation outcome is unchanged (the solver
    /// evaluates `keyof` internally either way). Generic `keyof T` classifies
    /// as `Resolved` and is left untouched.
    pub(super) fn normalize_concrete_keyof_relation_target(&mut self, target: TypeId) -> TypeId {
        use crate::query_boundaries::assignability::{
            AssignabilityEvalKind, classify_for_assignability_eval, is_keyof_type,
        };
        if !is_keyof_type(self.ctx.types, target)
            || !matches!(
                classify_for_assignability_eval(self.ctx.types, target),
                AssignabilityEvalKind::NeedsEnvEval
            )
        {
            return target;
        }
        let evaluated = self.evaluate_type_for_assignability(target);
        if evaluated != TypeId::ERROR
            && evaluated != TypeId::ANY
            && !is_keyof_type(self.ctx.types, evaluated)
        {
            evaluated
        } else {
            target
        }
    }
}
