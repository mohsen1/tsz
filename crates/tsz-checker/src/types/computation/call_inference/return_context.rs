use crate::query_boundaries::common::{self, TypeSubstitution};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::Atom;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn collect_awaited_return_context_substitution_by_shape(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut TypeSubstitution,
        depth: u8,
    ) -> bool {
        if depth > 8 {
            return false;
        }

        if let Some(awaited_arg) = self.awaited_application_arg(source) {
            for referenced in common::collect_referenced_types(self.ctx.types, awaited_arg) {
                let Some(tp) = common::type_param_info(self.ctx.types, referenced) else {
                    continue;
                };
                if tracked_type_params.contains(&tp.name)
                    && target != TypeId::UNKNOWN
                    && target != TypeId::ERROR
                    && !self.target_contains_blocking_return_context_type_params(
                        target,
                        tracked_type_params,
                    )
                {
                    substitution.insert(tp.name, target);
                    return true;
                }
            }
        }

        if let (Some(source_elems), Some(target_elems)) = (
            common::tuple_elements(self.ctx.types, source),
            common::tuple_elements(self.ctx.types, target),
        ) && source_elems.len() == target_elems.len()
        {
            let before_len = substitution.len();
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_awaited_return_context_substitution_by_shape(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    depth + 1,
                );
            }
            return substitution.len() > before_len;
        }

        if let (Some(source_elem), Some(target_elem)) = (
            common::array_element_type(self.ctx.types, source),
            common::array_element_type(self.ctx.types, target),
        ) {
            if let Some(awaited_arg) = self.awaited_application_arg(source_elem)
                && let Some((indexed_object, indexed_key)) =
                    common::index_access_parts(self.ctx.types, awaited_arg)
                && common::is_number_type(self.ctx.types, indexed_key)
                && let Some(tp) = common::type_param_info(self.ctx.types, indexed_object)
                && tracked_type_params.contains(&tp.name)
                && target != TypeId::UNKNOWN
                && target != TypeId::ERROR
                && !self.target_contains_blocking_return_context_type_params(
                    target,
                    tracked_type_params,
                )
            {
                substitution.insert(tp.name, target);
                return true;
            }
            return self.collect_awaited_return_context_substitution_by_shape(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                depth + 1,
            );
        }

        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            common::application_info(self.ctx.types, source),
            common::application_info(self.ctx.types, target),
        ) && self.return_context_application_bases_match(source_base, target_base)
            && source_args.len() == target_args.len()
        {
            let before_len = substitution.len();
            for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                self.collect_awaited_return_context_substitution_by_shape(
                    *source_arg,
                    *target_arg,
                    tracked_type_params,
                    substitution,
                    depth + 1,
                );
            }
            return substitution.len() > before_len;
        }

        false
    }
}
