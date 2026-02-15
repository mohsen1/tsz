use super::{Printer, get_trailing_comment_ranges};
use crate::printer::safe_slice;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Comment Emission Helpers
    // =========================================================================

    /// Emit trailing comments after a node's end position.
    /// Note: In TypeScript's AST, node.end often includes trailing trivia (including comments).
    /// We clamp the position to valid bounds and scan from there.
    pub(super) fn emit_trailing_comments(&mut self, end_pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        // Clamp position to valid range
        let pos = std::cmp::min(end_pos as usize, text.len());
        let comments = get_trailing_comment_ranges(text, pos);
        for comment in comments {
            // Skip if this comment was already emitted.
            // Since all_comments is sorted by position and comment_emit_idx advances
            // monotonically, a comment whose end position is at or before the current
            // all_comments[comment_emit_idx - 1].end has already been emitted.
            if self.comment_emit_idx > 0 {
                let last_emitted = &self.all_comments[self.comment_emit_idx - 1];
                if comment.end <= last_emitted.end {
                    continue;
                }
            }

            // Add space before trailing comment
            self.write_space();
            // Emit the comment text using safe slicing
            let comment_text = safe_slice::slice(text, comment.pos as usize, comment.end as usize);
            if !comment_text.is_empty() {
                self.write_comment(comment_text);
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

    /// Find the position right after the node's actual code content ends.
    /// This gives us the position where trailing comments begin. Our parser's
    /// node.end extends past trailing trivia into the next token's position,
    /// so we need to find the actual end of the node's code.
    ///
    /// Uses forward scanning with brace depth tracking to find the correct
    /// closing `}` at depth 0, avoiding confusion with parent scope braces.
    pub(super) fn find_token_end_before_trivia(&self, pos: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return end;
        };
        let bytes = text.as_bytes();
        let end_pos = std::cmp::min(end as usize, bytes.len());
        let start_pos = pos as usize;

        if start_pos >= end_pos {
            return end;
        }

        // Forward scan: track the last `}` or `;` at brace depth 0.
        // This correctly identifies the node's own closing token without
        // accidentally matching a parent scope's closing brace.
        let mut depth: i32 = 0;
        let mut last_token_end: Option<usize> = None;
        let mut last_non_trivia_at_depth0: Option<usize> = None;
        let mut i = start_pos;

        while i < end_pos {
            let ch = bytes[i];
            match ch {
                b'{' => {
                    depth += 1;
                    i += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        // We've gone past our scope into a parent - stop
                        break;
                    }
                    if depth == 0 {
                        // This is the closing brace at the top level of this node
                        last_token_end = Some(i + 1);
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
                b';' => {
                    if depth == 0 {
                        last_token_end = Some(i + 1);
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
                b'\'' | b'"' | b'`' => {
                    // Skip string literals to avoid false matches
                    let quote = ch;
                    i += 1;
                    while i < end_pos {
                        if bytes[i] == b'\\' {
                            i += 2; // skip escaped char
                        } else if bytes[i] == quote {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < end_pos && bytes[i + 1] == b'/' => {
                    // Skip single-line comments
                    i += 2;
                    while i < end_pos && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end_pos && bytes[i + 1] == b'*' => {
                    // Skip multi-line comments
                    i += 2;
                    while i + 1 < end_pos {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    if depth == 0 && !matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
            }
        }

        let mut token_end = last_token_end
            .or(last_non_trivia_at_depth0)
            .map_or(end, |e| e as u32);

        // Some transformed nodes report `end` before the terminating `;`.
        // Recover by scanning a short same-line suffix for `;` or `}`.
        if token_end <= end {
            let mut j = end_pos;
            while j < bytes.len() {
                match bytes[j] {
                    b' ' | b'\t' => j += 1,
                    b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'/' => break,
                    b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'*' => break,
                    b';' | b'}' => {
                        token_end = (j + 1) as u32;
                        break;
                    }
                    _ => break,
                }
            }
        }

        token_end
    }

    /// Emit all pending comments from `all_comments` whose end position is before `pos`.
    /// Uses the `comment_emit_idx` cursor to advance through comments.
    /// Similar to the top-level statement comment emission logic.
    pub(super) fn emit_comments_before_pos(&mut self, pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }
        let actual_start = self.skip_whitespace_forward(pos, pos + 1024);
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= actual_start {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    let comment_text =
                        crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    self.write_comment(comment_text);
                    if c_trailing {
                        self.write_line();
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }
    }

    /// Write comment text, trimming trailing whitespace from each line of multi-line comments.
    /// TypeScript strips trailing whitespace from multi-line comment lines in its emitter.
    pub(super) fn write_comment(&mut self, text: &str) {
        if text.contains('\n') {
            // Multi-line comment: trim trailing whitespace from each line
            let mut first = true;
            for line in text.split('\n') {
                if !first {
                    self.write("\n");
                }
                self.write(line.trim_end());
                first = false;
            }
        } else {
            self.write(text);
        }
    }

    /// Skip (suppress) all comments that belong to an erased declaration (interface, type alias).
    /// Advances `comment_emit_idx` past any comments whose end position falls within the node's range.
    pub(super) fn skip_comments_for_erased_node(&mut self, node: &Node) {
        // Find the actual end of the node's code content, excluding trailing trivia.
        // This prevents us from skipping comments that appear after the closing brace/token
        // but before the next statement (which should be emitted as leading comments for
        // that next statement).
        let actual_end = self.find_token_end_before_trivia(node.pos, node.end);

        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if c.end <= actual_end {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }
}
