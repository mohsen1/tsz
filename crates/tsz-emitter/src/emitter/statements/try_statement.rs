use super::super::{Printer, get_trailing_comment_ranges};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{CatchClauseData, Node};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_try_statement(&mut self, node: &Node) {
        let Some(try_stmt) = self.arena.get_try(node) else {
            return;
        };

        self.write("try ");
        self.emit_try_block_or_recovered_empty(try_stmt.try_block);

        if try_stmt.catch_clause.is_some() {
            self.write_line();
            if let Some(catch_node) = self.arena.get(try_stmt.catch_clause) {
                let catch_start = self.skip_trivia_forward(catch_node.pos, catch_node.end);
                self.emit_comments_before_pos(catch_start);
            }
            self.emit(try_stmt.catch_clause);
        }

        if try_stmt.finally_block.is_some() {
            if try_stmt.catch_clause.is_some() {
                self.emit_try_clause_trailing_comments_before_finally(try_stmt.catch_clause, node);
            }
            self.write_line();
            // Map the `finally` keyword to its source position.
            if let Some(finally_node) = self.arena.get(try_stmt.finally_block) {
                let search_start = if try_stmt.catch_clause.is_some() {
                    self.arena
                        .get(try_stmt.catch_clause)
                        .map_or(node.pos, |n| n.end)
                } else {
                    self.arena
                        .get(try_stmt.try_block)
                        .map_or(node.pos, |n| n.end)
                };
                self.map_token_after_skipping_whitespace(search_start, finally_node.pos);
            }
            self.write("finally ");
            self.emit(try_stmt.finally_block);
        } else if try_stmt.catch_clause.is_none() {
            self.write_line();
            if let Some((comment_pos, comment_end, _comment_has_trailing_newline)) =
                self.recovered_missing_finally_semicolon_comment(node, try_stmt.try_block)
            {
                self.write("finally { ");
                self.emit_recovered_comment(comment_pos, comment_end);
                self.write_line();
                self.write(" } ");
                self.emit_recovered_comment(comment_pos, comment_end);
            } else {
                self.write("finally { }");
            }
        }
    }

    fn emit_try_block_or_recovered_empty(&mut self, block_idx: NodeIndex) {
        let Some(block_node) = self.arena.get(block_idx) else {
            self.write("{");
            self.write_line();
            self.write("}");
            return;
        };
        if block_node.kind == syntax_kind_ext::BLOCK
            && self
                .arena
                .get_block(block_node)
                .is_some_and(|block| block.statements.nodes.is_empty())
            && self.find_block_opening_brace_pos(block_node).is_none()
        {
            self.write_with_end_marker("{");
            self.write_line();
            self.write_with_end_marker("}");
            return;
        }
        self.emit(block_idx);
    }

    fn emit_try_clause_trailing_comments_before_finally(
        &mut self,
        catch_idx: NodeIndex,
        try_node: &Node,
    ) {
        let Some(catch_node) = self.arena.get(catch_idx) else {
            return;
        };
        let Some(catch) = self.arena.get_catch_clause(catch_node) else {
            return;
        };
        let Some(catch_block_node) = self.arena.get(catch.block) else {
            return;
        };
        let token_end = self.find_block_closing_brace_end(catch_block_node);
        self.emit_trailing_comments_before(token_end, try_node.end);
    }

    fn recovered_missing_finally_semicolon_comment(
        &self,
        try_node: &Node,
        try_block_idx: NodeIndex,
    ) -> Option<(u32, u32, bool)> {
        let try_block_node = self.arena.get(try_block_idx)?;
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let search_start = self.find_block_closing_brace_end(try_block_node) as usize;
        let mut search_end = (try_node.end as usize).min(bytes.len());
        if search_end <= search_start {
            search_end = search_start;
            while search_end < bytes.len()
                && bytes[search_end] != b'\n'
                && bytes[search_end] != b'\r'
            {
                search_end += 1;
            }
        }
        let semicolon_pos = self
            .find_semicolon_pos_in_range(search_start as u32, search_end as u32)
            .or_else(|| {
                let mut line_end = search_start;
                while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r'
                {
                    line_end += 1;
                }
                self.find_semicolon_pos_in_range(search_start as u32, line_end as u32)
            })?;
        let comment = get_trailing_comment_ranges(text, semicolon_pos as usize + 1)
            .into_iter()
            .next()?;
        Some((comment.pos, comment.end, comment.has_trailing_newline))
    }

    fn emit_recovered_comment(&mut self, comment_pos: u32, comment_end: u32) {
        if let Some(text) = self.source_text
            && let Ok(comment_text) =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
        {
            self.write_comment_with_reindent(comment_text, Some(comment_pos));
        }
    }

    pub(in crate::emitter) fn emit_catch_clause(&mut self, node: &Node) {
        let Some(catch) = self.arena.get_catch_clause(node) else {
            return;
        };

        self.write("catch");

        if catch.variable_declaration.is_some() {
            // Check if catch variable has object rest that needs ES2018 lowering.
            let needs_rest_lowering = self.ctx.needs_es2018_lowering
                && !self.ctx.target_es5
                && self.catch_var_has_object_rest(catch.variable_declaration);

            if needs_rest_lowering
                && let Some(pattern_idx) = self.catch_var_pattern_idx(catch.variable_declaration)
            {
                let temp = self.get_temp_var_name();
                self.write(" ");
                self.map_token_after(node.pos, node.end, b'(');
                self.write("(");
                self.write(&temp);
                self.write(")");

                self.write(" {");
                self.write_line();
                self.increase_indent();

                self.write("var ");
                self.emit_object_rest_var_decl(pattern_idx, NodeIndex::NONE, Some(&temp));
                self.write(";");
                self.write_line();

                if let Some(block_node) = self.arena.get(catch.block)
                    && let Some(block) = self.arena.get_block(block_node)
                {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        self.write_line();
                    }
                }

                self.decrease_indent();
                self.write("}");
                return;
            }

            self.write(" ");
            self.map_token_after(node.pos, node.end, b'(');
            self.write("(");
            // Emit any inline comments between `(` and the variable declaration.
            if let Some(var_node) = self.arena.get(catch.variable_declaration) {
                if self.has_pending_comment_before(var_node.pos) {
                    self.write_space();
                }
                self.emit_comments_before_pos(var_node.pos);
                self.pending_block_comment_space = false;
            }
            self.emit(catch.variable_declaration);
            self.write(")");
        } else if self.catch_clause_has_recovered_empty_binding_parens(node, &catch) {
            self.write(" ()");
        } else if self.ctx.needs_es2019_lowering {
            let name = self.make_unique_name();
            self.write(" (");
            self.write(&name);
            self.write(")");
        }

        self.write(" ");
        self.emit(catch.block);
    }

    fn catch_clause_has_recovered_empty_binding_parens(
        &self,
        node: &Node,
        catch: &CatchClauseData,
    ) -> bool {
        if catch.variable_declaration.is_some() {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(block_node) = self.arena.get(catch.block) else {
            return false;
        };
        let bytes = text.as_bytes();
        let start = node.pos as usize;
        let end = (block_node.pos as usize).min(bytes.len());
        let Some(slice) = bytes.get(start..end) else {
            return false;
        };
        let mut pos = 0;
        while pos < slice.len() {
            if slice[pos] != b'(' {
                pos += 1;
                continue;
            }
            pos += 1;
            while pos < slice.len() && slice[pos].is_ascii_whitespace() {
                pos += 1;
            }
            return pos < slice.len() && slice[pos] == b')';
        }
        false
    }

    fn catch_var_has_object_rest(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(var_node) = self.arena.get(var_decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(var_node) else {
            return false;
        };
        self.pattern_has_object_rest(var_decl.name)
    }

    fn catch_var_pattern_idx(&self, var_decl_idx: NodeIndex) -> Option<NodeIndex> {
        let var_node = self.arena.get(var_decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(var_node)?;
        Some(var_decl.name)
    }
}
