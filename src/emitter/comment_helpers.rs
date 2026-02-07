use super::{CommentKind, Printer, get_leading_comment_ranges, get_trailing_comment_ranges};
use crate::printer::safe_slice;

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
                if comment.end as u32 <= last_emitted.end {
                    continue;
                }
            }

            // Add space before trailing comment
            self.write_space();
            // Emit the comment text using safe slicing
            let comment_text = safe_slice::slice(text, comment.pos as usize, comment.end as usize);
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
                    }
                    i += 1;
                }
                b';' => {
                    if depth == 0 {
                        last_token_end = Some(i + 1);
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
                    i += 1;
                }
            }
        }

        last_token_end.map_or(end, |e| e as u32)
    }

    /// Emit leading comments before a node's start position.
    #[allow(dead_code)] // Infrastructure for comment preservation
    pub(super) fn emit_leading_comments(&mut self, pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let comments = get_leading_comment_ranges(text, pos as usize);
        for comment in comments {
            // Use safe slicing to avoid panics
            let comment_text = safe_slice::slice(text, comment.pos as usize, comment.end as usize);
            if !comment_text.is_empty() {
                self.write(comment_text);
            }
            if comment.has_trailing_newline {
                self.write_line();
            } else if comment.kind == CommentKind::MultiLine {
                self.write_space();
            }
        }
    }

    /// Emit comments found in the gap between two source positions.
    /// Scans from `from_pos` forward for comments, stopping at `to_pos`.
    #[allow(dead_code)] // Infrastructure for comment preservation
    pub(super) fn emit_gap_comments(&mut self, from_pos: u32, to_pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let start = std::cmp::min(from_pos as usize, text.len());
        let end = std::cmp::min(to_pos as usize, text.len());

        if start >= end {
            return;
        }

        let gap_text = safe_slice::slice(text, start, end);
        if gap_text.is_empty() {
            return;
        }

        let bytes = gap_text.as_bytes();
        let len = bytes.len();
        let mut pos = 0;

        while pos < len {
            let ch = bytes[pos];

            // Skip whitespace
            if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                pos += 1;
                continue;
            }

            // Check for comment start
            if ch == b'/' && pos + 1 < len {
                let next = bytes[pos + 1];

                if next == b'/' {
                    // Single-line comment
                    let comment_start = start + pos;
                    let mut comment_end = pos + 2;
                    while comment_end < len
                        && bytes[comment_end] != b'\n'
                        && bytes[comment_end] != b'\r'
                    {
                        comment_end += 1;
                    }
                    let comment_text = safe_slice::slice(text, comment_start, start + comment_end);
                    if !comment_text.is_empty() {
                        self.write(comment_text);
                    }
                    self.write_line();

                    pos = comment_end;
                    if pos < len && bytes[pos] == b'\r' {
                        pos += 1;
                    }
                    if pos < len && bytes[pos] == b'\n' {
                        pos += 1;
                    }
                    continue;
                } else if next == b'*' {
                    // Multi-line comment
                    let comment_start = start + pos;
                    let mut comment_end = pos + 2;
                    let mut found_end = false;
                    while comment_end + 1 < len {
                        if bytes[comment_end] == b'*' && bytes[comment_end + 1] == b'/' {
                            comment_end += 2;
                            found_end = true;
                            break;
                        }
                        comment_end += 1;
                    }
                    if !found_end {
                        // Unterminated block comment - skip to end
                        comment_end = len;
                    }
                    let comment_text = safe_slice::slice(text, comment_start, start + comment_end);
                    if !comment_text.is_empty() {
                        self.write(comment_text);
                    }
                    self.write_line();

                    pos = comment_end;
                    continue;
                }
            }

            // Hit non-whitespace, non-comment content - stop scanning
            break;
        }
    }

    /// Emit comments in the gap between last_processed_pos and the given position.
    /// This handles comments that appear between AST nodes.
    #[allow(dead_code)] // Infrastructure for comment preservation
    pub(super) fn emit_comments_in_gap(&mut self, up_to_pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        // Scan for comments between last_processed_pos and up_to_pos
        let start = self.last_processed_pos as usize;
        let end = std::cmp::min(up_to_pos as usize, text.len());

        if start >= end || start >= text.len() {
            return;
        }

        // Use safe slicing for the gap
        let gap_text = safe_slice::slice(text, start, end);
        if gap_text.is_empty() {
            return;
        }

        let bytes = gap_text.as_bytes();
        let len = bytes.len();
        let mut pos = 0;

        while pos < len {
            let ch = bytes[pos];

            // Skip whitespace
            if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                pos += 1;
                continue;
            }

            // Check for comment start
            if ch == b'/' && pos + 1 < len {
                let next = bytes[pos + 1];

                if next == b'/' {
                    // Single-line comment
                    let comment_start = start + pos;
                    let mut comment_end = pos + 2;
                    while comment_end < len
                        && bytes[comment_end] != b'\n'
                        && bytes[comment_end] != b'\r'
                    {
                        comment_end += 1;
                    }
                    // Use safe slicing for comment text
                    let comment_text = safe_slice::slice(text, comment_start, start + comment_end);
                    if !comment_text.is_empty() {
                        self.write(comment_text);
                    }
                    self.write_line();

                    // Skip past the comment and newline
                    pos = comment_end;
                    if pos < len && bytes[pos] == b'\r' {
                        pos += 1;
                    }
                    if pos < len && bytes[pos] == b'\n' {
                        pos += 1;
                    }
                    continue;
                } else if next == b'*' {
                    // Multi-line comment
                    let comment_start = start + pos;
                    let mut comment_end = pos + 2;
                    while comment_end + 1 < len {
                        if bytes[comment_end] == b'*' && bytes[comment_end + 1] == b'/' {
                            comment_end += 2;
                            break;
                        }
                        comment_end += 1;
                    }
                    // Use safe slicing for comment text
                    let comment_text = safe_slice::slice(text, comment_start, start + comment_end);
                    if !comment_text.is_empty() {
                        self.write(comment_text);
                    }
                    self.write_line();

                    pos = comment_end;
                    continue;
                }
            }

            // Hit non-whitespace, non-comment content - stop scanning
            break;
        }
    }
}
