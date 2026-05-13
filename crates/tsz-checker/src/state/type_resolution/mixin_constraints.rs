use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallSignature, TypeId};

impl<'a> CheckerState<'a> {
    fn collect_constructor_return_check_signatures(
        &mut self,
        constructor_type: TypeId,
        signatures: &mut Vec<CallSignature>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        let evaluated = self.evaluate_application_type(constructor_type);
        let resolved = self.resolve_lazy_type(evaluated);
        if !visited.insert(resolved) {
            return;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, resolved)
        {
            signatures.extend(sigs);
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, resolved)
        {
            for member in members.iter().copied() {
                self.collect_constructor_return_check_signatures(member, signatures, visited);
            }
        }
    }

    pub(crate) fn heritage_call_has_invalid_mixin_constructor_constraint(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        let callee_type = self.get_type_of_node(call.expression);
        if let Some(function_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, callee_type)
            && function_shape.type_params.iter().any(|type_param| {
                let Some(constraint) = type_param.constraint else {
                    return false;
                };
                self.construct_constraint_has_invalid_mixin_rest(constraint)
            })
        {
            return true;
        }

        crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, callee_type)
            .is_some_and(|call_sigs| {
                call_sigs.iter().any(|call_sig| {
                    call_sig.type_params.iter().any(|type_param| {
                        let Some(constraint) = type_param.constraint else {
                            return false;
                        };
                        self.construct_constraint_has_invalid_mixin_rest(constraint)
                    })
                })
            })
    }

    fn construct_constraint_has_invalid_mixin_rest(&mut self, constraint: TypeId) -> bool {
        let mut construct_sigs = Vec::new();
        self.collect_constructor_return_check_signatures(
            constraint,
            &mut construct_sigs,
            &mut FxHashSet::default(),
        );
        construct_sigs.iter().any(|sig| {
            if sig.params.is_empty() {
                return false;
            }
            !(sig.params.len() == 1
                && sig.params[0].rest
                && !sig.params[0].optional
                && (sig.params[0].type_id == TypeId::ANY
                    || matches!(
                        crate::query_boundaries::class_type::array_element_type(
                            self.ctx.types,
                            sig.params[0].type_id,
                        ),
                        Some(element) if element == TypeId::ANY
                    )))
        })
    }
}
