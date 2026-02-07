use super::{Printer, get_trailing_comment_ranges};
use crate::parser::node::Node;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::printer::safe_slice;
use crate::scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Statements
    // =========================================================================

    pub(super) fn emit_block(&mut self, node: &Node) {
        let Some(block) = self.arena.get_block(node) else {
            return;
        };

        // Empty blocks: preserve original format (single-line vs multi-line)
        if block.statements.nodes.is_empty() {
            if self.is_single_line(node) {
                // Single-line empty block: { }
                self.write("{ }");
            } else {
                // Multi-line empty block: {\n}
                self.write("{");
                self.write_line();
                self.write("}");
            }
            // Emit trailing comments after the block's closing brace
            self.emit_trailing_comments(node.end);
            return;
        }

        // Single-line blocks: preserve single-line formatting from source.
        // tsc only emits single-line blocks when the original source was single-line.
        // It never collapses multi-line blocks to single lines.
        let is_single_statement = block.statements.nodes.len() == 1;
        let should_emit_single_line = is_single_statement && self.is_single_line(node);

        if should_emit_single_line {
            self.write("{ ");
            self.emit(block.statements.nodes[0]);
            self.write(" }");
            self.emit_trailing_comments(node.end);
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len {
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write("}");
        // Emit trailing comments after the block's closing brace
        self.emit_trailing_comments(node.end);
    }

    pub(super) fn emit_variable_statement(&mut self, node: &Node) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };

        // Skip ambient declarations (declare var/let/const)
        if self.has_declare_modifier(&var_stmt.modifiers) {
            return;
        }

        let is_exported = self.ctx.is_commonjs()
            && self.has_export_modifier(&var_stmt.modifiers)
            && !self.ctx.module_state.has_export_assignment;
        let is_default = self.has_default_modifier(&var_stmt.modifiers);

        // For CommonJS exported variables with no initializers, skip the
        // declaration entirely. The preamble `exports.X = void 0;` already
        // handles the export, and no local `var` is needed.
        if is_exported && self.all_declarations_lack_initializer(&var_stmt.declarations) {
            return;
        }

        // Collect declaration names for export assignment
        let export_names: Vec<String> = if is_exported {
            self.collect_variable_names(&var_stmt.declarations)
        } else {
            Vec::new()
        };

        // VariableStatement.declarations contains a VARIABLE_DECLARATION_LIST
        // Emit the declaration list (which handles the let/const/var keyword)
        for &decl_list_idx in &var_stmt.declarations.nodes {
            self.emit(decl_list_idx);
        }
        self.write_semicolon();

        // Emit trailing comments (e.g., var x = 1; // comment)
        self.emit_trailing_comment_after_semicolon(node);

        // CommonJS: emit exports.X = X; after the declaration
        if is_exported && !export_names.is_empty() {
            self.write_line();
            if is_default && export_names.len() == 1 {
                // export default const x = ... -> exports.default = x;
                self.write("exports.default = ");
                self.write(&export_names[0]);
                self.write(";");
            } else {
                // export const x = ..., y = ...; -> exports.x = x; exports.y = y;
                for name in &export_names {
                    self.write("exports.");
                    self.write(name);
                    self.write(" = ");
                    self.write(name);
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    /// Check if all variable declarations in a declaration list lack initializers
    pub(super) fn all_declarations_lack_initializer(&self, declarations: &NodeList) -> bool {
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if !decl.initializer.is_none() {
                    return false;
                }
            }
        }
        true
    }

    /// Collect variable names from a declaration list for CommonJS export
    fn collect_variable_names(&self, declarations: &NodeList) -> Vec<String> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    pub(super) fn collect_binding_names(&self, name_idx: NodeIndex, names: &mut Vec<String>) {
        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.arena.get_identifier(node) {
                names.push(id.escaped_text.clone());
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element(elem_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names(elem.name, names);
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_binding_names_from_element(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<String>,
    ) {
        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names(elem.name, names);
        }
    }

    pub(super) fn emit_expression_statement(&mut self, node: &Node) {
        let Some(expr_stmt) = self.arena.get_expression_statement(node) else {
            return;
        };

        self.emit(expr_stmt.expression);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    /// Emit trailing comments after a semicolon. Scans backward through the
    /// entire node range to find the semicolon, allowing it to work even when
    /// node.end is past the newline (at the start of the next statement).
    pub(super) fn emit_trailing_comment_after_semicolon(&mut self, node: &Node) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let bytes = text.as_bytes();
        let stmt_end = std::cmp::min(node.end as usize, bytes.len());
        let stmt_start = node.pos as usize;

        // Scan backwards to find the semicolon within this node's range
        let mut semi_pos = None;
        let mut i = stmt_end;
        while i > stmt_start {
            i -= 1;
            if bytes[i] == b';' {
                semi_pos = Some(i + 1);
                break;
            }
        }

        if let Some(pos) = semi_pos {
            let comments = get_trailing_comment_ranges(text, pos);
            for comment in comments {
                self.write_space();
                let comment_text =
                    safe_slice::slice(text, comment.pos as usize, comment.end as usize);
                if !comment_text.is_empty() {
                    self.write(comment_text);
                }
                // Advance the global comment index past this comment so it
                // won't be emitted again by the end-of-file comment sweep.
                while self.comment_emit_idx < self.all_comments.len() {
                    let c = &self.all_comments[self.comment_emit_idx];
                    if c.pos >= comment.pos as u32 && c.end <= comment.end as u32 {
                        self.comment_emit_idx += 1;
                        break;
                    } else if c.end > comment.end as u32 {
                        break;
                    }
                    self.comment_emit_idx += 1;
                }
            }
        }
    }

    pub(super) fn emit_if_statement(&mut self, node: &Node) {
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        self.write("if (");
        self.emit(if_stmt.expression);
        self.write(") ");
        self.emit(if_stmt.then_statement);

        if !if_stmt.else_statement.is_none() {
            self.write_line();
            self.write("else ");
            self.emit(if_stmt.else_statement);
        }
    }

    pub(super) fn emit_while_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("while (");
        self.emit(loop_stmt.condition);
        self.write(") ");
        self.emit(loop_stmt.statement);
    }

    pub(super) fn emit_for_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("for (");
        self.emit(loop_stmt.initializer);
        self.write(";");
        if !loop_stmt.condition.is_none() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        self.write(";");
        if !loop_stmt.incrementor.is_none() {
            self.write(" ");
            self.emit(loop_stmt.incrementor);
        }
        self.write(") ");
        self.emit(loop_stmt.statement);
    }

    pub(super) fn emit_for_in_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.write("for (");
        self.emit(for_in_of.initializer);
        self.write(" in ");
        self.emit(for_in_of.expression);
        self.write(") ");
        self.emit(for_in_of.statement);
    }

    pub(super) fn emit_for_of_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(");
        self.emit(for_in_of.initializer);
        self.write(" of ");
        self.emit(for_in_of.expression);
        self.write(") ");
        self.emit(for_in_of.statement);
    }

    pub(super) fn emit_return_statement(&mut self, node: &Node) {
        let Some(ret) = self.arena.get_return_statement(node) else {
            self.write("return");
            self.write_semicolon();
            return;
        };

        self.write("return");
        if !ret.expression.is_none() {
            self.write(" ");
            self.emit_expression(ret.expression);
        }
        self.write_semicolon();
    }

    // =========================================================================
    // Additional Statements
    // =========================================================================

    pub(super) fn emit_throw_statement(&mut self, node: &Node) {
        // ThrowStatement uses ReturnData (same structure)
        let Some(throw_data) = self.arena.get_return_statement(node) else {
            self.write("throw");
            self.write_semicolon();
            return;
        };

        self.write("throw ");
        self.emit(throw_data.expression);
        self.write_semicolon();
    }

    pub(super) fn emit_try_statement(&mut self, node: &Node) {
        let Some(try_stmt) = self.arena.get_try(node) else {
            return;
        };

        self.write("try ");
        self.emit(try_stmt.try_block);

        if !try_stmt.catch_clause.is_none() {
            self.write(" ");
            self.emit(try_stmt.catch_clause);
        }

        if !try_stmt.finally_block.is_none() {
            self.write(" finally ");
            self.emit(try_stmt.finally_block);
        }
    }

    pub(super) fn emit_catch_clause(&mut self, node: &Node) {
        let Some(catch) = self.arena.get_catch_clause(node) else {
            return;
        };

        self.write("catch");

        if !catch.variable_declaration.is_none() {
            self.write(" (");
            self.emit(catch.variable_declaration);
            self.write(")");
        }

        self.write(" ");
        self.emit(catch.block);
    }

    pub(super) fn emit_switch_statement(&mut self, node: &Node) {
        let Some(switch) = self.arena.get_switch(node) else {
            return;
        };

        self.write("switch (");
        self.emit(switch.expression);
        self.write(") ");
        // case_block is a NodeIndex pointing to a CaseBlock node
        self.emit(switch.case_block);
    }

    pub(super) fn emit_case_block(&mut self, node: &Node) {
        if !node.has_data() || node.kind != syntax_kind_ext::CASE_BLOCK {
            return;
        }
        let Some(case_block) = self.arena.blocks.get(node.data_index as usize) else {
            return;
        };

        self.write("{");
        self.write_line();
        self.increase_indent();

        for &clause_idx in &case_block.statements.nodes {
            self.emit(clause_idx);
        }

        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn emit_case_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("case ");
        self.emit(clause.expression);
        self.write(":");
        self.write_line();
        self.increase_indent();

        for &stmt in &clause.statements.nodes {
            self.emit(stmt);
            self.write_line();
        }

        self.decrease_indent();
    }

    pub(super) fn emit_default_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("default:");
        self.write_line();
        self.increase_indent();

        for &stmt in &clause.statements.nodes {
            self.emit(stmt);
            self.write_line();
        }

        self.decrease_indent();
    }

    pub(super) fn emit_break_statement(&mut self, node: &Node) {
        self.write("break");
        if let Some(jump) = self.arena.get_jump_data(node) {
            if !jump.label.is_none() {
                self.write(" ");
                self.emit(jump.label);
            }
        }
        self.write_semicolon();
    }

    pub(super) fn emit_continue_statement(&mut self, node: &Node) {
        self.write("continue");
        if let Some(jump) = self.arena.get_jump_data(node) {
            if !jump.label.is_none() {
                self.write(" ");
                self.emit(jump.label);
            }
        }
        self.write_semicolon();
    }

    pub(super) fn emit_labeled_statement(&mut self, node: &Node) {
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        self.emit(labeled.label);
        self.write(": ");
        self.emit(labeled.statement);
    }

    pub(super) fn emit_do_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("do ");
        self.emit(loop_stmt.statement);
        self.write(" while (");
        self.emit(loop_stmt.condition);
        self.write(")");
        self.write_semicolon();
    }

    pub(super) fn emit_debugger_statement(&mut self) {
        self.write("debugger");
        self.write_semicolon();
    }
}
