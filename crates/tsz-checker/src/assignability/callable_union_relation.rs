//! Shared callable-to-union assignability compatibility helpers.

use crate::state::CheckerState;
use tsz_solver::{ParamInfo, TypeId};

impl<'a> CheckerState<'a> {
    pub(in crate::assignability_domain) fn callable_source_satisfies_union_callable_arm(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_signatures = self.callable_relation_signatures(source, false);
        if source_signatures.is_empty() {
            return false;
        }

        let target = self.evaluate_type_with_env(target);
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, target)
        else {
            return false;
        };

        let mut target_signatures = Vec::new();
        for member in members {
            target_signatures.extend(self.callable_relation_signatures(member, true));
        }
        if target_signatures.is_empty() {
            return false;
        }

        source_signatures
            .iter()
            .any(|(source_params, source_return)| {
                target_signatures
                    .iter()
                    .any(|(target_params, target_return)| {
                        self.callable_relation_params_compatible(source_params, target_params)
                            && self
                                .diagnostic_relation_boolean_guard(*source_return, *target_return)
                    })
            })
    }

    fn callable_relation_signatures(
        &mut self,
        type_id: TypeId,
        require_plain_callable: bool,
    ) -> Vec<(Vec<ParamInfo>, TypeId)> {
        let mut signatures = Vec::new();
        let mut stack = vec![type_id];
        let mut seen = rustc_hash::FxHashSet::default();

        while let Some(candidate) = stack.pop() {
            if !seen.insert(candidate) {
                continue;
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, candidate)
            {
                signatures.push((shape.params.clone(), shape.return_type));
            }
            if let Some(callable) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, candidate)
            {
                let has_static_requirements = !callable.properties.is_empty()
                    || callable.string_index.is_some()
                    || callable.number_index.is_some()
                    || callable.symbol.is_some();
                if !require_plain_callable || !has_static_requirements {
                    signatures.extend(
                        callable
                            .call_signatures
                            .iter()
                            .map(|sig| (sig.params.clone(), sig.return_type)),
                    );
                }
            }
            if let Some(alias) = self.ctx.types.get_display_alias(candidate) {
                stack.push(alias);
            }
            let evaluated = self.evaluate_type_with_env(candidate);
            if evaluated != candidate {
                stack.push(evaluated);
            }
            let lazy_resolved = self.resolve_lazy_type(candidate);
            if lazy_resolved != candidate {
                stack.push(lazy_resolved);
            }
        }

        signatures
    }

    fn callable_relation_params_compatible(
        &mut self,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
    ) -> bool {
        let source_required = source_params
            .iter()
            .filter(|param| param.is_required())
            .count();
        let target_has_rest = target_params.iter().any(|param| param.rest);
        if source_required > target_params.len()
            || (!target_has_rest && source_params.len() > target_params.len())
        {
            return false;
        }

        source_params
            .iter()
            .zip(target_params.iter())
            .all(|(source_param, target_param)| {
                self.diagnostic_relation_boolean_guard(target_param.type_id, source_param.type_id)
            })
    }
}
