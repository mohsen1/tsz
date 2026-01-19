use super::{CommentKind, ThinPrinter, get_leading_comment_ranges, get_trailing_comment_ranges};
use crate::thin_printer::safe_slice;

impl<'a> ThinPrinter<'a> {
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
            // Add space before trailing comment
            self.write_space();
            // Emit the comment text using safe slicing
            let comment_text = safe_slice::slice(text, comment.pos as usize, comment.end as usize);
            if !comment_text.is_empty() {
                self.write(comment_text);
            }
        }
    }

    /// Emit leading comments before a node's start position.
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

    /// Emit comments in the gap between last_processed_pos and the given position.
    /// This handles comments that appear between AST nodes.
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
