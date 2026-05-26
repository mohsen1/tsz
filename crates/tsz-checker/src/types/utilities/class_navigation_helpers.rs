//! Class/function AST navigation helpers used by checker utilities.

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Get class expression returned from a function body.
    ///
    /// Searches for return statements that return class expressions.
    pub(crate) fn returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(body_idx)?;
        if node.kind != syntax_kind_ext::BLOCK {
            return self.class_expression_from_expr(body_idx);
        }
        let block = self.ctx.arena.get_block(node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }
            if let Some(expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(expr_idx);
            }
            let expr_node = self.ctx.arena.get(ret.expression)?;
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                && let Some(class_idx) =
                    self.class_declaration_from_identifier_in_block(block, &ident.escaped_text)
            {
                return Some(class_idx);
            }
        }
        None
    }

    /// Find class declaration by identifier name in a block.
    ///
    /// Searches for class declarations with the given name.
    pub(crate) fn class_declaration_from_identifier_in_block(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        name: &str,
    ) -> Option<NodeIndex> {
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let class = self.ctx.arena.get_class(stmt)?;
            if class.name.is_none() {
                continue;
            }
            let ident = self.ctx.arena.get_identifier_at(class.name)?;
            if ident.escaped_text == name {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// Get class expression from any expression node.
    ///
    /// Unwraps parenthesized expressions and returns the class expression if found.
    pub(crate) fn class_expression_from_expr(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return Some(current);
            }
            return None;
        }
    }

    /// Get function declaration from callee expression.
    ///
    /// Returns the function declaration if the callee is a function with a body.
    pub(crate) fn function_decl_from_callee(&self, callee_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if func.body.is_some() {
                return Some(decl_idx);
            }
        }

        if symbol.value_declaration.is_some() {
            let decl_idx = symbol.value_declaration;
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if func.body.is_some() {
                return Some(decl_idx);
            }
        }

        None
    }

    pub(crate) fn function_like_decl_from_callee(
        &mut self,
        callee_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if let Some(func_decl_idx) = self.function_decl_from_callee(callee_idx) {
            return Some(func_decl_idx);
        }

        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let method_name = self
            .ctx
            .arena
            .get_identifier(name_node)?
            .escaped_text
            .clone();
        let object_type = self.get_type_of_node(access.expression);
        let class_idx = self
            .get_class_decl_for_display_type(object_type)
            .map(|(class_idx, _)| class_idx)
            .or_else(|| {
                let object_node = self.ctx.arena.get(access.expression)?;
                self.ctx.arena.get_identifier(object_node)?;
                let sym_id = self.resolve_identifier_symbol(access.expression)?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                let value_decl = symbol.value_declaration;
                let decl_iter = value_decl
                    .is_some()
                    .then_some(value_decl)
                    .into_iter()
                    .chain(symbol.declarations.iter().copied());
                for decl_idx in decl_iter {
                    let var_decl_idx = if self
                        .ctx
                        .arena
                        .get_variable_declaration_at(decl_idx)
                        .is_some()
                    {
                        Some(decl_idx)
                    } else {
                        let parent_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
                        self.ctx.arena.get(parent_idx).and_then(|parent| {
                            self.ctx
                                .arena
                                .get_variable_declaration(parent)
                                .map(|_| parent_idx)
                        })
                    }?;
                    let var_decl = self.ctx.arena.get_variable_declaration_at(var_decl_idx)?;
                    let init_idx = var_decl.initializer;
                    let init_node = self.ctx.arena.get(init_idx)?;
                    if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                        return Some(init_idx);
                    }
                }
                None
            })?;
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;

        for &member_idx in &class.members.nodes {
            let Some(member_name) = self.get_member_name(member_idx) else {
                continue;
            };
            if member_name != method_name {
                continue;
            }
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            if self.ctx.arena.get_method_decl(member_node)?.body.is_some() {
                return Some(member_idx);
            }
        }

        None
    }

    pub(crate) fn returned_class_name_from_body(&self, body_idx: NodeIndex) -> Option<String> {
        if body_idx.is_none() {
            return None;
        }

        let body_node = self.ctx.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            let class_expr_idx = self.class_expression_from_expr(body_idx)?;
            return Some(self.get_class_name_from_decl(class_expr_idx));
        }

        let block = self.ctx.arena.get_block(body_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }

            if let Some(class_expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(self.get_class_name_from_decl(class_expr_idx));
            }

            let expr_node = self.ctx.arena.get(ret.expression)?;
            if expr_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(ret.expression)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some(self.get_class_name_from_decl(class_idx));
            }
        }

        None
    }
}
