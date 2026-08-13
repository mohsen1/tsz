use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Whether `idx` is a write target inside the LHS *pattern* of a
    /// destructuring assignment (`[a, ...b] = src`, `({ x } = src)`,
    /// `for ([a] of xs)`).
    ///
    /// Such an anchor names the type being written **to**; the value flowing
    /// into it is a computed slice (an element type, a rest array, an omitted
    /// rest object) with no expression node of its own. `tsc` renders the
    /// message's source side from that computed type (`typeToString`), so no
    /// source-expression-derived display repaint may run for these anchors —
    /// deriving one from the anchor repaints the source with the *target's*
    /// declared annotation, and deriving one from the enclosing assignment's
    /// RHS repaints a slice with the *whole* source's annotation.
    ///
    /// A plain assignment LHS (`x = y`, `obj.p = y`) is NOT covered: the walk
    /// must cross at least one pattern wrapper before reaching the assignment,
    /// so established non-destructuring display paths keep their behavior. A
    /// node inside a pattern *default* (`[a = dflt] = src` — the `dflt`) is a
    /// genuine source expression and is likewise not covered.
    pub(in crate::error_reporter) fn anchor_is_destructuring_assignment_write_target(
        &self,
        idx: NodeIndex,
    ) -> bool {
        // An anchor that is itself an `=` binary is an in-pattern default
        // (`[k = false] of map`, `[a = 1] = src`) or a plain assignment: the
        // judgement rendered at it compares the *default/RHS expression*
        // against the target, and that expression is a genuine source node
        // (whose fresh literals the display machinery widens, matching tsc).
        if self
            .ctx
            .arena
            .get(idx)
            .filter(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
            .and_then(|node| self.ctx.arena.get_binary_expr(node))
            .is_some_and(|bin| bin.operator_token == SyntaxKind::EqualsToken as u16)
        {
            return false;
        }
        let mut current = idx;
        let mut crossed_pattern = false;
        for _ in 0..32 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                {
                    crossed_pattern = true;
                    current = parent;
                }
                k if k == syntax_kind_ext::SPREAD_ELEMENT
                    || k == syntax_kind_ext::SPREAD_ASSIGNMENT
                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION =>
                {
                    current = parent;
                }
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let Some(bin) = self.ctx.arena.get_binary_expr(parent_node) else {
                        return false;
                    };
                    if bin.operator_token != SyntaxKind::EqualsToken as u16 || bin.left != current {
                        return false;
                    }
                    // Reaching an `=` LHS before crossing any pattern means
                    // the anchor is a plain assignment target (`x = y`) or an
                    // in-pattern default's target (`[k = false]`): both
                    // judgements rendered there compare a *real* RHS/default
                    // expression, so the source-expression machinery (and its
                    // fresh-literal widening) must stay engaged.
                    return crossed_pattern;
                }
                k if (k == syntax_kind_ext::FOR_IN_STATEMENT
                    || k == syntax_kind_ext::FOR_OF_STATEMENT)
                    && crossed_pattern =>
                {
                    return self
                        .ctx
                        .arena
                        .get_for_in_of(parent_node)
                        .is_some_and(|for_data| for_data.initializer == current);
                }
                _ => return false,
            }
        }
        false
    }

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
        // A destructuring-pattern write target receives a computed slice of
        // the RHS, not the RHS itself — walking up to the enclosing
        // assignment's RHS would attribute the whole source's
        // annotation/display to that slice.
        if self.anchor_is_destructuring_assignment_write_target(anchor_idx) {
            return None;
        }
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
