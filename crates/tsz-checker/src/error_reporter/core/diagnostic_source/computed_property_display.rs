//! Computed property key display normalization for object literal diagnostics.
//!
//! When computed property names use expressions like `[""+"foo"]` (string concat)
//! or `[+"foo"]` (unary plus), tsc collapses them to index signatures in TS2322.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Collect computed properties that would fall back to raw expression display.
    /// Returns (`fallback_indices`, `value_types`, `is_number_key`).
    pub(crate) fn collect_fallback_computed_properties(
        &mut self,
        literal: &tsz_parser::parser::node::LiteralExprData,
    ) -> (Vec<NodeIndex>, Vec<TypeId>, bool) {
        let mut fallback_indices = Vec::new();
        let mut value_types = Vec::new();
        let mut is_number_key = false;

        for child_idx in literal.elements.nodes.iter().copied() {
            let Some(child) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            let (name_idx, value_idx) = match child.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(p) = self.ctx.arena.get_property_assignment(child) else {
                        continue;
                    };
                    (p.name, p.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(p) = self.ctx.arena.get_shorthand_property(child) else {
                        continue;
                    };
                    (p.name, p.name)
                }
                _ => continue,
            };
            let Some(name_node) = self.ctx.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            if self.get_member_name_display_text(name_idx).is_some() {
                continue;
            }

            if let Some(computed) = self.ctx.arena.get_computed_property(name_node) {
                let expr = self.ctx.arena.get(computed.expression);
                if expr.is_some_and(|n| {
                    n.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                        && self.ctx.arena.get_unary_expr(n).is_some_and(|p| {
                            p.operator == tsz_scanner::SyntaxKind::PlusToken as u16
                        })
                }) {
                    is_number_key = true;
                }
                fallback_indices.push(child_idx);
                let value_type = self.get_type_of_node(value_idx);
                if value_type != TypeId::ERROR {
                    let widened = self.widen_type_for_display(value_type);
                    if !value_types.contains(&widened) {
                        value_types.push(widened);
                    }
                }
            }
        }
        (fallback_indices, value_types, is_number_key)
    }
}
