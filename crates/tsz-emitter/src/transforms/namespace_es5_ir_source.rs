use super::*;
use crate::transforms::emit_utils;

impl<'a> NamespaceES5Transformer<'a> {
    /// Extract leading comments from source text that fall within [`from_pos`, `to_pos`) range.
    /// Returns `IRNode::Raw` nodes since the text already includes comment delimiters.
    pub(super) fn extract_comments_in_range(&self, from_pos: u32, to_pos: u32) -> Vec<IRNode> {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return Vec::new(),
        };
        let mut result = Vec::new();
        for c in &self.comment_ranges {
            if c.pos >= from_pos && c.end <= to_pos {
                let text = c.get_text(source_text);
                if !text.is_empty() {
                    result.push(IRNode::Raw(text.to_string().into()));
                }
            }
            if c.pos >= to_pos {
                break; // Comments are sorted by position
            }
        }
        result
    }

    /// Skip whitespace and comments forward from `pos` to find the actual token start.
    /// Returns the position of the first non-trivia character.
    pub(super) fn skip_trivia_forward(&self, pos: u32, end: u32) -> u32 {
        emit_utils::skip_trivia_forward(self.source_text, pos, end)
    }

    /// Find the position after the code content of an erased statement (interface/type alias).
    /// Scans forward with brace-depth tracking to find the closing `}` or `;`.
    /// This is needed because `node.end` includes trailing trivia that may contain
    /// comments belonging to the next statement.
    pub(super) fn find_code_end_of_erased_stmt(&self, node_pos: u32, node_end: u32) -> u32 {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return node_end,
        };
        let bytes = source_text.as_bytes();
        let end = (node_end as usize).min(bytes.len());
        let mut i = node_pos as usize;
        let mut brace_depth: i32 = 0;
        let mut found_brace = false;

        while i < end {
            // Skip over comment ranges
            let pos = i as u32;
            let mut skipped_comment = false;
            for c in &self.comment_ranges {
                if c.pos <= pos && pos < c.end {
                    i = c.end as usize;
                    skipped_comment = true;
                    break;
                }
                if c.pos > pos {
                    break; // comments sorted by position
                }
            }
            if skipped_comment {
                continue;
            }

            match bytes[i] {
                b'{' => {
                    brace_depth += 1;
                    found_brace = true;
                }
                b'}' => {
                    brace_depth -= 1;
                    if found_brace && brace_depth == 0 {
                        return (i + 1) as u32;
                    }
                }
                b';' if brace_depth == 0 && !found_brace => {
                    // Type alias without braces: type Foo = number;
                    return (i + 1) as u32;
                }
                b'\'' | b'"' => {
                    // Skip string literal
                    let quote = bytes[i];
                    i += 1;
                    while i < end && bytes[i] != quote {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    if i < end {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }

        node_end
    }

    /// Extract standalone comments (on their own line) within [`from_pos`, `to_pos`).
    /// Unlike `extract_comments_in_range`, this filters out trailing comments
    /// that share a line with code - only comments on their own line are returned.
    pub(super) fn extract_standalone_comments_in_range(
        &self,
        from_pos: u32,
        to_pos: u32,
    ) -> Vec<IRNode> {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return Vec::new(),
        };
        let bytes = source_text.as_bytes();
        let mut result = Vec::new();
        for c in &self.comment_ranges {
            if c.pos >= from_pos && c.end <= to_pos {
                // Check if standalone: only whitespace before it on the line
                let mut line_start = c.pos as usize;
                while line_start > 0
                    && bytes[line_start - 1] != b'\n'
                    && bytes[line_start - 1] != b'\r'
                {
                    line_start -= 1;
                }
                let before = &source_text[line_start..c.pos as usize];
                if before.trim().is_empty() {
                    let text = c.get_text(source_text);
                    if !text.is_empty() {
                        result.push(IRNode::Raw(text.to_string().into()));
                    }
                }
            }
            if c.pos >= to_pos {
                break;
            }
        }
        result
    }

    /// Extract a trailing comment within a statement's span.
    ///
    /// In our parser, `node.end` includes trailing trivia, so comments appear
    /// WITHIN `[stmt_pos, stmt_end)` rather than after `stmt_end`. This method
    /// finds comments within the span that have code on the same line before them
    /// (i.e., they're trailing comments, not standalone leading comments).
    pub(super) fn extract_trailing_comment_in_stmt(
        &self,
        stmt_pos: u32,
        stmt_end: u32,
    ) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();

        for c in &self.comment_ranges {
            if c.pos >= stmt_pos && c.end <= stmt_end {
                // Check if there's non-whitespace code before this comment on the same line
                let mut line_start = c.pos as usize;
                while line_start > 0
                    && bytes[line_start - 1] != b'\n'
                    && bytes[line_start - 1] != b'\r'
                {
                    line_start -= 1;
                }
                let before_comment = &source_text[line_start..c.pos as usize];
                if !before_comment.trim().is_empty() {
                    let text = c.get_text(source_text);
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            if c.pos >= stmt_end {
                break;
            }
        }
        None
    }

    pub(super) fn extract_namespace_trailing_comment(&self, body_idx: NodeIndex) -> Option<String> {
        let source_text = self.source_text?;
        let body_node = self.arena.get(body_idx)?;
        let pos = self.find_module_block_close_pos(body_node)? as usize;

        let comments = crate::emitter::get_trailing_comment_ranges(source_text, pos + 1);
        if comments.is_empty() {
            return None;
        }

        Some(
            comments
                .iter()
                .map(|comment| source_text[comment.pos as usize..comment.end as usize].to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    pub(super) fn find_module_block_close_pos(&self, body_node: &Node) -> Option<u32> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let limit = std::cmp::min(body_node.end as usize, bytes.len());
        let mut pos = body_node.pos as usize;
        while pos < limit && bytes.get(pos) != Some(&b'{') {
            pos += 1;
        }
        if pos >= limit {
            return None;
        }

        let mut depth = 0u32;
        while pos < limit {
            match bytes[pos] {
                b'{' => {
                    depth += 1;
                    pos += 1;
                }
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(pos as u32);
                    }
                    pos += 1;
                }
                b'/' if pos + 1 < limit && bytes[pos + 1] == b'/' => {
                    pos += 2;
                    while pos < limit && !matches!(bytes[pos], b'\n' | b'\r') {
                        pos += 1;
                    }
                }
                b'/' if pos + 1 < limit && bytes[pos + 1] == b'*' => {
                    pos += 2;
                    while pos + 1 < limit && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                        pos += 1;
                    }
                    pos = std::cmp::min(pos + 2, limit);
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[pos];
                    pos += 1;
                    while pos < limit {
                        if bytes[pos] == b'\\' {
                            pos = std::cmp::min(pos + 2, limit);
                        } else if bytes[pos] == quote {
                            pos += 1;
                            break;
                        } else {
                            pos += 1;
                        }
                    }
                }
                _ => pos += 1,
            }
        }
        None
    }

    pub(super) fn trailing_same_line_comment_end_after(&self, pos: u32) -> Option<u32> {
        let source_text = self.source_text?;
        crate::emitter::get_trailing_comment_ranges(source_text, pos as usize)
            .last()
            .map(|comment| comment.end)
    }
}
