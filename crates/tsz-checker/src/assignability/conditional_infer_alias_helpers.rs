use crate::query_boundaries::application_keyof::type_param_info;
use crate::query_boundaries::state::type_resolution::{get_application_info, get_lazy_def_id};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn same_type_alias_application_uses_conditional_infer(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_info =
            self.application_info_preferring_display_alias_for_conditional_infer(source);
        let target_info =
            self.application_info_preferring_display_alias_for_conditional_infer(target);
        let Some((source_base, source_args)) = source_info else {
            return false;
        };
        let Some((target_base, target_args)) = target_info else {
            return false;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return false;
        }
        let Some(def_id) =
            crate::query_boundaries::conditional_infer_alias::application_base_def_id(
                self.ctx.types,
                &self.ctx,
                source_base,
            )
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return false;
        }
        crate::query_boundaries::conditional_infer_alias::application_base_uses_conditional_infer(
            self.ctx.types,
            &self.ctx,
            source_base,
        )
    }

    pub(crate) fn conditional_infer_alias_covariant_source_constraint_accepts(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) =
            self.application_info_preferring_display_alias_for_conditional_infer(source)
        else {
            return false;
        };
        let Some((target_base, target_args)) =
            self.application_info_preferring_display_alias_for_conditional_infer(target)
        else {
            return false;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return false;
        }
        if !self.same_type_alias_application_uses_conditional_infer(source, target) {
            return false;
        }
        let Some(def_id) =
            crate::query_boundaries::conditional_infer_alias::application_base_def_id(
                self.ctx.types,
                &self.ctx,
                source_base,
            )
        else {
            return false;
        };
        let variances =
            crate::query_boundaries::variance::compute_type_param_variances_with_resolver_cached(
                self.ctx.types,
                &self.ctx,
                self.ctx.types,
                def_id,
            );
        let Some(variances) = variances else {
            return false;
        };
        if variances.len() != source_args.len() {
            return false;
        }

        let mut saw_differing_arg = false;
        for (i, (&source_arg, &target_arg)) in
            source_args.iter().zip(target_args.iter()).enumerate()
        {
            if source_arg == target_arg {
                continue;
            }
            saw_differing_arg = true;
            if !variances
                .get(i)
                .is_some_and(|variance| variance.is_covariant())
            {
                return false;
            }
            let Some(source_param) = type_param_info(self.ctx.types, source_arg) else {
                return false;
            };
            let Some(constraint) = source_param.constraint else {
                return false;
            };
            if constraint != target_arg && !self.is_assignable_to(constraint, target_arg) {
                return false;
            }
        }

        saw_differing_arg
    }

    pub(crate) fn application_info_for_alias_argument_rejection(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        self.application_display_info(type_id)
            .or_else(|| self.non_generic_alias_body_application_info(type_id))
    }

    fn application_info_preferring_display_alias_for_conditional_infer(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        self.ctx
            .types
            .get_display_alias(type_id)
            .and_then(|alias| get_application_info(self.ctx.types, alias))
            .or_else(|| self.application_info_for_alias_argument_rejection(type_id))
    }

    fn non_generic_alias_body_application_info(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        let def_id = get_lazy_def_id(self.ctx.types, type_id)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        get_application_info(self.ctx.types, def.body?)
    }

    pub(crate) fn application_display_info(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        self.application_info_or_display_alias(type_id)
    }
}
