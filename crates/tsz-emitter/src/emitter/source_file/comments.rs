use super::super::Printer;
use tsz_parser::parser::node::SourceFileData;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_source_file_shebang(&mut self) {
        if let Some(text) = self.source_text
            && text.starts_with("#!")
        {
            if let Some(newline_pos) = text.find('\n') {
                self.write(text[..newline_pos].trim_end());
            } else {
                self.write(text.trim_end());
            }
            self.write_line();
        }
    }

    pub(in crate::emitter) fn emit_remaining_source_file_trailing_comments(&mut self) {
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_pos = self.all_comments[self.comment_emit_idx].pos;
                let c_end = self.all_comments[self.comment_emit_idx].end;
                let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                if let Ok(comment_text) =
                    crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                {
                    self.write_comment_with_reindent(comment_text, Some(c_pos));
                    if c_trailing {
                        self.write_line();
                    }
                }
                self.comment_emit_idx += 1;
            }
        }
    }

    pub(in crate::emitter) fn prepare_source_file_comments(
        &mut self,
        source: &SourceFileData,
        inside_module_wrapper: bool,
    ) -> (Option<u32>, bool) {
        self.all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                self.source_comment_ranges
                    .iter()
                    .filter(|c| {
                        let content = c.get_text(text);
                        if inside_module_wrapper {
                            if content.contains("<amd-dependency") {
                                return false;
                            }
                            let trimmed = content.trim_start_matches('/');
                            let trimmed = trimmed.trim_start();
                            if trimmed.starts_with("<reference") {
                                return false;
                            }
                        }
                        true
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let mut first_erased_stmt_pos: Option<u32> = None;
        let mut first_erased_is_import_export = false;
        if !self.ctx.flags.in_declaration_emit && !self.all_comments.is_empty() {
            let mut erased_ranges: Vec<(u32, u32)> = Vec::new();
            let mut prev_erased_end: Option<u32> = None;
            let mut seen_non_erased = false;
            let stmt_nodes = &source.statements.nodes;
            for (stmt_i, &stmt_idx) in stmt_nodes.iter().enumerate() {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let scan_end = stmt_nodes
                        .get(stmt_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(stmt_node.end, |next_node| next_node.pos);
                    let stmt_token_end = self.find_token_end_before_trivia(stmt_node.pos, scan_end);
                    let mut is_erased = self.is_erased_statement(stmt_node);
                    if !is_erased
                        && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                        && let Some(export) = self.arena.get_export_decl(stmt_node)
                        && let Some(inner_node) = self.arena.get(export.export_clause)
                        && self.is_erased_statement(inner_node)
                    {
                        is_erased = true;
                    }

                    if is_erased {
                        let range_start = if let Some(pe) = prev_erased_end {
                            pe
                        } else if first_erased_stmt_pos.is_none() && !seen_non_erased {
                            let actual_start =
                                self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                            first_erased_stmt_pos = Some(actual_start);
                            first_erased_is_import_export = matches!(
                                stmt_node.kind,
                                syntax_kind_ext::IMPORT_DECLARATION
                                    | syntax_kind_ext::EXPORT_DECLARATION
                            );
                            actual_start
                        } else {
                            stmt_node.pos
                        };
                        erased_ranges.push((range_start, stmt_token_end));
                        prev_erased_end = Some(stmt_token_end);
                    } else {
                        prev_erased_end = None;
                        seen_non_erased = true;
                    }
                }
            }
            if !erased_ranges.is_empty() {
                self.retain_comments_outside_erased_ranges(
                    &erased_ranges,
                    first_erased_stmt_pos,
                    first_erased_is_import_export,
                );
            }
        }

        self.comment_emit_idx = 0;
        (first_erased_stmt_pos, first_erased_is_import_export)
    }

    fn retain_comments_outside_erased_ranges(
        &mut self,
        erased_ranges: &[(u32, u32)],
        first_erased_stmt_pos: Option<u32>,
        first_erased_is_import_export: bool,
    ) {
        self.all_comments.retain(|c| {
            if erased_ranges
                .iter()
                .any(|&(start, end)| c.pos >= start && c.end <= end)
            {
                return false;
            }
            if let Some(fep) = first_erased_stmt_pos
                && first_erased_is_import_export
                && c.end <= fep
                && let Some(text) = self.source_text
            {
                let comment_text = c.get_text(text);
                let trimmed = comment_text.trim_start_matches('/');
                let trimmed = trimmed.trim_start();
                if trimmed.starts_with("<reference") {
                    if comment_text.contains("preserve=\"true\"") {
                        return true;
                    }
                    return crate::safe_slice::slice(text, c.end as usize, fep as usize)
                        .is_ok_and(|gap| gap.contains("\n\n") || gap.contains("\r\n\r\n"));
                }
            }
            true
        });
    }
}
