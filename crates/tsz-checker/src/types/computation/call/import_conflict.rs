use crate::call_checker::CallableContext;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn callee_is_import_conflict_module(&self, callee_expression: NodeIndex) -> bool {
        let Some(callee_ident) = self.ctx.arena.get_identifier_at(callee_expression) else {
            return false;
        };
        self.ctx
            .import_conflict_names
            .contains(callee_ident.escaped_text.as_str())
            && self
                .ctx
                .binder
                .get_symbols()
                .find_all_by_name(&callee_ident.escaped_text)
                .iter()
                .copied()
                .any(|candidate_id| {
                    self.ctx
                        .binder
                        .get_symbol(candidate_id)
                        .is_some_and(|candidate| {
                            candidate.has_any_flags(symbol_flags::MODULE)
                                && candidate.declarations.iter().copied().any(|decl_idx| {
                                    self.ctx.arena.get(decl_idx).is_some_and(|node| {
                                        node.kind == syntax_kind_ext::MODULE_DECLARATION
                                    })
                                })
                        })
                })
    }

    pub(super) fn error_not_callable_and_collect_any_args(
        &mut self,
        callee_type: TypeId,
        callee_expression: NodeIndex,
        args: &[NodeIndex],
    ) -> TypeId {
        self.error_not_callable_at(callee_type, callee_expression);
        self.collect_call_argument_types_with_context(
            args,
            |_i, _arg_count| Some(TypeId::ANY),
            false,
            None,
            CallableContext::none(),
        );
        TypeId::ERROR
    }
}
