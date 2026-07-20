use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_recursive_alias_application(&mut self, base: TypeId, args: &[TypeId]) -> bool {
        let application = self.ctx.types.application(base, args.to_vec());
        crate::query_boundaries::recursive_alias::is_recursive_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            application,
        )
    }

    pub(crate) fn type_alias_args_are_unwitnessed(
        &self,
        def_id: tsz_solver::def::DefId,
        arg_len: usize,
    ) -> bool {
        crate::query_boundaries::variance::compute_type_param_variances_with_resolver_cached(
            self.ctx.types.as_type_database(),
            &self.ctx,
            self.ctx.types,
            def_id,
        )
        .as_ref()
        .is_some_and(|variances| {
            variances.len() == arg_len && variances.iter().all(|v| v.is_independent())
        })
    }

    pub(crate) fn same_base_generic_mapped_application_has_type_param_arg(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((_def_id, source_args, target_args)) =
            self.same_base_generic_mapped_application_parts(source, target)
        else {
            return false;
        };
        source_args.iter().chain(target_args.iter()).any(|&arg| {
            crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, arg)
        })
    }

    pub(crate) fn same_base_generic_mapped_application_variance_accepts(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((def_id, source_args, target_args)) =
            self.same_base_generic_mapped_application_parts(source, target)
        else {
            return false;
        };
        if source_args
            .iter()
            .zip(target_args.iter())
            .all(|(source_arg, target_arg)| source_arg == target_arg)
        {
            return false;
        }
        if source_args
            .iter()
            .zip(target_args.iter())
            .any(|(&source_arg, &target_arg)| {
                (source_arg.is_any() && target_arg == TypeId::NEVER)
                    || (source_arg == TypeId::NEVER && target_arg.is_any())
            })
        {
            // The option-aware solver classifier owns this exceptional pair;
            // a partial declared mask cannot safely accept it here.
            return false;
        }

        let Some(variances) =
            crate::query_boundaries::variance::compute_type_param_variances_with_resolver_cached(
                self.ctx.types.as_type_database(),
                &self.ctx,
                self.ctx.types,
                def_id,
            )
        else {
            return false;
        };
        if variances.len() != source_args.len() {
            return false;
        }

        source_args
            .iter()
            .copied()
            .zip(target_args.iter().copied())
            .zip(variances.iter().copied())
            .all(|((source_arg, target_arg), variance)| {
                if source_arg == target_arg {
                    return true;
                }
                if variance.rejection_unreliable() || variance.needs_structural_fallback() {
                    return false;
                }
                if variance.is_contravariant() {
                    self.is_assignable_to(target_arg, source_arg)
                } else if variance.is_invariant() {
                    self.is_assignable_to(source_arg, target_arg)
                        && self.is_assignable_to(target_arg, source_arg)
                } else {
                    self.is_assignable_to(source_arg, target_arg)
                }
            })
    }

    fn same_base_generic_mapped_application_parts(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<(tsz_solver::def::DefId, Vec<TypeId>, Vec<TypeId>)> {
        let (source_base, source_args) =
            self.application_info_for_alias_argument_rejection(source)?;
        let (target_base, target_args) =
            self.application_info_for_alias_argument_rejection(target)?;
        if source_base != target_base || source_args.len() != target_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::conditional_infer_alias::application_base_def_id(
            self.ctx.types.as_type_database(),
            &self.ctx,
            source_base,
        )?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        def.body
            .is_some_and(|body| {
                crate::query_boundaries::assignability::is_generic_mapped_type(self.ctx.types, body)
            })
            .then_some((def_id, source_args, target_args))
    }
}
