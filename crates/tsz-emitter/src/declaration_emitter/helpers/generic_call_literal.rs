//! Literal inference helpers for generic call expression declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{FunctionData, NodeArena};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_reused_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.call_expression_returned_local_class_constructor_text(expr_idx, false)
            .or_else(|| {
                self.super_method_call_return_type_text(expr_idx)
                    .or_else(|| self.call_expression_source_return_type_text(expr_idx))
                    .or_else(|| self.call_expression_declared_return_type_text(expr_idx))
                    .or_else(|| self.generic_call_literal_type_text(expr_idx))
            })
    }

    pub(in crate::declaration_emitter) fn generic_call_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if !self.call_expression_has_generic_callee(expr_idx) {
            return None;
        }

        let type_id = self.get_node_type_or_names(&[expr_idx])?;
        if type_id == tsz_solver::types::TypeId::ANY || type_id == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }

        let interner = self.type_interner?;
        tsz_solver::type_queries::is_literal_type(interner, type_id)
            .then(|| self.print_type_id_for_inferred_declaration(type_id))
    }

    fn call_expression_has_generic_callee(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        if self.function_expression_has_type_parameters(call.expression) {
            return true;
        }

        let Some(sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            func.type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
                .then_some(())
        })
        .is_some()
    }

    fn function_expression_has_type_parameters(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        self.arena
            .get_function(expr_node)
            .and_then(|func| func.type_parameters.as_ref())
            .is_some_and(|params| !params.nodes.is_empty())
    }
}

fn callable_function_from_symbol_decl(
    source_arena: &NodeArena,
    decl_idx: NodeIndex,
) -> Option<&FunctionData> {
    if let Some(func) = source_arena
        .get(decl_idx)
        .and_then(|node| source_arena.get_function(node))
    {
        return Some(func);
    }

    let mut current = decl_idx;
    for _ in 0..8 {
        let node = source_arena.get(current)?;
        if let Some(var_decl) = source_arena.get_variable_declaration(node) {
            let initializer_node = source_arena.get(var_decl.initializer)?;
            if initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return source_arena.get_function(initializer_node);
            }
        }
        current = source_arena.parent_of(current)?;
    }

    None
}
