use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    fn terminal_assignment_source_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let mut current = expr_idx;
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 256 {
                return current;
            }

            let expr = self.ctx.arena.skip_parenthesized(current);
            let Some(node) = self.ctx.arena.get(expr) else {
                return current;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return expr;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(node) else {
                return expr;
            };
            if !self.is_assignment_operator(bin.operator_token) {
                return expr;
            }
            current = bin.right;
        }
    }

    pub(in crate::error_reporter) fn assignment_source_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(self.terminal_assignment_source_expression(bin.right));
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(self.terminal_assignment_source_expression(bin.right));
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(decl.initializer));
                }
                k if k == syntax_kind_ext::BINDING_ELEMENT => {
                    let elem = self.ctx.arena.get_binding_element(node)?;
                    if elem.initializer.is_some() {
                        return Some(self.terminal_assignment_source_expression(elem.initializer));
                    }
                    // Fall through to walk further up if the binding element has
                    // no own default — the parameter / variable initializer is
                    // the relevant source.
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(param.initializer));
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(node)?;
                    return prop.initializer.is_some().then_some(prop.initializer);
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(node)?;
                    return prop.name.is_some().then_some(prop.name);
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    // A shorthand method is both the property name and value.
                    // Use the declaration so diagnostics display the method
                    // call signature instead of resolving the name as a value.
                    return Some(current);
                }
                k if k == syntax_kind_ext::RETURN_STATEMENT => {
                    let ret = self.ctx.arena.get_return_statement(node)?;
                    return ret.expression.is_some().then_some(ret.expression);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(in crate::error_reporter) fn assignment_target_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(bin.left);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(bin.left);
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl.name.is_some().then_some(decl.name);
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param.name.is_some().then_some(param.name);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(crate) fn assignment_source_is_return_expression(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                && let Some(func) = self.ctx.arena.get_function(parent_node)
                && func.body == current
                && node.kind != syntax_kind_ext::BLOCK
            {
                return true;
            }
            current = ext.parent;
        }

        false
    }
}
