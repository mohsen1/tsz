//! Directive-prologue and trailing-comment emission helpers.
//!
//! Split from `statements/core.rs` (arch size ratchet).

use super::super::Printer;
use super::super::get_trailing_comment_ranges;
use crate::safe_slice;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_leading_directive_prologue_statements(
        &mut self,
        statements: &[NodeIndex],
        block_close_pos: u32,
    ) -> usize {
        let mut emitted_count = 0;
        for (stmt_i, &stmt_idx) in statements.iter().enumerate() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                break;
            };
            if !self.is_directive_prologue_statement(stmt_node) {
                break;
            }

            let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end > actual_start {
                        break;
                    }
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    if let Ok(comment_text) =
                        safe_slice::slice(text, c_pos as usize, c_end as usize)
                    {
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        } else if comment_text.starts_with("/*") {
                            self.pending_block_comment_space = true;
                        }
                    }
                    self.comment_emit_idx += 1;
                }
            }

            let before_emit_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_emit_len && !self.writer.is_at_line_start() {
                let upper_bound = statements
                    .get(stmt_i + 1)
                    .and_then(|&next_idx| self.arena.get(next_idx))
                    .map_or(block_close_pos, |next_node| next_node.pos);
                let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                let max_pos = if stmt_i + 1 >= statements.len() {
                    block_close_pos
                } else {
                    upper_bound
                };
                self.emit_trailing_comments_before(token_end, max_pos);
                self.write_line();
            }
            emitted_count += 1;
        }
        emitted_count
    }

    fn is_directive_prologue_statement(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && self
                .arena
                .get_expression_statement(node)
                .and_then(|stmt| self.arena.get(stmt.expression))
                .is_some_and(|expr| expr.is_string_literal())
    }

    /// Emit trailing comments after a semicolon. Scans backward through the
    /// entire node range to find the semicolon, allowing it to work even when
    /// node.end is past the newline (at the start of the next statement).
    pub(in crate::emitter) fn emit_trailing_comment_after_semicolon(&mut self, node: &Node) {
        self.emit_trailing_comment_after_semicolon_in_range(node.pos, node.end);
    }

    /// Like `emit_trailing_comment_after_semicolon` but with an explicit scan range.
    /// Use this when the node's full range includes erased content (e.g., type
    /// annotations with semicolons inside) that should not be scanned.
    pub(in crate::emitter) fn emit_trailing_comment_after_semicolon_in_range(
        &mut self,
        range_start: u32,
        range_end: u32,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let bytes = text.as_bytes();
        let capped_range_end = self
            .trailing_comment_scan_max_pos
            .map_or(range_end, |cap| cap.min(range_end));
        let stmt_end = std::cmp::min(capped_range_end as usize, bytes.len());
        let stmt_start = range_start as usize;

        // Scan forwards and keep the last outermost semicolon within this node's range.
        // This still ignores semicolons nested inside blocks/object literals, but it
        // does not get confused when node.end extends onto later `}` lines after the
        // statement's own trailing comment (e.g. `break; // done` inside `switch`).
        let mut semi_pos = None;
        let mut depth: i32 = 0;
        let mut i = stmt_start;
        while i < stmt_end {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b';' if depth == 0 => {
                    semi_pos = Some(i + 1);
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(pos) = semi_pos {
            let comments = get_trailing_comment_ranges(text, pos);
            for comment in comments {
                if let Some(max_pos) = self.trailing_comment_scan_max_pos
                    && comment.pos >= max_pos
                {
                    break;
                }
                self.write_space();
                if let Ok(comment_text) =
                    safe_slice::slice(text, comment.pos as usize, comment.end as usize)
                    && !comment_text.is_empty()
                {
                    self.write_comment_with_reindent(comment_text, Some(comment.pos));
                }
                // Advance the global comment index past this comment so it
                // won't be emitted again by the end-of-file comment sweep.
                while self.comment_emit_idx < self.all_comments.len() {
                    let c = &self.all_comments[self.comment_emit_idx];
                    if c.pos >= comment.pos && c.end <= comment.end {
                        self.comment_emit_idx += 1;
                        break;
                    } else if c.end > comment.end {
                        break;
                    }
                    self.comment_emit_idx += 1;
                }
            }
        }
    }
}
