//! Argument helpers for call-result diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn build_expanded_args_for_error(&mut self, args: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg_idx in args {
            if let Some(n) = self.ctx.arena.get(arg_idx)
                && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_expression) = self
                    .ctx
                    .arena
                    .get_spread(n)
                    .map(|spread| spread.expression)
                    .or_else(|| self.ctx.arena.get_children(arg_idx).first().copied())
            {
                let spread_type = self.get_type_of_node(spread_expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, spread_type)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, elems.len()));
                    continue;
                }
                // Array literal spreads have known element count; expand them.
                let inner_idx = self.ctx.arena.skip_parenthesized(spread_expression);
                if let Some(expr_node) = self.ctx.arena.get(inner_idx)
                    && expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, literal.elements.nodes.len()));
                    continue;
                }
            }
            expanded.push(arg_idx);
        }
        expanded
    }
}
