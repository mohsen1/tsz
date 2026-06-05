use super::{Node, NodeIndex, Printer, get_trailing_comment_ranges, syntax_kind_ext};

pub(super) struct RecoveredSwitchClass {
    pub(super) header: String,
    pub(super) inline_body: Option<String>,
}

impl<'a> Printer<'a> {
    pub(super) fn recovered_array_tail_for(
        &self,
        node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
    ) -> bool {
        if !(loop_stmt.initializer.is_some()
            && loop_stmt.condition.is_none()
            && loop_stmt.incrementor.is_none())
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(header) = text
            .as_bytes()
            .get(node.pos as usize..(node.end as usize).min(text.len()))
        else {
            return false;
        };
        header.contains(&b']') && header.contains(&b')') && !header.contains(&b';')
    }

    pub(super) fn recovered_empty_for_header_body_comment(
        &self,
        node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
    ) -> Option<(u32, u32, bool)> {
        if loop_stmt.initializer.is_some()
            || loop_stmt.condition.is_some()
            || loop_stmt.incrementor.is_some()
        {
            return None;
        }

        let text = self.source_text?;
        let body_node = self.arena.get(loop_stmt.statement)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return None;
        }

        let bytes = text.as_bytes();
        let search_start = body_node.pos as usize;
        let search_end = (body_node.end as usize).min(bytes.len());
        let brace_pos = bytes
            .get(search_start..search_end)?
            .iter()
            .position(|&b| b == b'{')
            .map(|offset| search_start + offset)?;

        let header_start = node.pos as usize;
        let header_end = brace_pos.min(bytes.len());
        if bytes.get(header_start..header_end)?.contains(&b';') {
            return None;
        }

        let comment = get_trailing_comment_ranges(text, brace_pos + 1)
            .into_iter()
            .next()?;
        Some((comment.pos, comment.end, comment.has_trailing_newline))
    }

    pub(super) fn recovered_class_after_unterminated_empty_switch(
        &self,
        node: &Node,
        case_block_idx: NodeIndex,
    ) -> Option<RecoveredSwitchClass> {
        let case_block_node = self.arena.get(case_block_idx)?;
        let case_block = self.arena.blocks.get(case_block_node.data_index as usize)?;
        if !case_block.statements.nodes.is_empty() {
            return None;
        }

        let text = self.source_text?;
        let start = std::cmp::min(node.pos as usize, text.len());
        let end = std::cmp::min(node.end as usize, text.len());
        let source = text.get(start..end)?;
        for line in source.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("class ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|&ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                .collect();
            if name.is_empty() {
                continue;
            }

            if rest[name.len()..].trim_start().starts_with('{') {
                return Some(RecoveredSwitchClass {
                    header: name,
                    inline_body: None,
                });
            }

            let Some(open_brace) = rest.find('{') else {
                continue;
            };
            let Some(close_brace) = rest[open_brace + 1..].rfind('}') else {
                continue;
            };
            let header = rest[..open_brace].trim_end().to_string();
            let inline_body = rest[open_brace + 1..open_brace + 1 + close_brace]
                .trim()
                .to_string();
            if !header.is_empty() && !inline_body.is_empty() {
                return Some(RecoveredSwitchClass {
                    header,
                    inline_body: Some(inline_body),
                });
            }
        }
        None
    }

    pub(super) fn emit_static_block_await_labeled_jump_recovery(
        &mut self,
        stmt_idx: NodeIndex,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let jump_keyword = if stmt_node.kind == syntax_kind_ext::BREAK_STATEMENT {
            "break"
        } else if stmt_node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
            "continue"
        } else {
            return false;
        };
        if !self.static_block_jump_source_has_await_label(stmt_node, jump_keyword) {
            return false;
        }

        self.write(jump_keyword);
        self.write(" ;");
        true
    }

    fn static_block_jump_source_has_await_label(
        &self,
        stmt_node: &Node,
        jump_keyword: &str,
    ) -> bool {
        if !self.ctx.flags.in_class_static_block {
            return false;
        }
        if self
            .arena
            .get_jump_data(stmt_node)
            .is_some_and(|jump| jump.label.is_some())
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = stmt_node.pos as usize;
        if start >= text.len() {
            return false;
        }
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset);
        let Ok(line) = crate::safe_slice::slice(text, start, line_end) else {
            return false;
        };
        let Some(rest) = line.trim_start().strip_prefix(jump_keyword) else {
            return false;
        };
        let rest = rest.trim_start();
        rest.starts_with("await")
            && rest["await".len()..]
                .chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
    }
}
