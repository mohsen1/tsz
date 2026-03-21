//! Super keyword type computation.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of the `super` keyword.
    ///
    /// Computes the type of `super` expressions:
    /// - `super()` calls: returns the base class constructor type
    /// - `super.property` access: returns the base class instance type
    /// - Static context: returns constructor type
    /// - Instance context: returns instance type
    pub(crate) fn get_type_of_super_keyword(&mut self, idx: NodeIndex) -> TypeId {
        // Check super expression validity and emit any errors
        self.check_super_expression(idx);

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return TypeId::ERROR;
        };

        let mut extends_expr_idx = NodeIndex::NONE;
        let mut extends_type_args = None;
        if let Some(current_class) = self.ctx.arena.get_class_at(class_info.class_idx)
            && let Some(heritage_clauses) = &current_class.heritage_clauses
        {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    extends_expr_idx = expr_type_args.expression;
                    extends_type_args = expr_type_args.type_arguments.clone();
                } else {
                    extends_expr_idx = type_idx;
                }
                break;
            }
        }

        // Detect `super(...)` usage by checking if the parent is a CallExpression whose callee is `super`.
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)))
            .and_then(|(parent_idx, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = self.ctx.arena.get_call_expr(parent_node)?;
                Some(call.expression == idx && parent_idx.is_some())
            })
            .unwrap_or(false);

        let is_static_context = self.find_enclosing_static_block(idx).is_some()
            || self.is_this_in_static_class_member(idx);

        if is_super_call || is_static_context {
            if extends_expr_idx.is_some()
                && let Some(ctor_type) = self.base_constructor_type_from_expression(
                    extends_expr_idx,
                    extends_type_args.as_ref(),
                )
            {
                return ctor_type;
            }

            let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                return TypeId::ERROR;
            };
            return self.get_class_constructor_type(base_class_idx, base_class);
        }

        if extends_expr_idx.is_some()
            && let Some(instance_type) = self
                .base_instance_type_from_expression(extends_expr_idx, extends_type_args.as_ref())
        {
            return instance_type;
        }

        let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return TypeId::ERROR;
        };

        self.get_class_instance_type(base_class_idx, base_class)
    }
}
