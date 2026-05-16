use super::super::Printer;
use tsz_parser::parser::node::{AccessExprData, Node};

impl<'a> Printer<'a> {
    pub(super) fn emit_property_access_commented_dot(
        &mut self,
        access: &AccessExprData,
        expr_node: &Node,
        name_node: &Node,
        dot_pos: u32,
    ) -> bool {
        let has_comment_between = self
            .all_comments
            .iter()
            .any(|comment| comment.pos >= expr_node.end && comment.end <= name_node.pos);
        if !has_comment_between {
            return false;
        }

        let newline_before_dot = self.source_range_has_newline_local(expr_node.end, dot_pos);
        let newline_after_dot = self.source_range_has_newline_local(dot_pos + 1, name_node.pos);

        if newline_before_dot {
            self.write_line();
            self.increase_indent();
        }

        let (emitted_before_dot, before_dot_trailing_newline) =
            self.emit_property_access_compact_comments(expr_node.end, dot_pos, !newline_before_dot);
        if emitted_before_dot && !before_dot_trailing_newline && newline_before_dot {
            self.write_space();
        }

        self.map_source_offset(dot_pos);
        self.write_dot_token(access.expression);

        if newline_after_dot {
            self.increase_indent();
        }
        let (_emitted_after_dot, after_dot_trailing_newline) =
            self.emit_property_access_compact_comments(dot_pos + 1, name_node.pos, true);
        if newline_after_dot && !after_dot_trailing_newline && !self.writer.is_at_line_start() {
            self.write_line();
        }

        self.emit_property_name_without_import_substitution(access.name_or_argument);

        if newline_after_dot {
            self.decrease_indent();
        }
        if newline_before_dot {
            self.decrease_indent();
        }
        true
    }

    pub(super) fn emit_property_access_compact_comments(
        &mut self,
        start_pos: u32,
        end_pos: u32,
        space_before_first_inline: bool,
    ) -> (bool, bool) {
        if self.ctx.options.remove_comments {
            return (false, false);
        }

        let Some(text) = self.source_text else {
            return (false, false);
        };

        let mut emitted_any = false;
        let mut last_had_trailing_newline = false;
        let mut cursor_pos = start_pos;

        while self.comment_emit_idx < self.all_comments.len() {
            let (comment_pos, comment_end, comment_has_newline) = {
                let comment = &self.all_comments[self.comment_emit_idx];
                (comment.pos, comment.end, comment.has_trailing_new_line)
            };

            if comment_pos >= end_pos {
                break;
            }
            if comment_end <= start_pos {
                self.comment_emit_idx += 1;
                continue;
            }

            let starts_on_new_line = self.source_range_has_newline_local(cursor_pos, comment_pos)
                || self.comment_preceded_by_newline(comment_pos);
            if emitted_any {
                if starts_on_new_line || last_had_trailing_newline {
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                } else {
                    self.write_space();
                }
            } else if space_before_first_inline && !starts_on_new_line {
                self.write_space();
            } else if starts_on_new_line && !self.writer.is_at_line_start() {
                self.write_line();
            }

            if let Ok(comment_text) =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
            {
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
            }
            if comment_has_newline {
                self.write_line();
            }

            emitted_any = true;
            last_had_trailing_newline = comment_has_newline;
            cursor_pos = comment_end;
            self.comment_emit_idx += 1;
        }

        (emitted_any, last_had_trailing_newline)
    }

    pub(super) fn source_range_has_newline_local(&self, start: u32, end: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(start as usize, text.len());
        let end = std::cmp::min(end as usize, text.len());
        start < end
            && text.as_bytes()[start..end]
                .iter()
                .any(|b| matches!(b, b'\n' | b'\r'))
    }

    pub(super) fn emit_optional_chain_inline_gap_comments_untracked(
        &mut self,
        start_pos: u32,
        dot_pos: u32,
    ) {
        if self.source_range_has_newline_local(start_pos, dot_pos) {
            return;
        }

        let saved_comment_idx = self.comment_emit_idx;
        self.emit_property_access_compact_comments(start_pos, dot_pos, true);
        self.comment_emit_idx = saved_comment_idx;
    }

    pub(super) fn emit_optional_property_access_downlevel_suffix(
        &mut self,
        access: &AccessExprData,
        expr_node: &Node,
        name_node: &Node,
        dot_pos: u32,
    ) {
        let newline_before_dot = self.source_range_has_newline_local(expr_node.end, dot_pos);
        let newline_after_dot = self.source_range_has_newline_local(dot_pos + 1, name_node.pos);

        if newline_before_dot && !self.writer.is_at_line_start() {
            self.write_line();
        }

        let (emitted_before_dot, before_dot_trailing_newline) =
            self.emit_property_access_compact_comments(expr_node.end, dot_pos, !newline_before_dot);
        if newline_before_dot && emitted_before_dot && !before_dot_trailing_newline {
            self.write_space();
        }

        self.map_source_offset(dot_pos);
        self.write_dot_token(access.expression);

        let (_emitted_after_dot, after_dot_trailing_newline) =
            self.emit_optional_property_access_post_dot_comments(dot_pos + 1, name_node.pos);
        if newline_after_dot && !after_dot_trailing_newline && !self.writer.is_at_line_start() {
            self.write_line();
        }

        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    fn emit_optional_property_access_post_dot_comments(
        &mut self,
        start_pos: u32,
        end_pos: u32,
    ) -> (bool, bool) {
        if self.ctx.options.remove_comments {
            return (false, false);
        }

        let Some(text) = self.source_text else {
            return (false, false);
        };

        let mut emitted_any = false;
        let mut last_had_trailing_newline = false;
        let mut cursor_pos = start_pos;
        let mut skipped_immediate_block = false;

        while self.comment_emit_idx < self.all_comments.len() {
            let (comment_pos, comment_end, comment_has_newline) = {
                let comment = &self.all_comments[self.comment_emit_idx];
                (comment.pos, comment.end, comment.has_trailing_new_line)
            };

            if comment_pos >= end_pos {
                break;
            }
            if comment_end <= start_pos {
                self.comment_emit_idx += 1;
                continue;
            }

            let starts_on_new_line = self.source_range_has_newline_local(cursor_pos, comment_pos)
                || self.comment_preceded_by_newline(comment_pos);

            let comment_text =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
                    .unwrap_or("");
            let immediate_post_dot_block = !emitted_any
                && !skipped_immediate_block
                && !starts_on_new_line
                && comment_text.starts_with("/*");

            if immediate_post_dot_block {
                skipped_immediate_block = true;
                if comment_has_newline && !self.writer.is_at_line_start() {
                    self.write_line();
                }
                cursor_pos = comment_end;
                last_had_trailing_newline = comment_has_newline;
                self.comment_emit_idx += 1;
                continue;
            }

            if emitted_any {
                if starts_on_new_line || last_had_trailing_newline {
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                } else {
                    self.write_space();
                }
            } else if starts_on_new_line && !self.writer.is_at_line_start() {
                self.write_line();
            }

            if !comment_text.is_empty() {
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
            }
            if comment_has_newline {
                self.write_line();
            }

            emitted_any = true;
            last_had_trailing_newline = comment_has_newline;
            cursor_pos = comment_end;
            self.comment_emit_idx += 1;
        }

        (
            emitted_any || skipped_immediate_block,
            last_had_trailing_newline,
        )
    }
}
