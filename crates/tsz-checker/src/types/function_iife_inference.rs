//! IIFE argument-based parameter inference helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn infer_iife_parameter_type_from_arguments(
        &mut self,
        func_idx: NodeIndex,
        param_index: usize,
        is_rest: bool,
        is_optional: bool,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_EXPRESSION, NEW_EXPRESSION, PARENTHESIZED_EXPRESSION,
        };

        if !self.ctx.arena.is_immediately_invoked(func_idx) {
            return None;
        }

        let mut callee = func_idx;
        for _ in 0..100 {
            let ext = self.ctx.arena.get_extended(callee)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent = ext.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == PARENTHESIZED_EXPRESSION {
                callee = parent;
                continue;
            }
            if parent_node.kind != CALL_EXPRESSION && parent_node.kind != NEW_EXPRESSION {
                return None;
            }
            let call = self.ctx.arena.get_call_expr(parent_node)?;
            if call.expression != callee {
                return None;
            }

            let args = call.arguments.as_ref().map(|a| &a.nodes);
            if is_rest {
                let Some(args) = args else {
                    return Some(self.ctx.types.array(TypeId::NEVER));
                };
                if param_index >= args.len() {
                    return Some(self.ctx.types.array(TypeId::NEVER));
                }
                let mut arg_types = Vec::with_capacity(args.len() - param_index);
                for &arg_idx in &args[param_index..] {
                    arg_types.push(self.get_type_of_node(arg_idx));
                }
                let elem = if arg_types.is_empty() {
                    TypeId::NEVER
                } else if arg_types.len() == 1 {
                    arg_types[0]
                } else {
                    self.ctx.types.factory().union(arg_types)
                };
                return Some(self.ctx.types.array(elem));
            }

            let Some(args) = args else {
                return is_optional.then_some(TypeId::UNDEFINED);
            };
            if let Some(&arg_idx) = args.get(param_index) {
                return Some(self.get_type_of_node(arg_idx));
            }
            return is_optional.then_some(TypeId::UNDEFINED);
        }
        None
    }
}
