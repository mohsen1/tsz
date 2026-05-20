use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, ParamInfo, TypeId};

use super::super::complex::is_contextually_sensitive;

impl<'a> CheckerState<'a> {
    pub(super) fn fresh_direct_function_call_signature(
        &mut self,
        callee_expression: NodeIndex,
    ) -> Option<tsz_solver::CallSignature> {
        let callee_node = self.ctx.arena.get(callee_expression)?;
        if callee_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(callee_expression)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let function_decl_count = symbol
            .all_declarations()
            .into_iter()
            .filter(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
            })
            .count();
        if function_decl_count > 1 {
            return None;
        }
        let decl_idx = symbol.value_declaration.into_option()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return None;
        }
        let func = self.ctx.arena.get_function(decl_node).cloned()?;

        let diagnostics_before = self.ctx.snapshot_diagnostics();
        let fresh_signature = self.call_signature_from_function(&func, decl_idx);
        self.ctx.rollback_diagnostics(&diagnostics_before);

        Some(fresh_signature)
    }

    pub(super) fn direct_function_call_type_for_type_argument_validation(
        &mut self,
        callee_expression: NodeIndex,
    ) -> Option<TypeId> {
        let fresh_signature = self.fresh_direct_function_call_signature(callee_expression)?;
        if fresh_signature.type_params.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().function(FunctionShape {
            type_params: fresh_signature.type_params,
            params: fresh_signature.params,
            this_type: fresh_signature.this_type,
            return_type: fresh_signature.return_type,
            type_predicate: fresh_signature.type_predicate,
            is_constructor: false,
            is_method: fresh_signature.is_method,
        }))
    }

    pub(super) fn refresh_callee_shape_type_param_constraints(
        &mut self,
        callee_expression: NodeIndex,
        mut shape: FunctionShape,
    ) -> FunctionShape {
        if shape.type_params.is_empty() {
            return shape;
        }

        let Some(fresh_signature) = self.fresh_direct_function_call_signature(callee_expression)
        else {
            return shape;
        };
        if fresh_signature.type_params.len() != shape.type_params.len() {
            return shape;
        }

        for (existing, fresh) in shape
            .type_params
            .iter_mut()
            .zip(fresh_signature.type_params.iter())
        {
            let existing_unresolved = existing.constraint.is_none_or(|constraint| {
                constraint == TypeId::UNKNOWN || constraint == TypeId::ERROR
            });
            let fresh_resolved = fresh.constraint.is_some_and(|constraint| {
                constraint != TypeId::UNKNOWN && constraint != TypeId::ERROR
            });
            if existing_unresolved && fresh_resolved {
                existing.constraint = fresh.constraint;
            }

            if existing.default.is_none() && fresh.default.is_some() {
                existing.default = fresh.default;
            }
        }

        shape
    }

    pub(super) fn setup_higher_order_callee_contextual_type(
        &mut self,
        callee_expression: NodeIndex,
        contextual_type: Option<TypeId>,
        args: &[NodeIndex],
    ) -> Option<TypeId> {
        let ctx_type = contextual_type?;
        if ctx_type == TypeId::ANY || ctx_type == TypeId::UNKNOWN || args.is_empty() {
            return None;
        }

        let mut expr_idx = callee_expression;
        loop {
            match self.ctx.arena.get(expr_idx) {
                Some(n) if n.kind == syntax_kind_ext::CALL_EXPRESSION => break,
                Some(n) if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    expr_idx = self.ctx.arena.get_parenthesized(n)?.expression;
                }
                _ => return None,
            }
        }

        if args.iter().copied().any(|arg_idx| {
            self.ctx
                .arena
                .get(arg_idx)
                .is_none_or(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT)
                || is_contextually_sensitive(self, arg_idx)
        }) {
            return None;
        }

        let snap = self.ctx.snapshot_diagnostics();
        let params = args
            .iter()
            .copied()
            .map(|arg_idx| ParamInfo {
                name: None,
                type_id: self.get_type_of_node_with_request(arg_idx, &TypingRequest::NONE),
                optional: false,
                rest: false,
            })
            .collect();
        self.ctx.rollback_diagnostics(&snap);

        Some(
            self.ctx
                .types
                .factory()
                .function(FunctionShape::new(params, ctx_type)),
        )
    }
}
