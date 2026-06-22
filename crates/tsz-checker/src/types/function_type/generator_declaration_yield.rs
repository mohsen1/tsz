use super::super::function_type_helpers::GeneratorBodyReturnCheckCtx;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(super) struct GeneratorDeclarationYieldCtx {
    pub(super) body: NodeIndex,
    pub(super) contextual_type: Option<TypeId>,
    pub(super) has_type_annotation: bool,
    pub(super) annotated_return_type: Option<TypeId>,
    pub(super) return_type: TypeId,
    pub(super) type_annotation: NodeIndex,
    pub(super) idx: NodeIndex,
    pub(super) function_is_async: bool,
    pub(super) early_yield_type: Option<TypeId>,
}

impl CheckerState<'_> {
    /// Recover inferred yield type for unannotated generator declarations.
    ///
    /// Function declarations defer their body walk to `check_function_declaration`
    /// so the full type-parameter scope chain is maintained. This suppressed
    /// pass computes only the signature's yield type; real body diagnostics still
    /// come from the later declaration check.
    pub(super) fn infer_generator_declaration_yield_type(
        &mut self,
        ctx: GeneratorDeclarationYieldCtx,
    ) -> Option<TypeId> {
        let GeneratorDeclarationYieldCtx {
            body,
            contextual_type,
            has_type_annotation,
            annotated_return_type,
            return_type,
            type_annotation,
            idx,
            function_is_async,
            early_yield_type,
        } = ctx;
        let yield_diag_snapshot = DiagnosticSpeculationSnapshot::new(&self.ctx);
        let saved_cf_context = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.label_stack.len(),
            self.ctx.had_outer_loop,
        );
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        let saved_yield_collection = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let saved_had_ts7057 = std::mem::replace(&mut self.ctx.generator_had_ts7057, false);

        let body_request = self.function_body_statement_request(false, contextual_type);
        self.check_function_body_statement_with_own_literal_context(body, &body_request);
        let inferred_yield = self.check_generator_body_return(GeneratorBodyReturnCheckCtx {
            is_generator: true,
            has_type_annotation,
            annotated_return_type,
            return_type,
            type_annotation,
            idx,
            function_is_async,
            early_yield_type,
            name_node: None,
            name_for_error: None,
        });
        let final_yield = self.concrete_or_empty_generator_yield(body, inferred_yield);

        self.ctx.generator_yield_operand_types = saved_yield_collection;
        self.ctx.generator_had_ts7057 = saved_had_ts7057;
        self.ctx.iteration_depth = saved_cf_context.0;
        self.ctx.switch_depth = saved_cf_context.1;
        self.ctx.label_stack.truncate(saved_cf_context.2);
        self.ctx.had_outer_loop = saved_cf_context.3;
        self.invalidate_function_body_for_param_retyping(body);
        yield_diag_snapshot.rollback(&mut self.ctx.diagnostic_state());
        final_yield
    }

    fn concrete_or_empty_generator_yield(
        &self,
        body: NodeIndex,
        inferred_yield: Option<TypeId>,
    ) -> Option<TypeId> {
        match inferred_yield {
            Some(yield_t)
                if yield_t != TypeId::NEVER || !self.generator_body_contains_yield(body, true) =>
            {
                Some(yield_t)
            }
            _ => None,
        }
    }

    /// Whether `node_idx` contains a `yield` / `yield*` for this generator, not
    /// one nested inside another function or class.
    fn generator_body_contains_yield(&self, node_idx: NodeIndex, is_root: bool) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if !is_root
            && matches!(
                node.kind,
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
            )
        {
            return false;
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.generator_body_contains_yield(child, false))
    }
}
